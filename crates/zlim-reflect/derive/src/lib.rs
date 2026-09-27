//! Procedural macros for `zlim-reflect`.
use proc_macro::TokenStream;
use syn::parse_macro_input;

// -----------------------------------------------------------------------------
// Modules

mod path;
mod reflect;
mod string_expr;
mod type_path;

// -----------------------------------------------------------------------------
// Derive macros

/// Derive the [`TypePath`] trait for a type.
///
/// # Default behaviour
///
/// Without any attributes the macro uses `module_path!()` and the Rust
/// identifier to build the required items:
///
/// ```rust, ignore
/// #[derive(TypePath)]
/// struct Foo;
///
/// // Generates:
/// // - type_path()     → "{module}::Foo"
/// // - type_name()     → "Foo"
/// // - const IDENT: &str    = "Foo";
/// // - const MODULE: Option<&str> = Some("{module}");
/// // - const CRATE: Option<&str>  = first segment of {module};
/// ```
///
/// # Custom path
///
/// Use `#[type_path = "..."]` to override the full path prefix:
///
/// ```rust, ignore
/// #[derive(TypePath)]
/// #[type_path = "my_crate::bar::Baz"]
/// struct Foo;
///
/// // Generates:
/// // - type_path()              → "my_crate::bar::Baz"
/// // - type_name()              → "Baz"
/// // - const IDENT: &str        = "Baz";
/// // - const MODULE: Option<&str> = Some("my_crate::bar");
/// // - const CRATE: Option<&str>  = Some("my_crate");
/// ```
///
/// # Generic types
///
/// Type and const generic parameters are automatically included in
/// `type_path()` and `type_name()` via `PathCell` caching:
///
/// ```rust, ignore
/// #[derive(TypePath)]
/// struct MyVec<T> { /* ... */ }
///
/// // for T = Vec<i32>:
/// // type_path()  → "{module}::MyVec<alloc::vec::Vec<i32>>"
/// // type_name()  → "MyVec<Vec<i32>>"
/// // - const IDENT: &str        = "MyVec";
/// // - const MODULE: Option<&str> = Some("{module}");
/// // - const CRATE: Option<&str>  = first segment of {module};
/// ```
///
/// # Generic types Custom path
///
/// Use `#[type_path = "..."]` to override the full path prefix, no need generic params:
///
/// ```rust, ignore
/// #[derive(TypePath)]
/// #[type_path = "a::vec::Vec"]
/// struct MyVec<T> { /* ... */ }
///
/// // for T = Vec<i32>:
/// // type_path()  → "a::vec::Vec<alloc::vec::Vec<i32>>"
/// // type_name()  → "Vec<Vec<i32>>"
/// // - const IDENT: &str        = "Vec";
/// // - const MODULE: Option<&str> = Some("a::vec");
/// // - const CRATE: Option<&str>  = Some("a");
/// ```
///
#[proc_macro_derive(TypePath, attributes(type_path))]
pub fn derive_type_path(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    let zlim_reflect = path::zlim_reflect_path();
    type_path::expand_type_path(&input, &zlim_reflect).into()
}

