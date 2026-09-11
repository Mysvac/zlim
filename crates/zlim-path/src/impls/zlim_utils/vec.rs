use zlim_utils::format_smol;
use zlim_utils::vec::{ArrayVec, SmallVec};

use crate::path::{PathCell, TypePath, concat};

// -----------------------------------------------------------------------------
// SmallVec<T, N>

impl<T: TypePath, const N: usize> TypePath for SmallVec<T, N> {
    fn type_path() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| {
            concat(&[
                "zlim_utils::vec",
                "::",
                "SmallVec",
                "<",
                T::type_path(),
                ", ",
                &format_smol!("{N}"),
                ">",
            ])
        })
    }

    fn type_name() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| {
            concat(&[
                "SmallVec",
                "<",
                T::type_name(),
                ", ",
                &format_smol!("{N}"),
                ">",
            ])
        })
    }

    const IDENT: &str = "SmallVec";
    const CRATE: Option<&str> = Some("zlim_utils");
    const MODULE: Option<&str> = Some("zlim_utils::vec");
}

// -----------------------------------------------------------------------------
// ArrayVec<T, N>

impl<T: TypePath, const N: usize> TypePath for ArrayVec<T, N> {
    fn type_path() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| {
            concat(&[
                "zlim_utils::vec",
                "::",
                "ArrayVec",
                "<",
                T::type_path(),
                ", ",
                &format_smol!("{N}"),
                ">",
            ])
        })
    }

    fn type_name() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| {
            concat(&[
                "ArrayVec",
                "<",
                T::type_name(),
                ", ",
                &format_smol!("{N}"),
                ">",
            ])
        })
    }

    const IDENT: &str = "ArrayVec";
    const CRATE: Option<&str> = Some("zlim_utils");
    const MODULE: Option<&str> = Some("zlim_utils::vec");
}

// -----------------------------------------------------------------------------
// tests

#[cfg(test)]
mod tests {
    use crate::path::TypePath;
    use zlim_utils::vec::{ArrayVec, SmallVec};

    #[test]
    #[rustfmt::skip]
    fn small_vec() {
        assert_eq!(<SmallVec<u8, 4>>::type_path(), "zlim_utils::vec::SmallVec<u8, 4>");
        assert_eq!(<SmallVec<u8, 4>>::type_name(), "SmallVec<u8, 4>");
        assert_eq!(<SmallVec<u8, 4>>::IDENT, "SmallVec");
        assert_eq!(<SmallVec<u8, 4>>::CRATE, Some("zlim_utils"));
        assert_eq!(<SmallVec<u8, 4>>::MODULE, Some("zlim_utils::vec"));

        // Nested / differently sized instantiations are distinct paths.
        assert_eq!(<SmallVec<SmallVec<u8, 2>, 16>>::type_path(), "zlim_utils::vec::SmallVec<zlim_utils::vec::SmallVec<u8, 2>, 16>");
        assert_eq!(<SmallVec<SmallVec<u8, 2>, 16>>::type_name(), "SmallVec<SmallVec<u8, 2>, 16>");
    }

    #[test]
    #[rustfmt::skip]
    fn array_vec() {
        assert_eq!(<ArrayVec<u8, 8>>::type_path(), "zlim_utils::vec::ArrayVec<u8, 8>");
        assert_eq!(<ArrayVec<u8, 8>>::type_name(), "ArrayVec<u8, 8>");
        assert_eq!(<ArrayVec<u8, 8>>::IDENT, "ArrayVec");
        assert_eq!(<ArrayVec<u8, 8>>::CRATE, Some("zlim_utils"));
        assert_eq!(<ArrayVec<u8, 8>>::MODULE, Some("zlim_utils::vec"));
    }
}
