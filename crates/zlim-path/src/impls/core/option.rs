use super::impl_simple_type_path;
use crate::path::{PathCell, TypePath, concat};
use core::option::Option;

impl_simple_type_path!(@Option<T>: "core", "option", "Option");

// -----------------------------------------------------------------------------
// tests

#[cfg(test)]
mod tests {
    use crate::path::TypePath;
    use core::option::Option;

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
            Option<()>,
            "core::option::Option<()>",
            "Option<()>",
            "Option",
            Some("core"),
            Some("core::option"),
        }

        assert_path! {
            Option<Option<()>>,
            "core::option::Option<core::option::Option<()>>",
            "Option<Option<()>>",
            "Option",
            Some("core"),
            Some("core::option"),
        }
    }
}
