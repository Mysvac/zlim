// -----------------------------------------------------------------------------
// LocalExecutor & MainExecutor

mod common;

use common::{LocalExecutor, MainExecutor};

// -----------------------------------------------------------------------------
// TaskPool Implementaions

pub use impls::{AsyncTaskPool, IoTaskPool, MainTaskPool};
pub use impls::{Scope, TaskPool, TaskPoolBuilder, block_on, run_local};

zlim_cfg::switch! {
    #[cfg(feature = "single_thread")] => {
        pub use zlim_cfg::disabled as multi_thread;
        pub use zlim_cfg::enabled as single_thread;
        mod single;
        use single as impls;
    }
    zlim_os::cfg::wasm => {
        pub use zlim_cfg::disabled as multi_thread;
        pub use zlim_cfg::enabled as single_thread;
        mod wasm;
        use wasm as impls;
    }
    zlim_os::cfg::android => {
        pub use zlim_cfg::enabled as multi_thread;
        pub use zlim_cfg::disabled as single_thread;
        mod multi;
        use multi as impls;
    }
    zlim_os::cfg::windows => {
        pub use zlim_cfg::enabled as multi_thread;
        pub use zlim_cfg::disabled as single_thread;
        mod multi;
        use multi as impls;
    }
    zlim_os::cfg::linux => {
        pub use zlim_cfg::enabled as multi_thread;
        pub use zlim_cfg::disabled as single_thread;
        mod multi;
        use multi as impls;
    }
    zlim_os::cfg::macos => {
        pub use zlim_cfg::enabled as multi_thread;
        pub use zlim_cfg::disabled as single_thread;
        mod multi;
        use multi as impls;
    }
    _ => {
        pub use zlim_cfg::disabled as multi_thread;
        pub use zlim_cfg::enabled as single_thread;
        mod single;
        use single as impls;
    }
}
