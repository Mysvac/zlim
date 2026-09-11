use std::path::{Path, PathBuf};

use super::impl_simple_type_path;
use crate::path::TypePath;

impl_simple_type_path!(Path:    "std", "path", "Path");
impl_simple_type_path!(PathBuf: "std", "path", "PathBuf");

// -----------------------------------------------------------------------------
// tests

#[cfg(test)]
mod tests {
    use crate::path::TypePath;
    use std::path::{Path, PathBuf};

    #[test]
    fn path() {
        assert_eq!(Path::type_path(), "std::path::Path");
        assert_eq!(Path::type_name(), "Path");
        assert_eq!(Path::IDENT, "Path");
        assert_eq!(Path::CRATE, Some("std"));
        assert_eq!(Path::MODULE, Some("std::path"));
    }

    #[test]
    fn path_buf() {
        assert_eq!(PathBuf::type_path(), "std::path::PathBuf");
        assert_eq!(PathBuf::type_name(), "PathBuf");
        assert_eq!(PathBuf::IDENT, "PathBuf");
        assert_eq!(PathBuf::CRATE, Some("std"));
        assert_eq!(PathBuf::MODULE, Some("std::path"));
    }
}
