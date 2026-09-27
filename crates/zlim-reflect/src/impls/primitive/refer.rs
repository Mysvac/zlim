use crate::path::{PathCell, TypePath, concat};

impl<T: TypePath + ?Sized> TypePath for &'static T {
    fn type_path() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| concat(&["&", T::type_path()]))
    }

    fn type_name() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| concat(&["&", T::type_name()]))
    }

    const IDENT: &'static str = "&_";
    const CRATE: Option<&'static str> = None;
    const MODULE: Option<&'static str> = None;
}

impl<T: TypePath + ?Sized> TypePath for &'static mut T {
    fn type_path() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| concat(&["&mut ", T::type_path()]))
    }

    fn type_name() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| concat(&["&mut ", T::type_name()]))
    }

    const IDENT: &'static str = "&mut _";
    const CRATE: Option<&'static str> = None;
    const MODULE: Option<&'static str> = None;
}
