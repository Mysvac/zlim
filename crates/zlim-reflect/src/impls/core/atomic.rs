use core::any::TypeId;
use core::fmt::{Debug, Formatter};
use core::hash::BuildHasher;
use core::sync::atomic::*;

use zlim_utils::hash::FixedState;

use crate::Reflect;
use crate::db::{TypeDB, TypeDatabase};
use crate::impls::CONVERT_TYPE_ERROR;
use crate::info::{OpaqueInfo, TypeInfo, Typed};
use crate::ops::{ApplyError, CloneError, Opaque};
use crate::path::TypePath;

zlim_reflect_derive::impl_reflect! {
    #[type_path = "core::sync::atomic::Ordering"]
    #[reflect(Clone, Debug, Hash, Eq)]
    pub enum Ordering {
        Relaxed,
        Release,
        Acquire,
        AcqRel,
        SeqCst,
    }
}

macro_rules! impl_reflect_for_atomic {
    ($ty:ty, $subty:ty) => {
        impl TypePath for $ty {
            #[inline]
            fn type_path() -> &'static str {
                concat!("core::sync::atomic::", stringify!($ty))
            }

            #[inline]
            fn type_name() -> &'static str {
                stringify!($ty)
            }

            const IDENT: &str = stringify!($ty);
            const CRATE: Option<&str> = Some("core");
            const MODULE: Option<&str> = Some("core::sync::atomic");
        }

        impl Typed for $ty {
            #[inline]
            fn type_info() -> &'static TypeInfo {
                static INFO: TypeInfo = TypeInfo::Opaque(OpaqueInfo::new::<$ty>());
                &INFO
            }
        }

        impl Reflect for $ty {
            crate::impls::impl_reflect_kind!(Opaque);

            fn reflect_clone(&self) -> Result<Box<dyn Reflect>, CloneError> {
                Ok(Box::new(<Self>::new(self.load(Ordering::SeqCst))))
            }

            fn reflect_apply(&mut self, value: &dyn Reflect) -> Result<(), ApplyError> {
                crate::impls::opaque_apply(self, value)
            }

            fn reflect_debug(&self, f: &mut Formatter) -> core::fmt::Result {
                Debug::fmt(&self.load(Ordering::SeqCst), f)
            }

            fn reflect_eq(&self, other: &dyn Reflect) -> bool {
                if let Some(other) = other.downcast_ref::<Self>() {
                    self.load(Ordering::SeqCst) == other.load(Ordering::SeqCst)
                } else {
                    false
                }
            }

            fn reflect_hash(&self) -> u64 {
                FixedState.hash_one(self.load(Ordering::SeqCst))
            }

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
        }

        impl Opaque for $ty {
            fn apply_str(&mut self, v: &str) -> Result<(), String> {
                match v.parse::<$subty>() {
                    Ok(x) => {
                        *self.get_mut() = x;
                        Ok(())
                    }
                    Err(e) => Err(e.to_string()),
                }
            }

            fn stringify(&self) -> String {
                self.load(Ordering::SeqCst).to_string()
            }
        }

        impl TypeDatabase for $ty {
            fn on_register(db: &'static TypeDB) {
                db.insert_defaultor(Self::default);
                db.insert_deserializer::<Self>();
                db.insert_serializer::<Self>();
                db.insert_convertor(Self::new);
                db.insert_convertor(<Self as Into<$ty>>::into);
            }

            fn register_dependencies() {}
        }

        crate::register!($ty);
    };
}

impl_reflect_for_atomic!(AtomicBool, bool);
impl_reflect_for_atomic!(AtomicU8, u8);
impl_reflect_for_atomic!(AtomicU16, u16);
impl_reflect_for_atomic!(AtomicU32, u32);
impl_reflect_for_atomic!(AtomicU64, u64);
impl_reflect_for_atomic!(AtomicUsize, usize);
impl_reflect_for_atomic!(AtomicI8, i8);
impl_reflect_for_atomic!(AtomicI16, i16);
impl_reflect_for_atomic!(AtomicI32, i32);
impl_reflect_for_atomic!(AtomicI64, i64);
impl_reflect_for_atomic!(AtomicIsize, isize);
