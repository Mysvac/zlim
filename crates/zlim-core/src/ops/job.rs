use crate::error::ZlimError;
use crate::job::JobDB;
use crate::world::World;

impl World {
    /// Run a given job.
    ///
    /// Return `Err` if the job is not registered or run failed.
    ///
    /// Return a `Err(SystemError::None)` if a condition job return `false`.
    ///
    /// Please note that this function is only applicable to non repetitive
    /// single executions, as it does not cache the data used by the Job to run.
    ///
    /// If repeated execution is required, insert the Job into a suitable
    /// Schedule so that it can reuse the cache and maximize parallel execution.
    pub fn run_job(&mut self, name: &str) -> Result<(), ZlimError> {
        match JobDB::get(name) {
            Some(db) => {
                let mut job = (db.ctor)("None");
                job.run(self).map_err(ZlimError::from)
            }
            None => {
                ::core::hint::cold_path();
                Err(ZlimError::error(format!(
                    "Try run a unregistered job `{name}`."
                )))
            }
        }
    }
}
