use std::collections::{BTreeMap, BTreeSet};

use super::impl_simple_type_path;
use crate::path::{PathCell, TypePath, concat};

impl_simple_type_path!(@BTreeSet<T>: "alloc", "collections", "BTreeSet");
impl_simple_type_path!(@BTreeMap<K, V>: "alloc", "collections", "BTreeMap");

// -----------------------------------------------------------------------------
// tests

#[cfg(test)]
mod tests {
    use crate::path::TypePath;
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    #[rustfmt::skip]
    fn btree_set() {
        assert_eq!(<BTreeSet<u8>>::type_path(), "alloc::collections::BTreeSet<u8>");
        assert_eq!(<BTreeSet<u8>>::type_name(), "BTreeSet<u8>");
        assert_eq!(<BTreeSet<u8>>::IDENT, "BTreeSet");
        assert_eq!(<BTreeSet<u8>>::CRATE, Some("alloc"));
        assert_eq!(<BTreeSet<u8>>::MODULE, Some("alloc::collections"));
    }

    #[test]
    #[rustfmt::skip]
    fn btree_map() {
        assert_eq!(<BTreeMap<u8, String>>::type_path(), "alloc::collections::BTreeMap<u8, alloc::string::String>");
        assert_eq!(<BTreeMap<u8, String>>::type_name(), "BTreeMap<u8, String>");
        assert_eq!(<BTreeMap<u8, String>>::IDENT, "BTreeMap");
        assert_eq!(<BTreeMap<u8, String>>::CRATE, Some("alloc"));
        assert_eq!(<BTreeMap<u8, String>>::MODULE, Some("alloc::collections"));
    }
}
