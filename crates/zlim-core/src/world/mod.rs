//! World — the central ECS runtime container.
//!
//! # Overview
//!
//! [`World`] owns every piece of ECS state: entities, components, resources,
//! tables, bundles, and the command queue.  It is the single entry point for
//! spawning entities, inserting/removing components, and running systems.
//!
//! # Architecture
//!
//! The world is composed of these subsystems:
//!
//! | Subsystem | Type | Purpose |
//! |-----------|------|---------|
//! | Entities | [`EntityAllocator`] + [`Entities`] | Allocates/recycles entity IDs; tracks spawn state, hierarchy, and storage location |
//! | Components | [`Components`] | Per-world registry of component type metadata and hooks |
//! | Tables | [`Tables`] | Dense columnar storage organised by component-set archetype |
//! | Bundles | [`Bundles`] | Maps bundle types to their component sets |
//! | Resources | [`Resources`] + [`Slots`] | Global singleton storage; slots hold the actual data |
//! | Commands | [`CommandQueue`] | Deferred structural mutations (entity/resource add/remove) |
//! | Schedules | [`Schedules`] | Named collection of executable schedules |
//! | Messages | [`Messages`] | Message type registry and queue rotation |
//! | Ticks | `last_run` / `this_run` | Change-detection epoch counters |
//!
//! # Access control
//!
//! Direct `&mut World` access can cause borrow-checker friction in
//! performance-sensitive paths.  [`WorldCell`] and [`DeferredWorld`] provide
//! escape hatches with explicit safety contracts:
//!
//! - [`WorldCell`] — unchecked pointer-like handle with three access levels
//!   (`read_only`, `data_mut`, `full_mut`).
//!
//! - [`DeferredWorld`] — wraps [`WorldCell`] behind a convenient `Deref<Target
//!   = World>` facade, suitable for command/deferred mutation workflows.
//!
//! # Change detection
//!
//! Each world carries a monotonically incrementing tick counter.  Component
//! storage records the tick at which each value was added and last changed.
//! Systems compare their private `last_run` tick against these stored ticks to
//! detect which components changed since their previous execution.
//!
//! [`EntityAllocator`]: crate::entity::EntityAllocator
//! [`Entities`]: crate::entity::Entities
//! [`Components`]: crate::component::Components
//! [`Tables`]: crate::table::Tables
//! [`Bundles`]: crate::bundle::Bundles
//! [`Resources`]: crate::resource::Resources
//! [`Slots`]: crate::slot::Slots
//! [`CommandQueue`]: crate::command::CommandQueue
//! [`Schedules`]: crate::schedule::Schedules
//! [`Messages`]: crate::message::Messages
//! [`WorldCell`]: cell::WorldCell
//! [`DeferredWorld`]: deferred::DeferredWorld

// -----------------------------------------------------------------------------
// Modules
// -----------------------------------------------------------------------------

mod cell;
mod deferred;
mod from_world;

// -----------------------------------------------------------------------------
// Re-exports
// -----------------------------------------------------------------------------

pub use cell::WorldCell;
pub use deferred::DeferredWorld;
pub use from_world::FromWorld;

// -----------------------------------------------------------------------------
// Inline Content
// -----------------------------------------------------------------------------

use core::fmt::{Debug, Display, Formatter};
use core::sync::atomic::AtomicU32;
use core::sync::atomic::Ordering;

use zlim_utils::define_atomic_id;
use zlim_utils::ext::CachePadded;

