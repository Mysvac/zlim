use core::any::TypeId;
use core::fmt::{Debug, Formatter};

use crate::db::{TypeDB, TypeDatabase};
use crate::impls::CONVERT_TYPE_ERROR;
use crate::info::{OpaqueInfo, ReflectKind, TypeInfo, Typed};
use crate::ops::{ApplyError, CloneError, Opaque};
use crate::{Reflect, TypePath};

impl TypePath for String {
    #[inline]
    fn type_path() -> &'static str {
        "alloc::string::String"
    }

    #[inline]
    fn type_name() -> &'static str {
        "String"
    }

    const IDENT: &str = "String";
    const CRATE: Option<&str> = Some("alloc");
    const MODULE: Option<&str> = Some("alloc::string");
}

impl Typed for String {
    #[inline]
    fn type_info() -> &'static TypeInfo {
        static INFO: TypeInfo = TypeInfo::Opaque(OpaqueInfo::new::<String>());
        &INFO
    }
}

impl Opaque for String {
    fn apply_str(&mut self, v: &str) -> Result<(), String> {
        self.clear();
        self.push_str(v);
        Ok(())
    }

    fn stringify(&self) -> String {
        self.clone()
    }
}

impl Reflect for String {
    crate::impls::impl_reflect_kind!(Opaque);

    fn reflect_clone(&self) -> Result<Box<dyn Reflect>, CloneError> {
        Ok(Box::new(self.clone()))
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

    fn reflect_apply(&mut self, value: &dyn Reflect) -> Result<(), ApplyError> {
        crate::impls::opaque_apply(self, value)
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

        Ok(Box::new(value.stringify()))
    }
}

impl TypeDatabase for String {
    fn on_register(db: &'static TypeDB) {
        db.insert_defaultor(Self::default);
        db.insert_serializer::<Self>();
        db.insert_deserializer::<Self>();
        // All Opaque can be directly converted to
        // String without a conversion function.
    }

    fn register_dependencies() {}
}

crate::register_reflect!(String);
