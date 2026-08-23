//! Global application initialization (startup collection).

use zlim_reflect::TypeDB;

use crate::component::ComponentDB;
use crate::job::JobDB;
use crate::job::JobGroup;
use crate::resource::ResourceDB;

#[cold]
fn init_internal() {
    let start = zlim_os::time::Instant::now();

    zlim_log::info!("Engine CoreInit Start...");

    zlim_task::cfg::single_thread! {
        TypeDB::collect();
        ResourceDB::collect();
        ComponentDB::collect();
        JobDB::collect();
        JobGroup::collect();
    }

    zlim_task::cfg::multi_thread! {
        zlim_task::MainTaskPool::get().scope(|s| {
            s.spawn(async { TypeDB::collect(); });
            s.spawn(async { ResourceDB::collect(); });
            s.spawn(async { ComponentDB::collect(); });
            s.spawn(async { JobDB::collect(); });
        });

        JobGroup::collect();
    }

    zlim_log::info!("Engine CoreInit Completed: {:?}", start.elapsed());
}

/// Runs all initialization functions exactly once.
///
/// Called by `App::run`.
#[inline]
pub fn core_init() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(init_internal);
}
