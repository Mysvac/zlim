#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, expect(internal_features, reason = "needed for fake_variadic"))]
#![cfg_attr(docsrs, feature(doc_cfg, rustdoc_internals))]

mod impls;
mod path;

pub use path::PathCell;
pub use path::TypePath;
pub use path::concat;
pub use zlim_path_derive::TypePath;

pub use zlim_path_derive as derive;

/// The zlim-path prelude.
pub mod prelude {
    // doc(hidden): keeps this path out of autocomplete suggestions.
    #[doc(hidden)]
    pub use crate::path::TypePath;
    #[doc(hidden)]
    pub use zlim_path_derive::TypePath;
}
