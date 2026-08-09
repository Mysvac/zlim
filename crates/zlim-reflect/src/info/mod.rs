//! Provides compile-time type information and metadata APIs.
//!
//! ## Menu
//!
//! - [`PathTable`]: A vtable storing function pointers for a single type's `TypePath` implementation.
//!
//! - [`Type`]: A compact type descriptor containing a `TypeId` and a [`PathTable`].
//!
//! - [`Attributes`]: An attribute container similar to `Map<TypeId, Box<dyn Any>>`.
//!
//! - [`Generics`]: A list of `GenericInfo` values describing instantiated generic parameters.
//!     - [`GenericInfo`]: An enum over `TypeParamInfo` and `ConstParamInfo`.
//!     - [`TypeParamInfo`]: Type-parameter metadata, including parameter name, `Type`, and optional default `Type`.
//!     - [`ConstParamInfo`]: Const-parameter metadata, including parameter name, `Type`, and const value.
//!
//! - [`TypeInfo`]: An enum representing compile-time type metadata. Variants include:
//!     - [`ArrayInfo`]: Array metadata, such as `[i32; 5]`, including capacity and item type information.
//!     - [`ListInfo`]: List-like metadata, such as `Vec<i32>`, including item type information.
//!     - [`TupleInfo`]: Tuple / tuple-struct metadata, such as `(i32, f32)` or `A(..)`, including field types and attributes.
//!     - [`StructInfo`]: Struct metadata, such as `A { .. }`, including field names, field types, and custom attributes.
//!     - [`EnumInfo`]: Enum metadata, including variant metadata and custom attributes.
//!     - [`MapInfo`]: Map-like metadata, such as `HashMap<K, V>`, including key and value type information.
//!     - [`SetInfo`]: Set-like metadata, such as `HashSet<T>`, including value type information.
//!     - [`OpaqueInfo`]: Metadata for opaque types, such as `struct A;` or `String`.
//!
//! - [`VariantInfo`]: An enum representing enum variant metadata. Variants include:
//!     - [`StructVariantInfo`]: Similar to `StructInfo`, but without generic metadata.
//!     - [`TupleVariantInfo`]: Similar to `TupleInfo`.
//!     - [`UnitVariantInfo`]: For unit variants with no fields, e.g. `A`.
//!
//! - Field Info:
//!     - [`NamedField`]: Metadata for struct fields, including name, field type, and custom attributes.
//!     - [`UnnamedField`]: Metadata for tuple and tuple-struct fields, including index, field type, and custom attributes.
//!
//! - Kind:
//!     - [`ReflectKind`]: The broad reflection kind, such as `Struct`, `Array`, or `Opaque`.
//!     - [`VariantKind`]: The enum variant kind: `Struct`, `Tuple`, or `Unit`.
//!
//! - [`Typed`]: A trait for obtaining `TypeInfo` for a concrete type.
//!
//! - [`DynamicTyped`]: Dynamic dispatch support for `Typed`.
//!
//! Other items:
//! - [`InfoCell`]: A cache for generic type information, keyed by `TypeId`.
//! - [`ConstParam`]: Storage for const-generic values (integers, `char`, `bool`).
//! - [`ReflectKindError`] / [`VariantKindError`]: Errors returned when a kind cast fails.
//! - [`AttributesBuilder`]: Builder for constructing [`Attributes`].

// -----------------------------------------------------------------------------
// Modules

mod array_info;
mod attributes;
mod enum_info;
mod field_info;
mod generics;
mod list_info;
mod map_info;
mod opaque_info;
mod set_info;
mod struct_info;
mod tuple_info;
mod type_info;
mod type_meta;
mod variant_info;

// -----------------------------------------------------------------------------
// Internal API

use attributes::{impl_attributes_fn, impl_with_attributes};
use generics::{impl_generics_fn, impl_with_generics};
use type_meta::impl_type_fn;

// -----------------------------------------------------------------------------
// Exports

pub use array_info::ArrayInfo;
pub use attributes::{Attributes, AttributesBuilder};
pub use enum_info::EnumInfo;
pub use field_info::{NamedField, UnnamedField};
pub use generics::{ConstParam, ConstParamInfo, TypeParamInfo};
pub use generics::{GenericInfo, Generics};
pub use list_info::ListInfo;
pub use map_info::MapInfo;
pub use opaque_info::OpaqueInfo;
pub use set_info::SetInfo;
pub use struct_info::StructInfo;
pub use tuple_info::TupleInfo;
pub use type_info::{DynamicTyped, TypeInfo, Typed};
pub use type_info::{InfoCell, ReflectKind, ReflectKindError};
pub use type_meta::{PathTable, Type};
pub use variant_info::{StructVariantInfo, TupleVariantInfo, UnitVariantInfo};
pub use variant_info::{VariantInfo, VariantKind, VariantKindError};

// -----------------------------------------------------------------------------
