//! World Container
//!
//! # World
//!
//! [`World`] is the central container of the ECS architecture; see the
//! crate-level documentation.
//!
//! # DeferredWorld
//!
//! [`DeferredWorld`] is a world in which only **data** is mutable.
//!
//! "Data-mutable" means the stored values of components and resources may be
//! read and written.
//!
//! The structure is immutable: operations such as spawning or despawning
//! entities, inserting or removing resources, and changing an entity's
//! component list are forbidden, because they can move the underlying memory
//! and invalidate external references.
//!
//! To modify the structure, push deferred commands through [`Commands`].
//!
//! This type is typically used only to implement component lifecycle hooks.
//!
//! # NonSendWorld
//!
//! [`NonSendWorld`] provides access to resources that do not implement
//! `Sync` or `Send`.
//!
//! It can be used as a system parameter, or created on demand with the
//! `with_non_send` functions on [`World`].  It is guaranteed to only ever
//! appear on the main thread, so users can safely touch such data.
//!
//! # WorldCell
//!
//! [`WorldCell`] is analogous to `&'_ UnsafeCell<World>`.
//!
//! In the ECS architecture, data such as components and resources is stored
//! in separate, disjoint locations, so non-conflicting accesses should be
//! executable in parallel.
//!
//! However, Rust's safe borrowing rules make it difficult to split a
//! `&mut World` into many internal sub-references.
//!
//! [`World`]: crate::world::World
//! [`DeferredWorld`]: crate::world::DeferredWorld
//! [`NonSendWorld`]: crate::world::NonSendWorld
//! [`WorldCell`]: crate::world::WorldCell
//! [`Commands`]: crate::command::Commands

// -----------------------------------------------------------------------------
// Modules
// -----------------------------------------------------------------------------

mod cell;
mod deferred;
mod from_world;
mod non_send;

// -----------------------------------------------------------------------------
// Re-exports
// -----------------------------------------------------------------------------

pub use cell::WorldCell;
pub use deferred::DeferredWorld;
pub use from_world::FromWorld;
pub use non_send::NonSendWorld;

// -----------------------------------------------------------------------------
// Inline Content
// -----------------------------------------------------------------------------

use core::fmt::{Debug, Display, Formatter};
use core::sync::atomic::AtomicU32;
use core::sync::atomic::Ordering;

use zlim_utils::define_atomic_id;
use zlim_utils::ext::CachePadded;

use crate::tick::CHECK_CYCLE;
use crate::tick::Tick;
use crate::time::TimeUpdateStrategy;

use crate::entity::EntityAllocator;
use crate::entity::RemoteAllocator;
use crate::entity::{Entities, RootEntities};

use crate::bundle::Bundles;
use crate::component::Components;
use crate::table::Tables;

use crate::message::Messages;
use crate::resource::Resources;

use crate::command::CommandQueue;
use crate::schedule::Schedules;

use crate::query::QueryCache;
use crate::system::SystemCache;
use crate::time::TimeCache;

use crate::error::ErrorHandler;
use crate::error::default_error_handler;

// -----------------------------------------------------------------------------
// WorldId
// -----------------------------------------------------------------------------

define_atomic_id!(
    /// Unique identifier for a [`World`] instance.
    ///
    /// `WorldId` values are handed out from a process-wide atomic counter, so no
    /// two live worlds ever share the same id.  It is mainly useful for debugging
    /// and logging; retrieve the id of a world with [`World::id`].
    ///
    /// Note that the id space is process-wide — creating an extremely large number
    /// of worlds within one process can exhaust the 32-bit id space, at which
    /// point allocation panics.
    WorldId
);

