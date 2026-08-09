// -----------------------------------------------------------------------------
// Modules
// -----------------------------------------------------------------------------

mod cell;
mod deferred;

// -----------------------------------------------------------------------------
// WorldCell
// -----------------------------------------------------------------------------

pub use cell::WorldCell;
pub use deferred::DeferredWorld;

// -----------------------------------------------------------------------------
// WorldId
// -----------------------------------------------------------------------------

use core::fmt::{Display, Formatter};
use core::sync::atomic::AtomicU32;
use core::sync::atomic::Ordering;

use zlim_task::MainTaskPool;
use zlim_utils::define_atomic_id;
use zlim_utils::ext::CachePadded;

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

use crate::component::Components;
use crate::entity::EntityAllocator;
use crate::entity::EntityTree;
use crate::resource::Resources;
use crate::slot::ResourceSlots;
use crate::table::Tables;
use crate::tick::CHECK_CYCLE;
use crate::tick::Tick;

pub struct World {
    pub id: WorldId,
    pub allocator: EntityAllocator,
    pub entities: EntityTree,
    pub components: Components,
    pub tables: Tables,
    pub resources: Resources,
    pub slots: ResourceSlots,
    pub this_run: CachePadded<AtomicU32>,
    pub last_run: Tick,
    pub last_check: Tick,
}

impl World {
    /// Creates a new world with the given unique id.
    pub fn alloc() -> Box<World> {
        Box::new(World {
            id: WorldId::alloc(),
            allocator: Default::default(),
            entities: Default::default(),
            components: Components::default(),
            resources: Resources::default(),
            tables: Tables::default(),
            slots: ResourceSlots::default(),
            this_run: CachePadded::new(AtomicU32::new(1)),
            last_run: Tick::new(0),
            last_check: Tick::new(0),
        })
    }

    /// Returns this world's unique id.
    pub fn id(&self) -> WorldId {
        self.id
    }

    /// Returns the tick used as `last_run` for change detection.
    pub fn last_run(&self) -> Tick {
        self.last_run
    }

    /// Returns the current world tick (`this_run`).
    pub fn this_run(&self) -> Tick {
        Tick::new(self.this_run.load(Ordering::Relaxed))
    }

    /// Returns the current world tick (`this_run`).
    ///
    /// Requires a mutable borrow of World in `full_mut`
    /// state (not `data_mut` state).
    pub fn this_run_fast(&mut self) -> Tick {
        Tick::new(*self.this_run.get_mut())
    }

    /// Advances `this_run` atomically and returns the previous tick value.
    ///
    /// This is primarily used by concurrent execution paths.
    pub fn advance_tick(&self) -> Tick {
        Tick::new(self.this_run.fetch_add(1, Ordering::Relaxed))
    }

    /// Resets the world's own change-detection baseline.
    ///
    /// After calling this, changes that happened before the current moment are
    /// no longer considered "new" from the world's perspective.
    ///
    /// This only affects the world's internal change tracking. It does not
    /// modify `last_run` values stored inside systems.
    ///
    /// Both systems and the world track changes using a `last_run` marker:
    /// a change is considered visible when it falls within `last_run..this_run`.
    ///
    /// Systems update their own `last_run` automatically after each run, while
    /// the world baseline must be reset manually. This function synchronizes the
    /// world baseline to the current tick.
    pub fn clear_trackers(&mut self) -> Tick {
        self.clamp_ticks();

        let last_run = *self.this_run.get_mut();
        let this_run = last_run.wrapping_add(1);

        self.last_run = Tick::new(last_run);
        *self.this_run.get_mut() = this_run;

        Tick::new(this_run)
    }

    /// Runs periodic tick-age validation across component/resource storages.
    ///
    /// Validation runs at most once per [`CHECK_CYCLE`] ticks, measured from
    /// the previous validation point (`last_check`).
    #[inline]
    pub fn clamp_ticks(&mut self) {
        #[cold]
        #[inline(never)]
        fn clamp_ticks_cold(world: &mut World) {
            let now = world.this_run_fast();

            const TASK_POOL: bool = zlim_task::cfg::multi_thread!();

            let tables = &mut world.tables;
            let slots = &mut world.slots;

            if TASK_POOL {
                let pool = MainTaskPool::get();

                pool.scope(|s| {
                    s.spawn(async move {
                        slots.iter_mut().for_each(|x| x.clamp_ticks(now));
                    });

                    for table in tables.iter_mut() {
                        s.spawn(async move {
                            table.clamp_ticks(now);
                        });
                    }
                });
            } else {
                slots.iter_mut().for_each(|x| x.clamp_ticks(now));
                tables.iter_mut().for_each(|x| x.clamp_ticks(now));
            }
        }

        let this_run = *self.this_run.get_mut();
        let last_check = self.last_check.get();

        if this_run.wrapping_sub(last_check) >= CHECK_CYCLE {
            clamp_ticks_cold(self)
        }
    }
}
