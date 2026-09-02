//! Job groups and group labels.

use core::hash::{BuildHasher, Hash, Hasher};
use std::collections::BTreeSet;
use std::sync::{PoisonError, RwLock};

use zlim_log as log;
use zlim_utils::debug::DebugLocation;
use zlim_utils::hash::{FixedState, HashMap, NoopState};
use zlim_utils::mem::Global;

use crate::job::{Job, JobDB, JobId, JobLabel};
use crate::register_job;
use crate::system::{AccessTable, SystemError, SystemFlags};
use crate::tick::Tick;
use crate::world::{World, WorldCell};

// -----------------------------------------------------------------------------

struct GroupBegin {
    id: JobId,
    last_run: Tick,
}

struct GroupEnd {
    id: JobId,
    last_run: Tick,
}

macro_rules! impl_simple_job {
    ($ty:ty, { $flag:expr }) => {
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

impl_simple_job!(GroupBegin, { SystemFlags::NO_OP });
impl_simple_job!(GroupEnd, {
    SystemFlags::NO_OP.union(SystemFlags::DEFERRED)
});
// ↑ `DEFERRED` is used to ensure that Schedule can use it to insert `ApplyDeferred`.

register_job!(GroupBegin);
register_job!(GroupEnd);

// -----------------------------------------------------------------------------

/// A named collection of jobs with ordering constraints.
///
/// A `JobGroup` is built by the [`job_group!`] macro from a name, a job list, an
/// optional run condition, and strong/weak ordering chains. Jobs are indexed
/// into [`Self::jobs`]; `order`, `weak_order` and `relaxed_order` store index pairs.
///
/// ```ignore
/// job_group! {
///     type: MyGroup,
///     name: "my_group",
///     jobs: [JobA, "group_job_b"],
///     condition: JobC, // optional
///     order: [["group_job_b", JobA]], // optional
///     // ...
/// }
/// ```
///
/// # Fields
///
/// - `name` : Unique identifier of JobGroup.
///   If repeated, the program may exhibit unexpected behavior.
///
/// - `jobs` : All jobs included in this JobGroup. All content
///   appearing in `order` and `weak_order` must be declared in `jobs`.
///
/// - `condition` : Optional preconditions.
///   A job with a return value of `bool` or `Result<bool, Error>`.
///   If it returns false or Error, the entire JobGroup will be quickly skipped.
///   It can be added automatically without being declared in the jobs.
///   It should not appear in `order` or `weak_order`, as this may lead to circular
///   dependencies, cause panic during `Schedule::initialize`.
///
/// - `order`: The subsequent jobs will only be executed after the previous
///   jobs have been completed **successfully**, and the results of deferred
///   commands queued by previous jobs is **definitely visible**.
///
/// - `weak_order`: The subsequent jobs will only be executed after the previous
///   jobs have been completed, **whether successful or not (even skipped)**. The results
///   of deferred commands queued by previous jobs is **definitely visible**.
///
/// - `relaxed_order`: The subsequent jobs will only be executed after the previous
///   jobs have been completed, **whether successful or not (even skipped)**. The results
///   of deferred commands queued by previous jobs **may not be visible**.
///
/// # Deferred Effect
///
/// `order` and `weak_order` require visibility of deferred commands.
///
/// In single threaded mode, this is directly achieved through sorting.
///
/// If using a multi-threaded executor, Schedule will insert some `ApplyDeferred`
/// Jobs when needed. Schedule has used many optimizations internally to reduce
/// such insertions and skip the negligible `ApplyDeferred` Job at runtime.
///
/// But it does affect performance because applying deferred commands requires
/// exclusive world accessing. Therefore, `relaxed_order` is a better choice
/// when there is no need to deferred commands visibility.
///
/// # Note on Job `run_if`
///
/// A job's `run_if` condition is considered part of the job itself. Although
/// execution is separated, the ordering constraints affect both the `run_if`
/// evaluation and the job body, and guarantee that deferred commands are
/// visible when the job runs.
///
/// When specifying ordering dependencies, you should always use the job's own
/// label or name, not the name of its `run_if` sub-item. Doing otherwise may
/// cause circular dependencies. As noted above, sub-item dependencies are handled
/// automatically — users should treat the `run_if` condition and the job body
/// as a single logical unit.
///
/// # Job Register
///
/// If `JobGroupLabel` is defined using [`job_group!`], the internal [`JobDB`]
/// will be automatically registered when it first generates a [`JobGroup`]
/// (e.g. `Schedule::insert_job_group` or `JobGroup::register`).
///
/// Of course, string format job label cannot be automatically registered,
/// while typed JobLabel can.
///
/// # Example
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_core::job::{JobGroup, JobGroupLabel};
///
/// #[job_fn(type = JobA, name = "group_job_a")]
/// fn job_a() {}
///
/// job_group! {
///     type: MyGroup,
///     name: "my_group",
///     jobs: [JobA, "group_job_b"],
///     order: [["group_job_b", JobA]],
///     weak_order: [
///         ["group_job_b", JobA],
///         [JobA, "group_job_b"], // multiple weak chains
///     ],
/// }
///
/// let group = MyGroup::group();
/// assert_eq!(group.name, "my_group");
///
/// // The jobs array is prefixed with internal begin/end markers:
/// assert_eq!(group.jobs[0].name(), "zlim_core::GroupBegin");
/// assert_eq!(group.jobs[1].name(), "zlim_core::GroupEnd");
///
/// // `register()` resolves and registers the type-based jobs first, then
/// // the group itself:
/// MyGroup::register();
/// assert!(JobGroup::get("my_group").is_some());
/// ```
///
/// [`job_group!`]: zlim_core_derive::job_group!
#[derive(Debug, Clone, Copy)]
pub struct JobGroup {
    /// The group's registered name.
    pub name: &'static str,
    /// Job ids in this group. Index `0` is the group-begin marker and index
    /// `1` is the group-end marker; real jobs start at index `2`.
    pub jobs: &'static [JobId],
    /// Index into [`Self::jobs`] of the group's run-condition job, if any.
    pub condition: Option<u16>,
    /// Strong (blocking) ordering edges as `(before, after)` index pairs
    /// into [`Self::jobs`].
    pub order: &'static [(u16, u16)],
    /// Weak (hint) ordering edges as `(before, after)` index pairs into
    /// [`Self::jobs`].
    pub weak_order: &'static [(u16, u16)],
    /// Relaxed (hint) ordering edges as `(before, after)` index pairs into
    /// [`Self::jobs`].
    pub relaxed_order: &'static [(u16, u16)],
    /// Source location where the group was defined.
    pub location: DebugLocation,
}

impl JobGroup {
    /// The identifier for the anonymous `JobGroup`.
    ///
    /// If you directly insert Jobs into the Schedule, they will
    /// belong to this anonymous group. This is just a placeholder,
    /// there is actually no such group.
    pub const ANONYMOUS: &str = "#anonymous";

    /// Return the initial job executed in this JobGroup.
    ///
    /// If this job is skipped, the entire JobGroup will be skipped.
    #[inline]
    pub fn first(&self) -> JobId {
        let index = self.condition.unwrap_or(0);
        self.jobs[index as usize]
    }

    /// The last job executed in this JobGroup,
    ///
    /// If it is skipped, it means that the entire JobGroup has not been executed.
    #[inline]
    pub fn last(&self) -> JobId {
        self.jobs[1]
    }
}
// -----------------------------------------------------------------------------

/// Used to reduce the occupancy time of locks.
#[derive(PartialEq, Eq)]
struct HS {
    h: u64,
    s: &'static str,
}

impl Hash for HS {
    #[inline(always)]
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.h);
    }
}

