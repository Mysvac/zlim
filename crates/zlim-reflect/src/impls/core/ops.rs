use core::fmt::Debug;
use core::ops::*;

zlim_reflect_derive::impl_reflect! {
    #[type_path = "core::ops::RangeFull"]
    #[reflect(Default, Debug, Clone, Hash, Eq)]
    pub struct RangeFull;
}

zlim_reflect_derive::impl_reflect! {
    #[type_path = "core::ops::Range"]
    #[reflect(Debug, Clone)]
    pub struct Range<Idx: Copy + Debug>{ pub start: Idx, pub end: Idx }
}

zlim_reflect_derive::impl_reflect! {
    #[type_path = "core::ops::RangeFrom"]
    #[reflect(Debug, Clone)]
    pub struct RangeFrom<Idx: Copy + Debug>{ pub start: Idx }
}

zlim_reflect_derive::impl_reflect! {
    #[type_path = "core::ops::RangeTo"]
    #[reflect(Debug, Clone)]
    pub struct RangeTo<Idx: Copy + Debug>{ pub end: Idx }
}

// zlim_reflect_derive::impl_reflect! {
//     #[type_path = "core::ops::RangeInclusive"]
//     #[reflect(Debug, Clone)]
//     pub struct RangeInclusive<Idx: Copy + Debug>{ .. }
// }

zlim_reflect_derive::impl_reflect! {
    #[type_path = "core::ops::RangeToInclusive"]
    #[reflect(Debug, Clone)]
    pub struct RangeToInclusive<Idx: Copy + Debug>{ pub end: Idx }
}

zlim_reflect_derive::impl_reflect! {
    #[type_path = "core::ops::RangeToInclusive"]
    #[reflect(Debug, Clone)]
    pub enum Bound<T: Clone + Debug>{
        Included(T),
        Excluded(T),
        Unbounded,
    }
}
