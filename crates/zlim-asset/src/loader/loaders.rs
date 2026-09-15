//! The [`AssetLoader`] registry and the reverse indexes used to pick a loader.

use core::any::TypeId;
use std::sync::Arc;

use zlim_path::TypePath;
use zlim_utils::ext::TypeMap;
use zlim_utils::hash::HashMap;
use zlim_utils::vec::SmallVec;

use crate::error::AmbiguousName;
use crate::loader::{AssetLoader, ErasedAssetLoader};
use crate::path::AssetPath;
use crate::utils::{iter_secondary_extensions, normalize_extension, normalize_extension_ref};

// -----------------------------------------------------------------------------

/// The loaders one short type name has.
///
/// The first loader that claims a name leaves it [`Unique`](Self::Unique); the second turns it into
/// [`Ambiguous`](Self::Ambiguous), which keeps the type paths of every claimant: the error names
/// them, and a look-up that found a candidate another way (by asset type or by extension) asks
/// whether it is one of them.
enum MaybeAmbiguous {
    Unique(u16),
    Ambiguous(Vec<&'static str>),
}

// -----------------------------------------------------------------------------
// AssetLoaders

cfg_select! {
    target_pointer_width = "32" => {
        const INLINE: usize = 4;
    }
    _ => {
        const INLINE: usize = 8;
    }
}

/// Every [`AssetLoader`] known to the asset server, plus the indexes used to
/// select one for a given asset.
///
/// Loaders are stored once and referenced by index. The maps below are the
/// reverse indexes consumed by [`find`](Self::find):
///
/// - `extension_to_loaders`: file extension → loaders, newest last, so the most
///   recently registered loader for an extension wins. Keys are lower-case and
///   without the leading dot: registration goes through `normalize_extension`
///   (which interns only an extension that has to be rewritten) and look-ups
///   through `normalize_extension_ref` (which borrows or allocates a temporary,
///   never interning, because the extension comes from the asset path).
///
/// - `type_path_to_loader` / `type_name_to_loader`: the loader's [`TypePath`] /
///   type name → loader. The type path is the strict form; the type name is the lenient one — a
///   look-up by name accepts a full type path as well, which is what lets a string read out of a
///   `.meta` file be used as it is — and a name several loaders share carries the type paths of all
///   of them (see [`MaybeAmbiguous`]), so that an ambiguity is reported instead of being guessed at.
///
/// - `asset_type_id_to_loaders`: the produced asset [`TypeId`] → loaders.
///
/// A loader may be *pre-registered* before it exists (see [`reserve`](Self::reserve)):
/// its index is handed out, the reverse indexes point at it like they do for a registered
/// loader, and the entry itself is a [`PendingAssetLoaderCell`] that only resolves once the
/// loader is registered — a waiter gets a [`PendingAssetLoader`] out of
/// [`get_by_index`](Self::get_by_index) and awaits its `get`. This is what lets a load started
/// before its loader is registered wait instead of failing. Registering that loader completes
/// the entry in place (see [`push`](Self::push)), which is what releases the waiters.
#[derive(Default)]
pub(crate) struct AssetLoaders {
    loaders: Vec<AssetLoaderCell>,
    extension_to_loaders: HashMap<&'static str, SmallVec<u16, INLINE>>,
    type_name_to_loader: HashMap<&'static str, MaybeAmbiguous>,
    type_path_to_loader: HashMap<&'static str, u16>,
    asset_type_id_to_loaders: TypeMap<SmallVec<u16, INLINE>>,
}

impl AssetLoaders {
    /// Returns the entry stored at `index`.
    ///
    /// The entry is an [`AssetLoaderCell`]: `Ok` is the loader itself, and `Err` marks a
    /// pre-registered loader that is not registered yet — the caller has to await
    /// [`PendingAssetLoader::get`] to get the loader itself.
    ///
    /// The index is never out of range: it always comes out of one of this registry's own maps.
    #[inline(always)]
    fn get_by_index(&self, index: u16) -> Result<Arc<dyn ErasedAssetLoader>, PendingAssetLoader> {
        match &self.loaders[index as usize] {
            Ok(loader) => Ok(loader.clone()),
            Err(pending) => {
                ::core::hint::cold_path();
                Err(PendingAssetLoader(pending.receiver.clone()))
            }
        }
    }

