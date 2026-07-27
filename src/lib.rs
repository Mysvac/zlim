//! Public facade of the zlim engine. Re-exports [`zlim_internal`] and
//! enables the `dylib` feature for faster dev iteration.

cfg_select! {
    target_family = "wasm" => {
        pub use zlim_internal::*;
    }
    feature = "dylib" => {
        pub use zlim_dylib::*;
    }
    _ => {
        pub use zlim_internal::*;
    }
}
