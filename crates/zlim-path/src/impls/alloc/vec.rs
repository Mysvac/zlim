use std::vec::Vec;

use super::impl_simple_type_path;
use crate::path::{PathCell, TypePath, concat};

impl_simple_type_path!(@Vec<T>: "alloc", "vec", "Vec");

// -----------------------------------------------------------------------------
// tests

#[cfg(test)]
mod tests {
    use crate::path::TypePath;

    #[test]
    #[rustfmt::skip]
    fn vec() {
        assert_eq!(<Vec<u8>>::type_path(), "alloc::vec::Vec<u8>");
        assert_eq!(<Vec<u8>>::type_name(), "Vec<u8>");
        assert_eq!(<Vec<u8>>::IDENT, "Vec");
        assert_eq!(<Vec<u8>>::CRATE, Some("alloc"));
        assert_eq!(<Vec<u8>>::MODULE, Some("alloc::vec"));
        assert_eq!(<Vec<Vec<u8>>>::type_path(), "alloc::vec::Vec<alloc::vec::Vec<u8>>");
        assert_eq!(<Vec<Vec<u8>>>::type_name(), "Vec<Vec<u8>>");
    }
}