impl Display for WorldId {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

// -----------------------------------------------------------------------------
// World
// -----------------------------------------------------------------------------

/// Central container for ECS runtime state.
///
/// `World` owns entities, components, systems, and all related metadata.
///
/// It is the primary entry point for structural mutations (spawn, insert,
/// remove, despawn) and for running systems.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// #[derive(TypePath, Component, Clone, Debug, PartialEq)]
/// struct Position {
///     x: f32,
///     y: f32,
/// }
///
/// let mut world = World::alloc();
///
/// // Spawn an entity with a component bundle.
/// let entity = world.spawn(Position { x: 1.0, y: 2.0 }, None);
/// assert!(entity.is_spawned());
/// assert_eq!(entity.get::<Position>(), Some(&Position { x: 1.0, y: 2.0 }));
///
/// // Query entities by component.
/// ::core::mem::drop(entity); // release the world borrow before querying
/// let total: f32 = world.query::<&Position, ()>().iter().map(|p| p.x).sum();
/// assert_eq!(total, 1.0);
/// ```
#[repr(C)]
pub struct World {
    /// Unique identifier for debugging and logging.
    pub(crate) id: WorldId,

    /// Tick representing the end of the previous system run.
    ///
    /// Used as the lower bound of the change-detection window:
    /// `last_run..this_run`.
    pub(crate) last_run: Tick,

    /// Tick of the most recent tick-clamp validation pass.
    ///
    /// Validation is throttled to at most once per [`CHECK_CYCLE`] ticks
    /// to avoid excessive overhead.
    pub(crate) last_check: Tick,

    /// Handler invoked when an error occurs during world operations.
    ///
    /// Fallback to [`default_error_handler`] if it's [`None`].
    pub(crate) error_handler: Option<ErrorHandler>,

    /// Lock-free entity ID allocator.
    pub(crate) allocator: EntityAllocator,

    /// Entity tree holding spawn state, hierarchy, and storage locations.
    pub(crate) entities: Entities,

    /// Per-world component type registry.
    pub(crate) components: Components,

    /// Dense columnar storage for all archetypes (component-set tables).
    pub(crate) tables: Tables,

    /// Bundle-type-to-component-set registry.
    pub(crate) bundles: Bundles,

    /// Global singleton resource storage.
    pub(crate) resources: Resources,

    /// Per-world message type registry and queue rotation state.
    pub(crate) messages: Messages,

    /// Named collection of schedules that can be run against this world.
    pub(crate) schedules: Schedules,

    /// Type-erased cache of system instances, keyed by their SystemId.
    pub(crate) system_cache: SystemCache,

    /// Per-world memoisation of query states, keyed by query type.
    pub(crate) query_cache: QueryCache,

    /// A cache used to accelerate access to the times.
    pub(crate) time_cache: TimeCache,
    /// How this world advances its real time each frame.
    pub(crate) time_strategy: TimeUpdateStrategy,

    /// Queue of deferred structural commands (entity/resource add/remove).
    pub(crate) command_queue: CommandQueue,
    /// Record the starting position of the world level command queue.
    pub(crate) command_start: usize,

    /// Current change-detection epoch, stored behind cache-line padding
    /// to reduce false sharing in multi-threaded contexts.
    pub(crate) this_run: CachePadded<AtomicU32>,
}

impl Debug for World {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("World")
            .field("id", &self.id())
            .field("this_run", &self.this_run())
            .field("last_run", &self.last_run())
            .field("entity_count", &self.entity_count())
            .field("resources", &self.resources)
            .field("components", &self.components)
            .field("schedules", &self.schedules.inner.keys())
            .finish()
    }
}

// -----------------------------------------------------------------------------
// Allocation
// -----------------------------------------------------------------------------

