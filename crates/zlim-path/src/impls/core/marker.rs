use super::impl_simple_type_path;
use crate::path::{PathCell, TypePath, concat};
use core::marker::{PhantomData, PhantomPinned};

impl_simple_type_path!(PhantomPinned: "core", "marker", "PhantomPinned");
impl_simple_type_path!(@PhantomData<T>: "core", "marker", "PhantomData");

// -----------------------------------------------------------------------------
// tests

#[cfg(test)]
mod tests {
    use crate::path::TypePath;
    use core::marker::{PhantomData, PhantomPinned};

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
    fn markers() {
        assert_path! {
            PhantomPinned,
            "core::marker::PhantomPinned",
            "PhantomPinned",
            "PhantomPinned",
            Some("core"),
            Some("core::marker"),
        }

        assert_path! {
            PhantomData<()>,
            "core::marker::PhantomData<()>",
            "PhantomData<()>",
            "PhantomData",
            Some("core"),
            Some("core::marker"),
        }

        assert_path! {
            PhantomData<PhantomPinned>,
            "core::marker::PhantomData<core::marker::PhantomPinned>",
            "PhantomData<PhantomPinned>",
            "PhantomData",
            Some("core"),
            Some("core::marker"),
        }
    }
}
