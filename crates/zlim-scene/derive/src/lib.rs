//! The `scn!` and `scn_list!` macros of `zlim-scene`.
//!
//! See the `zlim-scene` crate documentation for the syntax: the grammar is in the `parse` module,
//! and what it turns into in the `codegen` module.

use proc_macro::TokenStream;
use syn::parse_macro_input;

mod codegen;
mod parse;

use codegen::Ctx;
use parse::{SceneListRoot, SceneRoot};

/// Describes one entity, in the `scn` syntax.
///
/// ```ignore
/// scn! {
///     #Root
///     Sprite { color: RED }
///     Children [
///         #Child
///         Sprite { color: BLUE }
///         --
///         Link { to: #Root }
///     ]
/// }
/// ```
///
/// A scene can build on a cached scene asset, by naming it first, and include a scene of its own
/// anywhere else:
///
/// ```ignore
/// scn! {
///     : "scenes/player.scene"     // applied first; `Health.current` is cloned out of it
///     Health { current: 5 }       // edits the cached template, leaving `max` alone
///     @ Link { to: #Other }       // a scene included where it is written
/// }
/// ```
///
/// See the `zlim-scene` crate documentation for the whole grammar.
///
/// # How much fits
///
/// One entity holds at most **ninety-six** parts. A *part* is a run of entries of one kind:
/// statements written together are one part, scenes written together are another, and a nested
/// entity is one part however much is written inside it. Up to twelve parts are written as one
/// tuple; past that they are grouped eight at a time, which is where the ninety-six comes from.
/// Writing more than that is a compile error, and the scene wants part of its description moved into
/// a nested entity — which has the whole allowance again.
#[proc_macro]
pub fn scn(input: TokenStream) -> TokenStream {
    let root = parse_macro_input!(input as SceneRoot);
    let mut ctx = context();

    match codegen::scene_root(root.0, &mut ctx) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Describes several entities, in the `scn` syntax, separated by `--`.
///
/// The entities share one name scope, so a `#Name` of one resolves for the others.
///
/// # How much fits
///
/// A list holds at most **ninety-six** entities, since an entity is one part of the list. Up to
/// twelve of them are written as one tuple; past that they are grouped eight at a time, which is
/// where the ninety-six comes from. A longer list is a compile error, and wants splitting into
/// several lists.
#[proc_macro]
pub fn scn_list(input: TokenStream) -> TokenStream {
    let root = parse_macro_input!(input as SceneListRoot);
    let mut ctx = context();

    match codegen::scene_list_root(root.0, &mut ctx) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// The context of one invocation: the crate path, and where the invocation was written.
fn context() -> Ctx {
    let scene = zlim_scene_path();
    let call_site = proc_macro::Span::call_site();
    let file = call_site.file();
    let line = call_site.line();
    let column = call_site.column();

    Ctx {
        scene,
        file,
        line,
        column,
        span: proc_macro2::Span::from(call_site),
        names: Vec::new(),
    }
}

/// Resolves the path of the `zlim_scene` crate as the caller knows it.
fn zlim_scene_path() -> syn::Path {
    zlim_derive_utils::crate_path("zlim_scene")
}
