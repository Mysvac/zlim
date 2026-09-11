use super::impl_simple_type_path;
use crate::path::{PathCell, TypePath, concat};
use std::boxed::Box;

impl_simple_type_path!(@Box<T>: "alloc", "boxed", "Box");

// -----------------------------------------------------------------------------
// tests

#[cfg(test)]
mod tests {
    use crate::path::TypePath;
    use std::boxed::Box;

    #[test]
    #[rustfmt::skip]
    fn vec() {
        assert_eq!(<Box<u8>>::type_path(), "alloc::boxed::Box<u8>");
        assert_eq!(<Box<u8>>::type_name(), "Box<u8>");
        assert_eq!(<Box<u8>>::IDENT, "Box");
        assert_eq!(<Box<u8>>::CRATE, Some("alloc"));
        assert_eq!(<Box<u8>>::MODULE, Some("alloc::boxed"));
        assert_eq!(<Box<Box<u8>>>::type_path(), "alloc::boxed::Box<alloc::boxed::Box<u8>>");
        assert_eq!(<Box<Box<u8>>>::type_name(), "Box<Box<u8>>");
    }
}
