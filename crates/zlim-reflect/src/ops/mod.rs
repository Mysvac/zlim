//! Reflection operation traits and error types.
//!
//! These traits extend [`Reflect`] with data-access methods for different
//! type shapes. Each trait corresponds to one [`ReflectKind`] variant.
//!
//! # Traits
//!
//! | Trait | Shape |
//! |-------|-------|
//! | [`Opaque`] | Opaque / primitive types |
//! | [`Array`] | Fixed-size arrays (`[T; N]`) |
//! | [`List`] | Growable sequences (`Vec<T>`) |
//! | [`Map`] | Key-value maps (`HashMap<K, V>`) |
//! | [`Set`] | Hash sets (`HashSet<T>`) |
//! | [`Struct`] | Named structs (`S { a, b }`) |
//! | [`Tuple`] | Tuples / tuple-structs (`(A, B)`, `Foo(A)`) |
//! | [`Enum`] | Enums with unit/tuple/struct variants |
//!
//! # Hash and Equality
//!
//! All reflected types implement [`Hash`] and [`Eq`] via [`Reflect::reflect_hash`]
//! and [`Reflect::reflect_eq`], which provide default implementations. The
//! default implementation for [`Opaque`] types is text-based — values are
//! serialized via [`Opaque::stringify`] and then compared/hashed as strings.
//! This ensures correct behavior for types like `f32` where IEEE 754
//! floating-point equality (`NaN != NaN`) would otherwise break hash-table
//! lookups.
//!
//! Equality is **strict**: different concrete types (determined by [`TypeId`])
//! always compare as unequal, regardless of their contents.
//!
//! # FromReflect Conversion
//!
//! [`Reflect::from_reflect`] converts a boxed reflected value into a concrete
//! type. The conversion follows a fixed multi-phase workflow:
//!
//! 1. **Same type** — if the concrete types match, downcast and return
//!    directly (fast path).
//! 2. **Type database** — if the [`TypeDB`] has a registered conversion
//!    function from the source type to the target type, use it.
//! 3. **Field compatibility** — if the types differ and no conversion is
//!    registered, check whether the immediate (single-layer) fields are
//!    compatible. This check is **non-recursive**: only the top-level fields
//!    are examined; fields of fields are not inspected.
//! 4. **Construct** — if all required fields are compatible, unpack the source
//!    value and construct the target field by field.
//!
//! ## Strict vs Lenient
//!
//! - **Tuples and arrays** are **strict**: length must match exactly. Missing
//!   or extra elements cause conversion failure.
//! - **Structs and enum struct variants** are **lenient**: extra fields in the
//!   source are ignored, but every field not annotated with
//!   `#[reflect(default)]` must be present.
//!
//! ## Opaque Specialization
//!
//! [`Opaque`] types may specialize [`from_reflect`](Reflect::from_reflect):
//! since all opaque values can be serialized via [`Opaque::stringify`], they
//! can convert between different concrete types by serializing the source and
//! deserializing into the target.
//!
//! [`from_reflect`]: Reflect::from_reflect
//! [`ReflectKind`]: crate::info::ReflectKind
//! [`TypeDB`]: crate::db::TypeDB
//! [`Hash`]: core::hash::Hash
//! [`Eq`]: core::cmp::Eq
//! [`TypeId`]: core::any::TypeId

// -----------------------------------------------------------------------------
// Modules

mod array_ops;
mod enum_ops;
mod error;
mod kind;
mod list_ops;
mod map_ops;
mod opaque_ops;
mod reflect;
mod set_ops;
mod struct_ops;
mod tuple_ops;

// -----------------------------------------------------------------------------
// Exports

pub use error::{ApplyError, CloneError};
pub use kind::{ReflectMut, ReflectOwned, ReflectRef};
pub use reflect::Reflect;

pub use array_ops::{Array, ArrayItemIter};
pub use enum_ops::{Enum, VariantFieldIter};
pub use list_ops::{List, ListItemIter};
pub use map_ops::Map;
pub use opaque_ops::Opaque;
pub use set_ops::Set;
pub use struct_ops::{Struct, StructFieldIter};
pub use tuple_ops::{Tuple, TupleFieldIter};
