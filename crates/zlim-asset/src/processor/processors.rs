//! The [`AssetProcessor`] registry and the look-ups used to pick one.

use std::sync::Arc;

use zlim_path::TypePath;
use zlim_utils::hash::HashMap;
use zlim_utils::hash::map::Entry;

use crate::error::AmbiguousName;
use crate::path::AssetPath;
use crate::processor::{AssetProcessor, ErasedAssetProcessor};
use crate::utils::{intern_extension, iter_secondary_extensions, normalize_extension_ref};

// -----------------------------------------------------------------------------

/// The processors one short type name has.
///
/// The first processor that claims a name leaves it [`Unique`](Self::Unique); the second turns it
/// into [`Ambiguous`](Self::Ambiguous), which keeps the type paths of every claimant: the error
/// names them, and a look-up that found a candidate another way (by extension) asks whether it is
/// one of them.
enum MaybeAmbiguous {
    Unique(u32),
    Ambiguous(Vec<&'static str>),
}

// -----------------------------------------------------------------------------
// AssetProcessors

/// Every [`AssetProcessor`] known to the importer.
///
/// Processors are stored once and referenced by index. What the maps hold:
///
/// - `type_path_to_processor`: the processor's [`TypePath`] → processor. This is the strict form
///   (`my_crate::MyProcessor`).
/// - `type_name_to_processor`: the short type name → the processor that has it
///   ([`MaybeAmbiguous::Unique`]), or every processor that has it when several do
///   ([`MaybeAmbiguous::Ambiguous`]). A look-up by name is the lenient form — it accepts a
///   fully-qualified type path as well — which is what lets a `.meta` file name a processor without
///   the string having to be classified first.
/// - `extension_to_processor`: the file extension of a *source* asset → the processor that handles
///   it by default. Nothing lands here automatically: unlike a loader or a saver a processor
///   declares no extension list, so the mapping is set explicitly with
///   [`register_extension`](Self::register_extension).
///
/// Both keys and the extension map's value are cheap by construction: the extension is interned when
/// the mapping is set (`register_extension` is an explicit configuration call, and the mapping is
/// only ever added to), and the processor is stored as an index into `processors` rather than as
/// another type path to hash.
#[derive(Default)]
pub(crate) struct AssetProcessors {
    processors: Vec<Arc<dyn ErasedAssetProcessor>>,
    type_path_to_processor: HashMap<&'static str, u32>,
    type_name_to_processor: HashMap<&'static str, MaybeAmbiguous>,
    extension_to_processor: HashMap<&'static str, u32>,
}

// -----------------------------------------------------------------------------
// AssetProcessors

impl AssetProcessors {
    /// Returns the processor stored at `index`.
    ///
    /// The index is never out of range: it always comes out of one of this registry's own maps.
    #[inline(always)]
    fn get_by_index(&self, index: u32) -> Arc<dyn ErasedAssetProcessor> {
        self.processors[index as usize].clone()
    }

    /// Builds the error for a `name` that selected several processors.
    ///
    /// `name` is one of `type_name_to_processor`'s keys — both callers reach here out of the
    /// [`MaybeAmbiguous::Ambiguous`] arm — and the key is read back out of the map because that is
    /// where the name lives with the `'static` lifetime the error needs.
    fn ambiguous(&self, name: &str, type_paths: Vec<&'static str>) -> AmbiguousName {
        AmbiguousName {
            service: "processor",
            type_name: self.type_name_to_processor.get_key_value(name).unwrap().0,
            type_paths,
        }
    }

    /// Returns the processor registered under the processor type path `path`.
    ///
    /// The type path is the strict form (`my_crate::MyProcessor`): only a registered type path
    /// resolves it. A string that may also be a short name is looked up by
    /// [`get_by_name`](Self::get_by_name) instead.
    pub fn get_by_path(&self, path: &str) -> Option<Arc<dyn ErasedAssetProcessor>> {
        let index = self.type_path_to_processor.get(path).copied()?;
        Some(self.get_by_index(index))
    }

    /// Returns the processor registered under the processor type name `name`.
    ///
    /// The name is lenient: a fully-qualified type path resolves as well, so a string taken from a
    /// `.meta` file does not have to be classified as a long or a short name first.
    ///
    /// # Errors
    ///
    /// - `Err(None)`: no processor has that name.
    /// - `Err(Some(_))`: several processors share it; the [`AmbiguousName`] lists their type paths.
    pub fn get_by_name(
        &self,
        name: &str,
    ) -> Result<Arc<dyn ErasedAssetProcessor>, Option<AmbiguousName>> {
        if let Some(processor) = self.get_by_path(name) {
            return Ok(processor);
        }

        match self.type_name_to_processor.get(name) {
            Some(MaybeAmbiguous::Unique(index)) => Ok(self.get_by_index(*index)),
            Some(MaybeAmbiguous::Ambiguous(paths)) => {
                ::core::hint::cold_path();
                Err(Some(self.ambiguous(name, paths.clone())))
            }
            None => Err(None),
        }
    }

