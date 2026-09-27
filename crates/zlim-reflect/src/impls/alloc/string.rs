use core::fmt::{Debug, Formatter};

use crate::Reflect;
use crate::db::{TypeDB, TypeDatabase};
use crate::impls::impl_simple_type_path;
use crate::info::{OpaqueInfo, ReflectKind, TypeInfo, Typed};
use crate::ops::{ApplyError, CloneError, Opaque};

impl_simple_type_path!(String: "alloc", "string", "String");

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
        let value = match value.downcast::<Self>() {
            Ok(ret) => return Ok(ret),
            Err(e) => e,
        };

        if value.reflect_kind() != ReflectKind::Opaque {
            return Err(value);
        }

        let value = value.reflect_owned().into_opaque().unwrap();

        Ok(Box::new(value.stringify()))
    }
}

impl TypeDatabase for String {
    fn on_register(db: &'static TypeDB) {
        db.insert_defaultor::<Self>();
        db.insert_serializer::<Self>();
        db.insert_deserializer::<Self>();
    }

    fn register_dependencies() {}
}

crate::register_reflect!(String);
