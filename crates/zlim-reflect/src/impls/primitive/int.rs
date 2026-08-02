use core::any::TypeId;

use crate::Reflect;
use crate::db::{TypeDB, TypeDatabase};
use crate::impls::CONVERT_TYPE_ERROR;
use crate::info::{OpaqueInfo, TypeInfo};
use crate::ops::{ApplyError, CloneError};

// ----------------------------------------------------------------------------
// TypePath & Typed & Opaque
// ----------------------------------------------------------------------------

macro_rules! impl_opaque_typed {
    ($ty:ty) => {
        impl $crate::path::TypePath for $ty {
            #[inline]
            fn type_path() -> &'static str {
                stringify!($ty)
            }
            #[inline]
            fn type_name() -> &'static str {
                stringify!($ty)
            }
            const IDENT: &str = stringify!($ty);
            const MODULE: Option<&str> = None;
            const CRATE: Option<&str> = None;
        }

        impl $crate::info::Typed for $ty {
            #[inline]
            fn type_info() -> &'static TypeInfo {
                static INFO: TypeInfo = TypeInfo::Opaque(OpaqueInfo::new::<$ty>());
                &INFO
            }
        }

        impl $crate::ops::Opaque for $ty {
            #[inline]
            fn apply_str(&mut self, v: &str) -> Result<(), String> {
                match v.parse::<$ty>() {
                    Ok(val) => {
                        *self = val;
                        Ok(())
                    }
                    Err(e) => Err(e.to_string()),
                }
            }
            #[inline]
            fn stringify(&self) -> String {
                self.to_string()
            }
        }
    };
}

impl_opaque_typed!(bool);
impl_opaque_typed!(char);
impl_opaque_typed!(i8);
impl_opaque_typed!(i16);
impl_opaque_typed!(i32);
impl_opaque_typed!(i64);
impl_opaque_typed!(i128);
impl_opaque_typed!(isize);
impl_opaque_typed!(u8);
impl_opaque_typed!(u16);
impl_opaque_typed!(u32);
impl_opaque_typed!(u64);
impl_opaque_typed!(u128);
impl_opaque_typed!(usize);
impl_opaque_typed!(f32);
impl_opaque_typed!(f64);

// ----------------------------------------------------------------------------
// Reflect
// ----------------------------------------------------------------------------