    /// Returns `true` when a loader is registered under the loader type path `path`.
    ///
    /// An entry that is pre-registered but whose loader has not been registered yet reports
    /// `false`: this asks whether a loader is ready, not whether its index exists.
    pub fn contains(&self, path: &str) -> bool {
        let Some(&index) = self.type_path_to_loader.get(path) else {
            return false;
        };
        matches!(self.loaders.get(index as usize), Some(Ok(_)))
    }

    /// Returns the loader registered under the loader type path `path`.
    pub fn get_by_path(
        &self,
        path: &str,
    ) -> Option<Result<Arc<dyn ErasedAssetLoader>, PendingAssetLoader>> {
        let index = self.type_path_to_loader.get(path).copied()?;
        Some(self.get_by_index(index))
    }

    /// Finds the loader that should load the described asset.
    ///
    /// Every argument is optional and is only consulted when the steps before it produced
    /// nothing. In order:
    ///
    /// 1. **Loader type path** ([`get_by_path`](Self::get_by_path)) — the strict form, and what a
    ///    `.meta` file names. An empty string counts as *no* path. It is authoritative: the result is
    ///    returned as-is, even when no loader has that path (then the answer is `Err(None)` and the
    ///    later steps are *not* tried).
    ///
    /// 2. **Loader type name** — the lenient form: a full type path resolves as well, so a string
    ///    read out of a `.meta` file does not have to be classified first. An empty string counts as
    ///    *no* name. A name nothing has is `Err(None)`, but a name several loaders share does *not*
    ///    end the search: its candidates are remembered, and the steps below decide between them.
    ///
    /// 3. **Asset type** — the loaders that produce `asset_type_id`. This narrowing is only
    ///    applied to paths *without* a label, since a sub-asset may legitimately have a different
    ///    type than the loader producing it. If the type is known but nothing produces it, the
    ///    search stops here (`Err(None)`). If exactly one loader produces it, that loader is the
    ///    answer — unless a name was given as well, in which case it has to be one of that name's
    ///    candidates, and a loader that is not is `Err(None)`.
    ///
    /// 4. **Extension of `asset_path`** — its full extension first (`foo.tar.gz` → `tar.gz`), then
    ///    every secondary extension (`gz`), i.e. each suffix starting after a `.` (see
    ///    [`iter_secondary_extensions`]). Extensions are compared case-insensitively and a leading
    ///    `.` is ignored. Within one extension the newest loader wins; a loader that does not
    ///    produce `asset_type_id` (when the type is known) or is not one of the name's candidates
    ///    (when a name was given) is skipped, and the next candidate — or the next extension — is
    ///    tried. The asset type therefore always wins over the extension: within this phase a loader
    ///    that does not produce `asset_type_id` is never returned. That holds only when the type is
    ///    known and no loader was named explicitly — the explicit name of a `.meta` file is
    ///    returned unchecked (see *Guarantees*).
    ///
    /// Finally, when nothing matched:
    ///
    /// - With an asset type, its newest loader that is also one of the name's candidates (or its
    ///   newest loader at all, when no name was given) is used as a last resort and a warning is
    ///   logged — this only happens with several candidates, since a single one returned in step 3.
    /// - Without one, the answer is the ambiguity error when a name had several candidates, and
    ///   `Err(None)` otherwise.
    ///
    /// # Errors
    ///
    /// - `Err(None)`: nothing could be selected.
    /// - `Err(Some(_))`: a name was given, several loaders share it, and neither the asset type nor
    ///   the extension could tell which of them was meant; [`AmbiguousName`] lists the type paths
    ///   that were.
    ///
    /// # Guarantees
    ///
    /// The result is a best effort: `find` does *not* check that the loader actually produces
    /// `asset_type_id`. The caller verifies the produced type once the asset is loaded and
    /// reports a mismatch instead of storing it under the requested handle.
    ///
    /// The returned entry may still be waiting for its loader to be registered, so the loader
    /// itself has to be awaited out of it (`PendingAssetLoader::get`); that wait is what makes an
    /// asset loadable before the loader that reads it has been registered.
    ///
    /// [`iter_secondary_extensions`]: crate::utils::iter_secondary_extensions
    pub fn find(
        &self,
        type_path: Option<&str>,
        type_name: Option<&str>,
        asset_type_id: Option<TypeId>,
        asset_path: Option<&AssetPath<'_>>,
    ) -> Result<Result<Arc<dyn ErasedAssetLoader>, PendingAssetLoader>, Option<AmbiguousName>> {
        // ---------------------------------------------------------------
        // Type path or type name (an explicit choice in a `.meta` file)
        // ---------------------------------------------------------------

        let path = type_path.unwrap_or("");
        if !path.is_empty() {
            return self.get_by_path(path).ok_or(None);
        }

        // A name that several loaders share does not end the search: the asset type and the extension
        // below can still tell which of the candidates was meant, and only when they cannot is the
        // ambiguity itself the answer. The candidates are therefore kept as they are, and the
        // `AmbiguousName` is built where it is returned.
        let name = type_name.unwrap_or("");
        let ambiguous: Option<&[&'static str]> = if name.is_empty() {
            None
        } else {
            if let Some(entry) = self.get_by_path(name) {
                return Ok(entry);
            }
            match self.type_name_to_loader.get(name) {
                Some(MaybeAmbiguous::Unique(index)) => return Ok(self.get_by_index(*index)),
                Some(MaybeAmbiguous::Ambiguous(paths)) => Some(paths.as_slice()),
                None => return Err(None),
            }
        };

        // Whether `index` is one of the loaders the ambiguous name could have meant.
        let validate_ambiguous = |ambiguous: &[&str], index: u16| -> bool {
            ambiguous
                .iter()
                .any(|path| self.type_path_to_loader.get(*path) == Some(&index))
        };

        // ---------------------------------------------------------------
        // Asset type (only when the path has no label)
        // ---------------------------------------------------------------

        let label = asset_path.and_then(AssetPath::label);

        let mut candidates: Option<&[u16]> = None;

        if label.is_none()
            && let Some(type_id) = asset_type_id
        {
            candidates = Some(self.asset_type_id_to_loaders.get(type_id).ok_or(None)?);
        };

        if let Some(candidates) = candidates
            && candidates.len() == 1
        {
            let index = candidates[0];
            let Some(ambiguous) = ambiguous else {
                return Ok(self.get_by_index(index));
            };
            ::core::hint::cold_path();
            // The type has one loader, so the only question left is whether it is one of the
            // candidates the name could have meant.
            if validate_ambiguous(ambiguous, index) {
                return Ok(self.get_by_index(index));
            } else {
                return Err(None);
            }
        }

        // ---------------------------------------------------------------
        // Extension
        // ---------------------------------------------------------------

        let try_extension = |extension: &str| {
            // The extension comes from the asset path (user data), so it is normalized into a
            // borrowed or temporary string instead of being interned into the registry's pool.
            let extension = normalize_extension_ref(extension);
            let indices = self.extension_to_loaders.get(extension.as_ref())?;

            // Newest first: the loader registered last for the extension wins.
            for index in indices.iter().rev() {
                if let Some(list) = candidates
                    && !list.contains(index)
                {
                    // The asset type is known, so a loader that does not produce it is out.
                    continue;
                }

                if let Some(ambiguous) = ambiguous
                    && !validate_ambiguous(ambiguous, *index)
                {
                    ::core::hint::cold_path();
                    // A name was given and this loader is not one of its candidates.
                    continue;
                }

                return Some(*index);
            }

            None
        };

        // The extensions carried by the asset path.
        if let Some(full_extension) = asset_path.and_then(AssetPath::full_extension) {
            if let Some(index) = try_extension(full_extension) {
                return Ok(self.get_by_index(index));
            }

            for extension in iter_secondary_extensions(full_extension) {
                if let Some(index) = try_extension(extension) {
                    return Ok(self.get_by_index(index));
                }
            }
        }

        // ---------------------------------------------------------------
        // Fallback: the newest loader for the asset type
        // ---------------------------------------------------------------

        let Some(candidates) = candidates else {
            // No asset type to fall back on: the only thing left to report is the ambiguity, when
            // the name had several candidates.
            let Some(ambiguous) = ambiguous else {
                return Err(None);
            };

            ::core::hint::cold_path();
            let type_name = *self.type_name_to_loader.get_key_value(name).unwrap().0;
            return Err(Some(AmbiguousName {
                service: "loader",
                type_name,
                type_paths: ambiguous.to_vec(),
            }));
        };

        for &index in candidates.iter().rev() {
            if ambiguous.is_none_or(|a| validate_ambiguous(a, index)) {
                #[cfg(any(debug_assertions, feature = "debug"))]
                zlim_log::warn!(
                    "Multiple AssetLoaders found for Asset Path: `{asset_path:?}`; Asset TypeId: `{asset_type_id:?}`;"
                );
                return Ok(self.get_by_index(index));
            }
        }

        // An asset type was given, so a loader that produces it but is not one of the candidates the
        // name meant is a plain miss rather than an ambiguity.
        Err(None)
    }
}

impl AssetLoaders {
    /// Registers `loader`, making it reachable by loader type path, loader type
    /// name, produced asset type and file extension.
    ///
    /// Registering a loader that was [pre-registered](Self::reserve) completes
    /// that entry instead of adding a second one, which is what releases the
    /// assets waiting on it.
    pub fn push<L: AssetLoader>(&mut self, loader: L) {
        let type_path = <L as TypePath>::type_path();
        let type_name = <L as TypePath>::type_name();
        let asset_type = TypeId::of::<L::Asset>();
        let extensions = L::EXTENSIONS;

        let loader = Arc::new(loader);

        if let Some(&index) = self.type_path_to_loader.get(type_path) {
            ::core::hint::cold_path();
            let index = index as usize;

            match &mut self.loaders[index] {
                // A pre-registered entry: registering the loader it promised completes it in place,
                // which is what releases the assets waiting on it.
                Err(PendingAssetLoaderCell { sender, .. }) => {
                    let _ = sender.try_broadcast(loader.clone());
                    self.loaders[index] = Ok(loader);
                }
                // A duplicate registration: the newest loader replaces the previous one.
                Ok(_) => {
                    ::core::hint::cold_path();
                    self.loaders[index] = Ok(loader);
                    zlim_log::warn!(
                        "A duplicate Loader `{type_path}` was inserted, and the old value was overwritten.
                        The assets that have been loaded and are currently being loaded are not affected."
                    );
                }
            }

            return;
        }

        let Ok(loader_index) = u16::try_from(self.loaders.len()) else {
            core::hint::cold_path();
            unreachable!("too many asset loaders");
        };

        self.loaders.push(Ok(loader));

        self.push_internal(
            loader_index,
            type_path,
            type_name,
            asset_type,
            extensions,
            core::any::type_name::<L::Asset>(),
        );
    }

