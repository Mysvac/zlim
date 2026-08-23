use core::any::TypeId;
use core::panic::Location;

use zlim_log as log;
use zlim_utils::mem::Global;
use zlim_utils::vec::SmallVec;

use crate::Reflect;
use crate::path::TypePath;

// -----------------------------------------------------------------------------
// Attributes

/// A builder for [`Attributes`].
///
/// Collects type-erased attribute values and then freezes them into a
/// `'static` allocation via [`finish`](Self::finish).
///
/// Uses a stack-allocated [`SmallVec`] with capacity 2 — the common case of
/// zero or one attributes avoids any heap traffic.
///
/// # Duplicates
///
/// Adding a second attribute of the same Rust type logs a warning (and keeps
/// the duplicate — the first match wins at lookup time).
///
/// # Example
///
/// ```ignore
/// use zlim_reflect::info::Attributes;
///
/// #[derive(Clone, Copy)]
/// struct Name(pub &'static str);
///
/// let attrs = Attributes::builder()
///     .with(Name("hello"))
///     .finish();
///
/// assert!(attrs.has::<Name>());
/// ```
#[repr(transparent)]
pub struct AttributesBuilder {
    buffer: SmallVec<&'static dyn Reflect, 2>,
}

impl AttributesBuilder {
    /// Appends a copy of `value` to the attribute set.
    ///
    /// The value is stored in a `'static` allocation so it can outlive the
    /// builder and be shared immutably.
    ///
    /// # Warnings
    ///
    /// Logs a warning when an attribute with the same [`TypeId`] has
    /// already been added.
    #[track_caller]
    pub fn with<T: Reflect + TypePath + Copy>(mut self, value: T) -> Self {
        let id: TypeId = TypeId::of::<T>();

        if self.buffer.iter().any(|&r| r.type_id() == id) {
            ::core::hint::cold_path();
            log::warn!(
                "Duplicate attributes: `{}`.\n\t`{}`",
                T::IDENT,
                Location::caller()
            );
        }

        self.buffer.push(Global::alloc_value(value));

        self
    }

    /// Freezes the collected attributes into a [`Attributes`] instance.
    ///
    /// The internal slice is promoted to a `'static` allocation so the
    /// resulting [`Attributes`] is cheaply [`Copy`].
    // #[inline(never)] // `alloc_slice` is `#[inline(never)]`
    pub fn finish(self) -> Attributes {
        Attributes {
            attributes: Global::alloc_slice(&self.buffer),
        }
    }
}

/// A frozen, copyable collection of custom attributes.
///
/// Wraps a `'static` slice of type-erased [`Reflect`] values.  Lookups use
/// [`TypeId`] equality, so every Rust type can appear at most once.
///
/// # Creation
///
/// Use [`Attributes::builder()`] (or the [`AttributesBuilder`] API directly)
/// to assemble attributes, then call [`AttributesBuilder::finish`] to freeze
/// them.  For the empty case the constant [`Attributes::EMPTY`] avoids any
/// allocation.
///
/// # Performance
///
/// The struct is `Copy` and `#[repr(transparent)]` — it is a single pointer
/// and passes through registers.  Lookups are linear scans over the internal
/// slice, which is acceptable because attribute counts are expected to be
/// very small (typically 0–3).
///
/// [`Reflect`]: crate::Reflect
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Attributes {
    attributes: &'static [&'static dyn Reflect],
}

impl Attributes {
    /// An empty attribute set.
    ///
    /// Use this instead of calling [`builder().finish()`](Self::builder)
    /// when there are no attributes — it avoids both stack and heap
    /// allocations.
    pub const EMPTY: Self = Self { attributes: &[] };

    /// Creates a new [`AttributesBuilder`] for collecting attributes.
    ///
    /// The builder uses a stack-allocated buffer with room for 2 attributes
    /// before spilling to the heap.
    #[inline(always)]
    pub const fn builder() -> AttributesBuilder {
        AttributesBuilder {
            buffer: SmallVec::new(),
        }
    }

    /// Returns the number of attributes stored.
    #[inline(always)]
    pub fn len(self) -> usize {
        self.attributes.len()
    }

    /// Returns `true` if no attributes are stored.
    #[inline(always)]
    pub fn is_empty(self) -> bool {
        self.attributes.is_empty()
    }

