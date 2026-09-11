use zlim_utils::hash::{FixedState, HashMap, HashSet, NoopState, SparseState};

use super::impl_simple_type_path;
use crate::path::{PathCell, TypePath, concat};

impl_simple_type_path!(FixedState:  "zlim_utils", "hash", "FixedState");
impl_simple_type_path!(NoopState:   "zlim_utils", "hash", "NoopState");
impl_simple_type_path!(SparseState: "zlim_utils", "hash", "SparseState");

impl_simple_type_path!(@HashSet<K, S>:    "zlim_utils", "hash", "HashSet");
impl_simple_type_path!(@HashMap<K, V, S>: "zlim_utils", "hash", "HashMap");

// -----------------------------------------------------------------------------
// tests

#[cfg(test)]
mod tests {
    use crate::path::TypePath;
    use zlim_utils::hash::{FixedState, HashMap, HashSet, NoopState, SparseState};

    #[test]
    #[rustfmt::skip]
    fn states() {
        assert_eq!(FixedState::type_path(), "zlim_utils::hash::FixedState");
        assert_eq!(FixedState::type_name(), "FixedState");
        assert_eq!(FixedState::IDENT, "FixedState");
        assert_eq!(FixedState::CRATE, Some("zlim_utils"));
        assert_eq!(FixedState::MODULE, Some("zlim_utils::hash"));
        assert_eq!(NoopState::type_path(), "zlim_utils::hash::NoopState");
        assert_eq!(SparseState::type_path(), "zlim_utils::hash::SparseState");
    }

    #[test]
    #[rustfmt::skip]
    fn containers() {
        assert_eq!(<HashSet<u8, FixedState>>::type_path(), "zlim_utils::hash::HashSet<u8, zlim_utils::hash::FixedState>");
        assert_eq!(<HashSet<u8, FixedState>>::type_name(), "HashSet<u8, FixedState>");
        assert_eq!(<HashSet<u8, FixedState>>::IDENT, "HashSet");
        assert_eq!(<HashSet<u8, FixedState>>::CRATE, Some("zlim_utils"));
        assert_eq!(<HashSet<u8, FixedState>>::MODULE, Some("zlim_utils::hash"));
        assert_eq!(<HashMap<u8, String, FixedState>>::type_path(), "zlim_utils::hash::HashMap<u8, alloc::string::String, zlim_utils::hash::FixedState>");
        assert_eq!(<HashMap<u8, String, FixedState>>::type_name(), "HashMap<u8, String, FixedState>");
        assert_eq!(<HashMap<u8, String, FixedState>>::IDENT, "HashMap");
        assert_eq!(<HashMap<u8, String, FixedState>>::CRATE, Some("zlim_utils"));
        assert_eq!(<HashMap<u8, String, FixedState>>::MODULE, Some("zlim_utils::hash"));
    }
}
