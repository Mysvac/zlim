use std::collections::BinaryHeap;

use super::impl_simple_type_path;
use crate::path::{PathCell, TypePath, concat};

impl_simple_type_path!(@BinaryHeap<T>: "alloc", "collections", "BinaryHeap");

// -----------------------------------------------------------------------------
// tests

#[cfg(test)]
mod tests {
    use crate::path::TypePath;
    use std::collections::BinaryHeap;

    #[test]
    #[rustfmt::skip]
    fn binary_heap() {
        assert_eq!(<BinaryHeap<u8>>::type_path(), "alloc::collections::BinaryHeap<u8>");
        assert_eq!(<BinaryHeap<u8>>::type_name(), "BinaryHeap<u8>");
        assert_eq!(<BinaryHeap<u8>>::IDENT, "BinaryHeap");
        assert_eq!(<BinaryHeap<u8>>::CRATE, Some("alloc"));
        assert_eq!(<BinaryHeap<u8>>::MODULE, Some("alloc::collections"));
    }
}
