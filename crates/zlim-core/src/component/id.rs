//! Component identifiers.

// -----------------------------------------------------------------------------
// ComponentId
// -----------------------------------------------------------------------------

crate::utils::define_ident!(
    /// A unique identifier for a [`Component`] type.
    ///
    /// Component IDs are assigned sequentially at registration time and are
    /// **shared by all worlds** — the same type always maps to the same
    /// `ComponentId` in every world.  Obtain one from a [`ComponentDB`],
    /// e.g. [`ComponentDB::of::<T>()`](ComponentDB::of).id.
    ///
    /// The ID is niche-optimized over a `NonMaxU32`, so
    /// `Option<ComponentId>` has no size overhead. It supports `Copy`,
    /// `Eq`, `Ord`, `Hash`, `Debug`, and `Display`.
    ///
    /// [`Component`]: crate::component::Component
    /// [`ComponentDB`]: crate::component::ComponentDB
    /// [`ComponentDB::of`]: crate::component::ComponentDB::of
    ComponentId
);
