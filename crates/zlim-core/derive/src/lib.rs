#![allow(linker_messages, reason = "It's noisy and interferes with CI output")]

use proc_macro::TokenStream;
use syn::{DeriveInput, ItemFn, parse_macro_input};

// -----------------------------------------------------------------------------
// Modules

mod bundle;
mod component;
mod editor;
mod error;
mod job;
mod job_group;
mod message;
mod path;
mod query_data;
mod resource;
mod schedule;
mod schedule_stage;
mod system_param;
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
/// with their own attribute.
///
/// `#[error(…)]`: if no default is provided, **every** variant must carry
/// its own `#[error(…)]` annotation.
///
/// `#[zlim_error(severity)]`: valid severities are `ignore`, `debug`, `info`,
/// `warning`, `error`, and `panic`.  If no `#[zlim_error]` appears on either
/// the enum or any variant, no `From` impl is generated (and no error is
/// raised).  If only some variants carry it, an enum-level default is
/// required and used as the fallback.
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
/// `NEED_APPLY_EFFECT` to the logical OR of all field types' flags — a
/// bundle needs `apply_effect` only when at least one of its sub-bundles
/// does — and calls `apply_effect` on each field in declaration order.
///
/// - Each field in the struct represents a sub-bundle that will be combined.
/// - Components from all fields are collected and written in declaration
///   order.
/// - If duplicate components exist across fields, later fields override
///   earlier ones.
///
/// # Data Bundles (`#[bundle(data)]`)
///
/// Adding `#[bundle(data)]` tightens the field constraint to `DataBundle`
/// (instead of `Bundle`) and also emits an explicit `DataBundle` impl, which
/// requires `NEED_APPLY_EFFECT = false`.  Use this for bundles that contain
/// only pure data with no post-spawn side effects.
///
/// # Attribute
///
/// - `#[bundle(data)]` — mark the type as a pure-data bundle; every field
///   must be a `DataBundle`, and the type itself implements `DataBundle`.
///
/// # Limitations
///
/// - Only structs are supported; enums and unions are rejected.
/// - For generic types the generated impl adds
///   `Self: Send + Sync + Sized + 'static`.
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
/// // Default bundle — fields need Bundle; `NEED_APPLY_EFFECT` is the OR of
/// // the fields' flags.
/// #[derive(Bundle)]
/// struct SpawnBundle {
///     position: Position,
///     effect: SomeEffectField,
/// }
///
/// // Data bundle — fields need DataBundle; also implements DataBundle.
/// #[derive(Bundle)]
/// #[bundle(data)]
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
/// | `map_entities = path::fn` | Custom `map_entities` function; see below. |
/// | `on_add = path::fn` | `on_add` lifecycle hook. |
/// | `on_clone = path::fn` | `on_clone` lifecycle hook. |
/// | `on_insert = path::fn` | `on_insert` lifecycle hook. |
/// | `on_remove = path::fn` | `on_remove` lifecycle hook. |
/// | `on_discard = path::fn` | `on_discard` lifecycle hook. |
/// | `on_despawn = path::fn` | `on_despawn` lifecycle hook. |
/// | `serialize` | Register with serialization support (requires the type to implement `Serialize` and `Deserialize`) and set `SERIALIZE` to `true`. See below. |
///
/// - `copy` and `cloner = …` are mutually exclusive.
/// - `map_entities = …` conflicts with `#[entities]` field annotations.
///
/// # Required components
///
/// `#[require(A, B)]` declares components that must be present on any entity
/// with this component.  They are stored in the `Component::REQUIRED`
/// constant, auto-registered with this component, added to the entity's
/// table on spawn/insert, and initialised with their [`Default`] values when
/// not provided explicitly.  Every required component must implement
/// `Default`; transitive requirements are followed recursively.
///
/// ```ignore
/// #[derive(TypePath, Component, Clone, Default)]
/// #[require(GlobalTransform)]
/// struct Transform { /* ... */ }
/// ```
///
/// # `map_entities`
///
/// `map_entities = path::fn` delegates entity remapping to a user function
/// with the signature `fn(&mut Self, &mut M) where M: EntityMapper`.  The
/// generated `Component` impl forwards its `map_entities` call to that
/// function and sets `NO_ENTITY = false`.
///
/// # Field attributes
///
/// - `#[editor(get)]` / `#[editor(set)]` — expose the field to the
///   editor via `get_field` / `set_field` (requires `Reflect`).  Both may be
///   combined on the same field.
/// - `#[entities]` — mark a field as containing entities; auto-generates
///   `map_entities` and sets `NO_ENTITY = false`.  The field type must
///   implement `MapEntities`.
///
/// # Cloner
///
/// By default, uses `ComponentCloner::clonable::<Self>()`, which requires
/// `Clone`.  Use `copy` for `Copy` types or `cloner = …` for custom
/// logic.
///
/// # Serialization
///
/// The `Component` trait itself does **not** require serialization — plain
/// components are registered without serialization support, so types holding
/// non-serializable data (raw pointers, `Rc`, closures, …) can be used
/// directly.
///
/// To make a component serializable (e.g. for scene saving), add
/// `#[component(serialize)]` and derive `Serialize` / `Deserialize`:
///
/// ```ignore
/// #[derive(TypePath, Component, Clone, Serialize, Deserialize)]
/// #[component(serialize)]
/// struct Transform { x: f32, y: f32 }
/// ```
///
/// The derive then overrides `Component::register` to use
/// `register_serializable`, filling the serialization function pointers in
/// the component's `ComponentDB`, and sets `Component::SERIALIZE` to `true`.
///
/// # Required traits
///
/// The `Component` trait requires `TypePath`, `Send`, and `Sync`.  The
/// cloner additionally requires `Clone` (default) or `Copy` (with `copy`).
/// Components annotated with `serialize` must also implement `Serialize`
/// and `Deserialize`.  The recommended derive list for serializable
/// components is `#[derive(TypePath, Component, Clone, Serialize,
/// Deserialize)]`.
///
/// # Generic types and limitations
///
/// For generic types the generated impl adds
/// `Self: Clone + TypePath + Serialize + for<'de> Deserialize<'de> + Send +
/// Sync + Sized + 'static` (with `Copy` in place of `Clone` when `copy` is
/// used).  Only structs are supported.
///
/// # Examples
///
/// ```ignore
/// #[derive(TypePath, Component, Clone, Serialize, Deserialize)]
/// #[component(on_add = Self::on_add)]
/// struct Health {
///     #[editor(get, set)]
///     value: u32,
/// }
///
/// impl Health {
///     fn on_add(world: DeferredWorld, ctx: HookContext) { /* … */ }
/// }
/// ```
#[proc_macro_derive(Component, attributes(component, editor, entities, require))]
pub fn derive_component(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    component::expand(ast).into()
}

