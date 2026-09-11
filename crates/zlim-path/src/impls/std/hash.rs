use std::collections::{HashMap, HashSet};
use std::hash::RandomState;

use super::impl_simple_type_path;
use crate::path::{PathCell, TypePath, concat};

impl_simple_type_path!(RandomState: "std", "hash", "RandomState");

impl_simple_type_path!(@HashSet<K, S>:    "std", "collections", "HashSet");
impl_simple_type_path!(@HashMap<K, V, S>: "std", "collections", "HashMap");

// -----------------------------------------------------------------------------
// tests

#[cfg(test)]
mod tests {
    use crate::path::TypePath;
    use std::collections::{HashMap, HashSet};
    use std::hash::RandomState;

    #[test]
    fn random_state() {
        assert_eq!(RandomState::type_path(), "std::hash::RandomState");
        assert_eq!(RandomState::type_name(), "RandomState");
        assert_eq!(RandomState::IDENT, "RandomState");
        assert_eq!(RandomState::CRATE, Some("std"));
        assert_eq!(RandomState::MODULE, Some("std::hash"));
    }

    #[test]
    #[rustfmt::skip]
    fn hash_set() {
        assert_eq!(<HashSet<u8, RandomState>>::type_path(), "std::collections::HashSet<u8, std::hash::RandomState>");
        assert_eq!(<HashSet<u8, RandomState>>::type_name(), "HashSet<u8, RandomState>");
        assert_eq!(<HashSet<u8, RandomState>>::IDENT, "HashSet");
        assert_eq!(<HashSet<u8, RandomState>>::CRATE, Some("std"));
        assert_eq!(<HashSet<u8, RandomState>>::MODULE, Some("std::collections"));
    }

    #[test]
    #[rustfmt::skip]
    fn hash_map() {
        assert_eq!(<HashMap<u8, String, RandomState>>::type_path(), "std::collections::HashMap<u8, alloc::string::String, std::hash::RandomState>");
        assert_eq!(<HashMap<u8, String, RandomState>>::type_name(), "HashMap<u8, String, RandomState>");
        assert_eq!(<HashMap<u8, String, RandomState>>::IDENT, "HashMap");
        assert_eq!(<HashMap<u8, String, RandomState>>::CRATE, Some("std"));
        assert_eq!(<HashMap<u8, String, RandomState>>::MODULE, Some("std::collections"));
    }
}
