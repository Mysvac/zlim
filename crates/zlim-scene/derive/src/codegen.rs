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
/// scenes becomes a tuple of its own, so that a scene of a few dozen entries still fits the tuple
/// implementations.
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
                            let __template = _scene.get_or_insert_template::<#ty>();
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
                    quote! { let _ = _scene.get_or_insert_template::<#ty>(); },
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

    fits(parts.len(), ctx, PARTS_HINT)?;

    Ok(match parts.len() {
        0 => quote!(()),
        1 => parts.into_iter().next().expect("there is one part"),
        _ => quote!((#(#parts,)*)),
    })
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

    fits(scenes.len(), ctx, ENTITIES_HINT)?;

    Ok(quote!((#(#scenes,)*)))
}

// -----------------------------------------------------------------------------
// Tuples

/// The longest tuple `zlim-scene` implements [`Scene`](zlim_scene::Scene) and
/// [`SceneList`](zlim_scene::SceneList) for.
///
/// This mirrors the `range_invoke!` of the crate's `tuple` module. A composition longer than this is
/// not a composition at all, so it is reported here — with a count and a way out — instead of being
/// left to the trait solver, whose complaint would be about a tuple of thirteen scenes.
const MAX_PARTS: usize = 12;

/// What a part of a scene is, for the error of a scene that has too many of them.
const PARTS_HINT: &str = "a scene is the tuple of the runs it is written in: a run of statements \
                          (`#Name`, a template edit, `~…`) is one part, and a run of scenes \
                          (`Children`, `Parent`, `:`, `@`) is another. Write the entries of each \
                          kind together, or move some of them into a nested entity";

/// What a part of a scene list is, for the error of a list that has too many of them.
const ENTITIES_HINT: &str = "a scene list is the tuple of its entities: one entity is one part. \
                             Nest some of them under a `Children` list, or spawn them as several \
                             scenes";

/// Reports a composition whose parts do not fit in a tuple, and does nothing when they do.
fn fits(parts: usize, ctx: &Ctx, hint: &str) -> syn::Result<()> {
    if parts <= MAX_PARTS {
        return Ok(());
    }

    Err(syn::Error::new(
        ctx.span,
        format!(
            "this composition has {parts} parts, and a tuple holds at most {MAX_PARTS}: {hint}"
        ),
    ))
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
