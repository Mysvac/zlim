use core::any::TypeId;
use core::fmt::{Debug, Formatter};
use core::hash::BuildHasher;
use std::path::{Path, PathBuf};

use zlim_utils::hash::FixedState;

use crate::Reflect;
use crate::db::{TypeDB, TypeDatabase};
use crate::impls::CONVERT_TYPE_ERROR;
use crate::info::{OpaqueInfo, TypeInfo, Typed};
use crate::ops::{ApplyError, CloneError, Opaque};
use crate::path::TypePath;

// -----------------------------------------------------------------------------
// PathBuf

zlim_reflect_derive::impl_reflect! {
    #[type_path = "std::path::PathBuf"]
    #[reflect(Opaque, Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    pub struct PathBuf;
}

impl Opaque for PathBuf {
    fn apply_str(&mut self, v: &str) -> Result<(), String> {
        *self = PathBuf::from(v);
        Ok(())
    }

    fn stringify(&self) -> String {
        self.to_string_lossy().into_owned()
    }
}

// -----------------------------------------------------------------------------
// Path TypePath

impl TypePath for Path {
    #[inline]
    fn type_path() -> &'static str {
        "std::path::Path"
    }
    #[inline]
    fn type_name() -> &'static str {
        "Path"
    }

    const IDENT: &str = "Path";
    const CRATE: Option<&str> = Some("std");
    const MODULE: Option<&str> = Some("std::path");
}

// -----------------------------------------------------------------------------
// Typed

impl Typed for &'static Path {
    #[inline]
    fn type_info() -> &'static TypeInfo {
        static INFO: TypeInfo = TypeInfo::Opaque(OpaqueInfo::new::<&'static Path>());
        &INFO
    }
}

impl Opaque for &'static Path {
    #[inline]
    fn apply_str(&mut self, _: &str) -> Result<(), String> {
        Err(String::from("`&'static Path` does not support `apply_str`"))
    }

    #[inline]
    fn stringify(&self) -> String {
        self.to_string_lossy().into_owned()
    }
}

// -----------------------------------------------------------------------------
// Reflect

impl Reflect for &'static Path {
    crate::impls::impl_reflect_kind!(Opaque);

    fn reflect_eq(&self, other: &dyn Reflect) -> bool {
        if let Some(this) = other.downcast_ref::<Self>() {
            *self == *this
        } else {
            false
        }
    }

    fn reflect_hash(&self) -> u64 {
        FixedState.hash_one(self)
    }

    fn reflect_debug(&self, f: &mut Formatter) -> core::fmt::Result {
        Debug::fmt(self, f)
    }

    fn reflect_clone(&self) -> Result<Box<dyn Reflect>, CloneError> {
        Ok(Box::new(*self))
    }

    fn reflect_apply(&mut self, value: &dyn Reflect) -> Result<(), ApplyError> {
        crate::impls::opaque_apply(self, value)
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

// -----------------------------------------------------------------------------
// TypeDatabase

impl TypeDatabase for &'static Path {
    fn on_register(db: &'static TypeDB) {
        db.insert_serializer::<Self>();
        db.insert_convertor(<Self as Into<PathBuf>>::into);
    }

    fn register_dependencies() {}
}

crate::register!(&'static Path); // Register TypeDB
