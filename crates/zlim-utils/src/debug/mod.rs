//! Debug utilities that are conditionally compiled for zero-overhead in release.

mod debug_location;
mod debug_name;

pub use debug_location::DebugLocation;
pub use debug_name::DebugName;