    /// Returns the processor that handles source files with `extension` by default.
    pub fn get_by_extension(&self, extension: &str) -> Option<Arc<dyn ErasedAssetProcessor>> {
        // The extension comes from an asset path (user data), so it is normalized into a borrowed
        // or temporary string instead of being interned: a look-up must never grow the string pool.
        let index = self
            .extension_to_processor
            .get(normalize_extension_ref(extension).as_ref())?;

        Some(self.get_by_index(*index))
    }

    /// Finds the processor that should process the described asset.
    ///
    /// Every argument is optional and is only consulted when the steps before it produced nothing.
    /// In order:
    ///
    /// 1. **Processor type path** ([`get_by_path`](Self::get_by_path)) — the strict form, and what a
    ///    `.meta` file names. An empty string counts as *no* path. It is authoritative: the result is
    ///    returned as-is, even when no processor has that path (then the answer is `Err(None)` and the
    ///    later steps are *not* tried).
    ///
    /// 2. **Processor type name** — the lenient form: a full type path resolves as well, so a string
    ///    read out of a `.meta` file does not have to be classified first. An empty string counts as
    ///    *no* name. A name nothing has is `Err(None)`, but a name several processors share does *not*
    ///    end the search: its candidates are remembered, and the extension step below decides between
    ///    them.
    ///
    /// 3. **The source path's extension** — its full extension first (`foo.tar.gz` → `tar.gz`), then
    ///    every secondary extension (`gz`), both looked up in the default-processor map. When a name
    ///    had several candidates, the processor an extension selects has to be one of them; when it
    ///    is not, the extension does not match at all and the next one is tried.
    ///
    /// There is no last-resort fallback: a source asset with no matching processor is not processed,
    /// and the caller decides whether that is an error.
    ///
    /// # Errors
    ///
    /// - `Err(None)`: nothing could be selected.
    /// - `Err(Some(_))`: a name was given, several processors share it, and the extension could not
    ///   tell which of them was meant; [`AmbiguousName`] lists the type paths that were.
    pub fn find(
        &self,
        type_path: Option<&str>,
        type_name: Option<&str>,
        asset_path: Option<&AssetPath<'_>>,
    ) -> Result<Arc<dyn ErasedAssetProcessor>, Option<AmbiguousName>> {
        let path = type_path.unwrap_or("");
        if !path.is_empty() {
            return self.get_by_path(path).ok_or(None);
        }

        // A name that several processors share does not end the search: the extension below can still
        // tell which of the candidates was meant, and only when it cannot is the ambiguity itself the
        // answer. The candidates are therefore kept as they are, and the `AmbiguousName` is built
        // where it is returned.
        let name = type_name.unwrap_or("");
        let ambiguous: Option<&[&'static str]> = if name.is_empty() {
            None
        } else {
            if let Some(processor) = self.get_by_path(name) {
                return Ok(processor);
            }
            match self.type_name_to_processor.get(name) {
                Some(MaybeAmbiguous::Unique(index)) => return Ok(self.get_by_index(*index)),
                Some(MaybeAmbiguous::Ambiguous(paths)) => Some(paths.as_slice()),
                None => return Err(None),
            }
        };

        // Whether `index` is one of the processors the ambiguous name could have meant.
        let validate_ambiguous = |ambiguous: &[&str], index: u32| -> bool {
            ambiguous
                .iter()
                .any(|path| self.type_path_to_processor.get(*path) == Some(&index))
        };

        // The extensions carried by the source path.
        if let Some(full_extension) = asset_path.and_then(AssetPath::full_extension) {
            let try_extension = |extension: &str| -> Option<u32> {
                // The extension comes from an asset path (user data), so it is normalized into a
                // borrowed or temporary string instead of being interned: a look-up must never grow
                // the string pool.
                let index = *self
                    .extension_to_processor
                    .get(normalize_extension_ref(extension).as_ref())?;

                if let Some(ambiguous) = ambiguous
                    && !validate_ambiguous(ambiguous, index)
                {
                    ::core::hint::cold_path();
                    // A name was given and this processor is not one of its candidates.
                    return None;
                }

                Some(index)
            };

            if let Some(index) = try_extension(full_extension) {
                return Ok(self.get_by_index(index));
            }

            for extension in iter_secondary_extensions(full_extension) {
                if let Some(index) = try_extension(extension) {
                    return Ok(self.get_by_index(index));
                }
            }
        }

        // No extension matched: the only thing left to report is the ambiguity, when the name had
        // several candidates.
        let Some(ambiguous) = ambiguous else {
            return Err(None);
        };

        ::core::hint::cold_path();
        Err(Some(self.ambiguous(name, ambiguous.to_vec())))
    }