    /// Pre-registers an [`AssetLoader`] that will be registered later.
    ///
    /// The loader takes its place in every reverse index right away, so an asset
    /// that resolves to it does not fail: it waits until the loader is
    /// registered. Every asset loaded with one of the extensions the loader
    /// declares ([`AssetLoader::EXTENSIONS`], which is exactly what its real
    /// registration will claim) is blocked that way, which is how an extension can
    /// be claimed before the crate that implements its loader has been built.
    pub fn reserve<L: AssetLoader>(&mut self) {
        let type_path = <L as TypePath>::type_path();
        let type_name = <L as TypePath>::type_name();
        let asset_type = TypeId::of::<L::Asset>();

        if self.type_path_to_loader.contains_key(type_path) {
            zlim_log::warn!(
                "The AssetLoader `{type_path}` already exist (or preregistered) \
                before this preregister call, prepare operation is skipped.",
            );

            return;
        }

        let Ok(loader_index) = u16::try_from(self.loaders.len()) else {
            core::hint::cold_path();
            unreachable!("too many asset loaders");
        };

        // One slot is enough: a pre-registration is completed exactly once, and the overflow mode
        // keeps the send non-blocking (so it needs no task to run on). Every waiter holds a clone of
        // the receiver, so the value stays in the channel until all of them have read it.
        let (mut sender, receiver) = async_broadcast::broadcast(1);
        sender.set_overflow(true);

        self.loaders.push(Err(PendingAssetLoaderCell {
            type_path,
            sender,
            receiver,
        }));

        self.push_internal(
            loader_index,
            type_path,
            type_name,
            asset_type,
            L::EXTENSIONS,
            core::any::type_name::<L::Asset>(),
        );
    }

