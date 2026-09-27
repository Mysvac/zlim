use core::fmt::{Debug, Formatter};
use core::hash::BuildHasher;

use zlim_utils::hash::FixedState;

use crate::Reflect;
use crate::db::{TypeDB, TypeDatabase};
use crate::impls::impl_simple_type_path;
use crate::info::{OpaqueInfo, TypeInfo, Typed};
use crate::ops::{ApplyError, CloneError, Opaque};

// -----------------------------------------------------------------------------
// TypePath

impl_simple_type_path!(str: "str");

// -----------------------------------------------------------------------------
// Typed

impl Typed for &'static str {
    #[inline]
    fn type_info() -> &'static TypeInfo {
        static INFO: TypeInfo = TypeInfo::Opaque(OpaqueInfo::new::<&'static str>());
        &INFO
    }
}

// -----------------------------------------------------------------------------
// Opaque

impl Opaque for &'static str {
    #[inline]
    fn apply_str(&mut self, _: &str) -> Result<(), String> {
        Err(String::from("`&'static str` does not support `apply_str`"))
    }

    #[inline]
    fn stringify(&self) -> String {
        (*self).to_string()
    }
}

// -----------------------------------------------------------------------------
// Reflect

impl Reflect for &'static str {
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
        value.downcast::<Self>()
    }
}

// -----------------------------------------------------------------------------
// TypeDatabase

impl TypeDatabase for &'static str {
    fn on_register(db: &'static TypeDB) {
        db.insert_defaultor::<Self>();
        db.insert_serializer::<Self>();
    }

    fn register_dependencies() {}
}

crate::register_reflect!(&'static str); // Register TypeDB

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use crate::db::TypeDB;
    use crate::path::TypePath;
    use core::any::TypeId;

    /// Covers `str` alongside the `&'static str` reference that the reflection
    /// database actually stores, whose name is `&str` and whose `IDENT` drops
    /// the referent to `&_`.
    #[test]
    fn str_path() {
        assert_eq!(<str>::type_path(), "str");
        assert_eq!(<str>::type_name(), "str");
        assert_eq!(<str>::IDENT, "str");
        assert_eq!(<str>::CRATE, None);
        assert_eq!(<str>::MODULE, None);

        assert_eq!(<&'static str>::type_path(), "&str");
        assert_eq!(<&str>::type_path(), "&str");
        assert_eq!(<&str>::type_name(), "&str");
        assert_eq!(<&str>::IDENT, "&_");
        assert_eq!(<&str>::CRATE, None);
        assert_eq!(<&str>::MODULE, None);
    }

    #[test]
    fn registered() {
        TypeDB::collect();
        assert!(TypeDB::get_by_type(TypeId::of::<&str>()).is_some());
        assert!(TypeDB::get_by_path("&str").is_some());
    }
}
