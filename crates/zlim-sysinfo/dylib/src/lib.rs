//! Dynamic-library isolation layer for [`zlim-sysinfo`](https://github.com/Mysvac/zlim).
//!
//! When the engine is built with the `dylib` feature, the engine cdylib must
//! not link the [`sysinfo`] dependency itself: on Windows its object count
//! (together with the engine's) exceeds the linker's per-library limit
//! (LNK1189).  This crate exists to break that link: it compiles [`sysinfo`]
//! into its own standalone dynamic library and re-exports only the small API
//! surface [`zlim-sysinfo`](https://github.com/Mysvac/zlim) needs, which the
//! engine then imports dynamically.
//!
//! This crate deliberately depends on nothing from the zlim workspace: it is
//! a pure re-export shim over [`sysinfo`].
//!
//! [`sysinfo`]: https://crates.io/crates/sysinfo

cfg_select! {
    any(
        target_os = "linux",
        target_os = "windows",
        target_os = "android",
        target_os = "macos",
        target_os = "freebsd",
    ) => {
        pub use sysinfo::MINIMUM_CPU_UPDATE_INTERVAL;
        pub use sysinfo::ProcessesToUpdate;
        pub use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};
        pub use sysinfo::{Pid, get_current_pid};
    }
    _ => {}
}
