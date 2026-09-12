//! Asset lifecycle events.

use core::fmt::{Debug, Formatter};
use std::path::PathBuf;

use zlim_core::derive::Message;
use zlim_path::TypePath;

use crate::asset::Asset;
use crate::ident::AssetId;

// -----------------------------------------------------------------------------
// AssetSourceEvent
// -----------------------------------------------------------------------------

/// An "asset source change event" that occurs whenever asset (or asset metadata)
/// is created/added/removed.
///
/// Emitted by [watcher] when `notify` cargo feature is enabled (if possible).
///
/// The internal [`PathBuf`] is un-nomalized, may contains `\` seperator on windows.
///
/// [watcher]: crate::io::watcher
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssetSourceEvent {
    /// An asset at this path was added.
    AddedAsset(PathBuf),
    /// An asset at this path was modified.
    ModifiedAsset(PathBuf),
    /// An asset at this path was removed.
    RemovedAsset(PathBuf),
    /// An asset at this path was renamed.
    RenamedAsset { old: PathBuf, new: PathBuf },
    /// Asset metadata at this path was added.
    AddedMeta(PathBuf),
    /// Asset metadata at this path was modified.
    ModifiedMeta(PathBuf),
    /// Asset metadata at this path was removed.
    RemovedMeta(PathBuf),
    /// Asset metadata at this path was renamed.
    RenamedMeta { old: PathBuf, new: PathBuf },
    /// A folder at the given path was added.
    AddedFolder(PathBuf),
    /// A folder at the given path was removed.
    RemovedFolder(PathBuf),
    /// A folder at the given path was renamed.
    RenamedFolder { old: PathBuf, new: PathBuf },
    /// Something of unknown type was removed.
    ///
    /// It is the job of the event handler to determine the type.
    /// This exists because notify-rs produces "untyped" rename events
    /// without destination paths for unwatched folders, so we can't
    /// determine the type of the rename.
    RemovedUnknown {
        /// The path of the removed asset or folder (undetermined).
        ///
        /// This could be an asset path or a folder. This will not be a "meta file" path.
        path: PathBuf,
        /// This field is only relevant if `path` is determined to be an asset path (and therefore not a folder).
        ///
        /// - If this field is `true`, then this event corresponds to a meta removal (not an asset removal) .
        /// - If `false`, then this event corresponds to an asset removal (not a meta removal).
        is_meta: bool,
    },
}

// -----------------------------------------------------------------------------
// AssetEvent
// -----------------------------------------------------------------------------

/// Lifecycle events of the assets of type `A`.
///
/// Events are not written to the ECS immediately: [`Assets<A>`] pushes them
/// into its `queued_events` while assets are added, modified or removed, and
/// the `asset_events` job flushes that queue into a [`MessageQueue`] once per
/// frame (registered by the asset plugin).
///
/// Consumers therefore read them with [`MessageReader`] and see at most one
/// `Added` before the matching `Modified`/`Removed` pairs.
///
/// [`Assets<A>`]: crate::assets::Assets
/// [`MessageQueue`]: zlim_core::message::MessageQueue
/// [`MessageReader`]: zlim_core::message::MessageReader
#[derive(TypePath, Message)]
pub enum AssetEvent<A: Asset> {
    /// Emitted whenever an [`Asset`] is added.
    Added {
        /// The id the value was stored under.
        id: AssetId<A>,
    },
    /// Emitted whenever an [`Asset`] value is modified.
    Modified {
        /// The id of the modified value.
        id: AssetId<A>,
    },
    /// Emitted whenever an [`Asset`] is removed.
    Removed {
        /// The id that is no longer stored.
        id: AssetId<A>,
    },
    /// Emitted when the last strong handle of an [`Asset`] is dropped.
    Unused {
        /// The id whose reference count reached zero.
        id: AssetId<A>,
    },
    /// Emitted when an [`Asset`] and all of its recursive dependencies finished loading.
    FullyLoaded {
        /// The id of the fully loaded asset.
        id: AssetId<A>,
    },
}

impl<A: Asset> AssetEvent<A> {
    /// Returns the id this event refers to, whichever variant it is.
    #[inline]
    pub const fn id(&self) -> AssetId<A> {
        match self {
            Self::Added { id }
            | Self::Modified { id }
            | Self::Removed { id }
            | Self::Unused { id }
            | Self::FullyLoaded { id } => *id,
        }
    }

    /// Returns `true` if this is an [`Added`](Self::Added) event for `asset_id`.
    pub fn is_added(&self, asset_id: impl Into<AssetId<A>>) -> bool {
        matches!(self, Self::Added { id } if *id == asset_id.into())
    }

    /// Returns `true` if this is a [`Modified`](Self::Modified) event for `asset_id`.
    pub fn is_modified(&self, asset_id: impl Into<AssetId<A>>) -> bool {
        matches!(self, Self::Modified { id } if *id == asset_id.into())
    }

