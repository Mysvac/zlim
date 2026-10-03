//! Global application initialization (startup collection).

use zlim_reflect::TypeDB;

use crate::component::ComponentDB;
use crate::job::JobDB;
use crate::job::JobGroup;
use crate::resource::ResourceDB;

#[cold]
fn init_internal() {
    let start = zlim_os::time::Instant::now();

    #[cfg(feature = "trace")]
    let _span = zlim_log::info_span!("core init").entered();

    // Ensure the `GlobalPool` is already allocated,
    // to avoid triggering the lock on multi-threaded tasks.
    zlim_utils::mem::Global::alloc_str("core");

    // Multithreaded collection is unnecessary:
    // 1. RTTI collection relies on global static memory,
    //    so its parallelizability is low.
    // 2. Single-threaded collection ensures that related data
    //    is tightly packed in global memory, improving cache hit rates.
    // 3. Only JobGroup incurs significant overhead,
    //    but it is already multithreaded internally.
    TypeDB::collect();
    ResourceDB::collect();
    ComponentDB::collect();
    JobDB::collect();
    JobGroup::collect();

    zlim_log::debug!("Engine CoreInit finished: {:?}", start.elapsed());
}

/// Runs all initialization functions exactly once.
///
/// Called by `App::run`, after `Log` and `TaskPool`'s initialization.
#[inline]
pub fn core_init() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(init_internal);
}

#[cfg(test)]
mod tests {

    #[test]
    #[ignore = "manual trigger"]
    fn init_time() {
        zlim_log::LogConfig::default().apply();
        zlim_task::TaskPoolConfigs::default().apply();
        super::core_init();
    }
}
