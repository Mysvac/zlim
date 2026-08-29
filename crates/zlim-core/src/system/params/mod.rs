//! Built-in [`SystemParam`](crate::system::SystemParam) implementations
//! re-exported for ergonomic system signatures.
//!
//! This module re-exports the parameters that live in this directory:
//! [`Local`], [`ExclusiveMarker`], [`NonSendMarker`], and [`SystemTick`].
//! Resource and query parameters (`Res`, `ResMut`, `Query`, `Single`, ...)
//! are defined next to their types in `crate::borrow` and `crate::query`.

mod entity;
mod ifopt;
mod local;
mod marker;
mod resource;
mod tick;
mod tuple;
mod world;

pub use entity::HierarchyQuery;
pub use ifopt::If;
pub use local::Local;
pub use marker::{ExclusiveMarker, NonSendMarker};
pub use tick::SystemTick;
