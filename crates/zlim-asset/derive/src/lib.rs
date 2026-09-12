//! Procedural macros for deriving asset traits.

use proc_macro::TokenStream;
use syn::parse_macro_input;

// -----------------------------------------------------------------------------
// Crate path resolver

/// Resolve the canonical path to the `zlim_asset` crate.
///
/// The result depends on the caller's `Cargo.toml`:
/// - If `zlim` is a dependency → `::zlim::asset`
/// - If `zlim_asset` is a direct dependency → `::zlim_asset`
/// - If `zlim` is a dev-dependency → `::zlim::asset`
/// - Otherwise falls back to `::zlim_asset`
#[inline]
pub(crate) fn zlim_asset_path() -> syn::Path {
    zlim_derive_utils::crate_path("zlim_asset")
}

// -----------------------------------------------------------------------------
// Modules

mod asset;

// -----------------------------------------------------------------------------
// Derive macros

/// Derive the `Asset` trait and its `VisitAssetDependencies` implementation.
///
/// An asset must also be `TypePath + Send + Sync + 'static`, so it is normally combined with
/// `#[derive(TypePath)]`:
///
/// ```rust, ignore
/// #[derive(Asset, TypePath)]
/// struct Material {
///     #[asset(dependency)]
///     base_color_texture: Handle<Image>,
///     roughness: f32,
/// }
/// ```
///
/// # `#[asset(dependency)]`
///
/// Fields marked with `#[asset(dependency)]` are enumerated by `visit_dependencies`.
///
/// # Generic types
///
/// The generated `Asset` impl carries no extra bounds. A generic asset therefore needs the
/// required bounds written on the type itself, exactly like a hand-written impl.
#[proc_macro_derive(Asset, attributes(asset))]
pub fn derive_asset(input: TokenStream) -> TokenStream {
    let mut ast = parse_macro_input!(input as syn::DeriveInput);
    asset::expand_asset(&mut ast).into()
}

/// Derive the `VisitAssetDependencies` trait.
///
/// Use this for a type that is stored inside an asset but is not one itself,
/// so that it can be marked as a dependency of the outer asset:
///
/// ```rust, ignore
/// #[derive(VisitAssetDependencies)]
/// struct TextureRef(#[asset(dependency)] Handle<Image>);
/// ```
///
/// The attribute and the traversal rules are the same as for [`Asset`](macro@Asset) — see that
/// macro for the details.
#[proc_macro_derive(VisitAssetDependencies, attributes(asset))]
pub fn derive_visit_asset_dependencies(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as syn::DeriveInput);
    asset::expand_visit_dependencies(&ast).into()
}
