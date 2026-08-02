use core::hash::BuildHasherDefault;

use crate::path::{PathCell, TypePath, concat};

impl<H: TypePath> TypePath for BuildHasherDefault<H> {
    fn type_path() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| {
            concat(&["core::hash::BuildHasherDefault", "<", H::type_path(), ">"])
        })
    }

    fn type_name() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| concat(&["BuildHasherDefault", "<", H::type_name(), ">"]))
    }

    const IDENT: &str = "BuildHasherDefault";
    const CRATE: Option<&str> = Some("core");
    const MODULE: Option<&str> = Some("core::hash");
}
