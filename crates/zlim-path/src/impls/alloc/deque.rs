use std::collections::VecDeque;

use super::impl_simple_type_path;
use crate::path::{PathCell, TypePath, concat};

impl_simple_type_path!(@VecDeque<T>: "alloc", "collections", "VecDeque");

// -----------------------------------------------------------------------------
// tests

#[cfg(test)]
mod tests {
    use crate::path::TypePath;
    use std::collections::VecDeque;

    #[test]
    #[rustfmt::skip]
    fn vec_deque() {
        assert_eq!(<VecDeque<u8>>::type_path(), "alloc::collections::VecDeque<u8>");
        assert_eq!(<VecDeque<u8>>::type_name(), "VecDeque<u8>");
        assert_eq!(<VecDeque<u8>>::IDENT, "VecDeque");
        assert_eq!(<VecDeque<u8>>::CRATE, Some("alloc"));
        assert_eq!(<VecDeque<u8>>::MODULE, Some("alloc::collections"));
    }
}
