#![allow(linker_messages, reason = "It's noisy and interferes with CI output")]

extern crate proc_macro;

use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

// -----------------------------------------------------------------------------
// Modules

mod bundle;
mod component;
mod editor;
mod error;
mod path;
mod resource;
mod utils;

// -----------------------------------------------------------------------------
// Derive macros

/// Derive macro for `core::error::Error` with optional `Display` and
/// `ZlimError` conversions.
///
/// # Generated impls
///
/// | Conditions                        | Impls emitted                                  |
/// |-----------------------------------|------------------------------------------------|
/// | Always                            | `core::error::Error`                           |
/// | `#[error("...")]`                 | `core::fmt::Display`                           |
/// | `#[zlim_error(info/warning/…)]`   | `From<Self> for ZlimError` (implies `Into<ZlimError>`) |
///
/// # `#[error(…)]`
///
/// The content inside `#[error(…)]` works like [`format!`]:
/// field names are available directly, tuple fields need a leading
/// underscore (`_0`, `_1`, …), and arbitrary expressions are
/// supported as extra arguments.
///
/// ```ignore
/// #[derive(Error)]
/// #[error("limit {limit} exceeded (max {})", i32::MAX)]
/// struct LimitError { limit: i32 }
///
/// #[derive(Error)]
/// #[error("limit {_0} exceeded (max {_1})")]
/// struct LimitError2(i32, i32);
/// ```
///
/// # Enums — defaults and overrides
///
/// Place `#[error(…)]` / `#[zlim_error(severity)]` on the enum type to set
/// a default for all variants.  Individual variants can override the default
/// with their own attribute.  If no default is provided, **every** variant
/// must carry its own annotation.
///
/// # Examples
///
/// ```ignore
/// use zlim_core_derive::Error;
///
/// #[derive(Error)]
/// #[error("something went wrong: {msg}")]
/// #[zlim_error(warning)]
/// struct MyError { msg: String }
/// ```
///
/// ```ignore
/// use zlim_core_derive::Error;
///
/// #[derive(Error)]
/// #[error("a database error occurred")]
/// #[zlim_error(error)]
/// enum DbError {
///     #[error("connection refused")]
///     ConnectionRefused,
///     #[error("query timed out after {_0} ms")]
///     #[zlim_error(warning)]
///     Timeout(u64),
///     NotFound,
/// }
/// ```
#[proc_macro_derive(Error, attributes(error, zlim_error))]
pub fn derive_error(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    error::expand(&input).into()
}

/// Derives the `Bundle` trait implementation.
///
/// This macro automatically implements the `Bundle` trait for your struct,
/// allowing it to be used as a bundle when spawning entities.
///
/// # Default Behavior
///
/// By default, all fields must implement `Bundle`.  The generated impl sets
/// `NEED_APPLY_EFFECT = true` and calls `apply_effect` on each field in
/// declaration order.
///
/// - Each field in the struct represents a sub-bundle that will be combined.
/// - Components from all fields are collected and written in declaration
///   order.
/// - If duplicate components exist across fields, later fields override
///   earlier ones.
///
/// # Data Bundles (no_effect)
///
/// Adding `#[bundle(no_effect)]` tightens the field constraint to
/// `DataBundle` (instead of `Bundle`), sets `NEED_APPLY_EFFECT = false`,
/// and also emits a `DataBundle` impl.  Use this for bundles that contain
/// only pure data with no post-spawn side effects.
///
/// # Attribute
///
/// - `#[bundle(no_effect)]` — opt out of effect processing for this bundle
///   type.
///
/// # Examples
///
/// ```ignore
/// #[derive(TypePath, Component, Clone, Serialize, Deserialize)]
/// struct Position { x: f32, y: f32 }
///
/// #[derive(TypePath, Component, Clone, Serialize, Deserialize)]
/// struct Velocity { dx: f32, dy: f32 }
///
/// // Default bundle — fields need Bundle; NEED_APPLY_EFFECT = true.
/// #[derive(Bundle)]
/// struct SpawnBundle {
///     position: Position,
///     effect: SomeEffectField,
/// }
///
/// // Data bundle — fields need DataBundle; also implements DataBundle.
/// #[derive(Bundle)]
/// #[bundle(no_effect)]
/// struct MovableBundle {
///     position: Position,
///     velocity: Velocity,
/// }
/// ```
#[proc_macro_derive(Bundle, attributes(bundle))]
pub fn derive_bundle(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    bundle::expand(ast).into()
}

