//! The [`Asset`] contract, dependency enumeration, and the [`AssetComponent`] handle-to-id
//! protocol that change tracking is built on.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use zlim_core::component::Component;
use zlim_path::TypePath;
use zlim_utils::hash::{HashMap, HashSet};

use crate::handle::{ErasedHandle, Handle};
use crate::ident::{AssetId, ErasedAssetId};

pub use zlim_asset_derive::Asset;
pub use zlim_asset_derive::VisitAssetDependencies;

// -----------------------------------------------------------------------------
// Asset

/// Marker trait for every type that can be loaded and managed by the asset server.
#[diagnostic::on_unimplemented(
    message = "`{Self}` does not implement `Asset`",
    label = "invalid `Asset`",
    note = "consider annotating `{Self}` with `#[derive(Asset)]`"
)]
pub trait Asset: VisitAssetDependencies + TypePath + Send + Sync + 'static {}

// -----------------------------------------------------------------------------
// VisitAssetDependencies

/// Enumerates the ids of the assets a value directly depends on.
pub trait VisitAssetDependencies {
    /// Calls `visit` once for every asset this value directly depends on.
    fn visit_dependencies(&self, visit: &mut dyn FnMut(ErasedAssetId));
}

// -----------------------------------------------------------------------------
// () — placeholder asset

/// `()` is the placeholder [`Asset`]: it stands in for "no particular asset type",
/// which is what lets a bare [`AssetId`] of `()` be written where an asset type is
/// required but no asset is meant.
impl Asset for () {}

// A `()` carries no asset ids, so there is nothing to visit:
// reaching the visitor means the caller built a dependency graph for the placeholder.
// `unreachable!()` reports that instead of silently visiting nothing.
impl VisitAssetDependencies for () {
    #[cold]
    fn visit_dependencies(&self, _visit: &mut dyn FnMut(ErasedAssetId)) {
        unreachable!("`()` is a placeholder, should not be used to visit asset deps")
    }
}

// -----------------------------------------------------------------------------
// VisitAssetDependencies impls

impl VisitAssetDependencies for ErasedAssetId {
    #[inline]
    fn visit_dependencies(&self, visit: &mut dyn FnMut(ErasedAssetId)) {
        visit(*self);
    }
}

impl VisitAssetDependencies for ErasedHandle {
    #[inline]
    fn visit_dependencies(&self, visit: &mut dyn FnMut(ErasedAssetId)) {
        visit(self.id());
    }
}

impl<A: Asset> VisitAssetDependencies for Handle<A> {
    #[inline]
    fn visit_dependencies(&self, visit: &mut dyn FnMut(ErasedAssetId)) {
        visit(self.id().erased());
    }
}

impl<V: VisitAssetDependencies> VisitAssetDependencies for Option<V> {
    #[inline]
    fn visit_dependencies(&self, visit: &mut dyn FnMut(ErasedAssetId)) {
        if let Some(dependency) = self {
            dependency.visit_dependencies(visit);
        }
    }
}

impl<V: VisitAssetDependencies> VisitAssetDependencies for Box<V> {
    #[inline]
    fn visit_dependencies(&self, visit: &mut dyn FnMut(ErasedAssetId)) {
        (**self).visit_dependencies(visit);
    }
}

impl<V: VisitAssetDependencies, const N: usize> VisitAssetDependencies for [V; N] {
    fn visit_dependencies(&self, visit: &mut dyn FnMut(ErasedAssetId)) {
        for dependency in self {
            dependency.visit_dependencies(visit);
        }
    }
}

impl<V: VisitAssetDependencies> VisitAssetDependencies for [V] {
    fn visit_dependencies(&self, visit: &mut dyn FnMut(ErasedAssetId)) {
        for dependency in self {
            dependency.visit_dependencies(visit);
        }
    }
}

impl<V: VisitAssetDependencies> VisitAssetDependencies for Vec<V> {
    fn visit_dependencies(&self, visit: &mut dyn FnMut(ErasedAssetId)) {
        for dependency in self {
            dependency.visit_dependencies(visit);
        }
    }
}

impl<V: VisitAssetDependencies> VisitAssetDependencies for VecDeque<V> {
    fn visit_dependencies(&self, visit: &mut dyn FnMut(ErasedAssetId)) {
        for dependency in self {
            dependency.visit_dependencies(visit);
        }
    }
}

impl<V: VisitAssetDependencies> VisitAssetDependencies for HashSet<V> {
    fn visit_dependencies(&self, visit: &mut dyn FnMut(ErasedAssetId)) {
        for value in self {
            value.visit_dependencies(visit);
        }
    }
}

impl<K, V: VisitAssetDependencies> VisitAssetDependencies for HashMap<K, V> {
    fn visit_dependencies(&self, visit: &mut dyn FnMut(ErasedAssetId)) {
        for dependency in self.values() {
            dependency.visit_dependencies(visit);
        }
    }
}

impl<V: VisitAssetDependencies> VisitAssetDependencies for BTreeSet<V> {
    fn visit_dependencies(&self, visit: &mut dyn FnMut(ErasedAssetId)) {
        for dependency in self {
            dependency.visit_dependencies(visit);
        }
    }
}

impl<K, V: VisitAssetDependencies> VisitAssetDependencies for BTreeMap<K, V> {
    fn visit_dependencies(&self, visit: &mut dyn FnMut(ErasedAssetId)) {
        for dependency in self.values() {
            dependency.visit_dependencies(visit);
        }
    }
}

// -----------------------------------------------------------------------------
// AssetComponent

/// A component that exposes the id of the asset it refers to.
///
/// This is the handle-to-id protocol used by [`AssetChanged`]:
///
/// ```rust, ignore
/// #[derive(TypePath, Component)]
/// struct MaterialRef(Handle<Material>);
///
/// impl AssetComponent for MaterialRef {
///     type Asset = Material;
///     fn asset_id(&self) -> AssetId<Material> {
///         self.0.id()
///     }
/// }
///
/// // Then, on the entity that references the asset:
/// fn react(materials: Query<&MaterialRef, AssetChanged<MaterialRef>>) { /* ... */ }
/// ```
///
/// This trait exists only to support change detection. Implementing it is not
/// required for components that hold an asset; it is merely recommended, as it
/// enables [`AssetChanged`] to work with the component.
///
/// [`AssetChanged`]: crate::change::AssetChanged
pub trait AssetComponent: Component {
    /// The asset type this component refers to.
    type Asset: Asset;

    /// Returns the id of the referenced asset.
    fn asset_id(&self) -> AssetId<Self::Asset>;
}

// -----------------------------------------------------------------------------
