//! Resource identifiers.

// -----------------------------------------------------------------------------
// ResourceId
// -----------------------------------------------------------------------------

crate::utils::define_ident!(
    /// A unique identifier for a `Resource` type.
    ///
    /// IDs are assigned sequentially when a resource type is first
    /// registered, and are shared by all worlds.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_reflect::derive::TypePath;
    ///
    /// #[derive(TypePath, Resource)]
    /// struct Score(u32);
    ///
    /// // The id is assigned during registration:
    /// let db = ResourceDB::of::<Score>();
    /// let id = db.id;
    ///
    /// // Lookups by id return the same metadata:
    /// assert!(core::ptr::eq(ResourceDB::get_by_id(id), db));
    /// ```
    ///
    /// [`ResourceDB`]: crate::resource::ResourceDB
    ResourceId
);
