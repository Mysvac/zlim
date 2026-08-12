use core::any::TypeId;
use core::fmt::{Debug, Formatter};
use std::borrow::Cow;

use crate::db::{TypeDB, TypeDatabase};
use crate::impls::CONVERT_TYPE_ERROR;
use crate::info::{OpaqueInfo, ReflectKind, TypeInfo, Typed};
use crate::ops::{ApplyError, CloneError, Opaque};
use crate::path::{PathCell, concat};
use crate::{Reflect, TypePath};

impl<T: TypePath + ToOwned + ?Sized> TypePath for Cow<'static, T> {
    fn type_path() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| concat(&["alloc::borrow::Cow", "<", T::type_path(), ">"]))
    }

    fn type_name() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| concat(&["Cow", "<", T::type_name(), ">"]))
    }

    const IDENT: &str = "Cow";
    const CRATE: Option<&str> = Some("alloc");
    const MODULE: Option<&str> = Some("alloc::borrow");
}

impl Typed for Cow<'static, str> {
    fn type_info() -> &'static TypeInfo {
        static INFO: TypeInfo = TypeInfo::Opaque(OpaqueInfo::new::<Cow<'static, str>>());
        &INFO
    }
}

impl Opaque for Cow<'static, str> {
    fn apply_str(&mut self, v: &str) -> Result<(), String> {
        *self = Cow::Owned(String::from(v));
        Ok(())
    }

    fn stringify(&self) -> String {
        self.clone().into()
    }
}

impl Reflect for Cow<'static, str> {
    crate::impls::impl_reflect_kind!(Opaque);

    fn reflect_clone(&self) -> Result<Box<dyn Reflect>, CloneError> {
        Ok(Box::new(self.clone()))
    }

    fn reflect_apply(&mut self, value: &dyn Reflect) -> Result<(), ApplyError> {
        crate::impls::opaque_apply(self, value)
    }

    fn reflect_eq(&self, other: &dyn Reflect) -> bool {
        if let Some(o) = other.downcast_ref::<Self>() {
            *self == *o
        } else {
            false
        }
    }

    fn reflect_hash(&self) -> u64 {
        use ::core::hash::BuildHasher;
        zlim_utils::hash::FixedState.hash_one(self)
    }

    fn reflect_debug(&self, f: &mut Formatter) -> core::fmt::Result {
        Debug::fmt(self, f)
    }

    fn from_reflect(value: Box<dyn Reflect>) -> Result<Box<Self>, Box<dyn Reflect>> {
        let mut value = match value.downcast::<Self>() {
            Ok(ret) => return Ok(ret),
            Err(e) => e,
        };

        if let Some(db) = TypeDB::get_by_type(value.type_id()) {
            match db.convert(value, TypeId::of::<Self>()) {
                Ok(ret) => {
                    let converted = ret.downcast::<Self>();
                    return Ok(converted.expect(CONVERT_TYPE_ERROR));
                }
                Err(v) => value = v,
            }
        }

        if value.reflect_kind() != ReflectKind::Opaque {
            return Err(value);
        }

        let value = value.reflect_owned().into_opaque().unwrap();

        Ok(Box::new(Cow::Owned(value.stringify())))
    }
}

impl TypeDatabase for Cow<'static, str> {
    fn on_register(db: &'static TypeDB) {
        db.insert_defaultor(Self::default);
        db.insert_serializer::<Self>();
        db.insert_deserializer::<Self>();
    }

    fn register_dependencies() {}
}

crate::register_reflect!(Cow<'static, str>);
