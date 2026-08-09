use zlim_utils::mem::Global;

use super::{Attributes, Generics, Type, VariantInfo};
use super::{impl_attributes_fn, impl_with_attributes};
use super::{impl_generics_fn, impl_type_fn, impl_with_generics};
use crate::ops::Enum;
use crate::path::TypePath;

// -----------------------------------------------------------------------------
// EnumInfo

/// A container for compile-time enum info.
#[derive(Debug)]
pub struct EnumInfo {
    ty: Type,
    variants: &'static [VariantInfo],
    // Needed for deserialization.
    variant_names: &'static [&'static str],
    generics: Generics,
    attributes: Attributes,
}

impl EnumInfo {
    impl_type_fn!(ty);
    impl_generics_fn!(generics);
    impl_with_generics!(generics);
    impl_attributes_fn!(attributes);
    impl_with_attributes!(attributes);

    /// Creates a new [`EnumInfo`].
    ///
    /// The order of internal variants is fixed, depends on the input order.
    pub fn new<TEnum: Enum + TypePath>(variants: &[VariantInfo]) -> Self {
        let variant_names: Vec<&'static str> = variants.iter().map(|v| v.name()).collect();

        Self {
            ty: Type::of::<TEnum>(),
            variants: Global::alloc_slice(variants),
            variant_names: Global::alloc_slice(variant_names.as_slice()),
            generics: Generics::EMPTY,
            attributes: Attributes::EMPTY,
        }
    }

    /// Returns the underlying variants slice.
    #[inline]
    pub fn variants(&self) -> &'static [VariantInfo] {
        self.variants
    }

    /// Returns the underlying variant names slice.
    #[inline]
    pub fn variant_names(&self) -> &'static [&'static str] {
        self.variant_names
    }

    /// Returns the number of variants.
    #[inline]
    pub fn variant_len(&self) -> usize {
        self.variants.len()
    }

    /// Returns the [`VariantInfo`] for the given variant name, if present.
    ///
    /// Complexity: O(n) in the number of variants.
    #[inline]
    pub fn variant(&self, name: &str) -> Option<&'static VariantInfo> {
        self.variants.iter().find(|f| f.name() == name)
    }

    /// Returns the [`VariantInfo`] at the given index, if present.
    ///
    /// Complexity: O(1).
    #[inline]
    pub fn variant_at(&self, index: usize) -> Option<&'static VariantInfo> {
        self.variants.get(index)
    }

    /// Returns the index for the given variant name, if present.
    ///
    /// Complexity: O(n) in the number of variants.
    #[inline]
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.variant_names.iter().position(|n| *n == name)
    }

    /// Returns the name for the given variant index, if present.
    ///
    /// Complexity: O(1).
    #[inline]
    pub fn name_at(&self, index: usize) -> Option<&'static str> {
        self.variant_names.get(index).copied()
    }

    /// Returns the full path for a variant name, e.g. `Type::Variant`.
    pub fn variant_path(&self, name: &str) -> String {
        crate::path::concat(&[self.type_path(), "::", name])
    }
}

// -----------------------------------------------------------------------------
