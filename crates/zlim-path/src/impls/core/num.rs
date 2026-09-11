use super::impl_simple_type_path;
use crate::path::{PathCell, TypePath, concat};
use core::num::*;

impl_simple_type_path!(NonZeroU8:    "core", "num", "NonZeroU8");
impl_simple_type_path!(NonZeroU16:   "core", "num", "NonZeroU16");
impl_simple_type_path!(NonZeroU32:   "core", "num", "NonZeroU32");
impl_simple_type_path!(NonZeroU64:   "core", "num", "NonZeroU64");
impl_simple_type_path!(NonZeroU128:  "core", "num", "NonZeroU128");
impl_simple_type_path!(NonZeroUsize: "core", "num", "NonZeroUsize");
impl_simple_type_path!(NonZeroI8:    "core", "num", "NonZeroI8");
impl_simple_type_path!(NonZeroI16:   "core", "num", "NonZeroI16");
impl_simple_type_path!(NonZeroI32:   "core", "num", "NonZeroI32");
impl_simple_type_path!(NonZeroI64:   "core", "num", "NonZeroI64");
impl_simple_type_path!(NonZeroI128:  "core", "num", "NonZeroI128");
impl_simple_type_path!(NonZeroIsize: "core", "num", "NonZeroIsize");
impl_simple_type_path!(@Wrapping<T>: "core", "num", "Wrapping");
impl_simple_type_path!(@Saturating<T>: "core", "num", "Saturating");

// -----------------------------------------------------------------------------
// tests

#[cfg(test)]
mod tests {
    use crate::path::TypePath;
    use core::num::*;

    macro_rules! assert_path {
        (
            $t:ty,
            $a:expr,
            $b:expr,
            $c:expr,
            $d:expr,
            $e:expr,
        ) => {
            assert_eq!(<$t>::type_path(), $a);
            assert_eq!(<$t>::type_name(), $b);
            assert_eq!(<$t>::IDENT, $c);
            assert_eq!(<$t>::CRATE, $d);
            assert_eq!(<$t>::MODULE, $e);
        };
    }

    #[test]
    fn nums() {
        assert_path! {
            NonZeroI8,
            "core::num::NonZeroI8",
            "NonZeroI8",
            "NonZeroI8",
            Some("core"),
            Some("core::num"),
        }

        assert_path! {
            Wrapping<u8>,
            "core::num::Wrapping<u8>",
            "Wrapping<u8>",
            "Wrapping",
            Some("core"),
            Some("core::num"),
        }

        assert_path! {
            Saturating<NonZeroU32>,
            "core::num::Saturating<core::num::NonZeroU32>",
            "Saturating<NonZeroU32>",
            "Saturating",
            Some("core"),
            Some("core::num"),
        }
    }
}
