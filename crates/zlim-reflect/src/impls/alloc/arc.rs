//! [`TypePath`] for the reference-counted pointer.
//!
//! `Arc<T>` is named but not reflected, the same as [`Box<T>`]: the value behind the pointer is
//! shared, so reflection has no way to hand out the `&mut` every reflective operation works
//! through.
//!
//! [`TypePath`]: crate::path::TypePath
//! [`Box<T>`]: super::boxed

use std::sync::Arc;

use crate::impls::impl_simple_type_path;

impl_simple_type_path!(@Arc<T>: "alloc", "sync", "Arc");

// -----------------------------------------------------------------------------
// tests

#[cfg(test)]
mod tests {
    use crate::path::TypePath;
    use std::sync::Arc;

    #[test]
    #[rustfmt::skip]
    fn arc() {
        assert_eq!(<Arc<u8>>::type_path(), "alloc::sync::Arc<u8>");
        assert_eq!(<Arc<u8>>::type_name(), "Arc<u8>");
        assert_eq!(<Arc<u8>>::IDENT, "Arc");
        assert_eq!(<Arc<u8>>::CRATE, Some("alloc"));
        assert_eq!(<Arc<u8>>::MODULE, Some("alloc::sync"));
    }
}