    /// Returns the underlying `'static` slice of all attributes.
    ///
    /// Useful for iteration or when callers need to inspect every attribute.
    #[inline(always)]
    pub fn as_slice(self) -> &'static [&'static dyn Reflect] {
        self.attributes
    }

    /// Returns `true` if an attribute with the given [`TypeId`] is present.
    ///
    /// Prefer [`has`](Self::has) when the type is known at compile time —
    /// that call cannot get the [`TypeId`] wrong.
    ///
    /// Complexity: O(n) in the number of attributes.
    pub fn has_by_id(self, id: TypeId) -> bool {
        self.attributes.iter().any(|&r| r.type_id() == id)
    }

    /// Returns the attribute with the given [`TypeId`], or `None`.
    ///
    /// The returned reference is type-erased; use [`get`](Self::get) when
    /// the type is known statically.
    ///
    /// Complexity: O(n) in the number of attributes.
    pub fn get_by_id(self, id: TypeId) -> Option<&'static dyn Reflect> {
        self.attributes
            .iter()
            .find(|&&attr| attr.type_id() == id)
            .copied()
            .map(|v| v as &dyn Reflect)
    }

    /// Returns `true` if an attribute of type `T` is present.
    ///
    /// Complexity: O(n) in the number of attributes.
    pub fn has<T: core::any::Any>(self) -> bool {
        let id = TypeId::of::<T>();
        self.attributes.iter().any(|&r| r.type_id() == id)
    }

    /// Returns a reference to the attribute of type `T`, or `None`.
    ///
    /// Complexity: O(n) in the number of attributes.
    pub fn get<T: Reflect>(self) -> Option<&'static T> {
        for &attr in self.attributes {
            if let Some(a) = attr.downcast_ref::<T>() {
                return Some(a);
            }
        }
        None
    }
}

impl core::fmt::Debug for Attributes {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("Attributes").field(&self.attributes).finish()
    }
}

// -----------------------------------------------------------------------------
// Auxiliary macros
//
// These generate the `attributes()` accessor, attribute lookup helpers,
// and the `with_attributes()` builder-style setter on every info type that
// carries custom attributes. The `$field:ident` form is used when a type has a
// named field; the `()` form is used for enums (like `VariantInfo` / `TypeInfo`)
// that delegate through a match.

/// Implements `attributes()` plus the `get_attribute*` /
/// `has_attribute*` convenience methods.
///
/// # Forms
///
/// | Form | Use case |
/// |------|----------|
/// | `impl_attributes_fn!(field_name)` | Struct with a named `Attributes` field. |
/// | `impl_attributes_fn!()` | Enum that dispatches `attributes()` manually. |
macro_rules! impl_attributes_fn {
    ($field:ident) => {
        /// Returns the custom attributes for this item.
        #[inline(always)]
        pub const fn attributes(&self) -> $crate::info::Attributes {
            self.$field
        }

        $crate::info::impl_attributes_fn!();
    };
    () => {
        /// Returns the attribute of type `T`, if present.
        pub fn get_attribute<T: $crate::Reflect>(&self) -> Option<&T> {
            self.attributes().get::<T>()
        }

        /// Returns the attribute with the given [`TypeId`], if present.
        ///
        /// [`TypeId`]: core::any::TypeId
        pub fn get_attribute_by_id(
            &self,
            type_id: ::core::any::TypeId,
        ) -> Option<&dyn $crate::Reflect> {
            self.attributes().get_by_id(type_id)
        }

        /// Returns `true` if an attribute of type `T` is present.
        pub fn has_attribute<T: ::core::any::Any>(&self) -> bool {
            self.attributes().has_by_id(::core::any::TypeId::of::<T>())
        }

        /// Returns `true` if an attribute with the given [`TypeId`] is present.
        ///
        /// [`TypeId`]: core::any::TypeId
        pub fn has_attribute_by_id(&self, type_id: ::core::any::TypeId) -> bool {
            self.attributes().has_by_id(type_id)
        }
    };
}

/// Implements `with_attributes()` — a builder-style setter that **replaces**
/// (does not merge) the attributes field.
///
/// Primarily used by the proc-macro derive crate to attach attributes
/// discovered at compile time.
macro_rules! impl_with_attributes {
    ($field:ident) => {
        /// Replaces stored attributes (overwrite, do not merge).
        ///
        /// Used by the proc-macro crate.
        pub fn with_attributes(self, attributes: $crate::info::Attributes) -> Self {
            Self {
                $field: attributes,
                ..self
            }
        }
    };
}

pub(crate) use impl_attributes_fn;
pub(crate) use impl_with_attributes;

// -----------------------------------------------------------------------------
