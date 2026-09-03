#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]

mod diagnostic;
pub use diagnostic::SystemInfoDiagnosticsPlugin;

mod info;
pub use info::{SystemInfo, SystemInfoPlugin};