impl World {
    /// Allocates a new [`World`] with default subsystems and a fresh
    /// [`WorldId`].
    ///
    /// The world is returned as a `Box<World>` to keep the allocation on the
    /// heap — worlds are large and should not be moved frequently.
    ///
    /// In debug builds (outside tests), the function logs the time taken to
    /// initialise the world.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// assert_eq!(world.entity_count(), 0);
    ///
    /// let entity = world.spawn_empty(None);
    /// assert!(entity.is_spawned());
    /// ```
    #[inline(never)]
    pub fn alloc() -> Box<World> {
        let mut world = Box::new(World {
            id: WorldId::alloc(),
            last_run: Tick::new(0),
            last_check: Tick::new(0),
            error_handler: None,
            allocator: EntityAllocator::new(),
            entities: Entities::new(),
            components: Components::new(),
            tables: Tables::new(),
            bundles: Bundles::new(),
            resources: Resources::new(),
            command_queue: CommandQueue::silent(),
            command_start: 0,
            messages: Messages::new(),
            schedules: Schedules::new(),
            system_cache: SystemCache::new(),
            query_cache: QueryCache::new(),
            time_cache: TimeCache::new(),
            time_strategy: TimeUpdateStrategy::default(),
            this_run: CachePadded::new(AtomicU32::new(1)),
        });

        {
            // Initialize Time
            world.time_cache.apply(&mut world.resources);
        }

        {
            // Initialize Messages
            world.register_message::<crate::message::ReparentSignal>();
        }

        world
    }

    /// Returns the unique identifier of this world.
    #[inline(always)]
    pub fn id(&self) -> WorldId {
        self.id
    }
}

// -----------------------------------------------------------------------------
// Error handler
// -----------------------------------------------------------------------------

impl World {
    /// Returns a copy of the current error handler.
    ///
    /// The error handler is invoked when the world encounters a non-fatal
    /// error during an operation (e.g., a missing component during
    /// despawn).
    #[inline(always)]
    pub fn error_handler(&self) -> ErrorHandler {
        self.error_handler.unwrap_or(default_error_handler)
    }

    /// Replaces the world's error handler.
    ///
    /// The provided handler will be used for all subsequent errors until it
    /// is replaced again or the world is dropped.
    #[inline(always)]
    pub fn set_error_handler(&mut self, handler: ErrorHandler) {
        self.error_handler = Some(handler);
    }

    /// Set the world's error handler if it's missing.
    #[inline(always)]
    pub fn try_set_error_handler(&mut self, handler: ErrorHandler) {
        if self.error_handler.is_none() {
            self.error_handler = Some(handler);
        }
    }
}

// -----------------------------------------------------------------------------
// Fields
// -----------------------------------------------------------------------------

impl World {
    /// Returns the world's lock-free entity ID allocator.
    ///
    /// The allocator hands out fresh entity indices and recycles freed ones;
    /// it does **not** store component data.  See [`Entities`] for spawn
    /// state, hierarchy, and storage locations.
    #[inline(always)]
    pub fn allocator(&self) -> &EntityAllocator {
        &self.allocator
    }

    /// Returns the world's entity ID allocator for mutation.
    ///
    /// Prefer [`World::allocator`] unless you need to manually free or
    /// recycle entity IDs.
    #[inline(always)]
    pub fn allocator_mut(&mut self) -> &mut EntityAllocator {
        &mut self.allocator
    }

    /// Returns a [`RemoteAllocator`] connected to this world's allocator.
    ///
    /// The remote allocator can allocate entity IDs from other threads (or
    /// without direct `&World` access) while sharing the same underlying
    /// free-list state as the world.
    #[inline]
    pub fn remote_allocator(&self) -> RemoteAllocator {
        self.allocator.remote()
    }

    /// Returns the world's sparse entity metadata storage.
    ///
    /// Tracks each entity's generation, spawn state, storage location, and
    /// parent/child relationships.  See [`World::allocator`] for ID
    /// allocation and [`Tables`] for component data storage.
    ///
    /// [`Tables`]: crate::table::Tables
    #[inline(always)]
    pub fn entities(&self) -> &Entities {
        &self.entities
    }

    /// Returns the world's dense columnar table storage, organised by
    /// archetype (component-set).
    ///
    /// [`Tables`] holds the actual component data for every entity; see
    /// [`World::entities`] for entity metadata.
    #[inline(always)]
    pub fn tables(&self) -> &Tables {
        &self.tables
    }

    /// Returns the world's per-world component type registry.
    ///
    /// Contains metadata (hooks, layout, cloners) for every registered
    /// component type in this world.
    #[inline(always)]
    pub fn components(&self) -> &Components {
        &self.components
    }

