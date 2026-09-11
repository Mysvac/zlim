//! Provide [`TypePath`] .

use proc_macro::TokenStream;
use syn::parse_macro_input;

// -----------------------------------------------------------------------------
// zlim_path

#[inline]
pub(crate) fn zlim_path() -> syn::Path {
    zlim_derive_utils::crate_path("zlim_path")
}

// -----------------------------------------------------------------------------
// Mudules

mod string_expr;
mod type_path;

// -----------------------------------------------------------------------------
// zlim_path

/// Derive the [`TypePath`] trait for a type.
///
/// # Default behaviour
///
/// Without any attributes the macro uses `module_path!()` and the Rust
/// identifier to build the required items:
///
/// ```rust, ignore
/// #[derive(TypePath)]
/// struct Foo;
///
/// // Generates:
/// // - type_path()     → "{module}::Foo"
/// // - type_name()     → "Foo"
/// // - const IDENT: &str    = "Foo";
/// // - const MODULE: Option<&str> = Some("{module}");
/// // - const CRATE: Option<&str>  = first segment of {module};
/// ```
///
/// # Custom path
///
/// Use `#[type_path = "..."]` to override the full path prefix:
///
/// ```rust, ignore
/// #[derive(TypePath)]
/// #[type_path = "my_crate::bar::Baz"]
/// struct Foo;
///
/// // Generates:
/// // - type_path()              → "my_crate::bar::Baz"
/// // - type_name()              → "Baz"
/// // - const IDENT: &str        = "Baz";
/// // - const MODULE: Option<&str> = Some("my_crate::bar");
/// // - const CRATE: Option<&str>  = Some("my_crate");
/// ```
///
/// # Generic types
///
/// Type and const generic parameters are automatically included in
/// `type_path()` and `type_name()` via `PathCell` caching:
///
/// ```rust, ignore
/// #[derive(TypePath)]
/// struct MyVec<T> { /* ... */ }
///
/// // for T = Vec<i32>:
/// // type_path()  → "{module}::MyVec<alloc::vec::Vec<i32>>"
/// // type_name()  → "MyVec<Vec<i32>>"
/// // - const IDENT: &str        = "MyVec";
/// // - const MODULE: Option<&str> = Some("{module}");
/// // - const CRATE: Option<&str>  = first segment of {module};
/// ```
///
/// # Generic types Custom path
///
/// Use `#[type_path = "..."]` to override the full path prefix, no need generic params:
///
/// ```rust, ignore
/// #[derive(TypePath)]
/// #[type_path = "a::vec::Vec"]
/// struct MyVec<T> { /* ... */ }
///
/// // for T = Vec<i32>:
/// // type_path()  → "a::vec::Vec<alloc::vec::Vec<i32>>"
/// // type_name()  → "Vec<Vec<i32>>"
/// // - const IDENT: &str        = "Vec";
/// // - const MODULE: Option<&str> = Some("a::vec");
/// // - const CRATE: Option<&str>  = Some("a");
/// ```
#[proc_macro_derive(TypePath, attributes(type_path))]
pub fn derive_type_path(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    let zlim_path = zlim_path();
    type_path::expand_type_path(&input, &zlim_path).into()
}
