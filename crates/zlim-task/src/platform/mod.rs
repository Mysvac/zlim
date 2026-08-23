// -----------------------------------------------------------------------------
// LocalExecutor & MainExecutor

mod common;

use common::{LocalExecutor, MainExecutor};

// -----------------------------------------------------------------------------
// TaskPool Implementaions

pub use impls::{AsyncTaskPool, IoTaskPool, MainTaskPool};
pub use impls::{Scope, TaskPool, TaskPoolBuilder};
pub use impls::{block_on, block_on_main, run_local, set_main_thread};

cfg_select! {
    feature = "single_thread" => {
        pub use zlim_cfg::disabled as multi_thread;
        pub use zlim_cfg::enabled as single_thread;
        mod single;
        use single as impls;
    },
    target_family = "wasm" => {
        pub use zlim_cfg::disabled as multi_thread;
        pub use zlim_cfg::enabled as single_thread;
        mod wasm;
        use wasm as impls;
    }
    _ => {
        pub use zlim_cfg::enabled as multi_thread;
        pub use zlim_cfg::disabled as single_thread;
        mod multi;
        use multi as impls;
    }
}
