use std::borrow::Cow;

use zlim_utils::debug::DebugLocation;

use crate::job::{Job, JobDB, JobGroup, JobId, JobLabel};
use crate::register_job;
use crate::system::{AccessTable, SystemError, SystemFlags};
use crate::tick::Tick;
use crate::world::{World, WorldCell};

// -----------------------------------------------------------------------------
// StageBegin & StageEnd

pub(super) struct StageBegin {
    id: JobId,
    last_run: Tick,
}

pub(super) struct StageEnd {
    id: JobId,
    last_run: Tick,
}

macro_rules! impl_simple_job {
    ($ty:ty, { $flag:expr }) => {
        impl $ty {
            pub(super) const fn new(id: JobId) -> Self {
                Self {
                    id,
                    last_run: Tick::new(0),
                }
            }
        }

        impl JobLabel for $ty {
            fn name() -> &'static str {
                concat!("zlim_core::", stringify!($ty))
            }

            fn database() -> JobDB {
                JobDB {
                    name: concat!("zlim_core::", stringify!($ty)),
                    ctor: |group: &'static str| -> Box<dyn Job> {
                        Box::new(Self {
                            id: JobId::new(concat!("zlim_core::", stringify!($ty)), group),
                            last_run: Tick::new(0),
                        })
                    },
                    run_if: &[],
                    location: DebugLocation::caller(),
                }
            }
        }

        impl Job for $ty {
            // inline is useless for dynamic object

            #[inline]
            fn id(&self) -> JobId {
                self.id
            }

            #[inline]
            fn flags(&self) -> SystemFlags {
                $flag
            }

            #[inline]
            fn last_run(&self) -> Tick {
                self.last_run
            }

            #[inline]
            fn clamp_ticks(&mut self, now: Tick) {
                self.last_run.clamp_with(now);
            }

            #[inline]
            fn set_last_run(&mut self, last_run: Tick) {
                self.last_run = last_run;
            }

            #[inline]
            fn initialize(&mut self, world: &World) {
                self.last_run = world.last_run();
            }

            #[inline]
            fn register_access(&self, _: &mut AccessTable) {}

            #[inline]
            unsafe fn run_raw(&mut self, _: WorldCell<'_>) -> Result<(), SystemError> {
                Ok(())
            }

            #[inline]
            fn apply_deferred(&mut self, _: &mut World) {}
        }
    };
}

// A Noop Job is never actually run by the Schedule, so we don't need to insert a Tracy span for it.

impl_simple_job!(StageBegin, { SystemFlags::NO_OP });
impl_simple_job!(StageEnd, {
    SystemFlags::NO_OP.union(SystemFlags::DEFERRED)
});
// ↑ `DEFERRED` is used to ensure that Schedule can use it to insert `ApplyDeferred`.

register_job!(StageBegin);
register_job!(StageEnd);

// -----------------------------------------------------------------------------
// ScheduleStage

/// A stage in a [`Schedule`], used to divide execution into phases.
///
/// Each stage has a name, and jobs belonging to it are grouped under a shared
/// group name so that begin/end markers can be distinguished per stage.
///
/// `()` is a special stage, and most jobs are inserted directly into it. It is
/// anonymous and cannot be given an execution order relative to other stages.
///
/// # Example
///
/// ```ignore
/// let run_fixed_main = world.schedule_entry(RunFixedMainLoop);
///
/// // Insert ScheduleStage
/// run_fixed_main.insert_stage(FixedMainLoopStage::BeforeFixedMainLoop);
/// run_fixed_main.insert_stage(FixedMainLoopStage::FixedMainLoop);
/// run_fixed_main.insert_stage(FixedMainLoopStage::AfterFixedMainLoop);
///
/// // Config Stage order
/// run_fixed_main.insert_order(&[
///     // should we use weak_order instead ?
///     // FixedMainLoopStage::BeforeFixedMainLoop.stage_begin(), // optional
///     FixedMainLoopStage::BeforeFixedMainLoop.stage_end(),
///     FixedMainLoopStage::FixedMainLoop.stage_begin(),
///     FixedMainLoopStage::FixedMainLoop.stage_end(),
///     FixedMainLoopStage::AfterFixedMainLoop.stage_begin(),
///     // FixedMainLoopStage::AfterFixedMainLoop.stage_end(),    // optional
/// ]);
///
/// // Insert Job to stage
/// run_fixed_main.insert_job::<HelloWorld>(FixedMainLoop);
/// ```
///
/// [`Schedule`]: crate::schedule::Schedule
#[diagnostic::on_unimplemented(
    note = "consider annotating `{Self}` with `#[derive(ScheduleStage)]`"
)]
pub trait ScheduleStage {
    fn stage_name(&self) -> Cow<'_, str>;

    fn group_name(&self) -> &'static str {
        zlim_utils::str::intern_str(&format!("{}#stage", self.stage_name()))
    }

    fn stage_begin(&self) -> JobId {
        JobId::new(StageBegin::name(), self.group_name())
    }

    fn stage_end(&self) -> JobId {
        JobId::new(StageEnd::name(), self.group_name())
    }
}

impl ScheduleStage for () {
    fn stage_name(&self) -> Cow<'_, str> {
        Cow::Borrowed(JobGroup::ANONYMOUS)
    }

    fn group_name(&self) -> &'static str {
        JobGroup::ANONYMOUS
    }

    fn stage_begin(&self) -> JobId {
        JobId::new(StageBegin::name(), JobGroup::ANONYMOUS)
    }

    fn stage_end(&self) -> JobId {
        JobId::new(StageEnd::name(), JobGroup::ANONYMOUS)
    }
}

// -----------------------------------------------------------------------------
