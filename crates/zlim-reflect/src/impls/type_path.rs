//! The macro a plain [`TypePath`] implementation is written with.
//!
//! Most of the types this module covers are not reflected: they only need a stable name, which is
//! what this macro spells out — the path, the short name, the identifier, the crate and the module,
//! in the three shapes a type comes in (a plain type, one generic parameter, and two or three).
//!
//! [`impl_reflect!`](zlim_reflect_derive::impl_reflect) is what a reflected type is written with
//! instead: it produces this implementation too, together with the reflection itself.

/// Implements [`TypePath`] for a type whose path is spelled out here.
///
/// The arms are, in order: a non-generic type, one with a single generic parameter, and one with
/// two or three; the path is given as the crate, the module inside it, and the type name.
macro_rules! impl_simple_type_path {
    ($ty:ty: $primitive:literal) => {
        impl $crate::path::TypePath for $ty {
            #[inline(always)]
            fn type_path() -> &'static str {
                $primitive
            }
            #[inline(always)]
            fn type_name() -> &'static str {
                $primitive
            }

            const IDENT: &'static str = $primitive;
            const CRATE: Option<&'static str> = None;
            const MODULE: Option<&'static str> = None;
        }
    };
    ($ty:ty: $cpath:literal, $primitive:literal) => {
        impl $crate::path::TypePath for $ty {
            #[inline(always)]
            fn type_path() -> &'static str {
                ::core::concat!($cpath, "::", $primitive)
            }
            #[inline(always)]
            fn type_name() -> &'static str {
                $primitive
            }

            const IDENT: &'static str = $primitive;
            const CRATE: Option<&'static str> = Some($cpath);
            const MODULE: Option<&'static str> = Some($cpath);
        }
    };
    ($ty:ty: $cpath:literal, $mpath:literal, $primitive:literal) => {
        impl $crate::path::TypePath for $ty {
            #[inline(always)]
            fn type_path() -> &'static str {
                ::core::concat!($cpath, "::", $mpath, "::", $primitive)
            }
            #[inline(always)]
            fn type_name() -> &'static str {
                $primitive
            }

            const IDENT: &'static str = $primitive;
            const CRATE: Option<&'static str> = Some($cpath);
            const MODULE: Option<&'static str> = Some(::core::concat!($cpath, "::", $mpath));
        }
    };

    (@$ty:ident<$g:ident>: $cpath:literal, $mpath:literal, $primitive:literal) => {
        impl<$g: $crate::path::TypePath> $crate::path::TypePath for $ty<$g> {
            fn type_path() -> &'static str {
                static CELL: $crate::path::PathCell = $crate::path::PathCell::new();
                CELL.get_or_init::<Self>(|| {
                    $crate::path::concat(&[
                        ::core::concat!($cpath, "::", $mpath),
                        "::",
                        $primitive,
                        "<",
                        <$g>::type_path(),
                        ">",
                    ])
                })
            }

            fn type_name() -> &'static str {
                static CELL: $crate::path::PathCell = $crate::path::PathCell::new();
                CELL.get_or_init::<Self>(|| {
                    $crate::path::concat(&[$primitive, "<", <$g>::type_name(), ">"])
                })
            }

            const IDENT: &'static str = $primitive;
            const CRATE: Option<&'static str> = Some($cpath);
            const MODULE: Option<&'static str> = Some(::core::concat!($cpath, "::", $mpath));
        }
    };

    (@$ty:ident<$g1:ident, $g2:ident>: $cpath:literal, $mpath:literal, $primitive:literal) => {
        impl<$g1: $crate::path::TypePath, $g2: $crate::path::TypePath> $crate::path::TypePath
            for $ty<$g1, $g2>
        {
            fn type_path() -> &'static str {
                static CELL: $crate::path::PathCell = $crate::path::PathCell::new();
                CELL.get_or_init::<Self>(|| {
                    $crate::path::concat(&[
                        ::core::concat!($cpath, "::", $mpath),
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
                static CELL: $crate::path::PathCell = $crate::path::PathCell::new();
                CELL.get_or_init::<Self>(|| {
                    $crate::path::concat(&[
                        $primitive,
                        "<",
                        <$g1>::type_name(),
                        ", ",
                        <$g2>::type_name(),
                        ">",
                    ])
                })
            }

            const IDENT: &'static str = $primitive;
            const CRATE: Option<&'static str> = Some($cpath);
            const MODULE: Option<&'static str> = Some(::core::concat!($cpath, "::", $mpath));
        }
    };

    (@$ty:ident<$g1:ident, $g2:ident, $g3:ident>: $cpath:literal, $mpath:literal, $primitive:literal) => {
        impl<$g1: $crate::path::TypePath, $g2: $crate::path::TypePath, $g3: $crate::path::TypePath>
            $crate::path::TypePath for $ty<$g1, $g2, $g3>
        {
            fn type_path() -> &'static str {
                static CELL: $crate::path::PathCell = $crate::path::PathCell::new();
                CELL.get_or_init::<Self>(|| {
                    $crate::path::concat(&[
                        ::core::concat!($cpath, "::", $mpath),
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
                static CELL: $crate::path::PathCell = $crate::path::PathCell::new();
                CELL.get_or_init::<Self>(|| {
                    $crate::path::concat(&[
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

            const IDENT: &'static str = $primitive;
            const CRATE: Option<&'static str> = Some($cpath);
            const MODULE: Option<&'static str> = Some(::core::concat!($cpath, "::", $mpath));
        }
    };
}

pub(crate) use impl_simple_type_path;
