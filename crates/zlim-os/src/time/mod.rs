//! Re-exports `std::time` or `web_time`.

pub use core::time::Duration;
pub use core::time::TryFromFloatSecsError;
pub use impls::*;

crate::cfg::switch! {
    #[cfg(target_family = "wasm")] => {
        mod impls {
            pub use web_time::Instant;
            pub use web_time::SystemTime;
            pub use web_time::SystemTimeError;
            pub use web_time::UNIX_EPOCH;
        }
    }
    _ => {
        mod impls {
            pub use std::time::Instant;
            pub use std::time::SystemTime;
            pub use std::time::SystemTimeError;
            pub use std::time::UNIX_EPOCH;
        }
    }
}