    /// Returns the world's message type registry.
    #[inline(always)]
    pub fn messages(&self) -> &Messages {
        &self.messages
    }

    /// Returns the world's resource value storage.
    ///
    /// Resource **values** live here; metadata is looked up through the
    /// global [`ResourceDB`] (e.g. `ResourceDB::of::<R>()`).
    ///
    /// [`ResourceDB`]: crate::resource::ResourceDB
    #[inline(always)]
    pub fn resources(&self) -> &Resources {
        &self.resources
    }

    /// Returns the world's resource value storage.
    #[inline(always)]
    pub fn resources_mut(&mut self) -> &mut Resources {
        &mut self.resources
    }

    /// Returns the collection of schedules stored in this world.
    #[inline(always)]
    pub fn schedules(&self) -> &Schedules {
        &self.schedules
    }

    /// Returns the collection of schedules stored in this world for
    /// mutation.
    #[inline(always)]
    pub fn schedules_mut(&mut self) -> &mut Schedules {
        &mut self.schedules
    }
}

// -----------------------------------------------------------------------------
// Ticks — change detection
// -----------------------------------------------------------------------------

impl World {
    /// Returns the tick that marks the end of the previous system run.
    ///
    /// This is the lower bound of the change-detection window.  Components
    /// whose `added` or `changed` tick is strictly greater than `last_run`
    /// and less than or equal to `this_run` are considered "changed".
    #[inline(always)]
    pub fn last_run(&self) -> Tick {
        self.last_run
    }

    /// Returns the current tick (the upper bound of the change-detection
    /// window).
    ///
    /// This is a relaxed atomic load and can be called from shared
    /// references.
    #[inline]
    pub fn this_run(&self) -> Tick {
        Tick::new(self.this_run.load(Ordering::Relaxed))
    }

    /// Returns the current tick with minimal overhead.
    ///
    /// This bypasses the atomic load by accessing the inner value directly
    /// through `&mut self`.  It is intended for hot paths where the caller
    /// already holds exclusive access.
    #[inline]
    pub fn this_run_fast(&mut self) -> Tick {
        Tick::new(*self.this_run.get_mut())
    }

    /// Atomically increments the current tick and returns the _previous_
    /// value.
    ///
    /// This is the primary mechanism for advancing change-detection epochs.
    /// After the tick advances, any component modifications that occurred
    /// during the previous epoch become visible to systems that use the
    /// old tick as their baseline.
    #[inline]
    pub fn advance_tick(&self) -> Tick {
        Tick::new(self.this_run.fetch_add(1, Ordering::Relaxed))
    }

    /// Resets the world's own change-detection baseline to the current
    /// tick.
    ///
    /// After calling this, changes that happened before the current moment
    /// are no longer considered "new" from the world's perspective.
    ///
    /// This only affects the world's internal change tracking.  It does not
    /// modify `last_run` values stored inside individual systems.
    ///
    /// Both systems and the world track changes using a `last_run` marker:
    /// a change is considered visible when it falls within
    /// `last_run..this_run`.  Systems update their own `last_run`
    /// automatically after each run, while the world baseline must be reset
    /// manually.  This function synchronizes the world baseline to the
    /// current tick.
    pub fn clear_trackers(&mut self) -> Tick {
        self.clamp_ticks();

        let last_run = *self.this_run.get_mut();
        let this_run = last_run.wrapping_add(1);

        self.last_run = Tick::new(last_run);
        *self.this_run.get_mut() = this_run;

        Tick::new(this_run)
    }