macro_rules! impl_reflect {
    (@f) => {
        crate::impls::impl_reflect_kind!(Opaque);

        #[inline]
        fn reflect_clone(&self) -> Result<Box<dyn Reflect>, CloneError> { Ok(Box::new(*self)) }

        #[inline]
        fn reflect_apply(&mut self, value: &dyn Reflect) -> Result<(), ApplyError> { $crate::impls::opaque_apply(self, value) }

        #[inline]
        fn reflect_debug(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { ::core::fmt::Debug::fmt(self, f) }

        fn from_reflect(value: Box<dyn Reflect>) -> Result<Box<Self>, Box<dyn Reflect>> {
            let value = match value.downcast::<Self>() {
                Ok(ret) => return Ok(ret),
                Err(e) => e,
            };

            match TypeDB::get_by_type((*value).type_id()) {
                Some(db) => {
                    let converted = db.convert(value, TypeId::of::<Self>())?;
                    Ok(converted.downcast::<Self>().expect(CONVERT_TYPE_ERROR))
                }
                None => Err(value),
            }
        }
    };
    () => {
        impl_reflect!(@f);

        #[inline]
        fn reflect_eq(&self, other: &dyn Reflect) -> bool {
            if let Some(o) = other.downcast_ref::<Self>() { *self == *o } else { false }
        }

        #[inline]
        fn reflect_hash(&self) -> u64 { use ::core::hash::BuildHasher; zlim_utils::hash::FixedState.hash_one(self) }
    };
}

impl Reflect for bool {
    impl_reflect!();
}
impl Reflect for char {
    impl_reflect!();
}
impl Reflect for i8 {
    impl_reflect!();
}
impl Reflect for i16 {
    impl_reflect!();
}
impl Reflect for i32 {
    impl_reflect!();
}
impl Reflect for i64 {
    impl_reflect!();
}
impl Reflect for i128 {
    impl_reflect!();
}
impl Reflect for isize {
    impl_reflect!();
}
impl Reflect for u8 {
    impl_reflect!();
}
impl Reflect for u16 {
    impl_reflect!();
}
impl Reflect for u32 {
    impl_reflect!();
}
impl Reflect for u64 {
    impl_reflect!();
}
impl Reflect for u128 {
    impl_reflect!();
}
impl Reflect for usize {
    impl_reflect!();
}
impl Reflect for f32 {
    impl_reflect!(@f);
}
impl Reflect for f64 {
    impl_reflect!(@f);
}

// ----------------------------------------------------------------------------
// TypeDatabase + Register
// ----------------------------------------------------------------------------

macro_rules! impl_type_database {
    ($ty:ty, $($e:expr),* $(,)?) => {
        impl TypeDatabase for $ty {
            fn on_register(db: &'static TypeDB) {
                db.insert_defaultor(Self::default);
                db.insert_serializer::<Self>();
                db.insert_deserializer::<Self>();
                $( db.insert_convertor($e); )*
            }
        }

        crate::register!($ty); // Register TypeDB
    };
}

impl_type_database!(
    char,
    <Self as Into<String>>::into,
    <Self as Into<u32>>::into,
    <Self as Into<u64>>::into,
    <Self as Into<u128>>::into,
);

impl_type_database!(
    bool,
    |s: Self| s.to_string(),
    <Self as Into<u8>>::into,
    <Self as Into<i8>>::into,
    <Self as Into<u16>>::into,
    <Self as Into<i16>>::into,
    <Self as Into<u32>>::into,
    <Self as Into<i32>>::into,
    <Self as Into<u64>>::into,
    <Self as Into<i64>>::into,
    <Self as Into<u128>>::into,
    <Self as Into<i128>>::into,
    <Self as Into<usize>>::into,
    <Self as Into<isize>>::into,
    <Self as Into<f32>>::into,
    <Self as Into<f64>>::into,
);

impl_type_database!(
    i8,
    |s: Self| s.to_string(),
    |s: Self| s == 0,
    |s: Self| s as u8,
    |s: Self| s as u16,
    |s: Self| s as u32,
    |s: Self| s as u64,
    |s: Self| s as u128,
    |s: Self| s as usize,
    // |s: Self| s as i8,
    |s: Self| s as i16,
    |s: Self| s as i32,
    |s: Self| s as i64,
    |s: Self| s as isize,
    |s: Self| s as f32,
    |s: Self| s as f64,
);

impl_type_database!(
    i16,
    |s: Self| s.to_string(),
    |s: Self| s == 0,
    |s: Self| s as u8,
    |s: Self| s as u16,
    |s: Self| s as u32,
    |s: Self| s as u64,
    |s: Self| s as u128,
    |s: Self| s as usize,
    |s: Self| s as i8,
    // |s: Self| s as i16,
    |s: Self| s as i32,
    |s: Self| s as i64,
    |s: Self| s as isize,
    |s: Self| s as f32,
    |s: Self| s as f64,
);

impl_type_database!(
    i32,
    |s: Self| s.to_string(),
    |s: Self| s == 0,
    |s: Self| s as u8,
    |s: Self| s as u16,
    |s: Self| s as u32,
    |s: Self| s as u64,
    |s: Self| s as u128,
    |s: Self| s as usize,
    |s: Self| s as i8,
    |s: Self| s as i16,
    // |s: Self| s as i32,
    |s: Self| s as i64,
    |s: Self| s as isize,
    |s: Self| s as f32,
    |s: Self| s as f64,
);

impl_type_database!(
    i64,
    |s: Self| s.to_string(),
    |s: Self| s == 0,
    |s: Self| s as u8,
    |s: Self| s as u16,
    |s: Self| s as u32,
    |s: Self| s as u64,
    |s: Self| s as u128,
    |s: Self| s as usize,
    |s: Self| s as i8,
    |s: Self| s as i16,
    |s: Self| s as i32,
    // |s: Self| s as i64,
    |s: Self| s as isize,
    |s: Self| s as f32,
    |s: Self| s as f64,
);

impl_type_database!(
    i128,
    |s: Self| s.to_string(),
    |s: Self| s == 0,
    |s: Self| s as u8,
    |s: Self| s as u16,
    |s: Self| s as u32,
    |s: Self| s as u64,
    |s: Self| s as u128,
    |s: Self| s as usize,
    |s: Self| s as i8,
    |s: Self| s as i16,
    |s: Self| s as i32,
    // |s: Self| s as i64,
    |s: Self| s as isize,
    |s: Self| s as f32,
    |s: Self| s as f64,
);

impl_type_database!(
    isize,
    |s: Self| s.to_string(),
    |s: Self| s == 0,
    |s: Self| s as u8,
    |s: Self| s as u16,
    |s: Self| s as u32,
    |s: Self| s as u64,
    |s: Self| s as u128,
    |s: Self| s as usize,
    |s: Self| s as i8,
    |s: Self| s as i16,
    |s: Self| s as i32,
    |s: Self| s as i64,
    // |s: Self| s as isize,
    |s: Self| s as f32,
    |s: Self| s as f64,
);

impl_type_database!(
    u8,
    |s: Self| s.to_string(),
    |s: Self| s == 0,
    // |s: Self| s as u8,
    |s: Self| s as u16,
    |s: Self| s as u32,
    |s: Self| s as u64,
    |s: Self| s as u128,
    |s: Self| s as usize,
    |s: Self| s as i8,
    |s: Self| s as i16,
    |s: Self| s as i32,
    |s: Self| s as i64,
    |s: Self| s as isize,
    |s: Self| s as f32,
    |s: Self| s as f64,
);

impl_type_database!(
    u16,
    |s: Self| s.to_string(),
    |s: Self| s == 0,
    |s: Self| s as u8,
    // |s: Self| s as u16,
    |s: Self| s as u32,
    |s: Self| s as u64,
    |s: Self| s as u128,
    |s: Self| s as usize,
    |s: Self| s as i8,
    |s: Self| s as i16,
    |s: Self| s as i32,
    |s: Self| s as i64,
    |s: Self| s as isize,
    |s: Self| s as f32,
    |s: Self| s as f64,
);

impl_type_database!(
    u32,
    |s: Self| s.to_string(),
    |s: Self| s == 0,
    |s: Self| s as u8,
    |s: Self| s as u16,
    // |s: Self| s as u32,
    |s: Self| s as u64,
    |s: Self| s as u128,
    |s: Self| s as usize,
    |s: Self| s as i8,
    |s: Self| s as i16,
    |s: Self| s as i32,
    |s: Self| s as i64,
    |s: Self| s as isize,
    |s: Self| s as f32,
    |s: Self| s as f64,
);

impl_type_database!(
    u64,
    |s: Self| s.to_string(),
    |s: Self| s == 0,
    |s: Self| s as u8,
    |s: Self| s as u16,
    |s: Self| s as u32,
    // |s: Self| s as u64,
    |s: Self| s as u128,
    |s: Self| s as usize,
    |s: Self| s as i8,
    |s: Self| s as i16,
    |s: Self| s as i32,
    |s: Self| s as i64,
    |s: Self| s as isize,
    |s: Self| s as f32,
    |s: Self| s as f64,
);

impl_type_database!(
    u128,
    |s: Self| s.to_string(),
    |s: Self| s == 0,
    |s: Self| s as u8,
    |s: Self| s as u16,
    |s: Self| s as u32,
    |s: Self| s as u64,
    // |s: Self| s as u128,
    |s: Self| s as usize,
    |s: Self| s as i8,
    |s: Self| s as i16,
    |s: Self| s as i32,
    |s: Self| s as i64,
    |s: Self| s as isize,
    |s: Self| s as f32,
    |s: Self| s as f64,
);

impl_type_database!(
    usize,
    |s: Self| s.to_string(),
    |s: Self| s == 0,
    |s: Self| s as u8,
    |s: Self| s as u16,
    |s: Self| s as u32,
    |s: Self| s as u64,
    |s: Self| s as u128,
    // |s: Self| s as usize,
    |s: Self| s as i8,
    |s: Self| s as i16,
    |s: Self| s as i32,
    |s: Self| s as i64,
    |s: Self| s as isize,
    |s: Self| s as f32,
    |s: Self| s as f64,
);

impl_type_database!(
    f32,
    |s: Self| s.to_string(),
    |s: Self| s as u8,
    |s: Self| s as u16,
    |s: Self| s as u32,
    |s: Self| s as u64,
    |s: Self| s as u128,
    |s: Self| s as usize,
    |s: Self| s as i8,
    |s: Self| s as i16,
    |s: Self| s as i32,
    |s: Self| s as i64,
    |s: Self| s as isize,
    // |s: Self| s as f32,
    |s: Self| s as f64,
);

impl_type_database!(
    f64,
    |s: Self| s.to_string(),
    |s: Self| s as u8,
    |s: Self| s as u16,
    |s: Self| s as u32,
    |s: Self| s as u64,
    |s: Self| s as u128,
    |s: Self| s as usize,
    |s: Self| s as i8,
    |s: Self| s as i16,
    |s: Self| s as i32,
    |s: Self| s as i64,
    |s: Self| s as isize,
    |s: Self| s as f32,
    // |s: Self| s as f64,
);

// ----------------------------------------------------------------------------
// Tests
// ----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::db::TypeDB;
    use core::any::TypeId;

    #[test]
    fn registered() {
        TypeDB::collect();

        assert!(TypeDB::get_by_type(TypeId::of::<i8>()).is_some());
        assert!(TypeDB::get_by_type(TypeId::of::<i16>()).is_some());
        assert!(TypeDB::get_by_type(TypeId::of::<i32>()).is_some());
        assert!(TypeDB::get_by_type(TypeId::of::<i64>()).is_some());
        assert!(TypeDB::get_by_type(TypeId::of::<i128>()).is_some());
        assert!(TypeDB::get_by_type(TypeId::of::<isize>()).is_some());
        assert!(TypeDB::get_by_type(TypeId::of::<u8>()).is_some());
        assert!(TypeDB::get_by_type(TypeId::of::<u16>()).is_some());
        assert!(TypeDB::get_by_type(TypeId::of::<u32>()).is_some());
        assert!(TypeDB::get_by_type(TypeId::of::<u64>()).is_some());
        assert!(TypeDB::get_by_type(TypeId::of::<u128>()).is_some());
        assert!(TypeDB::get_by_type(TypeId::of::<usize>()).is_some());
        assert!(TypeDB::get_by_type(TypeId::of::<f64>()).is_some());
        assert!(TypeDB::get_by_type(TypeId::of::<f32>()).is_some());
        assert!(TypeDB::get_by_type(TypeId::of::<bool>()).is_some());
        assert!(TypeDB::get_by_type(TypeId::of::<char>()).is_some());

        assert!(TypeDB::get_by_path("i8").is_some());
        assert!(TypeDB::get_by_path("i16").is_some());
        assert!(TypeDB::get_by_path("i32").is_some());
        assert!(TypeDB::get_by_path("i64").is_some());
        assert!(TypeDB::get_by_path("i128").is_some());
        assert!(TypeDB::get_by_path("isize").is_some());
        assert!(TypeDB::get_by_path("u8").is_some());
        assert!(TypeDB::get_by_path("u16").is_some());
        assert!(TypeDB::get_by_path("u32").is_some());
        assert!(TypeDB::get_by_path("u64").is_some());
        assert!(TypeDB::get_by_path("u128").is_some());
        assert!(TypeDB::get_by_path("usize").is_some());
        assert!(TypeDB::get_by_path("f32").is_some());
        assert!(TypeDB::get_by_path("f64").is_some());
        assert!(TypeDB::get_by_path("bool").is_some());
        assert!(TypeDB::get_by_path("char").is_some());
    }
}