use crate::bundle::Bundles;
use crate::command::CommandQueue;
use crate::component::Components;
use crate::entity::Entities;
use crate::entity::EntityAllocator;
use crate::entity::RemoteAllocator;
use crate::error::ErrorHandler;
use crate::error::default_error_handler;
use crate::message::Messages;
use crate::query::QueryCache;
use crate::resource::Resources;
use crate::schedule::Schedules;
use crate::slot::Slots;
use crate::system::Systems;
use crate::table::Tables;
use crate::tick::CHECK_CYCLE;
use crate::tick::Tick;

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
/// `World` owns entities, components, resources, and all related metadata.
/// It is the primary entry point for structural mutations (spawn, insert,
/// remove, despawn) and for running systems.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_reflect::derive::TypePath;
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
/// drop(entity); // release the world borrow before querying
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
    /// Defaults to [`default_error_handler`] which logs the error at
    /// `error` level.
    pub(crate) error_handler: ErrorHandler,

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

    /// Global singleton resource type registry.
    pub(crate) resources: Resources,

    /// Global singleton resource data storage.
    pub(crate) slots: Slots,

    /// Queue of deferred structural commands (entity/resource add/remove).
    pub(crate) command_queue: CommandQueue,

    /// Message type registry and queue rotation state.
    pub(crate) messages: Messages,

    /// Named collection of schedules that can be run against this world.
    pub(crate) schedules: Schedules,

    /// Type-erased cache of system instances, keyed by their SystemId.
    pub(crate) systems: Systems,

    /// Per-world memoisation of query states, keyed by query type.
    pub(crate) query_cache: QueryCache,

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
            .field("schedules", &self.schedules.len())
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
        crate::cfg::debug! {
            let start = ::zlim_os::time::Instant::now();
        }

        let ret = Box::new(World {
            id: WorldId::alloc(),
            last_run: Tick::new(0),
            last_check: Tick::new(0),
            error_handler: default_error_handler,
            allocator: EntityAllocator::new(),
            entities: Entities::new(),
            components: Components::new(),
            tables: Tables::new(),
            bundles: Bundles::new(),
            resources: Resources::new(),
            slots: Slots::new(),
            command_queue: CommandQueue::new(),
            messages: Messages::new(),
            schedules: Schedules::new(),
            systems: Systems::new(),
            query_cache: QueryCache::new(),
            this_run: CachePadded::new(AtomicU32::new(1)),
        });

        crate::cfg::debug! {
            zlim_log::debug!("World({}) initialized: {:?}", ret.id, start.elapsed());
        }

        ret
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
        self.error_handler
    }

    /// Replaces the world's error handler.
    ///
    /// The provided handler will be used for all subsequent errors until it
    /// is replaced again or the world is dropped.
    #[inline(always)]
    pub fn set_error_handler(&mut self, handler: ErrorHandler) {
        self.error_handler = handler;
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

    /// Returns the world's per-world component type registry.
    ///
    /// Contains metadata (hooks, layout, cloners) for every registered
    /// component type in this world.
    #[inline(always)]
    pub fn components(&self) -> &Components {
        &self.components
    }

    /// Returns the world's global resource type registry.
    ///
    /// This is the per-world snapshot of the global [`ResourceDB`]
    /// registrations; resource **values** live in the world's [`Slots`].
    ///
    /// [`ResourceDB`]: crate::resource::ResourceDB
    /// [`Slots`]: crate::slot::Slots
    #[inline(always)]
    pub fn resources(&self) -> &Resources {
        &self.resources
    }

    /// Returns the world's message type registry.
    ///
    /// Records every message type registered through
    /// [`World::register_message`] together with its queue rotation
    /// function.
    ///
    /// [`World::register_message`]: crate::world::World::register_message
    #[inline(always)]
    pub fn messages(&self) -> &Messages {
        &self.messages
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

    /// Returns the world's dense columnar table storage, organised by
    /// archetype (component-set).
    ///
    /// [`Tables`] holds the actual component data for every entity; see
    /// [`World::entities`] for entity metadata.
    #[inline(always)]
    pub fn tables(&self) -> &Tables {
        &self.tables
    }

    /// Returns the world's singleton resource data storage.
    ///
    /// Each [`crate::slot::Slot`] holds one resource value together with its
    /// change-detection ticks; use [`World::resources`] for the type registry
    /// and the typed accessors such as
    /// [`World::resource`](crate::world::World::resource) for daily use.
    #[inline(always)]
    pub fn slots(&self) -> &Slots {
        &self.slots
    }

    /// Returns the world's type-erased system storage.
    #[inline(always)]
    pub fn systems(&self) -> &Systems {
        &self.systems
    }

    /// Returns the world's type-erased system storage for mutation.
    #[inline(always)]
    pub fn systems_mut(&mut self) -> &mut Systems {
        &mut self.systems
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
            let slots = &mut world.slots;

            zlim_task::cfg::multi_thread! {
                if {
                    let pool = zlim_task::MainTaskPool::get();
                    pool.scope(|s| {
                        s.spawn(async move {
                            slots.iter_mut().for_each(|x| x.clamp_ticks(now));
                        });

                        tables.iter_mut().for_each(|table| {
                            s.spawn(async move { table.clamp_ticks(now) });
                        });
                    });
                } else {
                    slots.iter_mut().for_each(|x| x.clamp_ticks(now));
                    tables.iter_mut().for_each(|x| x.clamp_ticks(now));
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
// Statistics
// -----------------------------------------------------------------------------

impl World {
    /// Returns the number of currently spawned (alive) entities.
    #[inline]
    pub fn entity_count(&self) -> usize {
        self.entities.count_spawned()
    }

    /// Returns the number of registered component types in this world.
    #[inline]
    pub fn component_count(&self) -> usize {
        self.components.len()
    }

    /// Returns the number of registered resource types in this world.
    #[inline]
    pub fn resource_count(&self) -> usize {
        self.resources.len()
    }
}

// -----------------------------------------------------------------------------
// Periodic Update
// -----------------------------------------------------------------------------

impl World {
    /// Periodic update function, usually called at the beginning of each frame.
    ///
    /// Note that this function is only used to update some internal data and
    /// does not include processing logic for a single frame.
    ///
    /// It refreshes the world's resource and component registries
    /// ([`World::resources`] / [`World::components`]) and runs the periodic
    /// tick-clamp validation ([`World::clamp_ticks`]).  Message queues are
    /// **not** rotated here — use [`World::update_messages`] for that.
    ///
    /// [`World::update_messages`]: crate::world::World::update_messages
    pub fn update_basic(&mut self) {
        self.resources.update();
        self.components.update();

        self.clamp_ticks();
    }

    /// Rotates every registered [`MessageQueue`], making messages written
    /// since the previous update readable.
    ///
    /// Call this once per frame (or per update loop) before systems read
    /// messages; new messages become visible only after the rotation.
    ///
    /// [`MessageQueue`]: crate::message::MessageQueue
    pub fn update_messages(&mut self) {
        let cell = self.cell();
        let messages = unsafe { &cell.read_only().messages };

        for meta in messages.as_slice() {
            let update = meta.update();
            unsafe { update(cell.data_mut()) };
        }
    }

    /// Runs the per-frame update of every schedule stored in this world.
    ///
    /// In single-threaded builds each [`crate::schedule::Schedule::update`] is
    /// run in turn; in multi-threaded builds the updates are dispatched to
    /// the main task pool in parallel.
    ///
    /// `Schedule::update` must not mutate the world; this is enforced with a
    /// `debug_assert` after the pass.
    pub fn update_schedules(&mut self) {
        let mut schedules = Schedules::new();

        ::core::mem::swap(&mut schedules, &mut self.schedules);

        zlim_task::cfg::single_thread! {
            schedules.iter_mut().for_each(|s| s.update(self));
        }

        zlim_task::cfg::multi_thread! {{
            zlim_task::MainTaskPool::get().scope(|s| {
                for schedule in schedules.iter_mut() {
                    s.spawn(async { schedule.update(self); });
                }
            });
        }}

        ::core::mem::swap(&mut schedules, &mut self.schedules);

        debug_assert!(
            schedules.is_empty(),
            "Schedule::update should not modify world"
        );
    }
}

// -----------------------------------------------------------------------------
