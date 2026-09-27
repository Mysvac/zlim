use core::any::TypeId;
use core::fmt::{Debug, Formatter};
use core::marker::PhantomData;

use crate::Reflect;
use crate::db::{TypeDB, TypeDatabase};
use crate::impls::impl_simple_type_path;
use crate::info::{GenericInfo, Generics, InfoCell, OpaqueInfo};
use crate::info::{TypeInfo, TypeParamInfo, Typed};
use crate::ops::{ApplyError, CloneError, Opaque, ReflectRef};
use crate::path::TypePath;

impl_simple_type_path!(@PhantomData<T>: "core", "marker", "PhantomData");

impl<T: TypePath + Send + Sync> Typed for PhantomData<T> {
    fn type_info() -> &'static TypeInfo {
        static CELL: InfoCell = InfoCell::new();
        CELL.get_or_init::<Self>(|| {
            TypeInfo::Opaque(OpaqueInfo::new::<Self>().with_generics(Generics::new(&[
                GenericInfo::Type(TypeParamInfo::new::<T>("T")),
            ])))
        })
    }
}

impl<T: TypePath + Send + Sync> Opaque for PhantomData<T> {
    fn apply_str(&mut self, v: &str) -> Result<(), String> {
        if v == "PhantomData" {
            Ok(())
        } else {
            Err(String::from("expect \"PhantomData\""))
        }
    }

    fn stringify(&self) -> String {
        String::from("PhantomData")
    }
}

impl<T: TypePath + Send + Sync> Reflect for PhantomData<T> {
    crate::impls::impl_reflect_kind!(Opaque);

    #[inline]
    fn reflect_clone(&self) -> Result<Box<dyn Reflect>, CloneError> {
        Ok(Box::new(Self))
    }

    #[inline]
    fn reflect_debug(&self, f: &mut Formatter) -> core::fmt::Result {
        Debug::fmt(self, f)
    }

    #[inline]
    fn reflect_eq(&self, other: &dyn Reflect) -> bool {
        other.type_id() == TypeId::of::<Self>()
    }

    #[inline]
    fn reflect_hash(&self) -> u64 {
        0
    }

    fn reflect_apply(&mut self, value: &dyn Reflect) -> Result<(), ApplyError> {
        #[inline(never)] // Ensure single compilation
        fn internal(this: &mut dyn Opaque, other: &dyn Reflect) -> Result<(), ApplyError> {
            let other_type = other.type_id();
            let this_type = this.type_id();
            if other_type == this_type {
                return Ok(());
            }

            let other: &dyn Opaque = other.reflect_ref().as_opaque().map_err(|e| {
                ::core::hint::cold_path();
                let src = this.reflect_type_path();
                let apply = other.reflect_type_path();
                ApplyError::mismatched_kind(src, apply, e.expected, e.received)
            })?;

            if other.reflect_type_ident() == "PhantomData" {
                return Ok(());
            }

            this.apply_str(&other.stringify()).map_err(|error| {
                ::core::hint::cold_path();
                let src = this.reflect_type_path();
                let apply = other.reflect_type_path();
                ApplyError { src, apply, error }
            })
        }

        internal(self, value)
    }

    fn from_reflect(value: Box<dyn Reflect>) -> Result<Box<Self>, Box<dyn Reflect>> {
        #[inline(never)] // Ensure single compilation
        fn internal(id: TypeId, value: Box<dyn Reflect>) -> Result<(), Box<dyn Reflect>> {
            let other_type = value.type_id();

            if other_type == id {
                return Ok(());
            }

            let value = value;

            let ReflectRef::Opaque(v) = value.reflect_ref() else {
                return Err(value);
            };

            if v.reflect_type_ident() == "PhantomData" {
                return Ok(());
            }

            if v.stringify() == "PhantomData" {
                return Ok(());
            }

            Err(value)
        }

        internal(TypeId::of::<Self>(), value).map(|_| Box::new(Self))
    }
}

impl<T: TypePath + Send + Sync> TypeDatabase for PhantomData<T> {
    fn on_register(db: &'static TypeDB) {
        db.insert_defaultor::<Self>();
        db.insert_serializer::<Self>();
        db.insert_deserializer::<Self>();
    }

    fn register_dependencies() {}
}
