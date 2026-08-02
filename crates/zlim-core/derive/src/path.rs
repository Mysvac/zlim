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

// ----------------------------------------------------------------------------
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

// ----------------------------------------------------------------------------
// Token-stream helpers — zlim_core

/// `#zlim_core::error::ZlimError`
pub fn zlim_error(zlim_core: &Path) -> TokenStream {
    quote!(#zlim_core::error::ZlimError)
}
