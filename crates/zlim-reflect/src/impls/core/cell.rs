//! [`TypePath`] for the interior-mutability cells of `core::cell`.
//!
//! The cells are not reflected: a value behind one is only reachable through a borrow, which
//! reflection has no way to hand out, so what is implemented here is the name alone.
//!
//! [`TypePath`]: crate::path::TypePath

use crate::impls::impl_simple_type_path;
use core::cell::{Cell, RefCell};

impl_simple_type_path!(@Cell<T>:    "core", "cell", "Cell");
impl_simple_type_path!(@RefCell<T>: "core", "cell", "RefCell");
