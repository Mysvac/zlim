use core::any::TypeId;

use crate::impls::impl_simple_type_path;
use crate::ops::Opaque;

impl_simple_type_path!(TypeId: "core", "any", "TypeId");

zlim_reflect_derive::impl_reflect! {
    #[reflect(Opaque, Clone, Debug, Eq, Hash)]
    pub struct TypeId;
}

impl Opaque for TypeId {
    fn apply_str(&mut self, _: &str) -> Result<(), String> {
        Err(String::from("TypeId cannot be convert from string."))
    }

    fn stringify(&self) -> String {
        format!("{self:?}")
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::db::TypeDB;
    use core::any::TypeId;

    #[test]
    fn is_registered() {
        TypeDB::collect();
        assert!(TypeDB::get_by_type(TypeId::of::<TypeId>()).is_some());
    }
}
