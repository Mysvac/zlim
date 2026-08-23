//! Cargo manifest based crate path resolution.
//!
//! This module reads the active `Cargo.toml` from `CARGO_MANIFEST_DIR` and
//! resolves crate names into absolute [`syn::Path`] values.
//!
//! A small cache keyed by manifest path and modified time avoids reparsing on
//! repeated lookups in the same process.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::path::{Path, PathBuf};
use std::sync::{PoisonError, RwLock};
use std::time::SystemTime;

use serde::Deserialize;
use serde::de::Visitor;

// -----------------------------------------------------------------------------
// Config

const ENGINE_NAME: &str = "zlim";
const ENGINE_PATH: &str = "::zlim";
const ENGINE_PREFIX: &str = "zlim_";

// -----------------------------------------------------------------------------
// Manifest

/// A container optimized for path comparation.
///
/// Conventional string comparison is char-by-char, but configuration file
/// paths often share the same prefix, making char-by-char comparison very
/// inefficient.
///
/// Standard library hash containers cannot be statically initialized and
/// have average performance.
///
/// This container prioritizes length comparison, quickly eliminating
/// non-matching items with a single comparison. Subsequent character
/// comparison prioritizes the second half, significantly improving performance.
#[derive(Debug, PartialEq, Eq)]
#[repr(transparent)]
struct RevPath(PathBuf);

impl PartialOrd for RevPath {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RevPath {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        let x: &[u8] = self.0.as_os_str().as_encoded_bytes();
        let y: &[u8] = other.0.as_os_str().as_encoded_bytes();
        x.len().cmp(&y.len()).then_with(|| {
            let len = x.len() >> 1;
            let (x1, x2) = x.split_at(len);
            let (y1, y2) = y.split_at(len);
            x2.cmp(y2).then_with(|| x1.cmp(y1))
        })
    }
}

#[derive(Default)]
#[repr(transparent)]
struct TableKeys(BTreeSet<String>);

impl<'a> Deserialize<'a> for TableKeys {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'a>,
    {
        struct TVisitor;

        impl<'de> Visitor<'de> for TVisitor {
            type Value = TableKeys;

            fn expecting(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                f.write_str("map")
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(TableKeys(BTreeSet::new()))
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut set = BTreeSet::new();
                while let Some(name) = map.next_key::<String>()? {
                    set.insert(name);
                    // `next_value` could be skipped,
                    // let _: toml::Value = map.next_value()?;
                }
                Ok(TableKeys(set))
            }
        }

        deserializer.deserialize_map(TVisitor)
    }
}

#[derive(Deserialize)]
struct Dependencies {
    #[serde(default)]
    dependencies: TableKeys,
    #[serde(rename = "dev-dependencies", default)]
    dev_dependencies: TableKeys,
}

impl Dependencies {
    #[inline]
    fn contains(&self, name: &str) -> bool {
        self.dependencies.0.contains(name)
    }

    #[inline]
    fn dev_contains(&self, name: &str) -> bool {
        self.dev_dependencies.0.contains(name)
    }
}

struct Manifest {
    manifest: Dependencies,
    modified_time: SystemTime,
}

// -----------------------------------------------------------------------------
// Implementation

/// Resolve a crate name to an absolute [`syn::Path`].
///
/// The `path` argument represents a Rust **module** path, not a Cargo crate
/// name — use `_` (underscore) rather than `-` (hyphen).  For example,
/// `zlim_core` refers to the crate that appears as `zlim-core` in `Cargo.toml`.
///
/// # Aliasing rule
///
/// When the requested crate name starts with `zlim_` and the `zlim` crate
/// appears in the manifest, the `zlim_` prefix is remapped to `::zlim::`,
/// producing `::zlim::<module>`.
///
/// # Resolution order
///
/// 1. If the name starts with `zlim_` and `zlim` is in `[dependencies]`,
///    return `::zlim::<module>`.
///
/// 2. If the exact name is in `[dependencies]`, return `::<name>`.
///
/// 3. If the name starts with `zlim_` and `zlim` is in `[dev-dependencies]`,
///    return `::zlim::<module>`.
///
/// 4. Otherwise, return `::<name>` as a fallback.
///
/// # Panics
///
/// Panics if `CARGO_MANIFEST_DIR` is missing, if `Cargo.toml` cannot be read,
/// or if the manifest content cannot be parsed.
///
/// # Examples
///
/// ```ignore
/// let core_path = zlim_derive_utils::crate_path("zlim_core");
/// ```
pub fn crate_path(path: &'static str) -> syn::Path {
    Manifest::shared(|manifest| manifest.find_crate_path(path))
}