    /// Runs periodic tick-age validation across component and resource
    /// storages.
    ///
    /// Over time, ticks can wrap around (they use a 32-bit counter).  When
    /// this happens, very old ticks become indistinguishable from very new
    /// ones, which can cause false positives in change detection.
    ///
    /// `clamp_ticks` mitigates this by periodically "clamping" stored
    /// ticks to a safe range relative to the current epoch.  The validation
    /// runs at most once per [`CHECK_CYCLE`] ticks, measured from the
    /// previous validation point (`last_check`).
    ///
    /// In multi-threaded builds, the clamping work is parallelised across
    /// the task pool.
    #[inline]
    pub fn clamp_ticks(&mut self) {
        #[cold]
        #[inline(never)]
        fn clamp_ticks_cold(world: &mut World) {
            let now = world.this_run_fast();

            let tables = &mut world.tables;
            let resources = &mut world.resources;
            let schedules = &mut world.schedules;

            zlim_task::cfg::multi_thread! {
                if {
                    let pool = zlim_task::MainTaskPool::get();
                    pool.scope(|s| {
                        s.spawn(async move {
                            resources.iter_mut().for_each(|x| x.clamp_ticks(now));
                        });

                        tables.iter_mut().for_each(|table| {
                            s.spawn(async move { table.clamp_ticks(now) });
                        });
                        schedules.iter_mut().for_each(|schedules| {
                            s.spawn(async move { schedules.clamp_ticks(now) });
                        });
                    });
                } else {
                    resources.iter_mut().for_each(|x| x.clamp_ticks(now));
                    tables.iter_mut().for_each(|x| x.clamp_ticks(now));
                    schedules.iter_mut().for_each(|x| x.clamp_ticks(now));
                }
            }
        }

        let this_run = *self.this_run.get_mut();
        let last_check = self.last_check.get();

        if this_run.wrapping_sub(last_check) >= CHECK_CYCLE {
            self.last_check = Tick::new(this_run);
            clamp_ticks_cold(self)
        }
    }
}

// -----------------------------------------------------------------------------
// Time
// -----------------------------------------------------------------------------

impl World {
    /// Returns the strategy used to advance this world's real time.
    ///
    /// A [`TimeUpdateStrategy::None`] strategy freezes the world's clocks —
    /// used by worlds whose time is supplied externally (e.g. the pipelined
    /// render world, which receives the main world's clocks during
    /// extraction).
    ///
    /// [`TimeUpdateStrategy::None`]: crate::time::TimeUpdateStrategy::None
    #[inline]
    pub fn time_strategy(&self) -> TimeUpdateStrategy {
        self.time_strategy
    }

    /// Sets the strategy used to advance this world's real time.
    #[inline]
    pub fn set_time_strategy(&mut self, strategy: TimeUpdateStrategy) {
        self.time_strategy = strategy;
    }
}

// -----------------------------------------------------------------------------
// Statistics
// -----------------------------------------------------------------------------

impl World {
    /// Return a iterator of the root entities.
    ///
    /// The root entities is unordered.
    #[inline]
    pub fn root_entities(&self) -> RootEntities<'_> {
        self.entities.root_entities()
    }

    /// Returns the number of currently spawned (alive) entities.
    #[inline]
    pub fn entity_count(&self) -> usize {
        self.entities.count_spawned()
    }

    /// Returns the number of registered component **types** in this world.
    #[inline]
    pub fn component_count(&self) -> usize {
        self.components.len()
    }

    /// Returns the number of registered resource **types** in this world.
    #[inline]
    pub fn resource_count(&self) -> usize {
        self.resources.iter().len()
    }
}

// -----------------------------------------------------------------------------
// Shrink
// -----------------------------------------------------------------------------

impl World {
    /// Shrink memory usage.
    pub fn shrink(&mut self) {
        // Pure memory release, should not be executed with multiple threads.
        self.tables.iter_mut().for_each(|x| x.shrink());
    }
}

// -----------------------------------------------------------------------------
// Periodic Update
// -----------------------------------------------------------------------------

