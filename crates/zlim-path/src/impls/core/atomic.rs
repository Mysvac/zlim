use super::impl_simple_type_path;
use crate::path::TypePath;
use core::sync::atomic::*;

impl_simple_type_path!(Ordering:    "core", "sync::atomic", "Ordering");
impl_simple_type_path!(AtomicU8:    "core", "sync::atomic", "AtomicU8");
impl_simple_type_path!(AtomicU16:   "core", "sync::atomic", "AtomicU16");
impl_simple_type_path!(AtomicU32:   "core", "sync::atomic", "AtomicU32");
impl_simple_type_path!(AtomicU64:   "core", "sync::atomic", "AtomicU64");
impl_simple_type_path!(AtomicUsize: "core", "sync::atomic", "AtomicUsize");
impl_simple_type_path!(AtomicI8:    "core", "sync::atomic", "AtomicI8");
impl_simple_type_path!(AtomicI16:   "core", "sync::atomic", "AtomicI16");
impl_simple_type_path!(AtomicI32:   "core", "sync::atomic", "AtomicI32");
impl_simple_type_path!(AtomicI64:   "core", "sync::atomic", "AtomicI64");
impl_simple_type_path!(AtomicIsize: "core", "sync::atomic", "AtomicIsize");

// should we implement TypePath for `AtomicPtr` ?

// -----------------------------------------------------------------------------
// tests

#[cfg(test)]
mod tests {
    use crate::path::TypePath;
    use core::sync::atomic::*;

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
    fn atomics() {
        assert_path! {
            Ordering,
            "core::sync::atomic::Ordering",
            "Ordering",
            "Ordering",
            Some("core"),
            Some("core::sync::atomic"),
        }

        assert_path! {
            AtomicU32,
            "core::sync::atomic::AtomicU32",
            "AtomicU32",
            "AtomicU32",
            Some("core"),
            Some("core::sync::atomic"),
        }

        assert_path! {
            AtomicIsize,
            "core::sync::atomic::AtomicIsize",
            "AtomicIsize",
            "AtomicIsize",
            Some("core"),
            Some("core::sync::atomic"),
        }
    }
}
