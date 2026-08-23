//! Bundle metadata and the per-world bundle registry.
#![expect(clippy::len_without_is_empty, reason = "useless")]

use core::any::TypeId;
use core::fmt::{Debug, Formatter};

use zlim_utils::{ext::TypeMap, hash::HashMap};

use crate::component::ComponentId;

// -----------------------------------------------------------------------------
// BundleId
// -----------------------------------------------------------------------------

crate::utils::define_ident!(
    /// Unique identifier for a [`BundleInfo`] within a [`Bundles`] registry.
    ///
    /// `BundleId` is an opaque, niche-optimized handle backed by `NonMaxU32`.
    /// It is valid only within the context of a single [`Bundles`] instance.
    ///
    /// [`Bundles`]: crate::bundle::Bundles
    BundleId
);

impl BundleId {
    /// The ID of the empty bundle `()`.
    ///
    /// The empty bundle contains no components and is always available.
    /// It is used as a sentinel for entities that are spawned without
    /// any components.
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::bundle::BundleId;
    ///
    /// // `()` is always registered as the very first bundle.
    /// assert_eq!(BundleId::EMPTY.index(), 0);
    /// ```
    pub const EMPTY: Self = BundleId::without_provenance(0);
}

// -----------------------------------------------------------------------------
// BundleInfo
// -----------------------------------------------------------------------------

/// Metadata describing a registered component bundle.
///
/// `BundleInfo` records the set of [`ComponentId`]s that a bundle provides.
/// It does **not** store any entity data — that lives in the [`Table`].
///
/// # Deduplication
///
/// Multiple `Bundle` types that resolve to the same sorted set of component
/// IDs will share a single `BundleInfo`.  This is a space optimisation: the
/// table is keyed by the component set, not by the Rust type.
///
/// # Example
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_reflect::derive::TypePath;
///
/// #[derive(TypePath, Component, Clone)]
/// struct Health(u32);
///
/// let mut world = World::alloc();
///
/// // Registering two bundle types with the same component set yields the
/// // same `BundleId`, so they share one `BundleInfo`.
/// let a = world.register_required_bundle::<Health>();
/// let b = world.register_required_bundle::<(Health,)>();
/// assert_eq!(a, b);
/// ```
///
/// [`Table`]: crate::table::Table
pub struct BundleInfo {
    id: BundleId,
    /// The sorted list of component IDs in this bundle.
    components: &'static [ComponentId],
}

impl Debug for BundleInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Bundle")
            .field("id", &self.id)
            .field("components", &self.components)
            .finish()
    }
}

impl BundleInfo {
    /// Creates a new `BundleInfo` from the given ID and component slice.
    ///
    /// # Panics
    ///
    /// Panics in debug builds if `components` is not sorted.
    #[inline(always)]
    pub(super) fn new(id: BundleId, components: &'static [ComponentId]) -> Self {
        debug_assert!(components.is_sorted());
        Self { id, components }
    }

    /// Returns the unique identifier of this bundle.
    #[inline(always)]
    pub fn id(&self) -> BundleId {
        self.id
    }

    /// Returns the complete (sorted) list of component IDs in this bundle.
    #[inline(always)]
    pub fn components(&self) -> &'static [ComponentId] {
        self.components
    }

    /// Checks whether this bundle contains the given component type.
    ///
    /// Uses a SIMD-accelerated linear search.
    #[inline(always)]
    pub fn contains_component(&self, id: ComponentId) -> bool {
        crate::utils::contains_component(id, self.components)
    }
}

// -----------------------------------------------------------------------------
// Bundles
// -----------------------------------------------------------------------------

/// A collection of registered [`BundleInfo`] entries.
///
/// `Bundles` is a per-world registry that maps bundle types to their
/// associated component sets.  It provides fast lookups by:
///
/// - **Component slice** — for target-table resolution.
/// - **TypeId** — for bundle-type-based queries.
/// - **BundleId** — for direct access to metadata.
///
/// # Empty bundle
///
/// The empty bundle `()` is always registered at index 0 with
/// [`BundleId::EMPTY`].
pub struct Bundles {
    infos: Vec<BundleInfo>,
    /// Maps component ID slices to bundle IDs.
    mapper: HashMap<&'static [ComponentId], BundleId>,
    /// Maps Rust type IDs to bundle IDs.
    required_map: TypeMap<BundleId>,
    explicit_map: TypeMap<BundleId>,
}

impl Debug for Bundles {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        Debug::fmt(self.infos.as_slice(), f)
    }
}

impl Bundles {
    pub(crate) fn new() -> Self {
        let mut val = Bundles {
            infos: Vec::new(),
            mapper: HashMap::new(),
            required_map: TypeMap::new(),
            explicit_map: TypeMap::new(),
        };

        // The empty bundle is always available.
        val.infos.push(BundleInfo::new(BundleId::EMPTY, &[]));
        val.mapper.insert(&[], BundleId::EMPTY);
        val.required_map.insert(TypeId::of::<()>(), BundleId::EMPTY);
        val.explicit_map.insert(TypeId::of::<()>(), BundleId::EMPTY);

        val
    }
}

