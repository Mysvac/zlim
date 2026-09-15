//! The [`AssetPath`]: represents a path to an asset in a "virtual filesystem".

use core::fmt::{Debug, Display, Formatter};
use std::path::{Path, PathBuf};

use atomicow::CowArc;
use serde::de::Visitor;
use serde::{Deserialize, Serialize};
use zlim_core::derive::Error;
use zlim_path::derive::TypePath;
use zlim_utils::str::SmolStr;

// -----------------------------------------------------------------------------
// ParseAssetPathError

/// An error that occurs when parsing a string type to create an [`AssetPath`] fails.
///
/// The parser splits the input into up to three parts: `[source://]path[#label]`.
///
/// - `source` is optional and is separated from `path` by `://`.
/// - `label` is optional and is separated from `path` by `#`.
///
/// # Rules
///
/// - If `://`(or `#`)  is present, the `source`(or `label`)
///   part must not be empty. This error is always checked.
///
/// - `://` and `#` may each appear at most once.
///   This error is only checked in debug mode.
///
/// - `://` must appear before `#`.
///   This error is only checked in debug mode.
#[derive(Error, Debug, PartialEq, Eq)]
pub enum ParseAssetPathError {
    /// Error that occurs when a second `://` or `#` is left inside the path segment.
    /// This error is only checked in debug mode.
    #[error("Asset path contains invalid `#` or `://` (duplicated?)")]
    InvalidPath,
    /// Error that occurs when the source segment contains a `#`.
    /// This error is only checked in debug mode.
    #[error("Asset source should not contains `#` character.")]
    InvalidSource,
    /// Error that occurs when the label segment contains a `://`.
    /// This error is only checked in debug mode.
    #[error("Asset label should not contains `://` string.")]
    InvalidLabel,
    /// Error that occurs when `://` is given without a source name in front of it.
    #[error("Asset source must be at least one character.")]
    MissingSource,
    /// Error that occurs when `#` is given without a label after it.
    #[error("Asset label must be at least one character.")]
    MissingLabel,
}

// -----------------------------------------------------------------------------
// AssetPath

/// Represents a path to an asset in a "virtual filesystem".
///
/// Asset paths consist of three main parts:
///
/// - [`AssetPath::source`]: An optional name of the [`AssetSource`] to load the asset from.
///   If one is not set the default source will be used (which is the `assets` folder by default).
///
/// - [`AssetPath::path`]: The "virtual filesystem path" pointing to an asset source file.
///   It is stored and parsed with the host platform's path rules: on Windows it may therefore
///   contain `\` separators, while `/` is the one separator every platform accepts.
///
/// - [`AssetPath::label`]: An optional "named sub asset". When assets are loaded, they are
///   allowed to load "sub assets" of any type, which are identified by a named "label".
///
/// [`AssetPath`] implements [`From`] for `&'static str`, `&'static Path`, and `&'a String`,
/// which allows us to optimize the static cases.
///
/// The [`AssetPath::path`] and [`AssetPath::label`] segments use [`CowArc`] for optimization,
/// preferring borrowing over allocating extra space.
///
/// The [`AssetPath::source`] segment uses [`SmolStr`] for optimization, since custom source
/// names typically do not exceed 23 bytes and can therefore always remain inline.
///
/// # Separators
///
/// The *internal* representation is platform-specific: the `path` segment keeps whatever the
/// host platform uses (on Windows a `\` is a separator, elsewhere it is an ordinary character)
/// and [`AssetPath::path`] returns it unchanged.
///
/// The guarantee is on the *textual* form instead: on Windows [`AssetPath::to_string`] — and with
/// it [`Debug`], [`Display`] and the [`Serialize`] impl — rewrites the host separator `\` to `/`,
/// so a path that came from the host serializes to the same text on every platform and can be
/// deserialized on another one. (A literal `\` inside a Unix path *name* is not portable and is
/// left alone.) Writing asset paths with `/` is always valid, since [`Path`] accepts `/` on all
/// platforms; [`normalize_separators`] is the input-side helper that turns a host-produced string
/// (for example the `file!()` literal an embedded asset records, which may have been written on
/// Windows) into that portable form.
///
/// [`AssetSource`]: crate::source::AssetSource
#[derive(Default, Clone, PartialEq, Eq, Hash, TypePath)]
#[type_path = "zlim_asset::path::AssetPath"]
pub struct AssetPath<'a> {
    source: Option<SmolStr>,
    path: CowArc<'a, Path>,
    label: Option<CowArc<'a, str>>,
}

