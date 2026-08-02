crate::utils::define_ident!(
    /// A unique identifier for a `Component` type within a specific `World`.
    ///
    /// `ComponentId` provides a type-safe way to identify component types at
    /// runtime. These IDs are only valid within the context of a single `World`
    /// instance and are not globally unique across different worlds.
    ComponentId
);
