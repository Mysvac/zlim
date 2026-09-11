use core::fmt::{Debug, Display, Formatter};
use std::path::{Path, PathBuf};

use atomicow::CowArc;
use serde::{Deserialize, Serialize, de::Visitor};
use zlim_core::derive::Error;

// -----------------------------------------------------------------------------
// AssetPath

/// An error that occurs when parsing a string type to create an [`AssetPath`] fails.
#[derive(Error, Debug, PartialEq, Eq)]
pub enum ParseAssetPathError {
    /// Error that occurs when a path string has deplicated `://` or `#`.
    #[error("Asset path contains invalid `#` or `://` (duplicated?)")]
    InvalidPath,
    /// Error that occurs when a path string has an [`AssetPath::source`]
    #[error("Asset source should not contains `#` character.")]
    InvalidSource,
    /// Error that occurs when a path string has an [`AssetPath::label`]
    #[error("Asset label should not contains `://` string.")]
    InvalidLabel,
    /// Error that occurs when a path string has an [`AssetPath::source`]
    #[error("Asset source must be at least one character.")]
    MissingSource,
    /// Error that occurs when a path string has an [`AssetPath::label`]
    #[error("Asset label must be at least one character.")]
    MissingLabel,
}

// -----------------------------------------------------------------------------
// AssetPath

/// Represents a path to an asset in a "virtual filesystem".
#[derive(Default, Clone, PartialEq, Eq, Hash)]
pub struct AssetPath<'a> {
    source: Option<CowArc<'a, str>>,
    path: CowArc<'a, Path>,
    label: Option<CowArc<'a, str>>,
}

impl AssetPath<'_> {
    /// Converts this into an "owned" value.
    pub fn into_owned(self) -> AssetPath<'static> {
        AssetPath {
            source: self.source.map(CowArc::into_owned),
            path: self.path.into_owned(),
            label: self.label.map(CowArc::into_owned),
        }
    }

    /// Clones this into an "owned" value.
    #[inline]
    pub fn clone_owned(&self) -> AssetPath<'static> {
        self.clone().into_owned()
    }
}

impl<'a> AssetPath<'a> {
    // Attempts to Parse a &str into an `AssetPath`'s components.
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
    pub fn try_parse(asset_path: &'a str) -> Result<AssetPath<'a>, ParseAssetPathError> {
        let (source, path, label) = Self::parse_internal(asset_path)?;
        Ok(AssetPath {
            source: source.map(CowArc::Borrowed),
            path: CowArc::Borrowed(path),
            label: label.map(CowArc::Borrowed),
        })
    }

    /// Creates a new [`AssetPath`] from a static string in the asset path format:
    pub fn try_parse_static(
        asset_path: &'static str,
    ) -> Result<AssetPath<'static>, ParseAssetPathError> {
        let (source, path, label) = Self::parse_internal(asset_path)?;
        Ok(AssetPath {
            source: source.map(CowArc::Borrowed),
            path: CowArc::Borrowed(path),
            label: label.map(CowArc::Borrowed),
        })
    }

    /// Creates a new [`AssetPath`] from a string in the asset path format:
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

impl<'a> AssetPath<'a> {
    /// Gets the path to the asset in the "virtual filesystem".
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

    /// Gets the "asset source".
    #[inline]
    pub fn source_cow(&self) -> Option<CowArc<'a, str>> {
        self.source.clone()
    }

    /// Gets the "sub-asset label".
    #[inline]
    pub fn label_cow(&self) -> Option<CowArc<'a, str>> {
        self.label.clone()
    }
}