// -----------------------------------------------------------------------------
// owned

impl AssetPath<'_> {
    /// Converts this into an "owned" value.
    pub fn into_owned(self) -> AssetPath<'static> {
        AssetPath {
            source: self.source.clone(),
            path: self.path.into_owned(),
            label: self.label.map(CowArc::into_owned),
        }
    }

    /// Clones this into an "owned" value.
    pub fn clone_owned(&self) -> AssetPath<'static> {
        AssetPath {
            source: self.source.clone(),
            path: self.path.clone_owned(),
            label: self.label.as_ref().map(CowArc::clone_owned),
        }
    }
}

// -----------------------------------------------------------------------------
// parse

impl<'a> AssetPath<'a> {
    // Attempts to Parse a &str into an `AssetPath`'s components.
    #[inline(never)]
    fn parse_internal(
        asset_path: &str,
    ) -> Result<(Option<&str>, &Path, Option<&str>), ParseAssetPathError> {
        use core::ops::Range;

        let mut source_range: Option<Range<usize>> = None;
        let mut path_range: Range<usize> = 0..asset_path.len();
        let mut label_range: Option<Range<usize>> = None;

        // Step-1 : find `://`, source delimiter
        let mut buffer = asset_path;
        let mut offset: usize = 0;
        while let Some(index) = buffer.find(':') {
            let bytes = buffer.as_bytes();
            if index + 2 >= bytes.len() {
                break;
            }
            if bytes[index + 1] == b'/' && bytes[index + 2] == b'/' {
                let total = index + offset;
                source_range = Some(0..total);
                path_range.start = total + 3;
                break;
            }
            buffer = &buffer[(index + 1)..];
            offset += index + 1;
        }

        // Step-2 : find `#`, label delimiter
        if let Some(index) = asset_path[path_range.start..].rfind('#') {
            let total = index + path_range.start;
            path_range.end = total;
            label_range = Some(total + 1..asset_path.len());
        }

        // Step-3 : create source + path + label
        let path_segment = &asset_path[path_range];
        let path = Path::new(path_segment);

        // validate path segment
        #[cfg(any(debug_assertions, feature = "debug"))]
        if path_segment.contains('#') || path_segment.contains("://") {
            ::core::hint::cold_path(); // E.g. `some/file#seg1#seg2`
            return Err(ParseAssetPathError::InvalidPath);
        }

        let source = match source_range {
            Some(source_range) => {
                // validate source segment
                if source_range.is_empty() {
                    ::core::hint::cold_path(); // E.g. `://some/file.test`
                    return Err(ParseAssetPathError::MissingSource);
                }
                #[cfg(any(debug_assertions, feature = "debug"))]
                if asset_path[source_range.clone()].contains('#') {
                    ::core::hint::cold_path(); // E.g. `a#b://some/file.test`
                    return Err(ParseAssetPathError::InvalidSource);
                }
                Some(&asset_path[source_range])
            }
            None => None,
        };

        let label = match label_range {
            Some(label_range) => {
                // validate label segment
                if label_range.is_empty() {
                    ::core::hint::cold_path(); // E.g. `some/file.test#`
                    return Err(ParseAssetPathError::MissingLabel);
                }
                #[cfg(any(debug_assertions, feature = "debug"))]
                if asset_path[label_range.clone()].contains("://") {
                    ::core::hint::cold_path(); // E.g. `some/file.test#a://b`
                    return Err(ParseAssetPathError::InvalidLabel);
                }
                Some(&asset_path[label_range])
            }
            None => None,
        };

        Ok((source, path, label))
    }

