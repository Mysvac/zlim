//! Job registry and job labels.

use core::fmt::{Debug, Formatter};
use std::sync::{PoisonError, RwLock};

use zlim_log as log;
use zlim_utils::debug::DebugLocation;
use zlim_utils::hash::HashMap;
use zlim_utils::mem::Global;

use super::Job;

// -----------------------------------------------------------------------------

/// Static metadata describing a registered job constructor.
///
/// A `JobDB` is produced by the `#[job_fn]` / `job!` macros and stored in the
/// global job registry. It holds the job's name, the constructor used to build
/// a boxed [`Job`], and the source location where it was defined.
///
/// # Example
///
/// ```no_run
/// use zlim_core::prelude::*;
/// use zlim_core::job::JobDB;
///
/// #[job_fn(type = MyJob, name = "my_job")]
/// fn my_job() {}
///
/// // Non-generic markers are auto-registered; `collect` loads them into the
/// // global registry:
/// JobDB::collect();
/// let db = JobDB::get("my_job").expect("the job should be registered");
/// assert_eq!(db.name, "my_job");
///
/// // The constructor builds a boxed job; `group` is the group name (empty
/// // for standalone jobs):
/// let mut job = (db.ctor)("my_group");
/// assert_eq!(job.id().group(), "my_group");
/// ```
#[derive(Clone, Copy)]
pub struct JobDB {
    /// The job's registered name.
    pub name: &'static str,
    /// Constructor that builds a boxed job for the given group; `group` is
    /// the group name, empty for standalone jobs.
    pub ctor: fn(group: &'static str) -> Box<dyn Job>,
    /// Constructors for the job's run conditions (`run_if`), each taking
    /// the job's group name.
    pub run_if: &'static [fn(group: &'static str) -> Box<dyn Job>],
    /// Source location where the job was defined.
    pub location: DebugLocation,
}

impl Debug for JobDB {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("JobDB")
            .field("name", &self.name)
            .field("location", &self.location)
            .finish()
    }
}

// -----------------------------------------------------------------------------
// REGISTRY

static REGISTRY: RwLock<HashMap<&'static str, &'static JobDB>> = RwLock::new(HashMap::new());

impl JobDB {
    /// Looks up a registered job by name.
    ///
    /// Returns `None` if no job with that name has been registered. The
    /// registry is populated by [`JobDB::collect`] (statically registered
    /// jobs) or [`JobDB::register`] (runtime registration).
    #[inline(never)]
    pub fn get(name: &str) -> Option<JobDB> {
        REGISTRY
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(name)
            .map(|x| **x)
    }
}

// -----------------------------------------------------------------------------
// Static Register

impl JobDB {
    /// Collects all statically-registered jobs into the global registry.
    ///
    /// This is idempotent and is typically called once at startup.
    pub fn collect() {
        #[cold]
        #[inline(never)]
        fn collect_internal() {
            let start = zlim_os::time::Instant::now();
            log::debug!("Collecting JobDB registrations...");

            for reg in zlim_reg::iter::<__JobReg__>() {
                (reg.0)();
            }

            let len = REGISTRY
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .len();
            log::debug!("JobDB({len}) collection finished in {:?}", start.elapsed());
        }

        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(collect_internal);
    }
}

// -----------------------------------------------------------------------------
// Dynamic Register

impl JobDB {
    /// Registers a job database at runtime.
    ///
    /// If a job with the same name is already registered, a warning is logged
    /// (unless the constructor is identical, in which case this is a no-op).
    #[inline(never)]
    pub fn register(db: JobDB) {
        let name = db.name;
        let mut registry = REGISTRY.write().unwrap_or_else(PoisonError::into_inner);

        if let Some(&x) = registry.get(name) {
            if core::ptr::fn_addr_eq(x.ctor, db.ctor) {
                return;
            }
            ::core::hint::cold_path();

            log::warn! {
                "duplicated job name `{}`, first location is `{}`, second location is `{}`",
                db.name, x.location, db.location,
            }
        }

        let sdb: &'static JobDB = Global::alloc_value(db);
        registry.insert(name, sdb);
    }
}

// -----------------------------------------------------------------------------
// Label

/// A type-level label that can register a job into the [`JobDB`] registry.
///
/// Implement this (or use the `#[job_fn]` / `job!` macros) to make a type
/// usable with [`Schedule::insert`] and [`register_job!`].
///
/// # Example
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_core::job::{JobDB, JobLabel};
///
/// // `#[job_fn]` implements `JobLabel` for the marker type:
/// #[job_fn(type = MyJob, name = "my_job")]
/// fn my_job() {}
///
/// assert_eq!(MyJob::name(), "my_job");
///
/// // `register()` inserts the job into the registry if it is not present:
/// MyJob::register();
/// assert!(JobDB::get("my_job").is_some());
/// ```
///
/// [`Schedule::insert`]: crate::schedule::Schedule::insert
/// [`register_job!`]: crate::register_job!
pub trait JobLabel {
    /// The job's registered name.
    fn name() -> &'static str;

    /// The metadata used to construct and register this job.
    fn database() -> JobDB;

    /// Registers this job if it is not already present.
    fn register() {
        let name = Self::name();

        if JobDB::get(name).is_some() {
            return;
        }

        core::hint::cold_path();
        JobDB::register(Self::database());
    }
}

// -----------------------------------------------------------------------------

/// A CTOR-registry handle used to eagerly register jobs.
#[repr(transparent)]
pub struct __JobReg__(fn());

zlim_reg::collect!(__JobReg__);

impl __JobReg__ {
    /// Creates a registration handle for a [`JobLabel`].
    pub const fn of<T: JobLabel>() -> Self {
        Self(T::register)
    }
}

/// Registers one or more [`JobLabel`] types in the CTOR registry.
///
/// The types are registered eagerly, before `main`, and become visible in
/// [`JobDB::get`] once [`JobDB::collect`] has run at startup.
///
/// ```no_run
/// use zlim_core::prelude::*;
///
/// #[job_fn(type = MyJob)]
/// fn my_job() {}
///
/// register_job!(MyJob);
///
/// JobDB::collect();
/// assert!(JobDB::get(MyJob::name()).is_some());
/// ```
#[macro_export]
macro_rules! register_job {
    ($($ty:ty),* $(,)?) => {
        const _: () = {
            $(
                $crate::__macro_exports__::__submit!(
                    $crate::job::__JobReg__::of::<$ty>()
                    => $crate::job::__JobReg__
                );
            )*
        };
    };
}