static REGISTRY: RwLock<HashMap<HS, &'static JobGroup, NoopState>> =
    RwLock::new(HashMap::with_hasher(NoopState));

impl JobGroup {
    /// Looks up a registered group by name.
    ///
    /// Returns `None` if no group with that name has been registered. The
    /// registry is populated by [`JobGroup::collect`] (statically registered
    /// groups) or [`JobGroup::register`] (runtime registration).
    #[inline(never)]
    pub fn get(name: &str) -> Option<&'static JobGroup> {
        // SAFETY: Temporary value
        let s: &'static str = unsafe { core::mem::transmute::<&str, &str>(name) };
        let h = FixedState.hash_one(s);
        REGISTRY
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&HS { h, s })
            .copied()
    }
}

// -----------------------------------------------------------------------------

impl JobGroup {
    #[cold]
    #[inline(never)]
    fn missing(group: &str, job: &str, location: DebugLocation) {
        panic!("{location}: cannot find job `{job}` in group `{group}`");
    }

    /// Builds a job group from its name, job list, condition, and ordering
    /// chains.
    ///
    /// Every job slot is converted into a [`JobId`] combining the job name
    /// with the group name; the actual `Job` instances are constructed from
    /// the global [`JobDB`] registry when the group is later inserted into a
    /// [`Schedule`]. Ordering chains reference jobs by name — edges that
    /// name a job outside `jobs` are reported and skipped.
    ///
    /// # Panics
    ///
    /// Panics if `jobs` contains more than `i16::MAX - 2` entries.
    ///
    /// [`Schedule`]: crate::schedule::Schedule
    #[track_caller]
    #[inline(never)]
    pub fn build(
        name: &'static str,
        jobs: &[&'static str],
        condition: Option<&'static str>,
        order: &[&[&'static str]],
        weak_order: &[&[&'static str]],
        relaxed_order: &[&[&'static str]],
    ) -> Self {
        let location = DebugLocation::caller();

        let h = FixedState.hash_one(name);
        let hs = HS { h, s: name };
        // try_read: Avoiding deadlocks
        if let Ok(guard) = REGISTRY.try_read() {
            let x = guard.get(&hs).copied();
            ::core::mem::drop(guard);

            if let Some(&group) = x {
                debug_assert_eq!(group.name, name);
                // A relaxed check
                let first_id = group.jobs.get(2);
                if jobs.first().copied() == first_id.map(|job| job.name()) {
                    return group;
                }

                ::core::hint::cold_path();
                log::error! {
                    "duplicated job group name `{}`, first location is `{}`, second location is `{}`",
                    name, group.location, location,
                }
            }
        }

        if jobs.len() > (i16::MAX - 2) as usize {
            ::core::hint::cold_path();
            panic!("{location}: too many jobs, cannot exceed `i16::MAX - 2`");
        }

        // If the `condition` is not as written in `jobs`, it will be automatically added.
        let (condition, condition_name): (Option<u16>, Option<&'static str>) = {
            if let Some(x) = condition {
                match jobs.iter().position(|y| *y == x) {
                    Some(index) => (Some((index + 2) as u16), None),
                    None => (Some((jobs.len() + 2) as u16), Some(x)),
                }
            } else {
                (None, None)
            }
        };

        let order: &'static [(u16, u16)] = {
            let mut order_buf = BTreeSet::<(u16, u16)>::new();

            for &chain in order.iter().filter(|&&x| !x.is_empty()) {
                for index in 1..chain.len() {
                    let a = chain[index - 1];
                    let b = chain[index];
                    let Some(a) = jobs.iter().position(|y| *y == a).map(|z| z as u16) else {
                        Self::missing(name, a, location);
                        break;
                    };
                    let Some(b) = jobs.iter().position(|y| *y == b).map(|z| z as u16) else {
                        Self::missing(name, b, location);
                        break;
                    };
                    // `+ 2`: [0] is GroupBegin, [1] is GroupEnd
                    order_buf.insert((a + 2, b + 2));
                }
            }
            // `GroupBegin` run after `condition`
            if let Some(cond_index) = condition {
                order_buf.insert((cond_index, 0));
            }

            // `GroupEnd` run after `GroupBegin`
            // Strict order.
            order_buf.insert((0, 1));

            // All job should run after `GroupBegin`.
            for index in 0..jobs.len() {
                let i = index as u16;
                order_buf.insert((0, i + 2));
            }

            let seq: Vec<(u16, u16)> = order_buf.into_iter().collect();

            crate::utils::SlicePool::u16x2(seq.as_slice())
        };

        let weak_order: &'static [(u16, u16)] = {
            let mut weak_order_buf = BTreeSet::<(u16, u16)>::new();

            for &chain in weak_order.iter().filter(|&&x| !x.is_empty()) {
                for index in 1..chain.len() {
                    let a = chain[index - 1];
                    let b = chain[index];
                    let Some(a) = jobs.iter().position(|y| *y == a).map(|z| z as u16) else {
                        Self::missing(name, a, location);
                        break;
                    };
                    let Some(b) = jobs.iter().position(|y| *y == b).map(|z| z as u16) else {
                        Self::missing(name, b, location);
                        break;
                    };
                    // `+ 2`: [0] is GroupBegin, [1] is GroupEnd
                    weak_order_buf.insert((a + 2, b + 2));
                }
            }
            let weak_seq: Vec<(u16, u16)> = weak_order_buf.into_iter().collect();

            crate::utils::SlicePool::u16x2(weak_seq.as_slice())
        };

        let relaxed_order: &'static [(u16, u16)] = {
            let mut relaxed_order_buf = BTreeSet::<(u16, u16)>::new();

            for &chain in relaxed_order.iter().filter(|&&x| !x.is_empty()) {
                for index in 1..chain.len() {
                    let a = chain[index - 1];
                    let b = chain[index];
                    let Some(a) = jobs.iter().position(|y| *y == a).map(|z| z as u16) else {
                        Self::missing(name, a, location);
                        break;
                    };
                    let Some(b) = jobs.iter().position(|y| *y == b).map(|z| z as u16) else {
                        Self::missing(name, b, location);
                        break;
                    };
                    // `+ 2`: [0] is GroupBegin, [1] is GroupEnd
                    relaxed_order_buf.insert((a + 2, b + 2));
                }
            }
            // All job should run before `GroupEnd`.
            // Relaxed ordered as it is independent of control flow.
            for index in 0..jobs.len() {
                let i = index as u16;
                relaxed_order_buf.insert((i + 2, 1));
            }
            let relaxed_seq: Vec<(u16, u16)> = relaxed_order_buf.into_iter().collect();

            crate::utils::SlicePool::u16x2(relaxed_seq.as_slice())
        };

        let jobs: &'static [JobId] = {
            let mut buf: Vec<JobId> = Vec::with_capacity(2 + jobs.len());
            buf.push(JobId::new(GroupBegin::name(), name));
            buf.push(JobId::new(GroupEnd::name(), name));
            jobs.iter().for_each(|&n| buf.push(JobId::new(n, name)));
            if let Some(condition_name) = condition_name {
                buf.push(JobId::new(condition_name, name));
                debug_assert_eq!(condition, Some((buf.len() as u16) - 1));
            }
            crate::utils::SlicePool::job_id(buf.as_slice())
        };

        debug_assert_eq!(jobs[0].name(), GroupBegin::name());
        debug_assert_eq!(jobs[1].name(), GroupEnd::name());

        Self {
            name,
            jobs,
            condition,
            order,
            weak_order,
            relaxed_order,
            location,
        }
    }
}

impl JobGroup {
    /// Registers a group into the global group registry.
    ///
    /// If a group with the same name is already registered, this is a no-op
    /// when the job lists match; otherwise an error is logged and the new
    /// group replaces the old one.
    #[inline(never)]
    pub fn register(group: JobGroup) {
        let name: &'static str = group.name;
        let h = FixedState.hash_one(name);
        let hs = HS { h, s: name };

        let mut registry = REGISTRY.write().unwrap_or_else(PoisonError::into_inner);

        if let Some(&x) = registry.get(&hs) {
            ::core::hint::cold_path();
            if x.jobs == group.jobs {
                return;
            }
            ::core::hint::cold_path();

            log::error! {
                "duplicated job group name `{}`, first location is `{}`, second location is `{}`",
                group.name, x.location, group.location,
            }
        }

        registry.insert(hs, Global::alloc_value(group));
    }
}

// -----------------------------------------------------------------------------

/// A type-level label that can register a job group.
///
/// Implement this (or use the `job_group!` macro) to make a type usable with
/// [`Schedule::insert_group`] and [`register_job_group!`].
///
/// # Example
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_core::job::{JobGroup, JobGroupLabel};
///
/// #[job_fn(type = JobA, name = "group_job_a")]
/// fn job_a() {}
///
/// // `job_group!` implements `JobGroupLabel` for the marker type:
/// job_group! {
///     type: MyGroup,
///     name: "my_group",
///     jobs: [JobA],
/// }
///
/// assert_eq!(MyGroup::name(), "my_group");
///
/// // `register()` registers the group's type-based jobs first, then the
/// // group itself:
/// MyGroup::register();
/// assert!(JobGroup::get("my_group").is_some());
/// ```
///
/// [`Schedule::insert_group`]: crate::schedule::Schedule::insert_group
/// [`register_job_group!`]: crate::register_job_group!
pub trait JobGroupLabel {
    /// The group's registered name.
    fn name() -> &'static str;

    /// The group metadata used to construct and register this group.
    fn group() -> JobGroup;

    /// Registers this group if it is not already present.
    fn register() {
        let name = Self::name();

        if JobGroup::get(name).is_some() {
            return;
        }

        core::hint::cold_path();

        JobGroup::register(Self::group());
    }
}

// -----------------------------------------------------------------------------

/// A CTOR-registry handle used to eagerly register job groups.
#[repr(transparent)]
pub struct __JobGroupReg__(fn());

zlim_reg::collect!(__JobGroupReg__);

impl __JobGroupReg__ {
    /// Creates a registration handle for a [`JobGroupLabel`].
    pub const fn of<T: JobGroupLabel>() -> Self {
        Self(T::register)
    }
}

/// Registers one or more [`JobGroupLabel`] types in the CTOR registry.
///
/// The types are registered eagerly, before `main`, and become visible in
/// [`JobGroup::get`] once [`JobGroup::collect`] has run at startup.
///
/// ```no_run
/// use zlim_core::prelude::*;
/// use zlim_core::job::JobGroup;
/// use zlim_core::register_job_group;
///
/// #[job_fn(type = JobA, name = "group_job_a")]
/// fn job_a() {}
///
/// job_group! {
///     type: MyGroup,
///     name: "my_group",
///     jobs: [JobA],
/// }
///
/// register_job_group!(MyGroup);
///
/// JobGroup::collect();
/// assert!(JobGroup::get("my_group").is_some());
/// ```
#[macro_export]
macro_rules! register_job_group {
    ($($ty:ty),* $(,)?) => {
        const _: () = {
            $(
                $crate::__macro_exports__::__submit!(
                    $crate::job::__JobGroupReg__::of::<$ty>()
                    => $crate::job::__JobGroupReg__
                );
            )*
        };
    };
}

impl JobGroup {
    /// Collects all statically-registered job groups into the global registry.
    ///
    /// This is idempotent and is typically called once at startup.
    pub fn collect() {
        #[cold]
        #[inline(never)]
        fn collect_internal() {
            let start = zlim_os::time::Instant::now();
            log::debug!("Collecting JobGroup registrations...");

            zlim_task::cfg::single_thread! {
                zlim_reg::iter::<__JobGroupReg__>().for_each(|f|(f.0)());
            }

            zlim_task::cfg::multi_thread! {
                zlim_task::MainTaskPool::get().scope(|s| {
                    zlim_reg::iter::<__JobGroupReg__>().for_each(|f| s.spawn(async { (f.0)() }));
                });
            }

            log::debug!("JobGroup collection finished in {:?}", start.elapsed());
        }

        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(collect_internal);
    }
}