    /// Creates a new [`AssetPath`] from a string in the asset path format:
    /// - An asset at the root: `"scene.gltf"`
    /// - An asset nested in some folders: `"some/path/scene.gltf"`
    /// - An asset with a "label": `"some/path/scene.gltf#Mesh0"`
    /// - An asset with a custom "source": `"custom://some/path/scene.gltf#Mesh0"`
    ///
    /// Prefer [`AssetPath::try_parse_static`] for static strings, as this will prevent
    /// allocations and reference counting for [`AssetPath::into_owned`].
    ///
    /// This will return a [`ParseAssetPathError`] if `asset_path` is in an invalid format.
    /// Note that some invalid formats are only checked in debug mode, for performance.
    pub fn try_parse(asset_path: &'a str) -> Result<AssetPath<'a>, ParseAssetPathError> {
        let (source, path, label) = Self::parse_internal(asset_path)?;
        Ok(AssetPath {
            source: source.map(SmolStr::from_str),
            path: CowArc::Borrowed(path),
            label: label.map(CowArc::Borrowed),
        })
    }

    /// Creates a new [`AssetPath`] from a static string in the asset path format:
    /// - An asset at the root: `"scene.gltf"`
    /// - An asset nested in some folders: `"some/path/scene.gltf"`
    /// - An asset with a "label": `"some/path/scene.gltf#Mesh0"`
    /// - An asset with a custom "source": `"custom://some/path/scene.gltf#Mesh0"`
    ///
    /// This will return a [`ParseAssetPathError`] if `asset_path` is in an invalid format.
    /// Note that some invalid formats are only checked in debug mode, for performance.
    pub fn try_parse_static(
        asset_path: &'static str,
    ) -> Result<AssetPath<'static>, ParseAssetPathError> {
        let (source, path, label) = Self::parse_internal(asset_path)?;
        Ok(AssetPath {
            source: source.map(SmolStr::from_str),
            path: CowArc::Borrowed(path),
            label: label.map(CowArc::Borrowed),
        })
    }

    /// Creates a new [`AssetPath`] from a string in the asset path format:
    /// - An asset at the root: `"scene.gltf"`
    /// - An asset nested in some folders: `"some/path/scene.gltf"`
    /// - An asset with a "label": `"some/path/scene.gltf#Mesh0"`
    /// - An asset with a custom "source": `"custom://some/path/scene.gltf#Mesh0"`
    ///
    /// Prefer [`AssetPath::parse_static`] for static strings, as this will prevent
    /// allocations and reference counting for [`AssetPath::into_owned`].
    ///
    /// # Panics
    ///
    /// Panics if the asset path is in an invalid format.
    ///
    /// Use [`AssetPath::try_parse`] instead for a fallible variant.
    #[inline]
    #[track_caller]
    pub fn parse(asset_path: &'a str) -> AssetPath<'a> {
        match Self::try_parse(asset_path) {
            Ok(path) => path,
            Err(e) => {
                core::hint::cold_path();
                panic!("{e}: {asset_path}")
            }
        }
    }

    /// Creates a new [`AssetPath`] from a static string in the asset path format:
    /// - An asset at the root: `"scene.gltf"`
    /// - An asset nested in some folders: `"some/path/scene.gltf"`
    /// - An asset with a "label": `"some/path/scene.gltf#Mesh0"`
    /// - An asset with a custom "source": `"custom://some/path/scene.gltf#Mesh0"`
    ///
    /// # Panics
    ///
    /// Panics if the asset path is in an invalid format.
    ///
    /// Use [`AssetPath::try_parse_static`] instead for a fallible variant.
    #[inline]
    #[track_caller]
    pub fn parse_static(asset_path: &'static str) -> AssetPath<'static> {
        match Self::try_parse_static(asset_path) {
            Ok(path) => path,
            Err(e) => {
                core::hint::cold_path();
                panic!("{e}: {asset_path}")
            }
        }
    }
}

// -----------------------------------------------------------------------------
// fields

impl<'a> AssetPath<'a> {
    /// Gets the path to the asset in the "virtual filesystem".
    ///
    /// Note that this function only return the `path` segment.
    /// If you need full asset path with source and label, use
    /// [`ToString::to_string`] instead.
    #[inline]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Gets the "asset source".
    #[inline]
    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    /// Gets the "sub-asset label".
    #[inline]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Gets the raw "asset source".
    #[inline]
    pub fn source_raw(&self) -> Option<SmolStr> {
        self.source.clone()
    }

    /// Gets the raw "sub-asset label".
    #[inline]
    pub fn label_raw(&self) -> Option<CowArc<'a, str>> {
        self.label.clone()
    }
}

// -----------------------------------------------------------------------------
// builder

impl<'a> AssetPath<'a> {
    /// Creates an empty [`AssetPath`] whose path is empty.
    ///
    /// The asset source is default and the label is none.
    #[inline]
    pub fn empty() -> AssetPath<'static> {
        AssetPath {
            source: None,
            path: CowArc::Static(Path::new("")),
            label: None,
        }
    }

