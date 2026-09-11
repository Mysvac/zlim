use super::impl_simple_type_path;
use crate::path::{PathCell, TypePath, concat};
use core::range::*;

impl_simple_type_path!(@RangeIter<T>:          "core", "range", "RangeIter");
impl_simple_type_path!(@RangeInclusiveIter<T>: "core", "range", "RangeInclusiveIter");
impl_simple_type_path!(@RangeFromIter<T>:      "core", "range", "RangeFromIter");

impl_simple_type_path!(@Range<T>:            "core", "range", "Range");
impl_simple_type_path!(@RangeInclusive<T>:   "core", "range", "RangeInclusive");
impl_simple_type_path!(@RangeFrom<T>:        "core", "range", "RangeFrom");
impl_simple_type_path!(@RangeToInclusive<T>: "core", "range", "RangeToInclusive");
