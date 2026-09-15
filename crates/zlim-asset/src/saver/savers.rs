//! The [`AssetSaver`] registry and the reverse indexes used to pick a saver.

use core::any::TypeId;
use std::sync::Arc;

use zlim_path::TypePath;
use zlim_utils::ext::TypeMap;
use zlim_utils::hash::HashMap;
use zlim_utils::vec::SmallVec;

use crate::error::AmbiguousName;
use crate::path::AssetPath;
use crate::saver::{AssetSaver, ErasedAssetSaver};
use crate::utils::{iter_secondary_extensions, normalize_extension, normalize_extension_ref};

// -----------------------------------------------------------------------------

/// The savers one short type name has.
///
/// The first saver that claims a name leaves it [`Unique`](Self::Unique); the second turns it into
/// [`Ambiguous`](Self::Ambiguous), which keeps the type paths of every claimant: the error names
/// them, and a look-up that found a candidate another way (by asset type or by extension) asks
/// whether it is one of them.
enum MaybeAmbiguous {
    Unique(u16),
    Ambiguous(Vec<&'static str>),
}

// -----------------------------------------------------------------------------
// AssetSavers

cfg_select! {
    target_pointer_width = "32" => {
        const INLINE: usize = 4;
    }
    _ => {
        const INLINE: usize = 8;
    }
}

/// Every [`AssetSaver`] known to the asset server, plus the indexes used to
/// select one for a given asset.
///
/// Savers are stored once and referenced by index. The maps below are the
/// reverse indexes consumed by [`find`](Self::find):
///
/// - `extension_to_savers`: file extension → savers, newest last, so the most
///   recently registered saver for an extension wins. Keys are lower-case and
///   without the leading dot: registration goes through `normalize_extension`
///   (which interns only an extension that has to be rewritten) and look-ups
///   through `normalize_extension_ref` (which borrows or allocates a temporary,
///   never interning, because the extension comes from the asset path).
///
/// - `type_path_to_saver` / `type_name_to_saver`: the saver's [`TypePath`] /
///   type name → saver, used when a caller names the saver it wants instead of
///   letting the asset type and the path decide — see [`SaveBuilder::with_saver`]
///   and [`SaveBuilder::with_saver_name`].
///
/// - `asset_type_id_to_savers`: the produced asset [`TypeId`] → savers.
///
/// [`SaveBuilder::with_saver`]: crate::server::SaveBuilder::with_saver
/// [`SaveBuilder::with_saver_name`]: crate::server::SaveBuilder::with_saver_name
#[derive(Default)]
pub(crate) struct AssetSavers {
    savers: Vec<Arc<dyn ErasedAssetSaver>>,
    extension_to_savers: HashMap<&'static str, SmallVec<u16, INLINE>>,
    type_name_to_saver: HashMap<&'static str, MaybeAmbiguous>,
    type_path_to_saver: HashMap<&'static str, u16>,
    asset_type_id_to_savers: TypeMap<SmallVec<u16, INLINE>>,
}

impl AssetSavers {
    /// Returns `true` if a saver is registered under the saver type path `path`.
    pub fn contains(&self, path: &str) -> bool {
        let Some(&index) = self.type_path_to_saver.get(path) else {
            return false;
        };
        self.savers.get(index as usize).is_some()
    }
}

impl AssetSavers {
    /// Returns the saver registered under the saver type path `path`.
    pub fn get_by_path(&self, path: &str) -> Option<Arc<dyn ErasedAssetSaver>> {
        let index = self.type_path_to_saver.get(path).copied()?;
        Some(self.savers[index as usize].clone())
    }