    /// Creates a new [`AssetPath`] from a [`Path`].
    ///
    /// The asset source is default and the label is none.
    #[inline]
    pub fn from_path(path: &'a Path) -> AssetPath<'a> {
        AssetPath {
            source: None,
            path: CowArc::Borrowed(path),
            label: None,
        }
    }

    /// Returns this asset path with the given path segment.
    #[inline]
    pub fn with_path(self, path: &'a Path) -> AssetPath<'a> {
        AssetPath {
            source: self.source,
            path: CowArc::Borrowed(path),
            label: self.label,
        }
    }

    /// Returns this asset path with the given asset source.
    #[inline]
    pub fn with_source(self, source: impl Into<SmolStr>) -> AssetPath<'a> {
        AssetPath {
            source: Some(source.into()),
            path: self.path,
            label: self.label,
        }
    }

    /// Returns this asset path with the given label.
    #[inline]
    pub fn with_label(self, label: impl Into<CowArc<'a, str>>) -> AssetPath<'a> {
        AssetPath {
            source: self.source,
            path: self.path,
            label: Some(label.into()),
        }
    }

    /// Returns this asset path with the label removed.
    #[inline]
    pub fn without_label(self) -> AssetPath<'a> {
        Self {
            source: self.source,
            path: self.path,
            label: None,
        }
    }

    /// Clone self without label.
    #[inline]
    #[must_use]
    pub fn clone_without_label(&self) -> AssetPath<'a> {
        Self {
            source: self.source.clone(),
            path: self.path.clone(),
            label: None,
        }
    }

    /// Removes a "sub-asset label" from this [`AssetPath`], if one was set.
    #[inline]
    pub fn remove_label(&mut self) -> Option<CowArc<'a, str>> {
        self.label.take()
    }
}

// -----------------------------------------------------------------------------
// parent

impl<'a> AssetPath<'a> {
    /// Returns an [`AssetPath`] for the parent folder of this path,
    /// if there is a parent folder in the path.
    ///
    /// The returned path keeps the same [`AssetPath::source`] as `self`,
    /// but its [`AssetPath::label`] is always reset to `None`.
    pub fn parent(&self) -> Option<AssetPath<'_>> {
        Some(AssetPath {
            path: match &self.path {
                CowArc::Borrowed(path) => CowArc::Borrowed(path.parent()?),
                CowArc::Static(path) => CowArc::Static(path.parent()?),
                CowArc::Owned(path) => CowArc::Borrowed(path.parent()?),
            },
            source: self.source.clone(),
            label: None,
        })
    }

    /// Clones the [`AssetPath`] of the parent folder of this path.
    ///
    /// The returned path keeps the same [`AssetPath::source`] as `self`,
    /// but its [`AssetPath::label`] is always reset to `None`.
    pub fn clone_parent(&self) -> Option<AssetPath<'a>> {
        Some(AssetPath {
            path: match &self.path {
                CowArc::Borrowed(path) => CowArc::Borrowed(path.parent()?),
                CowArc::Static(path) => CowArc::Static(path.parent()?),
                CowArc::Owned(path) => CowArc::Owned(path.parent()?.into()),
            },
            source: self.source.clone(),
            label: None,
        })
    }
}

// -----------------------------------------------------------------------------
// extension

impl<'a> AssetPath<'a> {
    /// Returns the last extension, excluding multiple `.` values.
    ///
    /// Ex: Returns `"ron"` for `"my_asset.config.ron"`
    ///
    /// Also strips out anything following a `?` to handle query parameters in URIs.
    #[inline]
    #[doc(alias = "get_extension")]
    pub fn extension(&self) -> Option<&str> {
        let full_extension = self.full_extension()?;
        match full_extension.rfind(".") {
            None => Some(full_extension),
            Some(index) => Some(&full_extension[(index + 1)..]),
        }
    }

