use super::impl_simple_type_path;
use crate::path::TypePath;
use std::string::String;

impl_simple_type_path!(String: "alloc", "string", "String");

// -----------------------------------------------------------------------------
// tests

#[cfg(test)]
mod tests {
    use crate::path::TypePath;

    #[test]
    fn string() {
        assert_eq!(String::type_path(), "alloc::string::String");
        assert_eq!(String::type_name(), "String");
        assert_eq!(String::IDENT, "String");
        assert_eq!(String::CRATE, Some("alloc"));
        assert_eq!(String::MODULE, Some("alloc::string"));
    }
}
