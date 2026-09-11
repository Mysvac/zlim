use std::borrow::Cow;

use crate::path::{PathCell, TypePath, concat};

// -----------------------------------------------------------------------------
// Cow<'static, T>

impl<T: TypePath + ToOwned + ?Sized> TypePath for Cow<'static, T> {
    fn type_path() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| concat(&["alloc::borrow::Cow<", T::type_path(), ">"]))
    }

    fn type_name() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| concat(&["Cow<", T::type_name(), ">"]))
    }

    const IDENT: &str = "Cow";
    const CRATE: Option<&str> = Some("alloc");
    const MODULE: Option<&str> = Some("alloc::borrow");
}

// -----------------------------------------------------------------------------
// tests

#[cfg(test)]
mod tests {
    use crate::path::TypePath;
    use std::borrow::Cow;

    #[test]
    #[rustfmt::skip]
    fn cow() {
        assert_eq!(<Cow<'static, str>>::type_path(), "alloc::borrow::Cow<str>");
        assert_eq!(<Cow<'static, str>>::type_name(), "Cow<str>");
        assert_eq!(<Cow<'static, str>>::IDENT, "Cow");
        assert_eq!(<Cow<'static, str>>::CRATE, Some("alloc"));
        assert_eq!(<Cow<'static, str>>::MODULE, Some("alloc::borrow"));
        assert_eq!(<Cow<'static, [u8]>>::type_path(), "alloc::borrow::Cow<[u8]>");
        assert_eq!(<Cow<'static, [u8]>>::type_name(), "Cow<[u8]>");
    }
}
