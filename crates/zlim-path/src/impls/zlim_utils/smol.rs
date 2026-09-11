use zlim_utils::str::SmolStr;

use super::impl_simple_type_path;
use crate::path::TypePath;

impl_simple_type_path!(SmolStr: "zlim_utils", "str", "SmolStr");

// -----------------------------------------------------------------------------
// tests

#[cfg(test)]
mod tests {
    use crate::path::TypePath;
    use zlim_utils::str::SmolStr;

    #[test]
    fn smol_str() {
        assert_eq!(SmolStr::type_path(), "zlim_utils::str::SmolStr");
        assert_eq!(SmolStr::type_name(), "SmolStr");
        assert_eq!(SmolStr::IDENT, "SmolStr");
        assert_eq!(SmolStr::CRATE, Some("zlim_utils"));
        assert_eq!(SmolStr::MODULE, Some("zlim_utils::str"));
    }
}