/// Derives the `Component` trait implementation.
///
/// This macro automatically implements the `Component` trait for your struct.
///
/// # Attributes (type-level, inside `#[component(...)]`)
///
/// | Attribute | Description |
/// |-----------|-------------|
/// | `copy` | Use `ComponentCloner::copyable` (requires `Copy`). |
/// | `cloner = path::fn` | Use `ComponentCloner::custom(path::fn)`. |
/// | `map_entities = path::fn` | Custom `map_entities` function. |
/// | `on_add = path::fn` | `on_add` lifecycle hook. |
/// | `on_clone = path::fn` | `on_clone` lifecycle hook. |
/// | `on_insert = path::fn` | `on_insert` lifecycle hook. |
/// | `on_remove = path::fn` | `on_remove` lifecycle hook. |
/// | `on_discard = path::fn` | `on_discard` lifecycle hook. |
/// | `on_despawn = path::fn` | `on_despawn` lifecycle hook. |
///
/// - `copy` and `cloner = …` are mutually exclusive.
/// - `map_entities = …` conflicts with `#[entities]` field annotations.
///
/// # Field attributes
///
/// - `#[editor(mutable)]` / `#[editor(readonly)]` — expose field to editor
///   (requires `Reflect`).
/// - `#[entities]` — mark a field as containing entities; auto-generates
///   `map_entities` and sets `NO_ENTITY = true`.  The field type must
///   implement `MapEntities`.
///
/// # Cloner
///
/// By default, uses `ComponentCloner::clonable::<Self>()`, which requires
/// `Clone`.  Use `copy` for `Copy` types or `cloner = …` for custom
/// logic.
///
/// # Examples
///
/// ```ignore
/// #[derive(TypePath, Component, Clone, Serialize, Deserialize)]
/// #[component(on_add = Self::on_add)]
/// struct Health {
///     #[editor(mutable)]
///     value: u32,
/// }
///
/// impl Health {
///     fn on_add(world: DeferredWorld, ctx: HookContext) { /* … */ }
/// }
/// ```
#[proc_macro_derive(Component, attributes(component, editor, entities))]
pub fn derive_component(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    component::expand(ast).into()
}

/// Derives the `Resource` trait implementation.
///
/// This macro automatically implements the `Resource` trait for your struct,
/// exposing fields marked with `#[editor(mutable)]` and `#[editor(readonly)]`
/// to the editor via the `field` / `field_mut` methods.
///
/// # Field attributes
///
/// - `#[editor(mutable)]` — the field appears in `FIELDS` and
///   `MUTABLE_FIELDS`, accessible through both `field` and `field_mut`.
/// - `#[editor(readonly)]` — the field appears in `FIELDS` and
///   `READONLY_FIELDS`, accessible through `field` only.
///
/// Unmarked fields are ignored by the editor layer.
/// Every annotated field must implement `Reflect`.
///
/// # TypePath
///
/// `TypePath` is **not** derived by this macro — apply `#[derive(TypePath)]`
/// separately.  When the struct has generic parameters the generated impl
/// requires `Self: TypePath`, which transitively constrains each param.
///
/// # Examples
///
/// ```ignore
/// use zlim_core::prelude::*;
/// use zlim_reflect::TypePath;
///
/// #[derive(TypePath, Resource)]
/// struct Player {
///     name: String,
///     #[editor(mutable)]
///     health: u32,
///     #[editor(readonly)]
///     id: u64,
/// }
/// ```
#[proc_macro_derive(Resource, attributes(editor))]
pub fn derive_resource(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    resource::expand(ast).into()
}
