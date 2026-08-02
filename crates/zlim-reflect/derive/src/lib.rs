//! Procedural macros for [`zlim_reflect`].
//!
//! [`zlim_reflect`]: https://crates.io/crates/zlim-reflect
#![allow(linker_messages, reason = "It's noisy and interferes with CI output")]

use proc_macro::TokenStream;
use syn::parse_macro_input;

// ----------------------------------------------------------------------------
// Modules

mod path;
mod reflect;
mod string_expr;
mod type_path;

// ----------------------------------------------------------------------------
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
/// - [`TypePath`] — stable compile-time type identifiers.
/// - `Typed` — static access to `TypeInfo` metadata.
/// - `Struct` (for named-field and tuple structs) or
///   `Enum` (for enums) — kind-specific field-accessor trait.
/// - `TypeDatabase` — enables type registration, conversion, and
///   auto-discovery via `TypeDB`.
///
/// For non-generic types (lifetime-only parameters are fine), a
/// `register!` call is also emitted so the type is automatically
/// discovered at program startup when `TypeDB::collect` is called.
///
/// Unit structs (`struct Foo;`) are treated as opaque — they implement
/// [`Reflect`] with `ReflectKind::Opaque` and do not receive a
/// `Struct` impl.
///
/// # Custom type path
///
/// `#[type_path = "..."]` overrides the default type path, same as with
/// `#[derive(TypePath)]`:
///
/// ```rust, ignore
/// #[derive(Reflect)]
/// #[type_path = "my_game::components::Position"]
/// struct Pos { x: f32, y: f32 }
/// ```
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
/// This attribute is type-level only.
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
/// `TypeDB::insert_defaultor` with `|| Self::default()`. This makes
/// the type constructible at runtime via `TypeDB::default`.
///
/// ```rust, ignore
/// #[derive(Reflect, Default)]
/// #[reflect(Default)]
/// struct SpawnPoint { x: f32, y: f32 }
/// ```
///
/// This attribute is type-level only.
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
/// `Default::default()`. This affects two methods:
///
/// - `from_reflect`: when the source omits this field,
///   `Default::default()` is used to construct it.
/// - `reflect_clone`: when the type does
///   **not** use `#[reflect(Clone)]`, ignored fields are constructed
///   via `Default::default()` during the field-by-field clone.
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
/// (e.g. inserting extra convertors) when the type is registered.
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
/// # Documentation reflection
///
/// When the `reflect_docs` feature is enabled on `zlim-reflect-derive`,
/// standard `#[doc = "..."]` attributes (including `/// ...` comments)
/// are collected and stored in the type's `TypeInfo`. This makes
/// documentation available at runtime through
/// `TypeInfo::docs`.
///
/// ```rust, ignore
/// /// The player's current health.
/// /// Ranges from 0 to 100.
/// #[derive(Reflect)]
/// struct Health(u32);
/// ```
///
/// The feature is off by default — enable it in your `Cargo.toml`:
///
/// ```toml
/// [dependencies]
/// zlim-reflect-derive = { features = ["reflect_docs"] }
/// ```
///
/// Without the feature, all `docs()` methods return `None` regardless
/// of doc comments in the source.
///
/// This applies at the type, field, and enum-variant levels.
///
/// # Auto-registration
///
/// For non-generic types the macro emits a `register!` call so the
/// type is discovered at startup. For generic types, use the
/// standalone `register!` macro with concrete instantiations:
///
/// ```rust, ignore
/// register!(MyGenericType<u32>, MyGenericType<String>);
/// ```
///
/// Repeated registration is safe.
///
/// Field types are automatically registered as dependencies in
/// `TypeDatabase::register_dependencies`.
#[proc_macro_derive(Reflect, attributes(reflect))]
pub fn derive_reflect(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as syn::DeriveInput);
    reflect::expand_reflect(&ast).into()
}

/// Implements reflection for foreign types.
///
/// It requires full type information and access to fields. ecause of the
/// orphan rule, this is typically used inside the reflection crate itself.
///
/// The usage is similar to [`derive Reflect`](derive_reflect).
///
/// ## Example
///
/// ```rust, ignore
/// impl_reflect! {
///     #[type_path = "core::option:Option"]
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

    let custom = match type_path::CustomPath::parse(&ast.attrs) {
        Ok(c) => c,
        Err(e) => return e.into_compile_error().into(),
    };

    if custom.path.is_none() {
        let msg = "#[type_path = \"...\"] must be specified when impl Reflect for Foreign Type.";
        return syn::Error::new(ast.ident.span(), msg)
            .into_compile_error()
            .into();
    }

    reflect::expand_reflect(&ast).into()
}
