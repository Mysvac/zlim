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
//! | Entities | [`EntityAllocator`] + [`EntityTree`] | Allocates/recycles entity IDs; tracks spawn state, hierarchy, and storage location |
//! | Components | [`Components`] | Per-world registry of component type metadata and hooks |
//! | Tables | [`Tables`] | Dense columnar storage organised by component-set archetype |
//! | Bundles | [`Bundles`] | Maps bundle types to their component sets |
//! | Resources | [`Resources`] + [`ResourceSlots`] | Global singleton storage; slots hold the actual data |
//! | Commands | [`CommandQueue`] | Deferred structural mutations (entity/resource add/remove) |
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
//! [`EntityTree`]: crate::entity::EntityTree
//! [`Components`]: crate::component::Components
//! [`Tables`]: crate::table::Tables
//! [`Bundles`]: crate::bundle::Bundles
//! [`Resources`]: crate::resource::Resources
//! [`ResourceSlots`]: crate::slot::ResourceSlots
//! [`CommandQueue`]: crate::command::CommandQueue
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
use crate::entity::EntityAllocator;
use crate::entity::EntityTree;
use crate::error::ErrorHandler;
use crate::error::default_error_handler;
use crate::resource::Resources;
use crate::slot::ResourceSlots;
use crate::table::Tables;
use crate::tick::CHECK_CYCLE;
use crate::tick::Tick;

// -----------------------------------------------------------------------------
// WorldId
// -----------------------------------------------------------------------------

define_atomic_id!(WorldId);

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
/// ```no_run
/// use zlim_core::world::World;
///
/// let mut world = World::alloc();
/// let entity = world.spawn((), None);
/// assert!(entity.is_spawned());
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
    pub(crate) entities: EntityTree,

    /// Per-world component type registry.
    pub(crate) components: Components,

    /// Dense columnar storage for all archetypes (component-set tables).
    pub(crate) tables: Tables,

    /// Bundle-type-to-component-set registry.
    pub(crate) bundles: Bundles,

    /// Global singleton resource type registry.
    pub(crate) resources: Resources,

    /// Global singleton resource data storage.
    pub(crate) resource_slots: ResourceSlots,

    /// Queue of deferred structural commands (entity/resource add/remove).
    pub(crate) command_queue: CommandQueue,

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
    #[inline(never)]
    pub fn alloc() -> Box<World> {
        crate::cfg::debug! {
            #[cfg(not(test))]
            let start = ::std::time::Instant::now();
        }

        let ret = Box::new(World {
            id: WorldId::alloc(),
            last_run: Tick::new(0),
            error_handler: default_error_handler,
            last_check: Tick::new(0),
            allocator: Default::default(),
            entities: Default::default(),
            components: Components::default(),
            tables: Tables::default(),
            bundles: Bundles::default(),
            resources: Resources::default(),
            resource_slots: ResourceSlots::default(),
            command_queue: CommandQueue::default(),
            this_run: CachePadded::new(AtomicU32::new(1)),
        });

        crate::cfg::debug! {
            #[cfg(not(test))]
            log::debug!("World({}) initialized: {:?}`", ret.id, start.elapsed());
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
            let slots = &mut world.resource_slots;

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
            clamp_ticks_cold(self)
        }
    }
}

// -----------------------------------------------------------------------------
// Ticks — change detection
// -----------------------------------------------------------------------------

impl World {
    /// Synchronises the per-world registries with their global counterparts.
    ///
    /// Component and resource types registered via `#[derive(Component)]` /
    /// `#[derive(Resource)]` are submitted to per-world registries
    /// lazily.  This function pulls in any types that were registered since
    /// the last update.
    ///
    /// This is called automatically during world initialisation and when
    /// types are first used by spawn/insert operations.  Users rarely need
    /// to call it directly.
    #[inline(always)]
    pub fn periodic_update(&mut self) {
        self.clamp_ticks();

        self.resources.update();
        self.components.update();
    }
}
