use core::fmt::{Debug, Formatter};
use core::hash::BuildHasher;
use std::path::{Path, PathBuf};

use zlim_utils::hash::FixedState;

use crate::Reflect;
use crate::db::{TypeDB, TypeDatabase};
use crate::impls::impl_simple_type_path;
use crate::info::{OpaqueInfo, TypeInfo, Typed};
use crate::ops::{ApplyError, CloneError, Opaque};

// -----------------------------------------------------------------------------
// PathBuf

impl_simple_type_path!(PathBuf: "std", "path", "PathBuf");

zlim_reflect_derive::impl_reflect! {
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

impl_simple_type_path!(Path: "std", "path", "Path");

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
        other.downcast_ref::<Self>().is_some_and(|p| *self == *p)
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
        value.downcast::<Self>()
    }
}

// -----------------------------------------------------------------------------
// TypeDatabase

impl TypeDatabase for &'static Path {
    fn on_register(db: &'static TypeDB) {
        db.insert_serializer::<Self>();
    }

    fn register_dependencies() {}
}

crate::register_reflect!(&'static Path); // Register TypeDB