    /// Finds the saver that should save the described asset.
    ///
    /// Every argument is optional and is only consulted when the steps before it produced
    /// nothing. In order:
    ///
    /// 1. **Saver type path** ([`get_by_path`](Self::get_by_path)) — the strict form: a saver's full
    ///    type path, matched exactly. An empty string counts as *no* path. It is authoritative: the
    ///    result is returned as-is, even when no saver has that path (then the answer is `Err(None)`
    ///    and the later steps are *not* tried).
    ///
    /// 2. **Saver type name** — the lenient form, and what the save builder's named saver passes
    ///    ([`with_saver`] / [`with_saver_name`]): a full type path resolves as well, so the caller
    ///    does not have to classify the string first. An empty string counts as *no* name. A name
    ///    nothing has is `Err(None)`, but a name several savers share does *not* end the search: its
    ///    candidates are remembered, and the steps below decide between them.
    ///
    /// 3. **Asset type** — the savers that produce `asset_type_id`. This narrowing is only applied
    ///    to paths *without* a label, since a sub-asset may legitimately have a different type than
    ///    the saver producing it. If the type is known but nothing produces it, the search stops
    ///    here (`Err(None)`). If exactly one saver produces it, that saver is the answer — unless a
    ///    name was given as well, in which case it has to be one of that name's candidates, and a
    ///    saver that is not is `Err(None)`.
    ///
    /// 4. **Extension of `asset_path`** — its full extension first (`foo.tar.gz` → `tar.gz`), then
    ///    every secondary extension (`gz`), i.e. each suffix starting after a `.` (see
    ///    [`iter_secondary_extensions`]). Extensions are compared case-insensitively and a leading
    ///    `.` is ignored. Within one extension the newest saver wins; a saver that does not produce
    ///    `asset_type_id` (when the type is known) or is not one of the name's candidates (when a
    ///    name was given) is skipped, and the next candidate — or the next extension — is tried. The
    ///    asset type therefore always wins over the extension when both are in play, and, for a path
    ///    without a label, `find` never returns a saver of another asset type.
    ///
    /// Finally, when nothing matched:
    ///
    /// - With an asset type, its newest saver that is also one of the name's candidates (or its
    ///   newest saver at all, when no name was given) is used as a last resort and a warning is
    ///   logged — this only happens with several candidates, since a single one returned in step 3.
    ///   If none of the type's savers is one of the name's candidates, the answer is `Err(None)`: a
    ///   plain miss rather than an ambiguity.
    /// - Without one, the answer is the ambiguity error when a name had several candidates, and
    ///   `Err(None)` otherwise.
    ///
    /// # Errors
    ///
    /// - `Err(None)`: nothing could be selected.
    /// - `Err(Some(_))`: a name was given, several savers share it, and neither the asset type nor
    ///   the extension could tell which of them was meant; [`AmbiguousName`] lists the type paths
    ///   that were.
    ///
    /// # Guarantees
    ///
    /// The result is a best effort: `find` does *not* check that the saver actually produces
    /// `asset_type_id`, and nothing after it does either — the value is handed to the saver as it
    /// is, so a mismatch panics inside the type-erased `save` and is reported as
    /// [`AssetSaverPanic`].
    ///
    /// [`AssetSaverPanic`]: crate::error::AssetSaverPanic
    /// [`iter_secondary_extensions`]: crate::utils::iter_secondary_extensions
    /// [`with_saver`]: crate::server::SaveBuilder::with_saver
    /// [`with_saver_name`]: crate::server::SaveBuilder::with_saver_name
    pub fn find(
        &self,
        type_path: Option<&str>,
        type_name: Option<&str>,
        asset_type_id: Option<TypeId>,
        asset_path: Option<&AssetPath<'_>>,
    ) -> Result<Arc<dyn ErasedAssetSaver>, Option<AmbiguousName>> {
        // ---------------------------------------------------------------
        // Type path or type name (an explicit choice by the caller)
        // ---------------------------------------------------------------

        let path = type_path.unwrap_or("");
        if !path.is_empty() {
            return self.get_by_path(path).ok_or(None);
        }

        // A name that several savers share does not end the search: the asset type and the extension
        // below can still tell which of the candidates was meant, and only when they cannot is the
        // ambiguity itself the answer. The candidates are therefore kept as they are, and the
        // `AmbiguousName` is built where it is returned.
        let name = type_name.unwrap_or("");
        let ambiguous: Option<&[&'static str]> = if name.is_empty() {
            None
        } else {
            if let Some(saver) = self.get_by_path(name) {
                return Ok(saver);
            }
            match self.type_name_to_saver.get(name) {
                Some(MaybeAmbiguous::Unique(index)) => {
                    return Ok(self.savers[*index as usize].clone());
                }
                Some(MaybeAmbiguous::Ambiguous(indices)) => Some(indices.as_slice()),
                None => return Err(None),
            }
        };

        // Whether `index` is one of the savers the ambiguous name could have meant.
        let validate_ambiguous = |amb: &[&str], index: u16| -> bool {
            amb.iter()
                .any(|path| self.type_path_to_saver.get(*path) == Some(&index))
        };

        // ---------------------------------------------------------------
        // Asset type (only when the path has no label)
        // ---------------------------------------------------------------

        let label = asset_path.and_then(AssetPath::label);

        let mut candidates: Option<&[u16]> = None;

        if label.is_none()
            && let Some(type_id) = asset_type_id
        {
            candidates = Some(self.asset_type_id_to_savers.get(type_id).ok_or(None)?);
        };

        if let Some(candidates) = candidates
            && candidates.len() == 1
        {
            let index = candidates[0];
            let Some(ambiguous) = ambiguous else {
                return Ok(self.savers[index as usize].clone());
            };
            ::core::hint::cold_path();
            // The type has one saver, so the only question left is whether it is one of the
            // candidates the name could have meant.
            if validate_ambiguous(ambiguous, index) {
                return Ok(self.savers[index as usize].clone());
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
            let indices = self.extension_to_savers.get(extension.as_ref())?;

            // Newest first: the saver registered last for the extension wins.
            for index in indices.iter().rev() {
                if let Some(list) = candidates
                    && !list.contains(index)
                {
                    // The asset type is known, so a saver that does not produce it is out.
                    continue;
                }

                if let Some(ambiguous) = ambiguous
                    && !validate_ambiguous(ambiguous, *index)
                {
                    ::core::hint::cold_path();
                    // A name was given and this saver is not one of its candidates.
                    continue;
                }

                return Some(*index);
            }
            None
        };

        // The extensions carried by the asset path.
        if let Some(full_extension) = asset_path.and_then(AssetPath::full_extension) {
            if let Some(index) = try_extension(full_extension) {
                return Ok(self.savers[index as usize].clone());
            }

            for extension in iter_secondary_extensions(full_extension) {
                if let Some(index) = try_extension(extension) {
                    return Ok(self.savers[index as usize].clone());
                }
            }
        }

        // ---------------------------------------------------------------
        // Fallback: the newest saver for the asset type
        // ---------------------------------------------------------------

        let Some(candidates) = candidates else {
            // No asset type to fall back on: the only thing left to report is the ambiguity, when
            // the name had several candidates.
            let Some(ambiguous) = ambiguous else {
                return Err(None);
            };

            ::core::hint::cold_path();
            let type_name = *self.type_name_to_saver.get_key_value(name).unwrap().0;
            return Err(Some(AmbiguousName {
                service: "saver",
                type_name,
                type_paths: ambiguous.to_vec(),
            }));
        };

        for &index in candidates.iter().rev() {
            if ambiguous.is_none_or(|a| validate_ambiguous(a, index)) {
                #[cfg(any(debug_assertions, feature = "debug"))]
                zlim_log::warn!(
                    "Multiple AssetSavers found for Asset Path: `{asset_path:?}`; Asset TypeId: `{asset_type_id:?}`;"
                );
                return Ok(self.savers[index as usize].clone());
            }
        }

        // An asset type was given, so a saver that produces it but is not one of the candidates the
        // name meant is a plain miss rather than an ambiguity.
        Err(None)
    }
}

impl AssetSavers {
    /// Registers `saver`, making it reachable by saver type path, saver type
    /// name, produced asset type and file extension.
    pub fn push<L: AssetSaver>(&mut self, saver: L) {
        let type_path = <L as TypePath>::type_path();
        let type_name = <L as TypePath>::type_name();
        let asset_type = TypeId::of::<L::Asset>();
        let extensions = L::EXTENSIONS;

        if let Some(&index) = self.type_path_to_saver.get(type_path) {
            ::core::hint::cold_path();
            self.savers[index as usize] = Arc::new(saver);
            zlim_log::warn!(
                "A duplicate Saver `{type_path}` was inserted, and the old value was overwritten.
                The assets that have been saved and are currently being saved are not affected."
            );
            return;
        }

        let Ok(saver_index) = u16::try_from(self.savers.len()) else {
            core::hint::cold_path();
            unreachable!("too many asset savers");
        };

        self.savers.push(Arc::new(saver));

        self.push_internal(
            saver_index,
            type_path,
            type_name,
            asset_type,
            extensions,
            core::any::type_name::<L::Asset>(),
        );
    }

