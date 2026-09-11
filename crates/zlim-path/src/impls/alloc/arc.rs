use super::impl_simple_type_path;
use crate::path::{PathCell, TypePath, concat};
use std::sync::Arc;

impl_simple_type_path!(@Arc<T>: "alloc", "sync", "Arc");

// -----------------------------------------------------------------------------
// tests

#[cfg(test)]
mod tests {
    use crate::path::TypePath;
    use std::sync::Arc;

    #[test]
    #[rustfmt::skip]
    fn vec() {
        assert_eq!(<Arc<u8>>::type_path(), "alloc::sync::Arc<u8>");
        assert_eq!(<Arc<u8>>::type_name(), "Arc<u8>");
        assert_eq!(<Arc<u8>>::IDENT, "Arc");
        assert_eq!(<Arc<u8>>::CRATE, Some("alloc"));
        assert_eq!(<Arc<u8>>::MODULE, Some("alloc::sync"));
        assert_eq!(<Arc<Arc<u8>>>::type_path(), "alloc::sync::Arc<alloc::sync::Arc<u8>>");
        assert_eq!(<Arc<Arc<u8>>>::type_name(), "Arc<Arc<u8>>");
    }
}