    /// Returns `true` if this is a [`Removed`](Self::Removed) event for `asset_id`.
    pub fn is_removed(&self, asset_id: impl Into<AssetId<A>>) -> bool {
        matches!(self, Self::Removed { id } if *id == asset_id.into())
    }

    /// Returns `true` if this is an [`Unused`](Self::Unused) event for `asset_id`.
    pub fn is_unused(&self, asset_id: impl Into<AssetId<A>>) -> bool {
        matches!(self, Self::Unused { id } if *id == asset_id.into())
    }

    /// Returns `true` if this is a [`FullyLoaded`](Self::FullyLoaded) event for `asset_id`.
    pub fn is_fully_loaded(&self, asset_id: impl Into<AssetId<A>>) -> bool {
        matches!(self, Self::FullyLoaded { id } if *id == asset_id.into())
    }
}

// The manual impls below deliberately do not require `A: Clone` / `A: Debug` / `A: PartialEq`:
// an event only carries an `AssetId<A>`, which is `Copy` and comparable for every `A: Asset`.

impl<A: Asset> Clone for AssetEvent<A> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<A: Asset> Copy for AssetEvent<A> {}

impl<A: Asset> Debug for AssetEvent<A> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Added { id } => f.debug_struct("Added").field("id", id).finish(),
            Self::Modified { id } => f.debug_struct("Modified").field("id", id).finish(),
            Self::Removed { id } => f.debug_struct("Removed").field("id", id).finish(),
            Self::Unused { id } => f.debug_struct("Unused").field("id", id).finish(),
            Self::FullyLoaded { id } => f.debug_struct("FullyLoaded").field("id", id).finish(),
        }
    }
}

impl<A: Asset> PartialEq for AssetEvent<A> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Added { id: left }, Self::Added { id: right })
            | (Self::Modified { id: left }, Self::Modified { id: right })
            | (Self::Removed { id: left }, Self::Removed { id: right })
            | (Self::Unused { id: left }, Self::Unused { id: right })
            | (Self::FullyLoaded { id: left }, Self::FullyLoaded { id: right }) => left == right,
            _ => false,
        }
    }
}

impl<A: Asset> Eq for AssetEvent<A> {}

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use zlim_path::derive::TypePath;

    use super::*;
    use crate::asset::VisitAssetDependencies;

    #[derive(TypePath)]
    struct TestAsset;

    impl VisitAssetDependencies for TestAsset {
        fn visit_dependencies(&self, _visit: &mut dyn FnMut(crate::ident::ErasedAssetId)) {}
    }

    impl Asset for TestAsset {}

    fn index_id(index: u32) -> AssetId<TestAsset> {
        AssetId::from(crate::ident::AssetIndex {
            index,
            generation: 0,
        })
    }

    #[test]
    fn predicates_match_their_own_variant() {
        let id = index_id(1);
        let added: AssetEvent<TestAsset> = AssetEvent::Added { id };
        let modified: AssetEvent<TestAsset> = AssetEvent::Modified { id };
        let removed: AssetEvent<TestAsset> = AssetEvent::Removed { id };
        let unused: AssetEvent<TestAsset> = AssetEvent::Unused { id };
        let loaded: AssetEvent<TestAsset> = AssetEvent::FullyLoaded { id };

        assert!(added.is_added(id) && !added.is_modified(id) && !added.is_removed(id));
        assert!(modified.is_modified(id) && !modified.is_added(id));
        assert!(removed.is_removed(id) && !removed.is_unused(id));
        assert!(unused.is_unused(id) && !unused.is_removed(id));
        assert!(loaded.is_fully_loaded(id) && !loaded.is_added(id));

        for event in [added, modified, removed, unused, loaded] {
            assert_eq!(event.id(), id);
        }
    }

    #[test]
    fn events_of_different_variants_are_not_equal() {
        let id = index_id(1);
        assert_ne!(
            AssetEvent::<TestAsset>::Added { id },
            AssetEvent::<TestAsset>::Modified { id }
        );
        assert_ne!(
            AssetEvent::<TestAsset>::Removed { id },
            AssetEvent::<TestAsset>::Unused { id }
        );
        assert_ne!(
            AssetEvent::<TestAsset>::Added { id: index_id(1) },
            AssetEvent::<TestAsset>::Added { id: index_id(2) }
        );
        assert_eq!(
            AssetEvent::<TestAsset>::Added { id },
            AssetEvent::<TestAsset>::Added { id }
        );
    }

    #[test]
    fn uuid_events_carry_the_uuid_id() {
        let id: AssetId<TestAsset> = AssetId::default();
        let event: AssetEvent<TestAsset> = AssetEvent::Added { id };
        assert!(event.is_added(id));
        assert_eq!(event.id().uuid(), Some(AssetId::<TestAsset>::DEFAULT_UUID));
    }
}