// ---------------------------------------------------------------------
// Methods

impl Bundles {
    /// Returns the number of registered bundles (including the empty bundle).
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.infos.len()
    }

    /// Returns the [`BundleInfo`] for the given [`BundleId`].
    #[inline(always)]
    pub fn get(&self, id: BundleId) -> Option<&BundleInfo> {
        self.infos.get(id.index())
    }

    /// Returns the [`BundleInfo`] for the given [`BundleId`] without
    /// bounds checking.
    ///
    /// # Safety
    ///
    /// The caller must ensure `id.index() < self.infos.len()`.
    #[inline(always)]
    pub unsafe fn get_unchecked(&self, id: BundleId) -> &BundleInfo {
        debug_assert!(id.index() < self.infos.len());
        unsafe { self.infos.get_unchecked(id.index()) }
    }

    /// Looks up the bundle ID for the given component set.
    ///
    /// Returns `None` if no bundle matches this exact set of components.
    pub fn get_by_arch(&self, components: &[ComponentId]) -> Option<BundleId> {
        self.mapper.get(components).copied()
    }

    /// Looks up the bundle ID for the given Rust type.
    ///
    /// Contains all components (including dependencies)
    ///
    /// Returns `None` if the type has not been registered as a bundle.
    pub fn get_required(&self, id: TypeId) -> Option<BundleId> {
        self.required_map.get(id).copied()
    }

    /// Looks up the bundle ID for the given Rust type.
    ///
    /// Only explicitly provided components.
    ///
    /// Returns `None` if the type has not been registered as a bundle.
    pub fn get_explicit(&self, id: TypeId) -> Option<BundleId> {
        self.explicit_map.get(id).copied()
    }
}

// ---------------------------------------------------------------------
// Internal

impl Bundles {
    /// Registers a new bundle for the given component set and returns its
    /// [`BundleId`].
    ///
    /// If a bundle with the same component set already exists, the existing
    /// ID is returned (deduplication).  The `type_id` is always recorded
    /// so that `get_by_type` works for all types that share this bundle.
    ///
    /// # Safety
    ///
    /// - Each `ComponentId` in `components` must be valid and registered.
    /// - `components` must be sorted.
    /// - `components` must not contain duplicates.
    #[inline]
    pub(crate) fn register_required(
        &mut self,
        type_id: TypeId,
        components: &'static [ComponentId],
    ) -> BundleId {
        if let Some(&id) = self.mapper.get(components) {
            // Already registered — map this type_id to the existing bundle.
            self.required_map.insert(type_id, id);
            id
        } else {
            core::hint::cold_path();
            let index = self.infos.len();
            let id = BundleId::without_provenance(index);

            self.infos.push(BundleInfo::new(id, components));
            self.mapper.insert(components, id);
            self.required_map.insert(type_id, id);

            id
        }
    }

    /// Registers a new bundle for the given component set and returns its
    /// [`BundleId`].
    ///
    /// If a bundle with the same component set already exists, the existing
    /// ID is returned (deduplication).  The `type_id` is always recorded
    /// so that `get_by_type` works for all types that share this bundle.
    ///
    /// # Safety
    ///
    /// - Each `ComponentId` in `components` must be valid and registered.
    /// - `components` must be sorted.
    /// - `components` must not contain duplicates.
    #[inline]
    pub(crate) fn register_explicit(
        &mut self,
        type_id: TypeId,
        components: &'static [ComponentId],
    ) -> BundleId {
        if let Some(&id) = self.mapper.get(components) {
            // Already registered — map this type_id to the existing bundle.
            self.explicit_map.insert(type_id, id);
            id
        } else {
            core::hint::cold_path();
            let index = self.infos.len();
            let id = BundleId::without_provenance(index);

            self.infos.push(BundleInfo::new(id, components));
            self.mapper.insert(components, id);
            self.explicit_map.insert(type_id, id);

            id
        }
    }

    /// Registers a new bundle for the given component set and returns its
    /// [`BundleId`].
    ///
    /// If a bundle with the same component set already exists, the existing
    /// ID is returned (deduplication).
    ///
    /// # Safety
    ///
    /// - Each `ComponentId` in `components` must be valid and registered.
    /// - `components` must be sorted.
    /// - `components` must not contain duplicates.
    #[inline]
    pub(crate) fn register_dynamic(&mut self, components: &'static [ComponentId]) -> BundleId {
        if let Some(&id) = self.mapper.get(components) {
            id
        } else {
            core::hint::cold_path();
            let index = self.infos.len();
            let id = BundleId::without_provenance(index);

            self.infos.push(BundleInfo::new(id, components));
            self.mapper.insert(components, id);

            id
        }
    }
}

// -----------------------------------------------------------------------------
