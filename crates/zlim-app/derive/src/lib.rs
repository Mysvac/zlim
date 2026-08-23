#![allow(linker_messages, reason = "It's noisy and interferes with CI output")]

use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

// -----------------------------------------------------------------------------
// Modules

mod app_label;
mod zlim_main;

// -----------------------------------------------------------------------------
// Macros

/// Adjust the main function to adapt to running on multiple platforms.
///
/// Insert some initialization function and platform specific content.
///
/// # Examples
///
/// ```ignore
/// #[zlim_main]
/// fn main() {
///     // ......
/// }
/// ```
#[proc_macro_attribute]
pub fn zlim_main(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let item_fn = parse_macro_input!(item as syn::ItemFn);
    zlim_main::expand(item_fn).into()
}

/// Derives the `AppLabel` trait implementation.
///
/// # Required Traits
///
/// The target type must implement the following traits:
/// - `Clone`
/// - `Debug`
/// - `Hash`
/// - `Eq`
///
/// # Examples
///
/// ```ignore
/// #[derive(AppLabel)]
/// struct RenderApp;
/// ```
#[proc_macro_derive(AppLabel)]
pub fn derive_app_label(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    app_label::expand(ast).into()
}
