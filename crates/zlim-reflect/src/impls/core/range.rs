use core::fmt::Debug;
use core::range::*;

zlim_reflect_derive::impl_reflect! {
    #[type_path = "core::range::Range"]
    #[reflect(Debug, Clone)]
    pub struct Range<Idx: Copy + Debug>{ pub start: Idx, pub end: Idx }
}

zlim_reflect_derive::impl_reflect! {
    #[type_path = "core::range::RangeInclusive"]
    #[reflect(Debug, Clone)]
    pub struct RangeInclusive<Idx: Copy + Debug>{ pub start: Idx, pub last: Idx }
}

zlim_reflect_derive::impl_reflect! {
    #[type_path = "core::range::RangeFrom"]
    #[reflect(Debug, Clone)]
    pub struct RangeFrom<Idx: Copy + Debug>{ pub start: Idx }
}

zlim_reflect_derive::impl_reflect! {
    #[type_path = "core::range::RangeToInclusive"]
    #[reflect(Debug, Clone)]
    pub struct RangeToInclusive<Idx: Copy + Debug>{ pub last: Idx }
}
