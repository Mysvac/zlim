//! The `embedded://` asset source: compile-time bytes registered into an in-memory tree.

// -----------------------------------------------------------------------------
// EmbeddedAssetRegistry

mod registry;

/// A [`Resource`] that manages embedded assets in a virtual in memory [`Dir`].
///
/// Generally this should not be interacted with directly: the [`embedded_asset!`]
/// macro populates it, and [`register_source`] turns it into the `embedded` asset source.
/// Loading by name goes through `load_embedded_asset!`, which resolves the asset against
/// [`AssetServer::load`].
///
/// [`Dir`]: crate::io::memory::Dir
/// [`embedded_asset!`]: crate::embedded_asset
/// [`Resource`]: trait@zlim_core::resource::Resource
/// [`register_source`]: EmbeddedAssetRegistry::register_source
#[doc(inline)]
pub use registry::EmbeddedAssetRegistry;

// -----------------------------------------------------------------------------
// Constant Value

/// The name of the `embedded` [`AssetSource`].
///
/// [`AssetSource`]: crate::source::AssetSource
pub const EMBEDDED: &str = "embedded";

// -----------------------------------------------------------------------------
// Macros

/// Returns the [`Path`] for a given `embedded` asset.
///
/// This is used internally by [`embedded_asset!`] and can be used to
/// get a [`Path`] that matches the [`AssetPath`] used by that asset.
///
/// # Panics
///
/// Panics when `$source_path` does not occur in the `file!()` path of the calling module.
///
/// [`embedded_asset!`]: crate::embedded_asset
/// [`AssetPath`]: crate::path::AssetPath
#[macro_export]
macro_rules! embedded_path {
    ($path_str: expr) => {{ $crate::embedded_path!("src", $path_str) }};
    ($source_path: expr, $path_str: expr) => {{
        $crate::io::embedded::__embedded_asset_path(
            ::core::file!(),
            ::core::env!("CARGO_CRATE_NAME"),
            $source_path.as_ref(),
            $path_str.as_ref(),
        )
    }};
}

/// Creates a new `embedded` asset by embedding the bytes of the given path into
/// the current binary and registering those bytes with the `embedded` [`AssetSource`].
///
/// This accepts the current [`App`] as the first parameter and a path `&str`
/// (relative to the current file) as the second.
///
/// By default this will generate an [`AssetPath`] using the following rules:
///
/// 1. Search for the first `$crate_name/src/` in the path and trim to the path past that point.
/// 2. Re-add the current `$crate_name` to the front of the path
///
/// For example, consider the following file structure in the theoretical `zlim_rock` crate,
/// which provides a [`Plugin`] that renders fancy rocks for scenes.
///
/// [`Plugin`]: zlim_app::Plugin
///
/// ```text
/// zlim_rock
/// ├── src
/// │   ├── render
/// │   │   ├── rock.wgsl
/// │   │   └── mod.rs
/// │   └── lib.rs
/// └── Cargo.toml
/// ```
///
/// `rock.wgsl` is a WGSL shader asset that the `zlim_rock` plugin author wants to bundle with
/// their crate. They invoke the following in `zlim_rock/src/render/mod.rs`:
///
/// ```ignore
/// embedded_asset!(app, "rock.wgsl");
/// ```
///
/// `rock.wgsl` can now be loaded by the [`AssetServer`] as follows:
///
/// ```ignore
/// // If we are loading the shader in the same module we used `embedded_asset!`:
/// let shader = load_embedded_asset!(&asset_server, "rock.wgsl");
///
/// // If the goal is to expose the asset **to the end user**:
/// let shader = asset_server.load::<Shader>("embedded://zlim_rock/render/rock.wgsl");
/// ```
///
/// # Panics
///
/// Panics when `$source_path` does not occur in the `file!()` path of the calling module.
///
/// [`AssetPath`]: crate::path::AssetPath
/// [`AssetSource`]: crate::source::AssetSource
#[macro_export]
macro_rules! embedded_asset {
    ($app: expr, $path: expr $(,)?) => {
        $crate::embedded_asset!($app, "src", $path)
    };
    ($app: expr, $source_path: expr, $path: expr $(,)?) => {{
        let world = $crate::io::embedded::__GetWorldMut::world_mut($app);
        let embedded = world
            .get_resource_mut::<$crate::io::embedded::EmbeddedAssetRegistry>()
            .expect("`EmbeddedAssetRegistry` must be inserted before using `embedded_asset!`");
        let path = $crate::embedded_path!($source_path, $path);
        let watched_path = $crate::io::embedded::__watched_path(file!(), $path);
        embedded.insert_asset(&watched_path, &path, include_bytes!($path).as_slice());
    }};
}

