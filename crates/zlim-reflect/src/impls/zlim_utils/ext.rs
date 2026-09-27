use crate::impls::impl_simple_type_path;
use crate::path::{PathCell, TypePath, concat};
use zlim_utils::ext::ArrayDeque;
use zlim_utils::ext::BlockList;
use zlim_utils::ext::TypeMap;
use zlim_utils::format_smol;

impl_simple_type_path!(@TypeMap<T>:    "zlim_utils", "ext", "TypeMap");
impl_simple_type_path!(@BlockList<T>:    "zlim_utils", "ext", "BlockList");

impl<T: TypePath, const N: usize> TypePath for ArrayDeque<T, N> {
    fn type_path() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| {
            concat(&[
                "zlim_utils::ext",
                "::",
                "ArrayDeque",
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
                "ArrayDeque",
                "<",
                T::type_name(),
                ", ",
                &format_smol!("{N}"),
                ">",
            ])
        })
    }

    const IDENT: &'static str = "ArrayDeque";
    const CRATE: Option<&'static str> = Some("zlim_utils");
    const MODULE: Option<&'static str> = Some("zlim_utils::ext");
}