    fn push_internal(
        &mut self,
        saver_index: u16,
        type_path: &'static str,
        type_name: &'static str,
        asset_type: TypeId,
        extensions: &[&'static str],
        asset_debug: &'static str,
    ) {
        use zlim_utils::hash::map::Entry;

        self.type_path_to_saver.insert(type_path, saver_index);

        match self.type_name_to_saver.entry(type_name) {
            Entry::Vacant(entry) => {
                entry.insert(MaybeAmbiguous::Unique(saver_index));
            }
            Entry::Occupied(mut entry) => {
                // More than one saver has this type name
                core::hint::cold_path();
                match entry.get_mut() {
                    MaybeAmbiguous::Unique(index) => {
                        let path = self.savers[*index as usize].type_path();
                        *entry.get_mut() = MaybeAmbiguous::Ambiguous(vec![path, type_path]);
                    }
                    MaybeAmbiguous::Ambiguous(items) => items.push(type_path),
                }
            }
        };

        let asset_type_savers = self.asset_type_id_to_savers.entry(asset_type).or_default();

        for &extension in extensions {
            let normalized = normalize_extension(extension);
            let list = self.extension_to_savers.entry(normalized).or_default();

            if list.iter().any(|i| asset_type_savers.contains(i)) {
                zlim_log::warn!(
                    "Duplicate AssetSaver registered for Asset `{asset_debug}` with  \
                    extension `{normalized}`. Saver must be specified in save config \
                    in order to save assets of this type with these extensions."
                );
            }
            // e.g. [`PNG`, `png`] is duplicate.
            let _ = (!list.contains(&saver_index)).then(|| list.push(saver_index));
        }

        asset_type_savers.push(saver_index);
    }
}

// -----------------------------------------------------------------------------