    /// Returns the full extension (including multiple '.' values).
    ///
    /// Ex: Returns `"config.ron"` for `"my_asset.config.ron"`
    ///
    /// Also strips out anything following a `?` to handle query parameters in URIs.
    #[inline]
    #[doc(alias = "get_full_extension")]
    pub fn full_extension(&self) -> Option<&str> {
        let file_name = self.path().file_name()?.to_str()?;
        let index = file_name.find('.')?;
        let mut extension = &file_name[index + 1..];

        // Strip off any query parameters
        let query = extension.find('?');
        if let Some(offset) = query {
            extension = &extension[..offset];
        }

        Some(extension)
    }
}

// -----------------------------------------------------------------------------
// resolve

impl<'a> AssetPath<'a> {
    /// Returns `true` if this [`AssetPath`] points outside its source folder.
    ///
    /// # Example
    ///
    /// ```
    /// # use zlim_asset::path::AssetPath;
    /// // Inside the default AssetSource.
    /// let path = AssetPath::parse("thingy.png");
    /// assert!( ! path.is_unapproved());
    /// let path = AssetPath::parse("gui/thingy.png");
    /// assert!( ! path.is_unapproved());
    ///
    /// // Inside a different AssetSource.
    /// let path = AssetPath::parse("embedded://thingy.png");
    /// assert!( ! path.is_unapproved());
    ///
    /// // Exits the `AssetSource`s directory.
    /// let path = AssetPath::parse("../thingy.png");
    /// assert!(path.is_unapproved());
    /// let path = AssetPath::parse("folder/../../thingy.png");
    /// assert!(path.is_unapproved());
    ///
    /// // This references the linux root directory.
    /// let path = AssetPath::parse("/home/thingy.png");
    /// assert!(path.is_unapproved());
    /// ```
    ///
    /// On Windows, absolute paths such as `C:/home/thingy.png` are also
    /// considered unapproved. On other platforms, `std::path::Path` treats
    /// them as an ordinary relative path, where `C:` is a legal, ordinary
    /// component (not a drive prefix). This case is therefore not part of
    /// the example above, as its behavior is platform-dependent.
    pub fn is_unapproved(&self) -> bool {
        use std::path::Component;
        let mut component_count: usize = 0;

        for component in self.path.components() {
            match component {
                Component::Prefix(_) | Component::RootDir => return true,
                Component::CurDir => {}
                Component::ParentDir if component_count == 0 => return true,
                Component::ParentDir => component_count -= 1,
                Component::Normal(_) => component_count += 1,
            }
        }

        false
    }

