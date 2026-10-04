//! Turning a parsed scene into the tokens that build it.

use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;

use crate::parse::{Entry, Field, Scene, SceneList, TemplateType, Value};

// -----------------------------------------------------------------------------
// Context

/// What one macro invocation shares while it generates code.
pub struct Ctx {
    /// The path of the `zlim_scene` crate, as the caller knows it.
    pub scene: syn::Path,

    /// The file the invocation was written in, which is part of a name's identity.
    pub file: String,

    /// The line the invocation was written at.
    pub line: usize,

    /// The column the invocation was written at.
    pub column: usize,

    /// The invocation as a whole, for the errors that are about the whole body rather than about
    /// one entry.
    pub span: Span,

    /// The names declared or mentioned so far, in the order they were first seen: the ordinal is
    /// part of a reference's identity, so a name has to keep the ordinal it was given.
    pub names: Vec<Ident>,
}

impl Ctx {
    /// Returns the ordinal of `name`, remembering it if it is new.
    fn name_index(&mut self, name: &Ident) -> usize {
        match self.names.iter().position(|known| known == name) {
            Some(index) => index,
            None => {
                self.names.push(name.clone());
                self.names.len() - 1
            }
        }
    }

    /// Returns the tokens the reference of `index` is built from.
    ///
    /// The file, line and column are the invocation's, so the references of one `scn!` never collide
    /// with those of another; `__call_id` tells two runs of the same invocation apart.
    fn reference(&self, index: usize) -> TokenStream {
        let scene = &self.scene;
        let file = &self.file;
        let line = self.line as u32;
        let column = self.column as u32;

        quote! {
            #scene::__macro_exports__::EntityReference::new(#file, #line, #column, #index, __call_id)
        }
    }
}

// -----------------------------------------------------------------------------
// Roots

/// Generates the scene of `scn!`.
pub fn scene_root(root: Scene, ctx: &mut Ctx) -> syn::Result<TokenStream> {
    let body = scene(&root, ctx)?;
    let scene = &ctx.scene;

    Ok(quote! {
        #scene::SceneScope({
            static __CALL_ID: #scene::__macro_exports__::CallCounter = #scene::__macro_exports__::CallCounter::new();
            let __call_id = __CALL_ID.increment();
            #body
        })
    })
}

/// Generates the scene list of `scn_list!`.
pub fn scene_list_root(root: SceneList, ctx: &mut Ctx) -> syn::Result<TokenStream> {
    let body = list(&root, ctx)?;
    let scene = &ctx.scene;

    Ok(quote! {
        #scene::SceneListScope({
            static __CALL_ID: #scene::__macro_exports__::CallCounter = #scene::__macro_exports__::CallCounter::new();
            let __call_id = __CALL_ID.increment();
            #body
        })
    })
}

// -----------------------------------------------------------------------------
// Scenes

/// The code of one run of entries: consecutive entries of the same kind are generated together.
enum Run {
    /// Entries that edit the scene as it is being resolved, which one closure can do in order.
    Statements(Vec<TokenStream>),

    /// Entries that are scenes of their own, resolved in place of the run.
    Scenes(Vec<TokenStream>),
}