impl Manifest {
    fn shared<R>(func: impl FnOnce(&Self) -> R) -> R {
        static MANIFESTS: RwLock<BTreeMap<RevPath, Manifest>> = RwLock::new(BTreeMap::new());

        // Obtain the file path and modification time.
        fn manifest_meta() -> (PathBuf, SystemTime) {
            let mut path = env::var_os("CARGO_MANIFEST_DIR")
                .map(PathBuf::from)
                .expect("CARGO_MANIFEST_DIR should be auto-defined by cargo.");

            path.push("Cargo.toml");

            let modified_time = std::fs::metadata(&path)
                .map_err(|_| panic!("Cargo manifest does not exist at path {path:?}"))
                .and_then(|metadata| metadata.modified())
                .expect("The Cargo.toml should have a modified time.");

            (path, modified_time)
        }

        fn read_manifest(path: &Path) -> Dependencies {
            let s = std::fs::read_to_string(path)
                .unwrap_or_else(|_| panic!("Failed to read cargo manifest: {path:?}"));
            toml::from_str(&s)
                .unwrap_or_else(|e| panic!("Failed to parse cargo manifest({path:?}): {e}"))
        }

        let (path, time) = manifest_meta();
        let rev_path = RevPath(path);

        let manifests = MANIFESTS.read().unwrap_or_else(PoisonError::into_inner);

        if let Some(manifest) = manifests.get(&rev_path)
            && manifest.modified_time == time
        {
            return func(manifest);
        }

        ::core::hint::cold_path();
        ::core::mem::drop(manifests);

        // the manifest is modified, reload it
        let manifest = Manifest {
            manifest: read_manifest(&rev_path.0),
            modified_time: time,
        };

        let result = func(&manifest);

        MANIFESTS
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(rev_path, manifest);

        result
    }

    fn find_crate_path(&self, name: &'static str) -> syn::Path {
        // find from `dependencies`
        if let Some(module) = name.strip_prefix(ENGINE_PREFIX)
            && self.manifest.contains(ENGINE_NAME)
        {
            let mut path: syn::Path = syn::parse_str(ENGINE_PATH).unwrap();
            let module: syn::PathSegment = syn::parse_str(module).unwrap();
            path.segments.push(module);
            return path;
        }

        if self.manifest.contains(name) {
            let mut path: syn::Path = syn::parse_str(name).unwrap();
            path.leading_colon = Some(Default::default());
            return path;
        }

        core::hint::cold_path();

        // find from `dev-dependencies`
        if let Some(module) = name.strip_prefix(ENGINE_PREFIX)
            && self.manifest.dev_contains(ENGINE_NAME)
        {
            let mut path: syn::Path = syn::parse_str(ENGINE_PATH).unwrap();
            let module: syn::PathSegment = syn::parse_str(module).unwrap();
            path.segments.push(module);
            return path;
        }

        let mut path: syn::Path = syn::parse_str(name).unwrap();
        path.leading_colon = Some(Default::default());
        path
    }
}

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize() {
        let toml = r#"
[dependencies]
zlim = "0.1"

[dev-dependencies]
zlim = "0.1"
serde = { version = "1.0", default-features = false }
quote = "1.0"
"#;
        let deps: Dependencies = toml::from_str(toml).unwrap();
        assert!(deps.contains("zlim"));
        assert!(deps.dev_contains("zlim"));
        assert!(deps.dev_contains("quote"));
        assert!(deps.dev_contains("serde"));
        assert!(!deps.contains("0.1"));
        assert!(!deps.contains("\"0.1\""));
        assert!(!deps.contains("quote"));
        assert!(!deps.dev_contains("version"));
    }
}