impl World {
    /// Periodic metadata refresh function.
    ///
    /// Must be called once per frame, at the beginning of each frame.
    ///
    /// This is an internal system function. It is automatically invoked by
    /// `zlim_app::App` for each world and should not be called by user code.
    ///
    /// This function is responsible for updating internal metadata and does not
    /// contain any per-frame business logic.
    ///
    /// It performs the following operations:
    ///
    /// - Refreshes the world's component registry ([`World::components`])
    ///
    /// - Runs periodic tick-clamp validation ([`World::clamp_ticks`])
    ///
    /// - Advances the world's time clocks and publishes them as resources
    ///   (unless the [`TimeUpdateStrategy`] is `None`; see
    ///   [`World::set_time_strategy`] to change it, e.g. for
    ///   worlds whose time is supplied externally)
    ///
    /// - Updates all message queues on every call.
    ///
    /// - Queues due delayed commands (see [`Commands::delayed`]) into the
    ///   world's command queue **without executing them** — like every
    ///   other metadata update here, execution is left to the caller (e.g.
    ///   [`World::flush`](Self::flush)).
    ///
    /// This is deliberately an associated function taking `world: &mut Self`
    /// rather than a `&mut self` method, so it can only be invoked
    /// explicitly as `World::refresh_metadata(&mut world)` — never through
    /// method-call syntax — avoiding accidental dispatch through trait
    /// methods with the same name.
    ///
    /// [`TimeUpdateStrategy`]: crate::time::TimeUpdateStrategy
    /// [`Commands::delayed`]: crate::command::Commands::delayed
    pub fn refresh_metadata(world: &mut Self) {
        world.components.update();
        world.clamp_ticks();
        World::update_times(world);
        crate::message::update_messages(world);
        crate::time::queue_delayed_commands(world);
    }

    /// Updates the internal states of all schedules, if they need updating.
    ///
    /// In multi-threaded mode, this attempts to perform updates in parallel.
    ///
    /// By default, schedule updates (including initialization) are deferred — they occur
    /// lazily when the schedule is first invoked. This means you typically do not need to
    /// call this function. However, deferred updates are inherently serial, since
    /// [`World::run_schedule`] requires exclusive access of world.
    ///
    /// Therefore, this function is intended to be used within `App::run` to eagerly
    /// initialize all schedule states in parallel during the global pre-frame loading
    /// phase, before the first frame begins.
    pub fn update_schedules(world: &mut Self) {
        let mut schedules = Schedules::new();

        ::core::mem::swap(&mut schedules, &mut world.schedules);

        zlim_task::cfg::single_thread! {
            schedules.iter_mut().for_each(|s| s.update(world));
        }

        zlim_task::cfg::multi_thread! {{
            zlim_task::MainTaskPool::get().scope(|s| {
                for schedule in schedules.iter_mut() {
                    s.spawn(async { schedule.update(world); });
                }
            });
        }}

        ::core::mem::swap(&mut schedules, &mut world.schedules);

        debug_assert!(
            schedules.is_empty(),
            "Schedule::update should not modify world"
        );
    }

    /// Changes the message update policy for this world from `Always` to signal-driven mode.
    ///
    /// [`World::refresh_metadata`] is typically invoked at the start of each frame. It checks
    /// the world's internal message update policy and decides whether to refresh the message
    /// queue (i.e., swap message buffers and release expired data).
    ///
    /// By default, the policy is set to `Always`, meaning messages are updated on every
    /// call to `refresh_metadata`.
    ///
    /// Calling this function transitions the policy from `Always` to a `Wait`/`Ready` state
    /// machine. In this mode:
    ///
    /// 1. The initial state is `Wait`; `refresh_metadata` will **not** update the queue.
    /// 2. You must submit an [`UpdateMessagesSignal`] job to transition the state to `Ready`.
    /// 3. On the next `refresh_metadata` call, the queue will be updated, and the state will
    ///    automatically reset back to `Wait`.
    ///
    /// This is useful for worlds that need to synchronize message updates with a separate
    /// logic loop running at a lower frequency than the main frame loop.
    ///
    /// [`UpdateMessagesSignal`]: crate::message::UpdateMessagesSignal
    pub fn enable_update_messages_signal(world: &mut Self) {
        crate::message::enable_manual_update(&mut world.messages);
    }
}

// -----------------------------------------------------------------------------

const _ASSERTIONS_: () = {
    const fn is_sync<T: Sync>() {}
    const fn is_send<T: Send>() {}
    is_sync::<World>();
    is_send::<World>();
};

// -----------------------------------------------------------------------------