/// Generates the scene of one entity: a tuple of the pieces that describe it, in entry order.
///
/// The order is what the tuple is for: a scene is composed of its entries, so an entry is resolved
/// after the ones written before it. Statements are collected into one closure per run, and a run of
/// scenes becomes a tuple of its own.
///
/// A composition longer than a tuple can hold is grouped rather than refused; see [`push_nested`].
fn scene(scene: &Scene, ctx: &mut Ctx) -> syn::Result<TokenStream> {
    let zlim_scene = ctx.scene.clone();
    let mut runs: Vec<Run> = Vec::new();

    for entry in &scene.entries {
        match entry {
            Entry::Name(name) => {
                let index = ctx.name_index(name);
                let reference = ctx.reference(index);
                statement(
                    &mut runs,
                    quote! { _scene.add_entity_reference(#reference); },
                );
            }
            Entry::Children(children) => {
                let children = list(children, ctx)?;
                scene_piece(&mut runs, quote! { #zlim_scene::SceneChildren(#children) });
            }
            Entry::Parent(parent) => {
                let value = value(parent, ctx);
                scene_piece(&mut runs, quote! { #zlim_scene::SceneParent::new(#value) });
            }
            Entry::TemplateValue(tokens) => {
                statement(&mut runs, quote! { _scene.insert_template(#tokens); });
            }
            Entry::Patch { template, fields } => {
                let ty = template_type(template, ctx);
                let assigns = fields.iter().map(|field| assign(field, ctx));
                statement(
                    &mut runs,
                    quote! {
                        {
                            let __template = _scene.get_or_init_template::<#ty>();
                            #(#assigns)*
                        }
                    },
                );
            }
            Entry::Constructor {
                template,
                function,
                args,
            } => {
                let path = &template.path;
                let tokens = if template.raw {
                    quote! { #path::#function(#args) }
                } else {
                    quote! { <#path as #zlim_scene::__macro_exports__::IntoTemplate>::Template::#function(#args) }
                };
                statement(&mut runs, quote! { _scene.insert_template(#tokens); });
            }
            Entry::Init(template) => {
                let ty = template_type(template, ctx);
                statement(
                    &mut runs,
                    quote! { let _ = _scene.get_or_init_template::<#ty>(); },
                );
            }
            Entry::CachedScene(path) => {
                scene_piece(
                    &mut runs,
                    quote! { #zlim_scene::CachedSceneAsset::new(#path) },
                );
            }
            Entry::Scene(scene) => {
                scene_piece(&mut runs, quote! { #scene });
            }
        }
    }

    let parts = runs.into_iter().map(|run| match run {
        Run::Statements(statements) => quote! {
            #zlim_scene::SceneFunction(move |_context, _scene| {
                #(#statements)*
            })
        },
        Run::Scenes(mut scenes) => match scenes.len() {
            1 => scenes.pop().expect("the run holds one scene"),
            _ => quote!((#(#scenes,)*)),
        },
    });

    let parts = parts.collect::<Vec<_>>();
    let mut composed = TokenStream::new();
    push_nested(&mut composed, &parts, ctx)?;

    Ok(composed)
}

/// Adds a statement to the current run, starting one if the last run was a scene.
fn statement(runs: &mut Vec<Run>, tokens: TokenStream) {
    match runs.last_mut() {
        Some(Run::Statements(statements)) => statements.push(tokens),
        _ => runs.push(Run::Statements(vec![tokens])),
    }
}

/// Adds a scene to the current run, starting one if the last run was a statement.
fn scene_piece(runs: &mut Vec<Run>, tokens: TokenStream) {
    match runs.last_mut() {
        Some(Run::Scenes(scenes)) => scenes.push(tokens),
        _ => runs.push(Run::Scenes(vec![tokens])),
    }
}

/// Generates a scene list: one `EntityScene` per entity, in order.
///
/// Like a scene, a list longer than a tuple can hold is grouped rather than refused; see
/// [`push_nested`].
fn list(list: &SceneList, ctx: &mut Ctx) -> syn::Result<TokenStream> {
    let zlim_scene = ctx.scene.clone();
    let scenes = list
        .scenes
        .iter()
        .map(|entity| {
            let entity = scene(entity, ctx)?;
            Ok(quote! { #zlim_scene::EntityScene(#entity) })
        })
        .collect::<syn::Result<Vec<_>>>()?;

    let mut composed = TokenStream::new();
    push_nested(&mut composed, &scenes, ctx)?;

    Ok(composed)
}

// -----------------------------------------------------------------------------
// Tuples

/// The longest tuple `zlim-scene` implements [`Scene`](zlim_scene::Scene) and
/// [`SceneList`](zlim_scene::SceneList) for.
///
/// This mirrors the `range_invoke!` of the crate's `tuple` module. A composition longer than this is
/// grouped; see [`MAX_COMPOSITION`].
const MAX_PARTS: usize = 12;

/// How many parts go into a group when the parts do not fit in one tuple.
const CHUNK: usize = 8;

/// The most parts one entity of a scene, or one list of them, can be written with.
///
/// One level of grouping is what a composition gets: [`MAX_PARTS`] groups of [`CHUNK`] parts, which
/// is ninety-six. That is the length a scene or a list is written with, and past it the answer is
/// not more nesting but fewer entities — so the macro says so, with the count, instead of leaving
/// the trait solver to complain about a tuple of a hundred scenes.
///
/// It also bounds the nesting: two levels of tuple are as deep as any composition of this macro
/// goes, whatever it holds.
const MAX_COMPOSITION: usize = MAX_PARTS * CHUNK;

/// Writes `parts` as a scene or a list composition.
///
/// Up to [`MAX_PARTS`] parts become one tuple, which is the common case and generates exactly what a
/// flat composition always did. Past that the parts are grouped [`CHUNK`] at a time, and the groups
/// become the parts of one more tuple — two levels, since the grouping only ever has to happen once.
///
/// The groups are tuples of the same pieces in the same order, so a nested composition is resolved
/// exactly as a flat one would be.
///
/// # Errors
///
/// Fails for a composition past [`MAX_COMPOSITION`], which no two levels of tuple hold.
fn push_nested(out: &mut TokenStream, parts: &[TokenStream], ctx: &Ctx) -> syn::Result<()> {
    if parts.len() > MAX_COMPOSITION {
        return Err(too_many(parts.len(), ctx));
    }

    if parts.len() <= MAX_PARTS {
        write_group(out, parts);
        return Ok(());
    }

    let groups = parts.chunks(CHUNK).map(|group| {
        let mut tuple = TokenStream::new();
        write_tuple(&mut tuple, group);
        tuple
    });

    write_tuple(out, &groups.collect::<Vec<_>>());
    Ok(())
}

/// Writes `parts` as one tuple into `out`.
///
/// A single part is written bare: it is the same composition without a layer of nesting that
/// resolves to nothing.
fn write_group(out: &mut TokenStream, parts: &[TokenStream]) {
    match parts {
        [] => {}
        [only] => out.extend(only.clone()),
        _ => write_tuple(out, parts),
    }
}

/// Writes `parts` as one tuple into `out`, a single part included.
///
/// A group of the grouping has to stay a tuple even when it is the only one: written bare, the level
/// above would hold that group's parts instead of the group, and a composition of ninety-seven would
/// come out as a tuple of thirteen.
fn write_tuple(out: &mut TokenStream, parts: &[TokenStream]) {
    out.extend(quote!((#(#parts),*)));
}

/// Builds the error of a composition too long to be written at all.
fn too_many(parts: usize, ctx: &Ctx) -> syn::Error {
    syn::Error::new(
        ctx.span,
        format!(
            "this composition has {parts} parts, and one scene or list holds at most \
             {MAX_COMPOSITION}: split it into several entities — a `Children [...]` list is resolved \
             as a composition of its own, and each of its entities has the whole allowance again"
        ),
    )
}

// -----------------------------------------------------------------------------
// Pieces

/// Generates the type a canonical template slot is addressed by.
fn template_type(template: &TemplateType, ctx: &Ctx) -> TokenStream {
    let scene = &ctx.scene;
    let path = &template.path;

    if template.raw {
        quote!(#path)
    } else {
        quote!(<#path as #scene::__macro_exports__::IntoTemplate>::Template)
    }
}

/// Generates one field assignment of a patch.
fn assign(field: &Field, ctx: &mut Ctx) -> TokenStream {
    let member = &field.member;
    let value = value(&field.value, ctx);
    quote! { __template.#member = #value; }
}

/// Generates a value: a name becomes the entity it stands for, anything else is passed through.
fn value(value: &Value, ctx: &mut Ctx) -> TokenStream {
    match value {
        Value::Name(name) => {
            let index = ctx.name_index(name);
            let reference = ctx.reference(index);
            let scene = &ctx.scene;
            quote! { #scene::__macro_exports__::EntityTemplate::EntityReference(#reference) }
        }
        Value::Tokens(tokens) => tokens.clone(),
    }
}

// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{Ctx, MAX_COMPOSITION, MAX_PARTS, push_nested};
    use proc_macro2::{Delimiter, TokenStream, TokenTree};

    /// A context with nothing in it: the count is all the macro needs to report a composition that
    /// is too long, and the grouping never looks at the invocation.
    fn context() -> Ctx {
        Ctx {
            scene: syn::parse_quote!(::zlim_scene),
            file: String::new(),
            line: 0,
            column: 0,
            span: proc_macro2::Span::call_site(),
            names: Vec::new(),
        }
    }

    /// The deepest tuple `tokens` holds.
    fn depth(tokens: &TokenStream) -> usize {
        let mut deepest = 0;

        for token in tokens.clone() {
            if let TokenTree::Group(group) = token {
                let below = match group.delimiter() {
                    Delimiter::Parenthesis => depth(&group.stream()),
                    _ => 0,
                };
                deepest = deepest.max(1 + below);
            }
        }

        deepest
    }

    /// Returns how many items the tuples of `tokens` hold at most.
    fn longest_tuple(tokens: &TokenStream) -> usize {
        let mut longest = 0;

        for token in tokens.clone() {
            if let TokenTree::Group(group) = token {
                let inner = group.stream();
                let below = longest_tuple(&inner);

                if group.delimiter() == Delimiter::Parenthesis {
                    let commas = inner
                        .clone()
                        .into_iter()
                        .filter(|token| token.to_string() == ",")
                        .count();
                    let items = commas + usize::from(!inner.to_string().ends_with(','));
                    longest = longest.max(items);
                }

                longest = longest.max(below);
            }
        }

        longest
    }

    /// Composes `count` parts.
    fn compose(count: usize) -> syn::Result<TokenStream> {
        let parts = (0..count)
            .map(|i| {
                let name = quote::format_ident!("part_{i}");
                quote::quote!(#name)
            })
            .collect::<Vec<_>>();

        let mut out = TokenStream::new();
        push_nested(&mut out, &parts, &context())?;
        Ok(out)
    }

    /// Composes `count` parts that the macro accepts.
    fn compose_ok(count: usize) -> TokenStream {
        compose(count).unwrap_or_else(|error| panic!("{count} parts do not compose: {error}"))
    }

    /// No composition the macro accepts is a tuple longer than the implementations cover, at any
    /// length — the grouping is the only thing standing between a long scene and a tuple the traits
    /// do not cover.
    #[test]
    fn a_composition_is_never_over_the_tuple_limit() {
        for count in [
            0,
            1,
            2,
            MAX_PARTS,
            MAX_PARTS + 1,
            MAX_COMPOSITION - 1,
            MAX_COMPOSITION,
        ] {
            let tokens = compose_ok(count);

            assert!(
                longest_tuple(&tokens) <= MAX_PARTS,
                "{count} parts made a tuple of {}",
                longest_tuple(&tokens)
            );
        }

        assert_eq!(compose_ok(0).to_string(), "", "an empty scene is nothing");
        assert_eq!(compose_ok(1).to_string(), "part_0", "one part is itself");
    }

    /// Up to the limit a composition is one tuple, exactly as it was before grouping existed.
    #[test]
    fn a_composition_within_the_limit_is_a_flat_tuple() {
        assert_eq!(depth(&compose_ok(2)), 1);
        assert_eq!(depth(&compose_ok(MAX_PARTS)), 1);
    }

    /// Past the limit the composition is grouped — one level of it, since a level of `MAX_PARTS`
    /// groups of `CHUNK` is the whole allowance.
    #[test]
    fn a_composition_past_the_limit_is_nested_once() {
        assert_eq!(depth(&compose_ok(MAX_PARTS + 1)), 2);
        assert_eq!(depth(&compose_ok(MAX_COMPOSITION)), 2);
    }

    /// A composition past what a scene or a list holds is an error, not a tuple the trait solver
    /// cannot resolve. The error says the count and what to do about it.
    #[test]
    fn a_composition_past_the_limit_is_an_error() {
        let over = MAX_COMPOSITION + 1;
        let error = compose(over).expect_err("one part too many is refused");

        assert!(
            error.to_string().contains(&over.to_string()),
            "the error counts the parts: {error}"
        );
        assert!(
            error.to_string().contains(&MAX_COMPOSITION.to_string()),
            "the error says how many fit: {error}"
        );
    }
}
