use std::ffi::{OsStr, OsString};

use super::impl_simple_type_path;
use crate::path::TypePath;

impl_simple_type_path!(OsString: "std", "ffi", "OsString");
impl_simple_type_path!(OsStr: "std", "ffi", "OsStr");

// -----------------------------------------------------------------------------
// tests

#[cfg(test)]
mod tests {
    use crate::path::TypePath;
    use std::ffi::{OsStr, OsString};

    #[test]
    fn os_string() {
        assert_eq!(OsString::type_path(), "std::ffi::OsString");
        assert_eq!(OsString::type_name(), "OsString");
        assert_eq!(OsString::IDENT, "OsString");
        assert_eq!(OsString::CRATE, Some("std"));
        assert_eq!(OsString::MODULE, Some("std::ffi"));
        assert_eq!(OsStr::type_path(), "std::ffi::OsStr");
        assert_eq!(OsStr::type_name(), "OsStr");
        assert_eq!(OsStr::IDENT, "OsStr");
        assert_eq!(OsStr::CRATE, Some("std"));
        assert_eq!(OsStr::MODULE, Some("std::ffi"));
    }
}
