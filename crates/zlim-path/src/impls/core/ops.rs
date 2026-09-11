use super::impl_simple_type_path;
use crate::path::{PathCell, TypePath, concat};
use core::ops::*;

impl_simple_type_path!(RangeFull:            "core", "ops", "RangeFull");

impl_simple_type_path!(@Bound<T>:            "core", "ops", "Bound");
impl_simple_type_path!(@RangeInclusive<T>:   "core", "ops", "RangeInclusive");
impl_simple_type_path!(@RangeToInclusive<T>: "core", "ops", "RangeToInclusive");
impl_simple_type_path!(@Range<T>:            "core", "ops", "Range");
impl_simple_type_path!(@RangeFrom<T>:        "core", "ops", "RangeFrom");
impl_simple_type_path!(@RangeTo<T>:          "core", "ops", "RangeTo");
