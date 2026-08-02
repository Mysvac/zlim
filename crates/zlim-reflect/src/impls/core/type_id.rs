use core::any::TypeId;

use crate::ops::Opaque;

zlim_reflect_derive::impl_reflect! {
    #[type_path = "core::any::TypeId"]
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

// ----------------------------------------------------------------------------
// Tests
// ----------------------------------------------------------------------------

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
