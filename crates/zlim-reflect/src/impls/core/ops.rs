use core::fmt::Debug;
use core::ops::*;

use crate::impls::impl_simple_type_path;

impl_simple_type_path!(RangeFull: "core", "ops", "RangeFull");
impl_simple_type_path!(@Range<T>: "core", "ops", "Range");
impl_simple_type_path!(@RangeFrom<T>: "core", "ops", "RangeFrom");
impl_simple_type_path!(@RangeTo<T>: "core", "ops", "RangeTo");
impl_simple_type_path!(@RangeInclusive<T>: "core", "ops", "RangeInclusive");
impl_simple_type_path!(@RangeToInclusive<T>: "core", "ops", "RangeToInclusive");
impl_simple_type_path!(@Bound<T>: "core", "ops", "Bound");

zlim_reflect_derive::impl_reflect! {
    #[reflect(Default, Debug, Clone, Hash, Eq)]
    pub struct RangeFull;
}

zlim_reflect_derive::impl_reflect! {
    #[reflect(Debug, Clone)]
    pub struct Range<Idx: Copy + Debug>{ pub start: Idx, pub end: Idx }
}

zlim_reflect_derive::impl_reflect! {
    #[reflect(Debug, Clone)]
    pub struct RangeFrom<Idx: Copy + Debug>{ pub start: Idx }
}

zlim_reflect_derive::impl_reflect! {
    #[reflect(Debug, Clone)]
    pub struct RangeTo<Idx: Copy + Debug>{ pub end: Idx }
}

// zlim_reflect_derive::impl_reflect! {
//     #[reflect(Debug, Clone)]
//     pub struct RangeInclusive<Idx: Copy + Debug>{ .. }
// }
zlim_reflect_derive::impl_reflect! {
    #[reflect(Debug, Clone)]
    pub struct RangeToInclusive<Idx: Copy + Debug>{ pub end: Idx }
}

zlim_reflect_derive::impl_reflect! {
    #[reflect(Debug, Clone)]
    pub enum Bound<T: Clone + Debug>{
        Included(T),
        Excluded(T),
        Unbounded,
    }
}
