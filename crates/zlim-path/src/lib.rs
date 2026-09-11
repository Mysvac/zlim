#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, expect(internal_features, reason = "needed for fake_variadic"))]
#![cfg_attr(docsrs, feature(doc_cfg, rustdoc_internals))]

mod impls;
mod path;

pub use path::PathCell;
pub use path::TypePath;
pub use path::concat;
pub use zlim_path_derive::TypePath;

pub use zlim_path_derive as derive;