    /// Sets the processor that handles `extension` by default.
    ///
    /// The extension is given without a leading dot and is matched case-insensitively; it is
    /// interned here, so this is meant to be called for the handful of extensions a program
    /// configures by hand, not per asset.
    ///
    /// `type_path` has to name a processor that is *already registered*: the mapping is stored as the
    /// processor's index, which is what makes the look-up a single hash and a vector index. A type
    /// path nothing is registered under is reported and ignored.
    pub fn register_extension(&mut self, extension: impl AsRef<str>, type_path: &'static str) {
        let Some(index) = self.type_path_to_processor.get(type_path).copied() else {
            ::core::hint::cold_path();
            zlim_log::error!(
                "Cannot make the processor `{type_path}` the default for `.{}`: \
                 it is not registered. Register the processor first.",
                extension.as_ref()
            );
            return;
        };

        self.extension_to_processor
            .insert(intern_extension(extension.as_ref()), index);
    }

    /// Registers `processor`, making it reachable by processor type path and type name.
    ///
    /// Registering a type path that is already there replaces the stored processor — which is how an
    /// updated processor takes effect — and the overwrite is reported as a warning. What the
    /// processed side already holds is left alone: assets that were processed before are not looked
    /// at again.
    pub fn push<P: AssetProcessor>(&mut self, processor: P) {
        let type_path = <P as TypePath>::type_path();
        let type_name = <P as TypePath>::type_name();

        if let Some(&index) = self.type_path_to_processor.get(type_path) {
            ::core::hint::cold_path();
            self.processors[index as usize] = Arc::new(processor);
            zlim_log::warn!(
                "A duplicate AssetProcessor `{type_path}` was inserted, and the old value was \
                overwritten. The assets that have already been processed are not affected."
            );
            return;
        }

        let Ok(index) = u32::try_from(self.processors.len()) else {
            ::core::hint::cold_path();
            unreachable!("too many asset processors");
        };

        self.processors.push(Arc::new(processor));
        self.type_path_to_processor.insert(type_path, index);

        match self.type_name_to_processor.entry(type_name) {
            Entry::Vacant(entry) => {
                entry.insert(MaybeAmbiguous::Unique(index));
            }
            Entry::Occupied(mut entry) => {
                // More than one processor has this type name
                ::core::hint::cold_path();
                match entry.get_mut() {
                    MaybeAmbiguous::Unique(other) => {
                        // The other field is read directly: `entry` borrows the name map, so a
                        // method call on `self` would overlap with it.
                        let path = self.processors[*other as usize].type_path();
                        *entry.get_mut() = MaybeAmbiguous::Ambiguous(vec![path, type_path]);
                    }
                    MaybeAmbiguous::Ambiguous(items) => items.push(type_path),
                }
            }
        }
    }
}

// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// A short name several processors share selects none of them: that is an ambiguity carrying the
    /// type paths it could have meant, not a plain miss.
    #[test]
    fn an_ambiguous_type_name_reports_the_type_paths_it_could_have_meant() {
        let mut processors = AssetProcessors::default();

        // `push` records the type paths of every claimant once a name is shared; the entries
        // themselves are not needed to reach the ambiguity.
        processors.type_name_to_processor.insert(
            "DupProcessor",
            MaybeAmbiguous::Ambiguous(vec!["a::DupProcessor", "b::DupProcessor"]),
        );

        let Err(Some(error)) = processors.get_by_name("DupProcessor") else {
            panic!("a name without a single owner is ambiguous");
        };

        assert_eq!(error.service, "processor");
        assert_eq!(error.type_name, "DupProcessor");

        let mut paths = error.type_paths;
        paths.sort_unstable();
        assert_eq!(paths, vec!["a::DupProcessor", "b::DupProcessor"]);

        // A name nothing has is a plain miss, and so is a path nothing has: neither look-up falls
        // through to the other map.
        assert!(matches!(
            processors.get_by_name("NoSuchProcessor"),
            Err(None)
        ));
        assert!(processors.get_by_path("no::SuchProcessor").is_none());
    }

    /// An ambiguity that the extension cannot narrow down is the answer: the look-up does not pick
    /// one of the candidates on its own.
    #[test]
    fn an_unnarrowed_ambiguous_name_is_reported_as_an_ambiguity() {
        let mut processors = AssetProcessors::default();
        processors.type_name_to_processor.insert(
            "DupProcessor",
            MaybeAmbiguous::Ambiguous(vec!["a::DupProcessor", "b::DupProcessor"]),
        );

        // No default processor handles the extension, so nothing narrows the name down.
        let path = AssetPath::parse("thing.unknown");
        let Err(Some(error)) = processors.find(None, Some("DupProcessor"), Some(&path)) else {
            panic!("an unresolved ambiguous name is the error");
        };

        assert_eq!(error.service, "processor");
        assert_eq!(error.type_name, "DupProcessor");

        let mut paths = error.type_paths;
        paths.sort_unstable();
        assert_eq!(paths, vec!["a::DupProcessor", "b::DupProcessor"]);
    }
}

// -----------------------------------------------------------------------------
