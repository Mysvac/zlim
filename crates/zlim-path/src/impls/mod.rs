mod primitive;

mod alloc;
mod core;
mod std;

mod zlim_utils;

#[cfg(feature = "uuid")]
mod uuid;

#[cfg(feature = "glam")]
mod glam;

macro_rules! impl_simple_type_path {
    ($ty:ty: $primitive:literal) => {
        impl TypePath for $ty {
            #[inline(always)]
            fn type_path() -> &'static str {
                $primitive
            }
            #[inline(always)]
            fn type_name() -> &'static str {
                $primitive
            }

            const IDENT: &str = $primitive;
            const CRATE: Option<&str> = None;
            const MODULE: Option<&str> = None;
        }
    };
    ($ty:ty: $cpath:literal, $primitive:literal) => {
        impl TypePath for $ty {
            #[inline(always)]
            fn type_path() -> &'static str {
                concat!($cpath, "::", $primitive)
            }
            #[inline(always)]
            fn type_name() -> &'static str {
                $primitive
            }

            const IDENT: &str = $primitive;
            const CRATE: Option<&str> = Some($cpath);
            const MODULE: Option<&str> = Some($cpath);
        }
    };
    ($ty:ty: $cpath:literal, $mpath:literal, $primitive:literal) => {
        impl TypePath for $ty {
            #[inline(always)]
            fn type_path() -> &'static str {
                concat!($cpath, "::", $mpath, "::", $primitive)
            }
            #[inline(always)]
            fn type_name() -> &'static str {
                $primitive
            }

            const IDENT: &str = $primitive;
            const CRATE: Option<&str> = Some($cpath);
            const MODULE: Option<&str> = Some(concat!($cpath, "::", $mpath));
        }
    };

    (@$ty:ident<$g:ident>: $cpath:literal, $mpath:literal, $primitive:literal) => {
        impl<$g: TypePath> TypePath for $ty<$g> {
            fn type_path() -> &'static str {
                static CELL: PathCell = PathCell::new();
                CELL.get_or_init::<Self>(|| {
                    concat(&[
                        concat!($cpath, "::", $mpath),
                        "::",
                        $primitive,
                        "<",
                        <$g>::type_path(),
                        ">",
                    ])
                })
            }

            fn type_name() -> &'static str {
                static CELL: PathCell = PathCell::new();
                CELL.get_or_init::<Self>(|| concat(&[$primitive, "<", <$g>::type_name(), ">"]))
            }

            const IDENT: &str = $primitive;
            const CRATE: Option<&str> = Some($cpath);
            const MODULE: Option<&str> = Some(concat!($cpath, "::", $mpath));
        }
    };

    (@$ty:ident<$g1:ident, $g2:ident>: $cpath:literal, $mpath:literal, $primitive:literal) => {
        impl<$g1: TypePath, $g2: TypePath> TypePath for $ty<$g1, $g2> {
            fn type_path() -> &'static str {
                static CELL: PathCell = PathCell::new();
                CELL.get_or_init::<Self>(|| {
                    concat(&[
                        concat!($cpath, "::", $mpath),
                        "::",
                        $primitive,
                        "<",
                        <$g1>::type_path(),
                        ", ",
                        <$g2>::type_path(),
                        ">",
                    ])
                })
            }

            fn type_name() -> &'static str {
                static CELL: PathCell = PathCell::new();
                CELL.get_or_init::<Self>(|| {
                    concat(&[
                        $primitive,
                        "<",
                        <$g1>::type_name(),
                        ", ",
                        <$g2>::type_name(),
                        ">",
                    ])
                })
            }

            const IDENT: &str = $primitive;
            const CRATE: Option<&str> = Some($cpath);
            const MODULE: Option<&str> = Some(concat!($cpath, "::", $mpath));
        }
    };

    (@$ty:ident<$g1:ident, $g2:ident, $g3:ident>: $cpath:literal, $mpath:literal, $primitive:literal) => {
        impl<$g1: TypePath, $g2: TypePath, $g3: TypePath> TypePath for $ty<$g1, $g2, $g3> {
            fn type_path() -> &'static str {
                static CELL: PathCell = PathCell::new();
                CELL.get_or_init::<Self>(|| {
                    concat(&[
                        concat!($cpath, "::", $mpath),
                        "::",
                        $primitive,
                        "<",
                        <$g1>::type_path(),
                        ", ",
                        <$g2>::type_path(),
                        ", ",
                        <$g3>::type_path(),
                        ">",
                    ])
                })
            }

            fn type_name() -> &'static str {
                static CELL: PathCell = PathCell::new();
                CELL.get_or_init::<Self>(|| {
                    concat(&[
                        $primitive,
                        "<",
                        <$g1>::type_name(),
                        ", ",
                        <$g2>::type_name(),
                        ", ",
                        <$g3>::type_name(),
                        ">",
                    ])
                })
            }

            const IDENT: &str = $primitive;
            const CRATE: Option<&str> = Some($cpath);
            const MODULE: Option<&str> = Some(concat!($cpath, "::", $mpath));
        }
    };
}

use impl_simple_type_path;
