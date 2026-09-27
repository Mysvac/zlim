#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]

mod diagnostic;
pub use diagnostic::SystemInfoDiagnosticsPlugin;

mod info;
pub use info::{SystemInfo, SystemInfoPlugin};

// The sysinfo plugins.
pub mod plugins {
    #[doc(no_inline)]
    pub use crate::SystemInfoDiagnosticsPlugin;
    #[doc(no_inline)]
    pub use crate::SystemInfoPlugin;
}
