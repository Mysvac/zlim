use core::fmt::{Debug, Formatter};
use core::hash::BuildHasher;
use core::panic::Location;

use zlim_utils::hash::FixedState;

use crate::db::TypeDatabase;
use crate::info::{OpaqueInfo, TypeInfo, Typed};
use crate::ops::{Opaque, Reflect};
use crate::path::TypePath;

impl TypePath for &'static Location<'static> {
    #[inline]
    fn type_path() -> &'static str {
        "core::panic::Location"
    }

    #[inline]
    fn type_name() -> &'static str {
        "Location"
    }

    const IDENT: &'static str = "Location";
    const CRATE: Option<&'static str> = Some("core");
    const MODULE: Option<&'static str> = Some("core::panic");
}

impl Typed for &'static Location<'static> {
    #[inline]
    fn type_info() -> &'static TypeInfo {
        static INFO: TypeInfo = TypeInfo::Opaque(OpaqueInfo::new::<&'static Location<'static>>());
        &INFO
    }
}

impl Reflect for &'static Location<'static> {
    crate::impls::impl_reflect_kind!(Opaque);

    fn reflect_clone(&self) -> Result<Box<dyn Reflect>, crate::ops::CloneError> {
        Ok(Box::new(*self))
    }

    fn reflect_apply(&mut self, value: &dyn Reflect) -> Result<(), crate::ops::ApplyError> {
        crate::impls::opaque_apply(self, value)
    }

    fn reflect_debug(&self, f: &mut Formatter) -> core::fmt::Result {
        Debug::fmt(self, f)
    }

    fn reflect_eq(&self, other: &dyn Reflect) -> bool {
        if let Some(o) = other.downcast_ref::<Self>() {
            *self == *o
        } else {
            false
        }
    }

    fn reflect_hash(&self) -> u64 {
        FixedState.hash_one(self)
    }

    fn from_reflect(value: Box<dyn Reflect>) -> Result<Box<Self>, Box<dyn Reflect>> {
        value.downcast::<Self>()
    }
}

impl Opaque for &'static Location<'static> {
    fn apply_str(&mut self, _: &str) -> Result<(), String> {
        Err("`&'static Location` cannot be converted from str".into())
    }

    fn stringify(&self) -> String {
        self.to_string()
    }
}

impl TypeDatabase for &'static Location<'static> {}

crate::register_reflect!(&'static Location<'static>);
