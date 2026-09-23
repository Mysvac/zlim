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
/// A scene is the tuple of the runs it is written in, and a tuple holds at most twelve parts, so a
/// scene that alternates between statements and scenes more than twelve times is a compile error.
/// Entries of the same kind written together are one part, and so are the entities of a list.
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
/// A list is a tuple of its entities, so a list of more than twelve of them is a compile error; the
/// same holds for the parts a single scene is written in.
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
