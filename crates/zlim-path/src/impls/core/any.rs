use core::any::TypeId;

use super::impl_simple_type_path;
use crate::path::TypePath;

impl_simple_type_path!(TypeId: "core", "any", "TypeId");

// -----------------------------------------------------------------------------
// tests

#[cfg(test)]
mod tests {
    use crate::path::TypePath;
    use core::any::TypeId;

    #[test]
    fn type_id() {
        assert_eq!(TypeId::type_path(), "core::any::TypeId");
        assert_eq!(TypeId::type_name(), "TypeId");
        assert_eq!(TypeId::IDENT, "TypeId");
        assert_eq!(TypeId::CRATE, Some("core"));
        assert_eq!(TypeId::MODULE, Some("core::any"));
    }
}
