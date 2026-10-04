//! The assets a scene needs before it can be resolved.

use core::any::TypeId;

use zlim_asset::asset::Asset;
use zlim_asset::path::AssetPath;
// -----------------------------------------------------------------------------
// SceneDependencies

/// An asset a scene needs before it can be resolved.
#[derive(Debug)]
pub struct SceneDependency {
    /// The type of the asset.
    pub type_id: TypeId,

    /// Where the asset is loaded from.
    pub path: AssetPath<'static>,
}

/// The assets a [`Scene`](crate::Scene) or [`SceneList`](crate::SceneList) needs loaded before it resolves.
///
/// Resolution reads what a description depends on — the texture behind a sprite, the patch a scene
/// includes — and those assets have to be there when it does. Registering them here is how a scene
/// says so; the caller that resolves it (the loader of a scene asset, for a scene that comes from a
/// file) starts the loads and waits for them.
#[derive(Debug, Default)]
pub struct SceneDependencies(Vec<SceneDependency>);

impl SceneDependencies {
    /// Creates an empty list.
    #[inline]
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    /// Registers an asset of type `A` at `path`.
    #[inline]
    pub fn register<A: Asset>(&mut self, path: impl Into<AssetPath<'static>>) {
        self.register_erased(TypeId::of::<A>(), path.into());
    }

    /// Registers an asset of the given type at `path`.
    #[inline]
    pub fn register_erased(&mut self, type_id: TypeId, path: AssetPath<'static>) {
        self.0.push(SceneDependency { type_id, path });
    }

    /// Returns the number of registered dependencies.
    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether nothing was registered.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterates the registered dependencies.
    #[inline]
    pub fn iter(&self) -> core::slice::Iter<'_, SceneDependency> {
        self.0.iter()
    }
}

impl IntoIterator for SceneDependencies {
    type Item = SceneDependency;
    type IntoIter = std::vec::IntoIter<SceneDependency>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