/// Derives the `Resource` trait implementation.
///
/// This macro automatically implements the `Resource` trait for your struct,
/// exposing fields marked with `#[editor(get)]` and `#[editor(set)]`
/// to the editor via the `get_field` / `set_field` methods.
///
/// # Type attributes
///
/// - `#[resource(serialize)]` — sets `Resource::SERIALIZE` to `true` and
///   overrides the generated `register()` to use `register_serializable`,
///   filling the serialization function pointers in the resource's
///   `ResourceDB`.  The type must implement `Serialize` and `Deserialize`.
///
/// # Field attributes
///
/// - `#[editor(get)]` — the field appears in `GETTER`, readable through
///   `get_field`.
/// - `#[editor(set)]` — the field appears in `SETTER`, writable through
///   `set_field`.
///
/// Both may be combined (`#[editor(get, set)]`) on the same field.
///
/// Unmarked fields are ignored by the editor layer.
/// Every annotated field must implement `Reflect`.
///
/// # TypePath
///
/// `TypePath` is **not** derived by this macro — apply `#[derive(TypePath)]`
/// separately.  When the struct has generic parameters the generated impl
/// requires `Self: TypePath`, which transitively constrains each param.
/// Only structs are supported; enums and unions are rejected.
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
///     #[editor(get, set)]
///     health: u32,
///     #[editor(get)]
///     id: u64,
/// }
/// ```
#[proc_macro_derive(Resource, attributes(editor, resource))]
pub fn derive_resource(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    resource::expand(ast).into()
}