/// Load an [embedded asset].
///
/// This is useful if the embedded asset in question is not publicly exposed, but
/// you need to use it internally.
///
/// # Example
///
/// ```ignore
/// let shader = load_embedded_asset!(&asset_server, "shaders/rock.wgsl");
/// ```
///
/// # Syntax
///
/// This macro takes two arguments:
/// 1. The provider of the asset server. It may be `AssetServer`, `World`, `App` or `SubApp`.
/// 2. The path to the asset to embed, as a string literal.
///
/// # Usage
///
/// The advantage compared to using directly [`AssetServer::load`] is:
/// - This also accepts [`World`], [`App`] and [`SubApp`] arguments.
/// - This uses the exact same path as `embedded_asset!`, so you can keep it
///   consistent.
///
/// As a rule of thumb:
/// - If the asset in used in the same module as it is declared using `embedded_asset!`,
///   use this macro.
/// - Otherwise, use `AssetServer::load`.
///
/// [embedded asset]: crate::embedded_asset!
#[macro_export]
macro_rules! load_embedded_asset {
    (@get: $path: literal, $provider: expr $(,)?) => {{
        let path = $crate::embedded_path!($path);
        let path = $crate::path::AssetPath::from(path)
            .with_source($crate::io::embedded::EMBEDDED);
        let asset_server = $crate::io::embedded::__GetAssetServer::get($provider);
        (path, asset_server)
    }};
    ($provider: expr, $path: literal $(,)?) => {{
        let (path, asset_server) = $crate::load_embedded_asset!(@get: $path, $provider);
        asset_server.load(path)
    }};
}

/// Associates `$handle` with the asset built from the **text** file at `$path_str`.
///
/// The file is embedded with [`include_str!`], so it is compiled into the binary and can never
/// fail to load. `$builder` receives the text and the path of the file, and returns the asset.
/// Extra arguments after `$builder` are passed to it after the text and the path.
///
/// Unlike an [embedded asset], this macro inserts the given data into [`Assets`] directly, so
/// the asset is not reachable through an [`AssetPath`] and does not hot-reload.
///
/// The input handle should point to an empty slot, otherwise the value will not be inserted.
///
/// # Example
///
/// ```ignore
/// let builder = |data: &'static str, path: &str| Shader::from_wgsl(data, path);
/// // - data: include_str!("shaders/default.wgsl")
/// // - path: file!() + "shaders/default.wgsl"
/// load_internal_asset!(app, shader_handle, "shaders/default.wgsl", builder);
/// ```
///
/// [`Assets`]: crate::assets::Assets
/// [`AssetPath`]: crate::path::AssetPath
///
/// [embedded asset]: crate::embedded_asset!
#[macro_export]
macro_rules! load_internal_asset {
    ($app: expr, $handle: expr, $path_str: expr, $builder: expr $(,)?) => {{
        let world = $crate::io::embedded::__GetWorldMut::world_mut($app);
        let mut assets = world.resource_mut::<$crate::assets::Assets<_>>();
        let ctor = || {
            let path = $crate::io::embedded::__normalize_file_str(::core::file!())
                .parent()
                .expect("`file!()` always names a file in a directory")
                .join($path_str);
            let path_string = path.to_string_lossy();
            ($builder)(::core::include_str!($path_str), path_string.as_ref())
        };
        assets
            .get_or_insert($handle.id(), ctor)
            .expect("the handle points at a valid asset slot");
    }};
    // A builder that needs more than the file's text and path takes its extra arguments after them.
    ($app: expr, $handle: expr, $path_str: expr, $builder: expr $(, $param:expr)+ $(,)?) => {{
        let world = $crate::io::embedded::__GetWorldMut::world_mut($app);
        let mut assets = world.resource_mut::<$crate::assets::Assets<_>>();
        let ctor = || {
            let path = $crate::io::embedded::__normalize_file_str(::core::file!())
                .parent()
                .expect("`file!()` always names a file in a directory")
                .join($path_str);
            let path_string = path.to_string_lossy();
            ($builder)(
                ::core::include_str!($path_str),
                path_string.as_ref(),
                $($param),+
            )
        };
        assets
            .get_or_insert($handle.id(), ctor)
            .expect("the handle points at a valid asset slot");
    }};
}

