use std::collections::LinkedList;

use super::impl_simple_type_path;
use crate::path::{PathCell, TypePath, concat};

impl_simple_type_path!(@LinkedList<T>: "alloc", "collections", "LinkedList");

// -----------------------------------------------------------------------------
// tests

#[cfg(test)]
mod tests {
    use crate::path::TypePath;
    use std::collections::LinkedList;

    #[test]
    #[rustfmt::skip]
    fn linked_list() {
        assert_eq!(<LinkedList<u8>>::type_path(), "alloc::collections::LinkedList<u8>");
        assert_eq!(<LinkedList<u8>>::type_name(), "LinkedList<u8>");
        assert_eq!(<LinkedList<u8>>::IDENT, "LinkedList");
        assert_eq!(<LinkedList<u8>>::CRATE, Some("alloc"));
        assert_eq!(<LinkedList<u8>>::MODULE, Some("alloc::collections"));
        assert_eq!(<LinkedList<LinkedList<u8>>>::type_path(), "alloc::collections::LinkedList<alloc::collections::LinkedList<u8>>");
        assert_eq!(<LinkedList<LinkedList<u8>>>::type_name(), "LinkedList<LinkedList<u8>>");
    }
}