/// Derives the `SystemParam` trait implementation.
///
/// Supports structs whose fields are themselves `SystemParam` entries.
///
/// # What The Derive Generates
///
/// For a struct `P<'w, 's>`, this derive generates an unsafe
/// `impl SystemParam for P<'_, '_>` that:
/// - composes field states into one tuple `State`,
/// - forwards access registration to all fields,
/// - fetches each field value during system execution and reconstructs `P`.
///
/// This means the derived parameter behaves like a thin composition layer over
/// existing parameter implementations.
///
/// # Lifetime requirements
///
/// The struct must declare exactly two lifetimes named `'w` and `'s`.
/// Any other lifetime shape is rejected.
///
/// Lifetime meanings:
/// - `'w`: data borrowed from `World` during one system run.
/// - `'s`: data borrowed from the system-local parameter state.
///
/// # Field requirements
///
/// Every field type must implement `SystemParam`.
///
/// Typical field examples:
/// - `Res<'w, T>`
/// - `Local<'s, T>`
/// - `Commands<'w, 's>`
/// - `Query<'w, 's, ...>`
///
/// # Example
///
/// ```ignore
/// #[derive(Resource, TypePath)]
/// struct Counter(u32);
///
/// #[derive(SystemParam)]
/// struct CounterParam<'w, 's> {
///     counter: Res<'w, Counter>,
///     local: Local<'s, u32>,
///     commands: Commands<'w, 's>,
/// }
/// ```
///
/// Generic parameters are supported as long as the struct carries the bounds
/// required by its field types:
///
/// ```ignore
/// #[derive(SystemParam)]
/// struct GenericParam<'w, 's, T: Resource + Sync> {
///     value: Res<'w, T>,
///     _marker: PhantomData<&'s ()>,
/// }
/// ```
///
/// # Notes
///
/// Access registration forwards the system's `strict` flag to each field's
/// `register_access`.  Once a field reports a conflict, the remaining fields
/// are registered with `strict = false` so the same conflict is not logged
/// repeatedly.
///
/// If a field's `SystemParam::Item<'w, 's>` cannot be inferred as the same
/// field shape expected by the struct definition, implement `SystemParam`
/// manually for the outer type.
///
/// If one of the required lifetimes (`'w` / `'s`) is not used by real fields,
/// Rust may emit an "unused lifetime parameter" warning.
/// You can keep the lifetime explicit by adding a marker field such as:
/// `PhantomData<(&'w (), &'s ())>`.
///
/// ```ignore
/// #[derive(SystemParam)]
/// struct MarkerOnly<'w, 's> {
///     _marker: PhantomData<(&'w (), &'s ())>,
/// }
/// ```
#[proc_macro_derive(SystemParam)]
pub fn derive_system_param(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    system_param::expand(ast).into()
}

