use core::any::TypeId;
use core::fmt::{Debug, Formatter};
use core::marker::PhantomData;

use crate::Reflect;
use crate::db::{TypeDB, TypeDatabase};
use crate::info::{GenericInfo, Generics, InfoCell, OpaqueInfo};
use crate::info::{ReflectKind, TypeInfo, TypeParamInfo, Typed};
use crate::ops::{ApplyError, CloneError, Opaque};
use crate::path::{PathCell, TypePath, concat};

impl<T: TypePath> TypePath for PhantomData<T> {
    fn type_path() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| {
            concat(&["core::marker::PhantomData", "<", T::type_path(), ">"])
        })
    }

    fn type_name() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| concat(&["PhantomData", "<", T::type_name(), ">"]))
    }

    const IDENT: &str = "PhantomData";
    const CRATE: Option<&str> = Some("core");
    const MODULE: Option<&str> = Some("core::marker");
}

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

            if let Some(db) = TypeDB::get_by_type(other_type)
                && db.contains_convertor(this_type)
                && let Ok(cloned) = other.reflect_clone()
                && let Ok(_) = db.convert(cloned, this_type)
            {
                return Ok(());
            }

            // Phase 3: cast `other` to `&dyn Opaque`.
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

            let mut value = value;

            if let Some(db) = TypeDB::get_by_type(other_type) {
                match db.convert(value, id) {
                    Ok(_) => return Ok(()),
                    Err(e) => value = e,
                }
            }

            if value.reflect_kind() != ReflectKind::Opaque {
                return Err(value);
            }

            if value.reflect_type_ident() == "PhantomData" {
                return Ok(());
            }

            let v = value.reflect_owned().into_opaque().unwrap();

            if v.stringify() == "PhantomData" {
                return Ok(());
            }

            Err(v)
        }

        internal(TypeId::of::<Self>(), value).map(|_| Box::new(Self))
    }
}

impl<T: TypePath + Send + Sync> TypeDatabase for PhantomData<T> {
    fn on_register(db: &'static TypeDB) {
        db.insert_defaultor(Self::default);
        db.insert_serializer::<Self>();
        db.insert_deserializer::<Self>();
    }

    fn register_dependencies() {}
}
