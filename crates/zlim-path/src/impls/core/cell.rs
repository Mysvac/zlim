use super::impl_simple_type_path;
use crate::path::{PathCell, TypePath, concat};
use core::cell::{Cell, RefCell};

impl_simple_type_path!(@Cell<T>:    "core", "cell", "Cell");
impl_simple_type_path!(@RefCell<T>: "core", "cell", "RefCell");