/// Derives the `QueryData` trait implementation.
///
/// Supports structs whose fields are themselves `QueryData` entries
/// (`&'w T`, `Ref<'w, T>`, `Mut<'w, T>`, `Option<...>`, tuples, ...).
/// The generated impl composes the per-field `State` / `Cache` / `Item`
/// types and delegates every query method to the fields.
///
/// # Lifetime requirements
///
/// The struct may declare no lifetime parameters, or exactly one lifetime
/// named `'w` (the world borrow).  Any other lifetime shape is rejected.
///
/// # Field requirements
///
/// Every field type must implement `QueryData`.  Mutable component access
/// must use `Mut<'w, T>` — raw `&'w mut T` fields
/// are rejected.
///
/// # `#[query_data(readonly)]`
///
/// When the struct only performs shared access (all fields are read-only),
/// add `#[query_data(readonly)]` so the generated impl also implements
/// `ReadOnlyQueryData`, making `Query<D, F>` [`Copy`].
/// For structs with mutable fields the derive instead generates a hidden
/// companion struct `{Name}ReadOnly<'w, ...>` — the read-only counterpart
/// — and uses it as the `QueryData::ReadOnly` type, so `Query::as_readonly()`
/// works automatically.
///
/// # `#[query_slice(type = Name)]`
///
/// Generates a slice-item companion type `Name<'w, ...>` whose fields are
/// the per-field `QuerySlice::SliceItem` types (e.g. `&'w [T]` for a
/// `&'w T` field), and implements `QuerySlice` for the derived struct so
/// `Query::iter_slice()` / `iter_slice_mut()` work.
///
/// The slice companions are table-level views produced through
/// `fetch_slice`; their per-entity `fetch` always yields `None`.
///
/// ```ignore
/// #[derive(QueryData)]
/// #[query_data(readonly)]
/// #[query_slice(type = FooSlice)]
/// struct Foo<'w> {
///     a: &'w A,
///     b: &'w B,
/// }
/// // generates `struct FooSlice<'w> { a: &'w [A], b: &'w [B], ... }`
/// // and `impl QuerySlice for Foo<'w> { type SliceItem<'w> = FooSlice<'w>; ... }`
/// ```
///
/// # Example
///
/// ```ignore
/// use zlim_core::borrow::Mut;
/// use zlim_core::derive::{Component, QueryData};
///
/// #[derive(Component, Clone)]
/// struct Position { x: f32, y: f32 }
///
/// #[derive(Component, Clone)]
/// struct Velocity { x: f32, y: f32 }
///
/// #[derive(QueryData)]
/// #[query_data(readonly)]
/// struct ReadVelocity<'w> {
///     velocity: &'w Velocity,
/// }
///
/// #[derive(QueryData)]
/// struct MoveData<'w> {
///     position: Mut<'w, Position>,
///     velocity: &'w Velocity,
/// }
/// ```
///
/// # Notes
///
/// Slice fetching requires every field type to implement
/// `QuerySlice`; only the built-in single-component forms do.
#[proc_macro_derive(QueryData, attributes(query_data, query_slice))]
pub fn derive_query_data(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    query_data::expand(ast).into()
}

// -----------------------------------------------------------------------------
// Job macros