/// Associates `$handle` with the asset built from the **binary** file at `$path_str`.
///
/// The file is embedded with [`include_bytes!`], so it is compiled into the binary and can never
/// fail to load. `$builder` receives the bytes and the path of the file, and returns the asset.
///
/// Unlike an [embedded asset], this macro inserts the given data into [`Assets`] directly, so
/// the asset is not reachable through an [`AssetPath`] and does not hot-reload.
///
/// The input handle should point to an empty slot, otherwise the value will not be inserted.
///
/// # Example
///
/// ```ignore
/// let builder = |data: &'static [u8], path: &str| Image::from_buffer(data, path);
/// // - data: include_bytes!("textures/icon.png")
/// // - path: file!() + "textures/icon.png"
/// load_internal_binary_asset!(app, image_handle, "textures/icon.png", builder);
/// ```
///
/// [`Assets`]: crate::assets::Assets
/// [`AssetPath`]: crate::path::AssetPath
///
/// [embedded asset]: crate::embedded_asset!
#[macro_export]
macro_rules! load_internal_binary_asset {
    ($app: expr, $handle: expr, $path_str: expr, $builder: expr) => {{
        let world = $crate::io::embedded::__GetWorldMut::world_mut($app);
        let mut assets = world.resource_mut::<$crate::assets::Assets<_>>();
        let ctor = || {
            let path = $crate::io::embedded::__normalize_file_str(::core::file!())
                .parent()
                .expect("`file!()` always names a file in a directory")
                .join($path_str);
            let path_string = path.to_string_lossy();
            ($builder)(::core::include_bytes!($path_str), path_string.as_ref())
        };
        assets
            .get_or_insert($handle.id(), ctor)
            .expect("the handle points at a valid asset slot");
    }};
}

// -----------------------------------------------------------------------------
// Macro providers

use std::path::{Path, PathBuf};

use zlim_app::{App, SubApp};
use zlim_core::world::World;

use crate::server::AssetServer;

#[doc(hidden)]
pub trait __GetWorldMut {
    /// Returns the world of `this`.
    fn world_mut(this: &mut Self) -> &mut World;
}

#[doc(hidden)]
pub trait __GetAssetServer {
    fn get(this: &Self) -> &AssetServer;
}

impl __GetWorldMut for App {
    #[inline]
    fn world_mut(this: &mut Self) -> &mut World {
        this.main_world_mut()
    }
}

impl __GetWorldMut for SubApp {
    #[inline]
    fn world_mut(this: &mut Self) -> &mut World {
        SubApp::world_mut(this)
    }
}

impl __GetWorldMut for World {
    #[inline]
    fn world_mut(this: &mut Self) -> &mut World {
        this
    }
}

impl __GetAssetServer for AssetServer {
    #[inline]
    fn get(this: &Self) -> &AssetServer {
        this
    }
}

impl __GetAssetServer for App {
    #[inline]
    fn get(this: &Self) -> &AssetServer {
        this.main_world().resource::<AssetServer>()
    }
}

impl __GetAssetServer for SubApp {
    #[inline]
    fn get(this: &Self) -> &AssetServer {
        this.world().resource::<AssetServer>()
    }
}

impl __GetAssetServer for World {
    #[inline]
    fn get(this: &Self) -> &AssetServer {
        this.resource::<AssetServer>()
    }
}

// -----------------------------------------------------------------------------

/// Returns the path used by the watcher.
#[doc(hidden)]
pub fn __watched_path(source_file_path: &'static str, asset_path: &'static str) -> PathBuf {
    crate::cfg::watch! {
        if {
            let path = __normalize_file_str(source_file_path)
                .parent()
                .unwrap()
                .join(asset_path);
            crate::utils::normalize_path(&path)
        } else {
            let _ = source_file_path;
            let _ = asset_path;
            PathBuf::new()
        }
    }
}

/// Returns the path the compile-time `file!()` string names, with `\` rewritten to `/` on
/// non-Windows targets.
#[doc(hidden)]
pub fn __normalize_file_str(file: &str) -> PathBuf {
    #[cfg(target_family = "windows")]
    return PathBuf::from(file);

    // ↓ `file!()` is a compile-time string, so cross-compilation needs to be handled here.
    #[cfg(not(target_family = "windows"))]
    if file.as_bytes().contains(&b'\\') {
        let mut buffer = String::from(file);
        crate::path::normalize_separators(&mut buffer);
        PathBuf::from(buffer)
    } else {
        PathBuf::from(file)
    }
}

/// Maps a source file path onto its `embedded` asset path.
///
/// # Panics
///
/// Panics when `src_prefix` does not occur in `file_path`, because the asset path cannot be
/// derived from a file that is not inside that source directory.
#[doc(hidden)]
pub fn __embedded_asset_path(
    file_path: &str,
    crate_name: &str,
    src_prefix: &Path,
    asset_path: &Path,
) -> PathBuf {
    let file_path: PathBuf = __normalize_file_str(file_path);

    let mut maybe_parent = file_path.parent();
    let after_src = loop {
        let Some(parent) = maybe_parent else {
            panic!("Failed to find src_prefix {src_prefix:?} in {file_path:?}")
        };
        if parent.ends_with(src_prefix) {
            break file_path.strip_prefix(parent).unwrap();
        }
        maybe_parent = parent.parent();
    };
    let asset_path = after_src.parent().unwrap().join(asset_path);

    Path::new(crate_name).join(asset_path)
}