/// Derive the [`Reflect`] trait and associated traits.
///
/// `#[derive(Reflect)]` automatically implements the following traits:
///
/// - [`Reflect`] — the core reflection trait (`reflect_clone`,
///   `reflect_apply`, `reflect_eq`, `reflect_hash`, `reflect_debug`,
///   `from_reflect`).
/// - `Typed` — static access to `TypeInfo` metadata.
/// - `Struct` (for named-field and tuple structs) or
///   `Enum` (for enums) — kind-specific field-accessor trait.
/// - `TypeDatabase` — enables type registration and auto-discovery via
///   `TypeDB`.
///
/// For non-generic types (lifetime-only parameters are fine), a
/// `register_reflect!` call is also emitted so the type is automatically
/// discovered at program startup when `TypeDB::collect` is called.
///
/// Unit structs (`struct Foo;`) are treated as opaque — they implement
/// [`Reflect`] with `ReflectKind::Opaque` and do not receive a
/// `Struct` impl.
///
/// # Opaque types — `#[reflect(Opaque)]`
///
/// Marks a type as opaque regardless of its structure. The macro does
/// **not** generate an `Opaque` trait impl — you must implement
/// `Opaque` manually. The generated [`Reflect`] impl uses
/// `ReflectKind::Opaque` and delegates `reflect_apply` /
/// `reflect_eq` / etc. to the `Opaque` trait methods.
///
/// ```rust, ignore
/// #[derive(Reflect)]
/// #[reflect(Opaque)]
/// struct MyWrapper(String);
///
/// impl Opaque for MyWrapper {
///     fn stringify(&self) -> String {
///         self.0.clone()
///     }
///
///     fn apply_str(&mut self, v: &str) -> Result<(), String> {
///         self.0 = v.into();
///         Ok(())
///     }
/// }
/// ```
///
/// This attribute is type-level only. It is also why there is no
/// `#[reflect(Opaque = false)]`: supplying the `Opaque` impl is exactly what
/// this flag already asks for.
///
/// # Opting out of a generated trait — `#[reflect(Trait = false)]`
///
/// Each implementation the macro would emit can be turned off, so that you can
/// write your own:
///
/// | Attribute | Skipped |
/// |-----------|---------|
/// | `#[reflect(Reflect = false)]` | `Reflect` |
/// | `#[reflect(Typed = false)]` | `Typed` |
/// | `#[reflect(Struct = false)]` | `Struct`, on a named-field struct |
/// | `#[reflect(Tuple = false)]` | `Tuple`, on a tuple struct |
/// | `#[reflect(Enum = false)]` | `Enum`, on an enum |
/// | `#[reflect(TypeDatabase = false)]` | `TypeDatabase`, and the type's automatic registration |
///
/// Skip one only when you are providing the implementation.
///
/// # Two rules for the kind traits
///
/// **A kind trait can only be skipped on the kind that generates it.** The type
/// decides which one the macro emits, so `Struct = false` belongs on a
/// named-field struct and nothing else. This is rejected rather than ignored:
///
/// ```text
/// error: `Pair` cannot use `#[reflect(Tuple = false)]`: its reflection kind is
/// `Struct`, so `Struct` is the only kind trait the macro generates for it.
/// `#[reflect(Tuple = false)]` applies to a tuple struct.
/// ```
///
/// A unit struct (`struct Foo;`) reflects as `Opaque`, so it follows that rule
/// too — `Struct = false` on it is an error.
///
/// **A kind trait can only be skipped together with `Reflect`.** The generated
/// `Reflect` dispatches `reflect_kind` / `reflect_ref` / `reflect_mut` /
/// `reflect_owned` to the kind trait, so it cannot be compiled without it. Write
/// both by hand and turn off both:
///
/// ```rust, ignore
/// #[derive(Reflect)]
/// #[reflect(Reflect = false, Struct = false)]
/// struct Point { x: f32, y: f32 }
///
/// impl Struct for Point { /* ... */ }
/// impl Reflect for Point { /* ... */ }
/// ```
///
/// These are checked before anything is generated, so a type that gets one wrong
/// reports that one thing instead of a page of "trait bound is not satisfied".
///
/// # The other two
///
/// `TypeDatabase` is the pair to `register_reflect!`, so opting out of it also
/// stops the type from being discovered at startup — a type left out that way
/// has to be registered by hand.
///
/// `Opaque` is not in the table: `#[reflect(Opaque)]` already means "treat this
/// as opaque and supply the `Opaque` impl yourself", so a `= false` spelling of
/// the same name would only be a second way to say it.
///
/// # Optimization with standard traits
///
/// If a type implements standard Rust traits, the reflection impls can
/// delegate to them directly (avoiding field-by-field reflection
/// overhead). The macro cannot detect trait impls automatically, so
/// you must opt in with attributes:
///
/// | Attribute | Effect |
/// |-----------|--------|
/// | `#[reflect(Clone)]` | `reflect_clone` delegates to `Clone::clone` |
/// | `#[reflect(Eq)]` | `reflect_eq` downcasts and calls `PartialEq::eq` |
/// | `#[reflect(Hash)]` | `reflect_hash` delegates to `Hash::hash` |
/// | `#[reflect(Debug)]` | `reflect_debug` delegates to `Debug::fmt` |
///
/// ```rust, ignore
/// #[derive(Reflect)]
/// #[reflect(Clone, Eq, Hash, Debug)]
/// struct Health(i32);
/// ```
///
/// When a standard-trait fast path is taken, the corresponding
/// kind-specific fallback (e.g. `struct_eq()`) is skipped entirely.
///
/// These attributes are type-level only.
///
/// # Default constructor — `#[reflect(Default)]`
///
/// When set, the generated `TypeDatabase::on_register` calls
/// `TypeDB::insert_defaultor::<Self>()`, which stores `Self::default` itself.
/// This makes the type constructible at runtime via `TypeDB::default`.
///
/// ```rust, ignore
/// #[derive(Reflect, Default)]
/// #[reflect(Default)]
/// struct SpawnPoint { x: f32, y: f32 }
/// ```
///
/// This attribute is type-level only.
///
/// # Serialize — `#[reflect(Serialize)]` & `#[reflect(Deserialize)]`
///
/// The reflective serializer cannot know that a type has a serde
/// implementation, so serialization goes through the reflection fallback unless
/// the type says otherwise. These two flags make it say otherwise: the
/// generated `TypeDatabase::on_register` calls
/// `TypeDB::insert_serializer::<Self>()` / `TypeDB::insert_deserializer::<Self>()`,
/// which store the function pointers to `Serialize::serialize` /
/// `Deserialize::deserialize`.
///
/// | Attribute | Effect |
/// |-----------|--------|
/// | `#[reflect(Serialize)]` | `TypeDB::reflect_serialize` calls `serde::Serialize` directly |
/// | `#[reflect(Deserialize)]` | `TypeDB::reflect_deserialize` calls `serde::Deserialize` directly |
///
/// ```rust, ignore
/// #[derive(Reflect, serde::Serialize, serde::Deserialize)]
/// #[reflect(Serialize, Deserialize)]
/// struct Settings { volume: f32 }
/// ```
///
/// Both paths produce the same document — the flags change how the value is
/// produced, not its shape. Without them the fallback walks `Reflect::reflect_ref`
/// and serializes the value kind by kind; with them serde writes it in one go.
///
/// Two things follow from where the pointer is stored:
///
/// - It belongs to **the type**, so it only takes effect once that type is in
///   the `TypeDB` — for a non-generic type the derive's own auto-registration
///   already covers it, and anything else has to be registered.
/// - The type must actually implement `Serialize` / `Deserialize`; that is a
///   trait bound on `insert_serializer` / `insert_deserializer`, checked when
///   the type is registered.
///
/// A **field** can ask for the same thing with `#[reflect(serialize)]` /
/// `#[reflect(deserialize)]`. That is the form to use when the field's type is a
/// concrete instantiation of a generic one: a generic type's own `TypeDatabase`
/// impl cannot commit to serde for a parameter it does not know — see the
/// field-level section below.
///
/// # Custom attributes — `#[reflect(@expr)]`
///
/// Attaches arbitrary reflected values as custom metadata. These are
/// stored in the type's `Attributes` and retrievable at runtime.
/// The expression must evaluate to a type implementing [`Reflect`].
///
/// ```rust, ignore
/// #[derive(Reflect)]
/// #[reflect(@0.1_f32, @"hello")]
/// struct Config {
///     #[reflect(@false)]
///     enabled: bool,
/// }
/// ```
///
/// Multiple attributes of the same Rust type are not supported — the
/// last one wins (they are stored by `TypeId`).
///
/// This attribute can be used at the type, field, and enum-variant
/// levels.
///
/// # Field-level attributes
///
/// ## `#[reflect(ignore)]`
///
/// Excludes a field from reflection entirely. Ignored fields do not
/// count toward `field_len`, are skipped by iterators, and cannot be
/// accessed through the reflection API.
///
/// **Important:** without `#[reflect(clone)]` or `#[reflect(default)]`
/// on the field, `reflect_clone` and `from_reflect` cannot construct
/// ignored fields and will always return an error. Strongly consider
/// pairing `#[reflect(ignore)]` with `#[reflect(default)]` (see below)
/// so the field can be initialized via `Default::default()`.
///
/// ```rust, ignore
/// #[derive(Reflect)]
/// struct MyRes<T> {
///     data: Vec<T>,
///     #[reflect(ignore, default)]
///     _marker: std::marker::PhantomData<T>,
/// }
/// ```
///
/// ## `#[reflect(clone)]`
///
/// Declares that the field's type implements [`Clone`]. When the
/// type does **not** use the type-level `#[reflect(Clone)]` fast path,
/// `reflect_clone` clones this field via [`Clone::clone`] instead of
/// the generic `reflect_clone_field` fallback.
///
/// This is a field-level attribute.
///
/// ```rust, ignore
/// #[derive(Reflect)]
/// struct Data {
///     #[reflect(clone)]
///     id: u64,
///     #[reflect(clone)]
///     name: String,
/// }
/// ```
///
/// ## `#[reflect(default)]`
///
/// Marks a field as having a fallback default value via
/// `Default::default()`. This affects three places:
///
/// - `from_reflect`: when the source omits this field,
///   `Default::default()` is used to construct it.
/// - `reflect_clone`: when the type does
///   **not** use `#[reflect(Clone)]`, ignored fields are constructed
///   via `Default::default()` during the field-by-field clone.
/// - `TypeDatabase::register_dependencies`: the field's type gets its
///   `Default` constructor registered, so it is constructible through
///   `TypeDB::default` without carrying `#[reflect(Default)]` itself.
///
/// The registration matters most for generic field types, for the reason given
/// under `#[reflect(serialize)]` below: a generic type's own registration cannot
/// commit to the traits of a parameter it does not know.
///
/// ```rust, ignore
/// #[derive(Reflect)]
/// struct Settings {
///     volume: f32,
///     #[reflect(default)]
///     theme: String,   // defaults to ""
/// }
/// ```
///
/// ## `#[reflect(serialize)]` and `#[reflect(deserialize)]`
///
/// Register the field's type as serde serializable / deserializable in
/// `TypeDatabase::register_dependencies`, as if the type carried the
/// type-level `#[reflect(Serialize)]` / `#[reflect(Deserialize)]`.
///
/// A type registers the traits it names at the type level, and that is enough
/// for a plain field. It is not enough for a *generic* field: the impl for
/// `Vec<T>` only forwards the `TypeDatabase` bound to `T`, so it cannot register
/// `Serialize` / `Deserialize` — whether `Vec<T>` is serde-able depends on `T`,
/// which the impl does not know. A concrete field such as `Vec<String>` is
/// nevertheless serde-able, and saying so on the field registers the fast-path
/// pointers for that exact type, so serializing and deserializing it goes
/// through `Serialize` / `Deserialize` instead of the reflective fallback.
///
/// The field type must actually implement `Serialize` / `Deserialize`. Both are
/// checked when the container is registered, not when it is defined.
///
/// ```rust, ignore
/// #[derive(Reflect)]
/// struct Settings {
///     // `Vec<String>` is serde-able, but `Vec<T>`'s registration cannot know
///     // that; the field says so for this concrete type.
///     #[reflect(serialize, deserialize)]
///     themes: Vec<String>,
/// }
/// ```
///
/// ## `#[reflect(remote = Wrapper)]`
///
/// Marks a field whose type lives in another crate, so that reflection reaches it through the
/// local *wrapper* named here instead. The field keeps holding the remote type; the accessors,
/// `unpack`, and `from_reflect` all hand out the wrapper, which must implement
/// `zlim_reflect::remote::ReflectRemote`.
///
/// ```rust, ignore
/// #[derive(Reflect)]
/// struct Holder {
///     #[reflect(remote = TheirTypeRemote)]
///     data: some_lib::TheirType,
/// }
/// ```
///
///
/// # Overriding method implementations
///
/// By default the macro generates `from_reflect` and `reflect_apply`
/// using the standard field-by-field logic. Use these attributes to
/// provide custom implementations:
///
/// | Attribute | Overrides |
/// |-----------|-----------|
/// | `#[reflect(from_reflect = fn)]` | `Reflect::from_reflect` |
/// | `#[reflect(reflect_apply = fn)]` | `Reflect::reflect_apply` |
/// | `#[reflect(on_register = fn)]` | `TypeDatabase::on_register` (additional) |
///
/// The provided function's first parameter must be the type itself
/// (e.g. `&mut Self`, not `&mut dyn Struct`). For generic types the
/// function is called with matching generic arguments (e.g.
/// `my_apply::<A>` for `struct MyType<A>`). The macro does not validate
/// the signature — mismatches surface as normal Rust compile errors.
///
/// ```rust, ignore
/// fn my_apply(this: &mut Special, other: &dyn Reflect) -> Result<(), ApplyError> { /* ... */ }
///
/// #[derive(Reflect)]
/// #[reflect(reflect_apply = my_apply)]
/// struct Special { data: Vec<u8> }
/// ```
///
/// The `on_register` override is **additional** — it does not replace the
/// default `on_register` logic. The provided function is called after the
/// standard registration completes, so you can run custom setup code
/// when the type is registered.
///
/// ```rust, ignore
/// fn my_on_register(db: &TypeDB) {
///     // Additional setup when this type is registered.
/// }
///
/// #[derive(Reflect)]
/// #[reflect(on_register = my_on_register)]
/// struct MyType { value: i32 }
/// ```
///
/// Specifying `reflect_clone`, `reflect_eq`, or other method names
/// that are not in the override list produces a compile error.
///
/// These attributes are type-level only.
///
/// # Auto-registration
///
/// For non-generic types the macro emits a `register_reflect!` call so the
/// type is discovered at startup. For generic types, use the
/// standalone `register_reflect!` macro with concrete instantiations:
///
/// ```rust, ignore
/// register_reflect!(MyGenericType<u32>, MyGenericType<String>);
/// ```
///
/// Repeated registration is safe.
///
/// Field types are automatically registered as dependencies in
/// `TypeDatabase::register_dependencies`; a field flag such as
/// `#[reflect(default)]`, `#[reflect(serialize)]` or
/// `#[reflect(deserialize)]` additionally registers the matching trait for the
/// field's type.
#[proc_macro_derive(Reflect, attributes(reflect))]
pub fn derive_reflect(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as syn::DeriveInput);
    reflect::expand_reflect(&ast).into()
}

/// Implements reflection for foreign types.
///
/// It requires full type information and access to fields. Because of the
/// orphan rule, this is typically used inside the reflection crate itself.
///
/// This macro emits the reflection only. The type's [`TypePath`] is written
/// separately, next to the invocation, with `impl_simple_type_path!` — or by
/// hand when that macro does not cover the shape. A foreign type has no
/// `module_path!()` of its own to be named by, so its path belongs where it is
/// spelled out, not here.
///
/// The usage is otherwise similar to [`derive Reflect`](derive_reflect).
///
/// ## Example
///
/// ```rust, ignore
/// impl_simple_type_path!(@Option<T>: "core", "option", "Option");
///
/// impl_reflect! {
///     #[reflect(Default)]
///     enum Option<T> {
///         Some(T),
///         None,
///     }
/// }
/// ```
///
/// See [`derive Reflect`](derive_reflect) for more details.
#[proc_macro]
pub fn impl_reflect(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as syn::DeriveInput);
    reflect::expand_reflect(&ast).into()
}