/// Attribute macro that turns a function into a job marker type.
///
/// # Generated code
///
/// For a function `test_system`, this macro generates a marker struct named
/// by the `type` argument that derives `TypePath` and implements
/// `JobLabel`:
///
/// - `name()` returns the marker's `TypePath` — the `name` string when
///   given, otherwise `<Self as TypePath>::type_path()` (i.e.
///   `module_path!()::TypeName`).
/// - `database()` constructs a `JobDB` whose `ctor` wraps the function
///   through `IntoJob`.
///
/// Non-generic functions are additionally registered at program startup
/// through `zlim_reg::submit!`, so their databases appear in
/// `JobDB::collect`.
///
/// # Arguments
///
/// | Argument | Description |
/// |----------|-------------|
/// | `type = Name` or `type = Name<GENERICS>` | Identifier of the generated marker type and its optional generic parameters. |
/// | `name = "..."` | Optional unique string identifier of the job; defaults to the marker's `TypePath` (`<Self as TypePath>::type_path()`). Must be a valid path when given. |
/// | `run_if = expr` or `run_if = [expr, ...]` | Optional run conditions: each is a system returning `bool` / `Result<bool, E>` that gates the job. |
/// | `strict = true\|false` | Whether the job registers its access strictly (logs conflicts). Defaults to `true`. |
///
/// # Generic functions
///
/// Generic functions are supported: every type parameter must implement
/// `TypePath`, and the marker type is derived `TypePath` (with
/// `#[type_path = "..."]` when a `name` is given), so `name()` returns
/// `Self::type_path()`.  Generic markers cannot be auto-registered — use
/// `JobDB::register` manually.  Lifetime parameters are not supported.
///
/// # Example
///
/// ```ignore
/// // Without `name`, the job name is the type's path.
/// #[job_fn(type = PlayerMove)]
/// fn player_move() {}
///
/// assert_eq!(PlayerMove::name(), <PlayerMove as TypePath>::type_path());
///
/// // With `name`
/// #[job_fn(type = TestSystem, name = "test_system")]
/// fn test_system() {}
///
/// assert_eq!(TestSystem::name(), "test_system");
///
/// // With condition system
/// fn condition() -> bool {
///     true
/// }
///
/// #[job_fn(type = TestSystem2, run_if = condition)]
/// fn test_system2() {}
/// ```
#[proc_macro_attribute]
pub fn job_fn(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = parse_macro_input!(attr as job::JobAttr);
    let item = parse_macro_input!(item as ItemFn);

    match job::expand_attr(attr, item) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Macro that turns a system expression into a job marker type.
///
/// Unlike the [`job_fn`] attribute macro, this macro marks an arbitrary
/// system expression (typically a pipeline built with `IntoSystem::pipe`)
/// instead of a plain function.
///
/// # Arguments
///
/// | Argument | Description |
/// |----------|-------------|
/// | `type: Name` or `type: Name<GENERICS>` | The generated marker type and its generic parameters. |
/// | `name: "..."` | Optional unique string identifier of the job; defaults to the marker's `TypePath` (`<Self as TypePath>::type_path()`). Must be a valid path when given. |
/// | `run_if: expr` or `run_if: [expr, ...]` | Optional run conditions: each is a system returning `bool` / `Result<bool, E>` that gates the job. |
/// | `system: EXPR` | The system expression wrapped by the job's `ctor`. |
/// | `strict: true\|false` | Whether the job registers its access strictly (logs conflicts). Defaults to `true`. |
///
/// # Generic markers
///
/// When generics are declared, every type parameter must implement
/// `TypePath`, the marker type is derived `TypePath` (with
/// `#[type_path = "..."]` when a `name` is given), and no automatic
/// registration is emitted.
///
/// # Example
///
/// ```ignore
/// fn test_system1() -> u8 { 1 }
/// fn test_system2(input: In<u8>) {}
///
/// job! {
///     type: TestSystem,
///     name: "test_system",
///     system: test_system1.pipe(test_system2),
/// }
///
/// // Without `name`, the job name is the type's path.
/// job! {
///     type: PlayerMove,
///     system: test_system1.pipe(test_system2),
/// }
///
/// assert_eq!(PlayerMove::name(), "my_module::PlayerMove");
/// ```
///
/// [`job_fn`]: macro@job_fn
#[proc_macro]
pub fn job(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as job::DefineJobInput);
    job::expand_define(input).into()
}

/// Macro that turns a job list into a job group marker type.
///
/// # Generated code
///
/// This macro generates a marker struct named by the `type` argument that
/// implements `JobGroupLabel`:
///
/// - `name()` returns the `name` string (or `Self::type_path()` for generic
///   markers).
/// - `group()` resolves every slot — string literals are used as-is, types
///   implementing `JobLabel` are resolved through
///   `<Type as JobLabel>::name()` — and builds the group through
///   `JobGroup::build`.
/// - `register()` registers the group's type-based jobs and condition (in
///   list order, via `JobLabel::register()`) before registering the group
///   itself.  String slots carry no type and are skipped.
///
/// # Arguments
///
/// | Argument | Description |
/// |----------|-------------|
/// | `type: Name` or `type: Name<GENERICS>` | The generated marker type and its generic parameters. **Required.** |
/// | `name: "..."` | Unique string identifier of the group. **Required.** |
/// | `jobs: [...]` | The job slots of the group. **Required.** |
/// | `condition: ...` | Optional run condition; a single slot. |
/// | `order: [...]` | Optional ordered constraints; a list of slot lists. |
/// | `weak_order: [...]` | Optional weak ordered constraints; a list of slot lists. |
/// | `relaxed_order: [...]` | Optional relaxed (non-blocking) ordered constraints; a list of slot lists. |
///
/// # Generic markers
///
/// When generics are declared, every type parameter must implement
/// `TypePath`, the marker type is derived `TypePath` with
/// `#[type_path = "..."]`, and `name()` returns `Self::type_path()`.
///
/// # Registration
///
/// Non-generic markers register themselves at program startup through
/// `zlim_reg::submit!`, so their groups appear after
/// `JobGroup::collect`.  Generic markers cannot be auto-registered (a
/// startup static cannot reference generic parameters) — register each
/// concrete instantiation manually, e.g. through
/// `zlim_reg::submit!(__JobGroupReg__::of::<GenericGroup<u32>>() => __JobGroupReg__)`.
///
/// # Ordering and conditions
///
/// `condition`, `order`, `weak_order`, and `relaxed_order` reference job
/// slots by name (either a string literal or a `JobLabel` type).
/// `JobGroup::build` resolves those names to indices into the group's jobs
/// array, which is prefixed with internal `GroupBegin`/`GroupEnd` markers —
/// slot indices are therefore shifted by `+2`, and `condition` is stored as
/// an index into that array.
///
/// # Example
///
/// ```ignore
/// #[job_fn(type = LabelA, name = "label_a")]
/// fn label_a() {}
///
/// job_group! {
///     type: ExampleGroup,
///     name: "example_group",
///     jobs: [LabelA, "label_b"],
///     condition: LabelA,
///     order: [["label_b", LabelA]],
/// }
/// ```
#[proc_macro]
pub fn job_group(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as job_group::JobGroupInput);
    job_group::expand(input).into()
}

