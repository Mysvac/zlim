use core::fmt::Debug;
use core::range::*;

use crate::impls::impl_simple_type_path;

impl_simple_type_path!(@Range<T>: "core", "range", "Range");
impl_simple_type_path!(@RangeInclusive<T>: "core", "range", "RangeInclusive");
impl_simple_type_path!(@RangeFrom<T>: "core", "range", "RangeFrom");
impl_simple_type_path!(@RangeToInclusive<T>: "core", "range", "RangeToInclusive");

zlim_reflect_derive::impl_reflect! {
    #[reflect(Debug, Clone)]
    pub struct Range<Idx: Copy + Debug>{ pub start: Idx, pub end: Idx }
}

zlim_reflect_derive::impl_reflect! {
    #[reflect(Debug, Clone)]
    pub struct RangeInclusive<Idx: Copy + Debug>{ pub start: Idx, pub last: Idx }
}

zlim_reflect_derive::impl_reflect! {
    #[reflect(Debug, Clone)]
    pub struct RangeFrom<Idx: Copy + Debug>{ pub start: Idx }
}

zlim_reflect_derive::impl_reflect! {
    #[reflect(Debug, Clone)]
    pub struct RangeToInclusive<Idx: Copy + Debug>{ pub last: Idx }
}