impl<'a> AssetPath<'a> {
    /// Creates a empty [`AssetPath`] with a empty path.
    pub fn empty() -> AssetPath<'static> {
        AssetPath {
            source: None,
            path: CowArc::Static(Path::new("")),
            label: None,
        }
    }

    /// Creates a new [`AssetPath`] from a [`Path`].
    #[inline]
    pub fn from_path(path: &'a Path) -> AssetPath<'a> {
        AssetPath {
            source: None,
            path: CowArc::Borrowed(path),
            label: None,
        }
    }

    /// Creates a new [`AssetPath`] from a [`PathBuf`].
    #[inline]
    pub fn from_path_buf(path_buf: PathBuf) -> AssetPath<'static> {
        AssetPath {
            source: None,
            path: CowArc::Owned(path_buf.into()),
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
    pub fn with_source(self, source: impl Into<CowArc<'a, str>>) -> AssetPath<'a> {
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

    /// Returns this asset path that removed label.
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
}

impl<'a> AssetPath<'a> {
    /// Removes a "sub-asset label" from this [`AssetPath`], if one was set.
    #[inline]
    pub fn remove_label(&mut self) {
        self.label = None;
    }

    /// Takes the "sub-asset label" from this [`AssetPath`], if one was set.
    #[inline]
    pub fn take_label(&mut self) -> Option<CowArc<'a, str>> {
        self.label.take()
    }
}

impl<'a> AssetPath<'a> {
    /// Returns an [`AssetPath`] for the parent folder of this path.
    pub fn parent(&self) -> Option<AssetPath<'_>> {
        Some(AssetPath {
            path: match &self.path {
                CowArc::Borrowed(path) => CowArc::Borrowed(path.parent()?),
                CowArc::Static(path) => CowArc::Static(path.parent()?),
                CowArc::Owned(path) => CowArc::Borrowed(path.parent()?),
            },
            source: match &self.source {
                Some(x) => Some(CowArc::Borrowed(x.as_ref())),
                None => None,
            },
            label: None,
        })
    }

    /// Clones the [`AssetPath`] of the parent folder of this path.
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

impl<'a> AssetPath<'a> {
    /// Returns the last extension, excluding multiple `.` values.
    pub fn extension(&self) -> Option<&str> {
        let full_extension = self.full_extension()?;
        match full_extension.rfind(".") {
            None => Some(full_extension),
            Some(index) => Some(&full_extension[(index + 1)..]),
        }
    }

    /// Returns the full extension (including multiple '.' values).
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

impl<'a> AssetPath<'a> {
    /// Resolves an [`AssetPath`] relative to `self`.
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
    pub fn resolve_str(&self, path: &str) -> Result<AssetPath<'static>, ParseAssetPathError> {
        self.resolve_internal(path, false)
    }

    /// Parses `path` as an [`AssetPath`], then resolves it relative to `self` using embedded
    pub fn resolve_embed_str(&self, path: &str) -> Result<AssetPath<'static>, ParseAssetPathError> {
        self.resolve_internal(path, true)
    }

    fn resolve_from_parts(
        &self,
        replace: bool,
        source: Option<&str>,
        rpath: &Path,
        rlabel: Option<&str>,
    ) -> AssetPath<'static> {
        let mut base_path = PathBuf::from(self.path());
        if replace && !self.path.to_str().unwrap().ends_with('/') {
            // No error if base is empty (per RFC 1808).
            base_path.pop();
        }

        // Strip off leading slash
        let mut is_absolute = false;
        let rpath = match rpath.strip_prefix("/") {
            Ok(p) => {
                is_absolute = true;
                p
            }
            _ => rpath,
        };

        let mut result_path = if !is_absolute && source.is_none() {
            base_path
        } else {
            PathBuf::new()
        };
        result_path.push(rpath);
        result_path = normalize_path(result_path.as_path());

        AssetPath {
            source: match source {
                Some(s) => Some(CowArc::Owned(s.into())),
                None => self.source.clone().map(CowArc::into_owned),
            },
            path: CowArc::Owned(result_path.into()),
            label: rlabel.map(|l| CowArc::Owned(l.into())),
        }
    }

    fn resolve_internal(
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

    /// Returns `true` if this [`AssetPath`] points outside its source folder.
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
}

/// Normalizes the path by collapsing all occurrences of '.' and '..' dot-segments
fn normalize_path(path: &Path) -> PathBuf {
    let size_hint = path.as_os_str().len();
    let mut result_path = PathBuf::with_capacity(size_hint);

    for elt in path.iter() {
        if elt == "." {
            // Skip
        } else if elt == ".." {
            // Note: If the result_path ends in `..`, Path::file_name returns None,
            // so we'll end up preserving it.
            if result_path.file_name().is_some() {
                // This assert is just a sanity check - we already know the path
                // has a file_name, so we know there is something to pop.
                assert!(result_path.pop());
            } else {
                // Preserve ".." if insufficient matches (per RFC 1808).
                result_path.push(elt);
            }
        } else {
            result_path.push(elt);
        }
    }
    result_path
}

impl<'a> Debug for AssetPath<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        Display::fmt(self, f)
    }
}

impl<'a> Display for AssetPath<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        if let Some(name) = &self.source {
            write!(f, "{name}://")?;
        }

        write!(f, "{}", self.path.display())?;

        if let Some(label) = &self.label {
            write!(f, "#{label}")?;
        }

        Ok(())
    }
}

impl<'a> Serialize for AssetPath<'a> {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[inline(never)]
        fn to_string(asset_path: &AssetPath<'_>) -> String {
            use core::fmt::Write;

            let source: Option<&str> = asset_path.source.as_deref();
            let path: &Path = asset_path.path.as_ref();
            let label: Option<&str> = asset_path.label.as_deref();

            let hint = path.as_os_str().len()
                + source.map(|x| x.len() + 3).unwrap_or(0)
                + label.map(|x| x.len() + 1).unwrap_or(0);

            let mut buffer = String::with_capacity(hint);

            if let Some(source) = source {
                buffer.push_str(source);
                buffer.push_str("://");
            }

            write!(&mut buffer, "{}", path.display()).unwrap();

            if let Some(label) = label {
                buffer.push('#');
                buffer.push_str(label);
            }
            buffer
        }

        to_string(self).serialize(serializer)
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

            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match AssetPath::try_parse(v.as_str()) {
                    Ok(val) => Ok(val.into_owned()),
                    Err(err) => {
                        ::core::hint::cold_path();
                        Err(serde::de::Error::custom(format_args!("{err}: `{v}`")))
                    }
                }
            }
        }

        deserializer.deserialize_string(AssetPathVisitor)
    }
}

// This is only implemented for static lifetimes to ensure `Path::clone`
// does not allocate by ensuring that this is stored as a `CowArc::Static`.
impl From<&'static str> for AssetPath<'static> {
    #[inline]
    fn from(asset_path: &'static str) -> Self {
        Self::parse_static(asset_path)
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
        Self {
            source: None,
            path: CowArc::Static(path),
            label: None,
        }
    }
}

impl<'a> From<&'a PathBuf> for AssetPath<'a> {
    #[inline]
    fn from(path: &'a PathBuf) -> Self {
        Self {
            source: None,
            path: CowArc::Borrowed(path.as_path()),
            label: None,
        }
    }
}

impl From<PathBuf> for AssetPath<'static> {
    #[inline]
    fn from(path: PathBuf) -> Self {
        Self {
            source: None,
            path: path.into(),
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
