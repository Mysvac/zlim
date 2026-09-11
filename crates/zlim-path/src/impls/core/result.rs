use super::impl_simple_type_path;
use crate::path::{PathCell, TypePath, concat};
use core::result::Result;

impl_simple_type_path!(@Result<T, E>: "core", "result", "Result");

// -----------------------------------------------------------------------------
// tests

#[cfg(test)]
mod tests {
    use crate::path::TypePath;
    use core::result::Result;

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
    fn option() {
        assert_path! {
            Result<(), u8>,
            "core::result::Result<(), u8>",
            "Result<(), u8>",
            "Result",
            Some("core"),
            Some("core::result"),
        }

        assert_path! {
            Result<Result<(), u8>, Result<i8, bool>>,
            "core::result::Result<core::result::Result<(), u8>, core::result::Result<i8, bool>>",
            "Result<Result<(), u8>, Result<i8, bool>>",
            "Result",
            Some("core"),
            Some("core::result"),
        }
    }
}
