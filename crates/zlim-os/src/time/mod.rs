//! Re-exports `std::time` or `web_time`.

crate::cfg::switch! {
    #[cfg(target_family = "wasm")] => {
        pub use web_time::*;
    }
    _ => {
        pub use std::time::*;
    }
}