    fn push_internal(
        &mut self,
        loader_index: u16,
        type_path: &'static str,
        type_name: &'static str,
        asset_type: TypeId,
        extensions: &[&'static str],
        asset_debug: &'static str,
    ) {
        use zlim_utils::hash::map::Entry;

        self.type_path_to_loader.insert(type_path, loader_index);

        match self.type_name_to_loader.entry(type_name) {
            Entry::Vacant(entry) => {
                entry.insert(MaybeAmbiguous::Unique(loader_index));
            }
            Entry::Occupied(mut entry) => {
                // More than one loader has this type name
                core::hint::cold_path();
                match entry.get_mut() {
                    MaybeAmbiguous::Unique(index) => {
                        // The other field is read directly: `entry` borrows the name map, so a
                        // method call on `self` would overlap with it.
                        let path = match &self.loaders[*index as usize] {
                            Ok(loader) => loader.type_path(),
                            Err(pending) => pending.type_path,
                        };
                        *entry.get_mut() = MaybeAmbiguous::Ambiguous(vec![path, type_path]);
                    }
                    MaybeAmbiguous::Ambiguous(items) => items.push(type_path),
                }
            }
        };

        let asset_type_loaders = self.asset_type_id_to_loaders.entry(asset_type).or_default();

        for &extension in extensions {
            let normalized = normalize_extension(extension);
            let list = self.extension_to_loaders.entry(normalized).or_default();

            if list.iter().any(|i| asset_type_loaders.contains(i)) {
                zlim_log::warn!(
                    "Duplicate AssetLoader (pre)registered for Asset `{asset_debug}` with \
                    extension `{normalized}`. Loader must be specified in a `.meta` \
                    file in order to load assets of this type with these extensions.",
                );
            }
            // e.g. [`PNG`, `png`] is duplicate.
            let _ = (!list.contains(&loader_index)).then(|| list.push(loader_index));
        }

        asset_type_loaders.push(loader_index);
    }
}

// -----------------------------------------------------------------------------
// AssetLoaderCell

/// The entry a loader index points at: the loader itself, or the promise of one.
type AssetLoaderCell = Result<Arc<dyn ErasedAssetLoader>, PendingAssetLoaderCell>;

/// A pre-registered loader that has not been registered yet.
struct PendingAssetLoaderCell {
    /// The loader's type path.
    ///
    /// The entry is in the reverse indexes before the loader itself exists, so when a second loader
    /// claims the same type name, the paths the ambiguity collects have to come from here for the
    /// entries that are not registered yet.
    type_path: &'static str,
    sender: async_broadcast::Sender<Arc<dyn ErasedAssetLoader>>,
    receiver: async_broadcast::Receiver<Arc<dyn ErasedAssetLoader>>,
}

pub(crate) struct PendingAssetLoader(async_broadcast::Receiver<Arc<dyn ErasedAssetLoader>>);

impl PendingAssetLoader {
    /// Waits for the loader this entry was pre-registered for.
    pub(crate) async fn get(mut self) -> Option<Arc<dyn ErasedAssetLoader>> {
        self.0.recv().await.ok()
    }
}

// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// An ambiguity that neither the asset type nor the extension can narrow down is the answer:
    /// the look-up does not pick one of the candidates on its own.
    #[test]
    fn an_unnarrowed_ambiguous_name_is_reported_as_an_ambiguity() {
        let mut loaders = AssetLoaders::default();
        loaders.type_name_to_loader.insert(
            "DupLoader",
            MaybeAmbiguous::Ambiguous(vec!["a::DupLoader", "b::DupLoader"]),
        );

        // No loader handles the extension, so nothing narrows the name down.
        let path = AssetPath::parse("thing.unknown");
        let Err(Some(error)) = loaders.find(None, Some("DupLoader"), None, Some(&path)) else {
            panic!("an unresolved ambiguous name is the error");
        };

        assert_eq!(error.service, "loader");
        assert_eq!(error.type_name, "DupLoader");

        let mut paths = error.type_paths;
        paths.sort_unstable();
        assert_eq!(paths, vec!["a::DupLoader", "b::DupLoader"]);
    }
}

// -----------------------------------------------------------------------------
