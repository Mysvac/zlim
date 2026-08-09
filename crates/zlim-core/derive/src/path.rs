//! Centralized path management for derive macros.
//!
//! This module keeps every reference to `zlim_core`'s internal layout in one
//! place.  When items move within `zlim_core`, only the helpers in this module
//! need updating — the derive macros themselves remain unchanged.
//!
//! # Organisation
//!
//! - Re-export full-path marker types from [`zlim_derive_utils`].
//! - [`zlim_core_path`] — resolves the canonical `syn::Path` to `zlim_core`.
//! - Token-stream helpers — each takes a `&Path` to `zlim_core` and emits
//!   an absolute path into that crate.

use proc_macro2::TokenStream;
use quote::quote;
use syn::Path;

// -----------------------------------------------------------------------------
// Crate path resolver

/// Resolve the canonical path to the `zlim_core` crate.
///
/// The result depends on the caller's `Cargo.toml`:
/// - If `zlim` is a dependency → `::zlim::core`
/// - If `zlim_core` is a direct dependency → `::zlim_core`
/// - If `zlim` is a dev-dependency → `::zlim::core`
/// - Otherwise falls back to `::zlim_core`
#[inline]
pub fn zlim_core_path() -> Path {
    zlim_derive_utils::crate_path("zlim_core")
}

// -----------------------------------------------------------------------------
// Token-stream helpers — zlim_core

/// `#zlim_core::error::ZlimError`
pub fn zlim_error(zlim_core: &Path) -> TokenStream {
    quote!(#zlim_core::error::ZlimError)
}

/// `#zlim_core::bundle::Bundle`
pub fn bundle_(zlim_core: &Path) -> TokenStream {
    quote!(#zlim_core::bundle::Bundle)
}

/// `#zlim_core::bundle::DataBundle`
pub fn data_bundle_(zlim_core: &Path) -> TokenStream {
    quote!(#zlim_core::bundle::DataBundle)
}

/// `#zlim_core::bundle::ComponentCollector`
pub fn component_collector_(zlim_core: &Path) -> TokenStream {
    quote!(#zlim_core::bundle::ComponentCollector)
}

/// `#zlim_core::bundle::ComponentWriter`
pub fn component_writer_(zlim_core: &Path) -> TokenStream {
    quote!(#zlim_core::bundle::ComponentWriter)
}

/// `#zlim_core::ops::Entity`
pub fn entity_owned_(zlim_core: &Path) -> TokenStream {
    quote!(#zlim_core::ops::EntityOwned)
}

/// `#zlim_core::OwningPtr`
pub fn owning_ptr_(zlim_core: &Path) -> TokenStream {
    quote!(#zlim_core::OwningPtr)
}

/// `#zlim_core::__macro_exports__::__Reflect`
pub fn reflect_(zlim_core: &Path) -> TokenStream {
    quote!(#zlim_core::__macro_exports__::__Reflect)
}

/// `#zlim_core::__macro_exports__::__TypePath`
pub fn type_path_(zlim_core: &Path) -> TokenStream {
    quote!(#zlim_core::__macro_exports__::__TypePath)
}

/// `#zlim_core::resource::Resource`
pub fn resource_(zlim_core: &Path) -> TokenStream {
    quote!(#zlim_core::resource::Resource)
}

/// `#zlim_core::component::Component`
pub fn component_(zlim_core: &Path) -> TokenStream {
    quote!(#zlim_core::component::Component)
}

/// `#zlim_core::component::ComponentHook`
pub fn component_hook_(zlim_core: &Path) -> TokenStream {
    quote!(#zlim_core::component::ComponentHook)
}

/// `#zlim_core::clone::ComponentCloner`
pub fn component_cloner_(zlim_core: &Path) -> TokenStream {
    quote!(#zlim_core::clone::ComponentCloner)
}

/// `#zlim_core::entity::MapEntities`
pub fn map_entities_(zlim_core: &Path) -> TokenStream {
    quote!(#zlim_core::entity::MapEntities)
}

/// `#zlim_core::entity::EntityMapper`
pub fn entity_mapper_(zlim_core: &Path) -> TokenStream {
    quote!(#zlim_core::entity::EntityMapper)
}

/// `#zlim_core::__macro_exports__::__Serialize`
pub fn serialize_(zlim_core: &Path) -> TokenStream {
    quote!(#zlim_core::__macro_exports__::__Serialize)
}

/// `#zlim_core::__macro_exports__::__Deserialize`
pub fn deserialize_(zlim_core: &Path) -> TokenStream {
    quote!(#zlim_core::__macro_exports__::__Deserialize)
}
