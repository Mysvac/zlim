use super::impl_simple_type_path;
use crate::path::TypePath;
use uuid::{NonNilUuid, Uuid};

impl_simple_type_path!(Uuid: "uuid", "Uuid");
impl_simple_type_path!(NonNilUuid: "uuid", "NonNilUuid");

// -----------------------------------------------------------------------------
// tests

#[cfg(test)]
mod tests {
    use crate::path::TypePath;
    use uuid::{NonNilUuid, Uuid};

    #[test]
    fn uuid() {
        assert_eq!(Uuid::type_path(), "uuid::Uuid");
        assert_eq!(Uuid::type_name(), "Uuid");
        assert_eq!(Uuid::IDENT, "Uuid");
        assert_eq!(Uuid::CRATE, Some("uuid"));
        assert_eq!(Uuid::MODULE, Some("uuid"));
        assert_eq!(NonNilUuid::type_path(), "uuid::NonNilUuid");
        assert_eq!(NonNilUuid::type_name(), "NonNilUuid");
        assert_eq!(NonNilUuid::IDENT, "NonNilUuid");
        assert_eq!(NonNilUuid::CRATE, Some("uuid"));
        assert_eq!(NonNilUuid::MODULE, Some("uuid"));
    }
}
