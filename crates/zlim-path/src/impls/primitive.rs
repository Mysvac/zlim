use zlim_utils::format_smol;

use super::impl_simple_type_path;
use crate::path::{PathCell, TypePath, concat};

// -----------------------------------------------------------------------------
// simple

impl_simple_type_path!(u8:    "u8"   );
impl_simple_type_path!(u16:   "u16"  );
impl_simple_type_path!(u32:   "u32"  );
impl_simple_type_path!(u64:   "u64"  );
impl_simple_type_path!(u128:  "u128" );
impl_simple_type_path!(usize: "usize");
impl_simple_type_path!(i8:    "i8"   );
impl_simple_type_path!(i16:   "i16"  );
impl_simple_type_path!(i32:   "i32"  );
impl_simple_type_path!(i64:   "i64"  );
impl_simple_type_path!(i128:  "i128" );
impl_simple_type_path!(isize: "isize");
impl_simple_type_path!(bool:  "bool" );
impl_simple_type_path!(char:  "char" );
impl_simple_type_path!(f32:   "f32"  );
impl_simple_type_path!(f64:   "f64"  );
impl_simple_type_path!(str:   "str"  );
impl_simple_type_path!(():    "()"   );
// ↓ need rust version 1.100 ↓
// impl_simple_type_path!(!:    "!"  );

// -----------------------------------------------------------------------------
// reference

impl<T: TypePath + ?Sized> TypePath for &'static T {
    fn type_path() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| concat(&["&", T::type_path()]))
    }

    fn type_name() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| concat(&["&", T::type_name()]))
    }

    const IDENT: &str = "&_";
    const CRATE: Option<&str> = None;
    const MODULE: Option<&str> = None;
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

    const IDENT: &str = "&mut _";
    const CRATE: Option<&str> = None;
    const MODULE: Option<&str> = None;
}

// -----------------------------------------------------------------------------
// array

impl<T: TypePath> TypePath for [T] {
    fn type_path() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| concat(&["[", <T>::type_path(), "]"]))
    }

    fn type_name() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| concat(&["[", <T>::type_name(), "]"]))
    }

    const IDENT: &str = "[_]";
    const CRATE: Option<&str> = None;
    const MODULE: Option<&str> = None;
}

impl<T: TypePath, const N: usize> TypePath for [T; N] {
    fn type_path() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| concat(&["[", T::type_path(), "; ", &format_smol!("{N}"), "]"]))
    }

    fn type_name() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| concat(&["[", T::type_name(), "; ", &format_smol!("{N}"), "]"]))
    }

    const IDENT: &str = "[_; _]";
    const CRATE: Option<&str> = None;
    const MODULE: Option<&str> = None;
}

// -----------------------------------------------------------------------------
// tuple

macro_rules! comma_space {
    ($_:ident) => {
        ", _"
    };
}

macro_rules! impl_tuple_type_path {
    (0: []) => {};
    (1: [ $index:tt : $name:ident ]) => {
        #[cfg_attr(docsrs, doc(fake_variadic))]
        #[cfg_attr(docsrs, doc = "This trait is implemented for tuples up to 12 items long.")]
        impl<$name: TypePath> TypePath for ($name,) {
            fn type_path() -> &'static str {
                static CELL: PathCell = PathCell::new();
                CELL.get_or_init::<Self>(|| concat(&["(" , <$name>::type_path() , ",)"]))
            }

            fn type_name() -> &'static str {
                static CELL: PathCell = PathCell::new();
                CELL.get_or_init::<Self>(|| concat(&["(" , <$name>::type_name() , ",)"]))
            }

            const IDENT: &str = "(_,)";
            const CRATE: Option<&str> = None;
            const MODULE: Option<&str> = None;
        }
    };
    ($_:literal: [$zero_index:tt : $zero_name:ident , $($index:tt : $name:ident),*]) => {
        #[cfg_attr(docsrs, doc(hidden))]
        impl<$zero_name: TypePath, $($name: TypePath),*> TypePath for ($zero_name, $($name),*) {
            fn type_path() -> &'static str {
                static CELL: PathCell = PathCell::new();
                CELL.get_or_init::<Self>(|| {
                    concat(&["(", <$zero_name>::type_path() $(, ", ", <$name>::type_path())* , ")"])
                })
            }

            fn type_name() -> &'static str {
                static CELL: PathCell = PathCell::new();
                CELL.get_or_init::<Self>(|| {
                    concat(&["(", <$zero_name>::type_name() $(, ", ", <$name>::type_name())* , ")"])
                })
            }

            const IDENT: &str = concat!( "(_", $( comma_space!{ $name } , )* ")" );
            const CRATE: Option<&str> = None;
            const MODULE: Option<&str> = None;
        }
    };
}

zlim_utils::range_invoke!(impl_tuple_type_path, 12);

// -----------------------------------------------------------------------------
// tests

#[cfg(test)]
mod tests {
    use crate::path::TypePath;

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
    fn simple_path() {
        assert_path! {
            (), "()", "()", "()", None, None,
        }

        assert_path! {
            u8, "u8", "u8", "u8", None, None,
        }

        assert_path! {
            bool, "bool", "bool", "bool", None, None,
        }

        assert_path! {
            f32, "f32", "f32", "f32", None, None,
        }
    }

    #[test]
    fn array_path() {
        assert_path! {
            [u8],
            "[u8]",
            "[u8]",
            "[_]",
            None,
            None,
        }

        assert_path! {
            [u8; 4],
            "[u8; 4]",
            "[u8; 4]",
            "[_; _]",
            None,
            None,
        }

        assert_path! {
            [[u8; 4]; 3],
            "[[u8; 4]; 3]",
            "[[u8; 4]; 3]",
            "[_; _]",
            None,
            None,
        }

        assert_path! {
            [&'static str; 2],
            "[&str; 2]",
            "[&str; 2]",
            "[_; _]",
            None,
            None,
        }
    }

    /// The one-element and mixed-arity cases pin down the placeholder `IDENT`
    /// form: element types become `_`, and the trailing comma of a one-tuple
    /// survives in `type_path` while `IDENT` stays `(_,)`.
    #[test]
    fn tuple_path() {
        assert_path! {
            (u8,),
            "(u8,)",
            "(u8,)",
            "(_,)",
            None,
            None,
        }

        assert_path! {
            (u8, (u8,), i32),
            "(u8, (u8,), i32)",
            "(u8, (u8,), i32)",
            "(_, _, _)",
            None,
            None,
        }
    }

    #[test]
    fn reference_path() {
        assert_path! {
            &'static u8,
            "&u8",
            "&u8",
            "&_",
            None,
            None,
        }

        assert_path! {
            &'static mut u8,
            "&mut u8",
            "&mut u8",
            "&mut _",
            None,
            None,
        }

        assert_path! {
            &'static str,
            "&str",
            "&str",
            "&_",
            None,
            None,
        }
    }
}
