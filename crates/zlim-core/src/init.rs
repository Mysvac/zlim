//! Global application initialization (startup collection).

use zlim_reflect::TypeDB;

use crate::component::ComponentDB;
use crate::job::JobDB;
use crate::job::JobGroup;
use crate::resource::ResourceDB;

#[cold]
#[cfg_attr(target_family = "windows", unsafe(link_section = ".ZINIT"))]
#[cfg_attr(target_family = "wasm", unsafe(link_section = ".text.zliminit"))]
#[cfg_attr(target_os = "linux", unsafe(link_section = ".text.zliminit"))]
#[cfg_attr(target_os = "android", unsafe(link_section = ".text.zliminit"))]
#[cfg_attr(target_os = "macos", unsafe(link_section = "__TEXT,__zlim_init"))]
#[cfg_attr(target_os = "ios", unsafe(link_section = "__TEXT,__zlim_init"))]
fn init_internal() {
    let start = zlim_os::time::Instant::now();

    #[cfg(feature = "trace")]
    let _span = zlim_log::info_span!("core init").entered();

    zlim_log::debug!("Engine CoreInit Start...");

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

    zlim_log::debug!("Engine CoreInit Completed: {:?}", start.elapsed());
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
    #[ignore = "manually trigger"]
    fn init_time() {
        zlim_log::LogConfig::default().apply();
        zlim_task::TaskPoolConfigs::default().apply();
        super::core_init();
    }
}