    /// Resolves an [`AssetPath`] relative to `self`.
    ///
    /// Semantics:
    /// - If `path` is label-only (default source, empty path, label set), replace `self`'s label.
    /// - If `path` begins with `/`, treat it as rooted at the asset-source root (not the filesystem).
    /// - If `path` has an explicit source (`name://...`), it replaces the base source.
    /// - Relative segments are concatenated and normalized (`.`/`..` removal), preserving extra `..` if the base underflows.
    ///
    /// # Example
    ///
    /// ```
    /// # use zlim_asset::path::AssetPath;
    /// let base = AssetPath::parse("a/b");
    /// assert_eq!(base.resolve(&AssetPath::parse("c")), AssetPath::parse("a/b/c"));
    /// assert_eq!(base.resolve(&AssetPath::parse("./c")), AssetPath::parse("a/b/c"));
    /// assert_eq!(base.resolve(&AssetPath::parse("../c")), AssetPath::parse("a/c"));
    /// assert_eq!(base.resolve(&AssetPath::parse("c.png")), AssetPath::parse("a/b/c.png"));
    /// assert_eq!(base.resolve(&AssetPath::parse("/c")), AssetPath::parse("c"));
    /// assert_eq!(AssetPath::parse("a/b.png").resolve(&AssetPath::parse("#c")), AssetPath::parse("a/b.png#c"));
    /// assert_eq!(AssetPath::parse("a/b.png#c").resolve(&AssetPath::parse("#d")), AssetPath::parse("a/b.png#d"));
    /// ```
    pub fn resolve(&self, path: &AssetPath<'_>) -> AssetPath<'static> {
        let is_label_only =
            path.source().is_none() && path.path.as_os_str().is_empty() && path.label.is_some();

        if is_label_only {
            // path.label.is_some is checked above
            let label = path.label.as_ref().unwrap();
            self.clone_owned().with_label(label.clone_owned())
        } else {
            let explicit_source = path.source.as_deref();
            self.resolve_from_parts(false, explicit_source, path.path(), path.label())
        }
    }

    /// Resolves an [`AssetPath`] relative to `self` using embedded (RFC 1808) semantics.
    ///
    /// Semantics:
    /// - Remove the "file portion" of the base before concatenation (unless the base ends with `/`).
    /// - Otherwise identical to [`AssetPath::resolve`].
    ///
    /// # Example
    ///
    /// ```
    /// # use zlim_asset::path::AssetPath;
    /// let base = AssetPath::parse("a/b");
    /// assert_eq!(base.resolve_embed(&AssetPath::parse("c")), AssetPath::parse("a/c"));
    /// assert_eq!(base.resolve_embed(&AssetPath::parse("./c")), AssetPath::parse("a/c"));
    /// assert_eq!(base.resolve_embed(&AssetPath::parse("../c")), AssetPath::parse("c"));
    /// assert_eq!(base.resolve_embed(&AssetPath::parse("c.png")), AssetPath::parse("a/c.png"));
    /// assert_eq!(base.resolve_embed(&AssetPath::parse("/c")), AssetPath::parse("c"));
    /// assert_eq!(AssetPath::parse("a/b.png").resolve_embed(&AssetPath::parse("#c")), AssetPath::parse("a/b.png#c"));
    /// assert_eq!(AssetPath::parse("a/b.png#c").resolve_embed(&AssetPath::parse("#d")), AssetPath::parse("a/b.png#d"));
    /// ```
    pub fn resolve_embed(&self, path: &AssetPath<'_>) -> AssetPath<'static> {
        let is_label_only =
            path.source().is_none() && path.path.as_os_str().is_empty() && path.label.is_some();

        if is_label_only {
            // path.label.is_some is checked above
            let label = path.label.as_ref().unwrap();
            self.clone_owned().with_label(label.clone_owned())
        } else {
            let explicit_source = path.source.as_deref();
            self.resolve_from_parts(true, explicit_source, path.path(), path.label())
        }
    }

    /// Parses `path` as an [`AssetPath`], then resolves it relative to `self`.
    ///
    /// Returns an error if parsing fails.
    ///
    /// For more details, see [`AssetPath::resolve`].
    pub fn resolve_str(&self, path: &str) -> Result<AssetPath<'static>, ParseAssetPathError> {
        self.resolve_str_internal(path, false)
    }

    /// Parses `path` as an [`AssetPath`], then resolves it relative to `self` using embedded
    /// (RFC 1808) semantics.
    ///
    /// Returns an error if parsing fails.
    ///
    /// For more details, see [`AssetPath::resolve_embed`].
    pub fn resolve_embed_str(&self, path: &str) -> Result<AssetPath<'static>, ParseAssetPathError> {
        self.resolve_str_internal(path, true)
    }

    fn resolve_from_parts(
        &self,
        replace: bool,
        source: Option<&str>,
        rpath: &Path,
        rlabel: Option<&str>,
    ) -> AssetPath<'static> {
        let mut base_path = PathBuf::from(self.path());

        if replace {
            // TODO: use unstable fn Path::has_trailing_sep instead
            let bytes = self.path.as_os_str().as_encoded_bytes();
            let last = bytes.last().copied();
            let _ = (last != Some(b'/')).then(|| base_path.pop());
        }

        // Strip off leading slash
        let (rpath, is_absolute) = match rpath.strip_prefix("/") {
            Ok(stripped) => (stripped, true),
            Err(_) => (rpath, false),
        };

        let mut result_path = if !is_absolute && source.is_none() {
            base_path
        } else {
            PathBuf::new()
        };

        result_path.push(rpath);

        if result_path.iter().any(|elt| elt == "..") {
            // PathBuf::canonicalize(), but faster
            ::core::hint::cold_path();
            result_path = crate::utils::normalize_path(&result_path);
        }