/// Derives the `ScheduleLabel` trait implementation.
///
/// # Required Traits
///
/// The target type must implement the following traits:
/// - `Send` + `Sync` (required by the `ScheduleLabel` trait itself)
/// - `Clone`
/// - `Debug`
/// - `Hash`
/// - `Eq` (and therefore `PartialEq`)
///
/// # Examples
///
/// ```ignore
/// #[derive(ScheduleLabel, Clone, Debug, Hash, PartialEq, Eq)]
/// enum GameStage {
///     Begin,
///     Input,
///     Physics,
///     Logic,
///     Animation,
///     Collision,
///     Render,
///     End,
/// }
/// ```
#[proc_macro_derive(ScheduleLabel)]
pub fn derive_schedule_label(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    schedule::expand(ast).into()
}

/// Derives the `ScheduleStage` trait implementation.
///
/// # Supported Shapes
///
/// Only **unit structs** and **data-less enums** (every variant is a unit
/// variant) are supported; generics, unions, structs with fields, and enums
/// with data-carrying variants are rejected.  The target type must implement
/// `TypePath`, usually obtained through `#[derive(TypePath)]`.
///
/// - Unit struct — `stage_name` returns `TypePath::type_path()` directly.
/// - Enum — `stage_name` returns `format!("{}::{}", TypePath::type_path(),
///   variant)` for the matched variant.
///
/// # Examples
///
/// ```ignore
/// #[derive(TypePath, ScheduleStage)]
/// struct Startup;
///
/// #[derive(TypePath, ScheduleStage)]
/// enum MainStage {
///     Update,
///     Render,
/// }
///
/// assert_eq!(Startup.stage_name(), "crate_name::Startup");
/// assert_eq!(MainStage::Update.stage_name(), "crate_name::MainStage::Update");
/// ```
#[proc_macro_derive(ScheduleStage)]
pub fn derive_schedule_stage(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    schedule_stage::expand(ast).into()
}

/// Derives the `Message` trait implementation.
///
/// # Required Traits
///
/// The target type must satisfy the `Message` bounds: `Send`, `Sync`,
/// `TypePath`, and `'static`.  `TypePath` is usually obtained through
/// `#[derive(TypePath)]`, so the recommended derive list is
/// `#[derive(TypePath, Message)]`.
///
/// For generic types the generated impl adds the
/// `Self: Send + Sync + TypePath + 'static` where-bound.
///
/// # Examples
///
/// ```ignore
/// #[derive(TypePath, Message)]
/// struct Collision {
///     lhs: u32,
///     rhs: u32,
/// }
/// ```
#[proc_macro_derive(Message)]
pub fn derive_message(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    message::expand(ast)
}
