use crate::error::AssetLoadError;

// -----------------------------------------------------------------------------
// LoadState

/// The load state of an asset.
#[derive(Clone, Debug)]
pub enum LoadState {
    /// The asset has not started loading yet
    NotLoaded,

    /// The asset is in the process of loading.
    Loading,

    /// The asset has been loaded and has been added to the world.
    Loaded,

    /// The asset failed to load.
    ///
    /// The underlying [`AssetLoadError`] is referenced by `Arc` clones in all
    /// related [`DependencyLoadState`]s and  [`RecursiveDependencyLoadState`]s
    /// in the asset's dependency tree.
    Failed(AssetLoadError),
}

impl LoadState {
    /// Returns `true` if this instance is [`LoadState::NotLoaded`]
    pub const fn is_not_loaded(&self) -> bool {
        matches!(self, Self::NotLoaded)
    }

    /// Returns `true` if this instance is [`LoadState::Loading`]
    pub const fn is_loading(&self) -> bool {
        matches!(self, Self::Loading)
    }

    /// Returns `true` if this instance is [`LoadState::Loaded`]
    pub const fn is_loaded(&self) -> bool {
        matches!(self, Self::Loaded)
    }

    /// Returns `true` if this instance is [`LoadState::Failed`]
    pub const fn is_failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }
}

// -----------------------------------------------------------------------------
// DependencyLoadState

/// The load state of an asset's dependencies.
#[derive(Clone, Debug)]
pub enum DependencyLoadState {
    /// The asset has not started loading yet
    NotLoaded,

    /// Dependencies are still loading
    Loading,

    /// Dependencies have all loaded
    Loaded,

    /// The asset failed to load.
    ///
    /// The underlying [`AssetLoadError`] is referenced by `Arc` clones in
    /// all related [`LoadState`]s and [`RecursiveDependencyLoadState`]s
    /// in the asset's dependency tree.
    Failed(AssetLoadError),
}

impl DependencyLoadState {
    /// Returns `true` if this instance is [`DependencyLoadState::NotLoaded`]
    pub const fn is_not_loaded(&self) -> bool {
        matches!(self, Self::NotLoaded)
    }

    /// Returns `true` if this instance is [`DependencyLoadState::Loading`]
    pub const fn is_loading(&self) -> bool {
        matches!(self, Self::Loading)
    }

    /// Returns `true` if this instance is [`DependencyLoadState::Loaded`]
    pub const fn is_loaded(&self) -> bool {
        matches!(self, Self::Loaded)
    }

    /// Returns `true` if this instance is [`DependencyLoadState::Failed`]
    pub const fn is_failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }
}

// -----------------------------------------------------------------------------
// RecursiveDependencyLoadState

/// The recursive load state of an asset's dependencies.
#[derive(Clone, Debug)]
pub enum RecursiveDependencyLoadState {
    /// The asset has not started loading yet
    NotLoaded,

    /// Dependencies are still loading
    Loading,

    /// Dependencies have all loaded
    Loaded,

    /// The asset failed to load.
    ///
    /// The underlying [`AssetLoadError`] is referenced by `Arc` clones
    /// in all related [`LoadState`]s and [`DependencyLoadState`]s in
    /// the asset's dependency tree.
    Failed(AssetLoadError),
}

impl RecursiveDependencyLoadState {
    /// Returns `true` if this instance is [`RecursiveDependencyLoadState::NotLoaded`]
    pub const fn is_not_loaded(&self) -> bool {
        matches!(self, Self::NotLoaded)
    }

    /// Returns `true` if this instance is [`RecursiveDependencyLoadState::Loading`]
    pub const fn is_loading(&self) -> bool {
        matches!(self, Self::Loading)
    }

    /// Returns `true` if this instance is [`RecursiveDependencyLoadState::Loaded`]
    pub const fn is_loaded(&self) -> bool {
        matches!(self, Self::Loaded)
    }

    /// Returns `true` if this instance is [`RecursiveDependencyLoadState::Failed`]
    pub const fn is_failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }
}