        AssetPath {
            // An explicit `name://` in the resolved path replaces the base source.
            source: match source {
                Some(x) => Some(SmolStr::from_str(x)),
                None => self.source.clone(),
            },
            path: CowArc::Owned(result_path.into()),
            label: rlabel.map(|l| CowArc::Owned(l.into())),
        }
    }

    fn resolve_str_internal(
        &self,
        path: &str,
        replace: bool,
    ) -> Result<AssetPath<'static>, ParseAssetPathError> {
        if let Some(label) = path.strip_prefix('#') {
            // It's a label only
            Ok(self.clone_owned().with_label(label.to_owned()))
        } else {
            let (source, rpath, rlabel) = AssetPath::parse_internal(path)?;
            Ok(self.resolve_from_parts(replace, source, rpath, rlabel))
        }
    }
}

// -----------------------------------------------------------------------------
// Serialize

impl<'a> AssetPath<'a> {
    /// Formats this asset path as `[source://]path[#label]`.
    ///
    /// On Windows the host separator `\` is rewritten to `/`, so a path produced by the host
    /// serializes to the same text on every platform (and this is therefore what [`Serialize`]
    /// writes). See the separator policy on [`AssetPath`].
    #[expect(
        clippy::inherent_to_string_shadow_display,
        reason = "the inherent method is the documented entry point; format! / Display delegate to it"
    )]
    pub fn to_string(&self) -> String {
        use core::fmt::Write;

        let source: Option<&str> = self.source.as_deref();
        let path: &Path = self.path.as_ref();
        let label: Option<&str> = self.label.as_deref();

        let hint = path.as_os_str().len()
            + source.map(|x| x.len() + 3).unwrap_or(0)
            + label.map(|x| x.len() + 1).unwrap_or(0);

        let mut buffer = String::with_capacity(hint);

        if let Some(source) = source {
            buffer.push_str(source);
            buffer.push_str("://");
        }

        let _start = buffer.len();

        write!(&mut buffer, "{}", path.display()).unwrap();

        #[cfg(target_family = "windows")]
        if buffer.as_bytes()[_start..].contains(&b'\\') {
            #[expect(unsafe_code, reason = "raw bytes modification")]
            for b in unsafe { &mut buffer.as_bytes_mut()[_start..] } {
                *b = if *b == b'\\' { b'/' } else { *b };
            }
            // faster than ↓, testing in https://godbolt.org/
            // #[expect(unsafe_code, reason = "raw bytes modification")]
            // unsafe {
            //     let iter = buffer.as_bytes_mut()[_start..].iter_mut();
            //     iter.filter(|c| **c == b'\\').for_each(|c| *c = b'/');
            // }
        }

        if let Some(label) = label {
            buffer.push('#');
            buffer.push_str(label);
        }

        buffer
    }
}

impl<'a> Debug for AssetPath<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        Display::fmt(&self.to_string(), f)
    }
}

impl<'a> Display for AssetPath<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        Display::fmt(&self.to_string(), f)
    }
}

impl<'a> Serialize for AssetPath<'a> {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AssetPath<'static> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct AssetPathVisitor;

        impl<'de> Visitor<'de> for AssetPathVisitor {
            type Value = AssetPath<'static>;

            fn expecting(&self, formatter: &mut Formatter) -> core::fmt::Result {
                formatter.write_str("string AssetPath")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match AssetPath::try_parse(v) {
                    Ok(val) => Ok(val.into_owned()),
                    Err(err) => {
                        ::core::hint::cold_path();
                        Err(serde::de::Error::custom(format_args!("{err}: `{v}`")))
                    }
                }
            }

            #[inline]
            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                self.visit_str(&v)
            }
        }

        deserializer.deserialize_string(AssetPathVisitor)
    }
}

// -----------------------------------------------------------------------------
// Conversion

// This is only implemented for static lifetimes: the resulting path borrows the input, and the
// `AssetPath<'static>` it is stored in requires that borrow to live for `'static`.
impl From<&'static str> for AssetPath<'static> {
    #[inline]
    fn from(asset_path: &'static str) -> Self {
        AssetPath::parse_static(asset_path)
    }
}

impl<'a> From<&'a String> for AssetPath<'a> {
    #[inline]
    fn from(asset_path: &'a String) -> Self {
        AssetPath::parse(asset_path.as_str())
    }
}

impl From<String> for AssetPath<'static> {
    #[inline]
    fn from(asset_path: String) -> Self {
        AssetPath::parse(asset_path.as_str()).into_owned()
    }
}

// This is only implemented for static lifetimes to ensure `Path::clone`
// does not allocate by ensuring that this is stored as a `CowArc::Static`.
impl From<&'static Path> for AssetPath<'static> {
    #[inline]
    fn from(path: &'static Path) -> Self {
        AssetPath {
            source: None,
            path: CowArc::Static(path),
            label: None,
        }
    }
}

impl<'a> From<&'a PathBuf> for AssetPath<'a> {
    #[inline]
    fn from(path: &'a PathBuf) -> Self {
        AssetPath {
            source: None,
            path: CowArc::Borrowed(path.as_path()),
            label: None,
        }
    }
}

impl From<PathBuf> for AssetPath<'static> {
    #[inline]
    fn from(path: PathBuf) -> Self {
        AssetPath {
            source: None,
            path: CowArc::Owned(path.into()),
            label: None,
        }
    }
}

impl<'a, 'b> From<&'a AssetPath<'b>> for AssetPath<'b> {
    #[inline]
    fn from(value: &'a AssetPath<'b>) -> Self {
        value.clone()
    }
}

impl<'a> From<AssetPath<'a>> for PathBuf {
    #[inline]
    fn from(value: AssetPath<'a>) -> Self {
        value.path().to_path_buf()
    }
}

impl<'a> From<AssetPath<'a>> for String {
    #[inline]
    fn from(value: AssetPath<'a>) -> Self {
        value.to_string()
    }
}

impl<'a> From<&AssetPath<'a>> for String {
    #[inline]
    fn from(value: &AssetPath<'a>) -> Self {
        value.to_string()
    }
}

// -----------------------------------------------------------------------------
// Source Id

impl<'a> AssetPath<'a> {
    /// Returns this asset path with the given [`AssetSourceId`].
    ///
    /// [`AssetSourceId::Default`] clears a source that was set before.
    ///
    /// [`AssetSourceId`]: crate::ident::AssetSourceId
    /// [`AssetSourceId::Default`]: crate::ident::AssetSourceId::Default
    #[inline]
    pub fn with_source_id(self, source: crate::ident::AssetSourceId) -> AssetPath<'a> {
        AssetPath {
            source: source.into(),
            path: self.path,
            label: self.label,
        }
    }

    /// Gets the "asset source" as an [`AssetSourceId`], which is the form the source registry is
    /// keyed by; an asset path without a source is [`AssetSourceId::Default`].
    ///
    /// [`AssetSourceId`]: crate::ident::AssetSourceId
    /// [`AssetSourceId::Default`]: crate::ident::AssetSourceId::Default
    #[inline]
    pub fn source_id(&self) -> crate::ident::AssetSourceId {
        crate::ident::AssetSourceId::from(self.source.clone())
    }
}

// -----------------------------------------------------------------------------
// Separators

/// Converts every `\` separator in `s` to `/`.
///
/// This is an *input-side* helper: it turns a host-produced path string (such as the `file!()`
/// literal an embedded asset is registered under, which may have been written on Windows) into the
/// portable, `/`-separated form used by [`AssetPath`]'s textual representation. It is **not**
/// required before parsing a string into an [`AssetPath`], because parsing keeps the host
/// platform's representation and only [`AssetPath::to_string`] normalizes separators on output
/// (see the separator policy on [`AssetPath`]).
///
/// The rewrite is a pure ASCII byte substitution and therefore infallible. Callers that start
/// from an [`OsStr`](std::ffi::OsStr) should do the lossy conversion first ([`Path::display`]
/// / [`OsStr::to_string_lossy`]), which is also what [`AssetPath::to_string`] does; invalid bytes
/// then become `U+FFFD` as usual.
///
/// [`OsStr::to_string_lossy`]: std::ffi::OsStr::to_string_lossy
#[inline]
pub fn normalize_separators(s: &mut str) {
    #[expect(unsafe_code, reason = "raw bytes modification")]
    for b in unsafe { s.as_bytes_mut() } {
        *b = if *b == b'\\' { b'/' } else { *b };
    }
    // faster than ↓ , testing in https://godbolt.org/
    // #[expect(unsafe_code, reason = "raw bytes modification")]
    // unsafe {
    //     let iter = s.as_bytes_mut().iter_mut();
    //     iter.filter(|c| **c == b'\\').for_each(|c| *c = b'/');
    // }
}

// -----------------------------------------------------------------------------
