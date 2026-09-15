# zlim-asset

The asset system: the infrastructure for loading, caching and processing assets.

## Assets

An asset type is defined through the `Asset` trait, and collects its dependencies through
`VisitAssetDependencies`. Every asset is a `TypePath` as well, since that is how the type is named
where a `.meta` file has to name it:

```rust, no_run
use zlim_asset::prelude::Asset;
use zlim_path::TypePath;

#[derive(Asset, TypePath)]
pub struct MyAsset { /* ... */ }
```

An *asset path* is what loading, saving and processing work with, defined by the `AssetPath` type.

```rust, no_run
use zlim_asset::prelude::AssetPath;

let _ = AssetPath::parse("images/example.png");
let _ = AssetPath::parse("https://example.png");
let _ = AssetPath::parse("embedded://config.obj#mesh");
```

An asset is addressed by a three-part path: `source://` + `path` + `#label`, where the source and
the label are optional. A part that is not given has to be *absent*, separators included: `path#` is
not a valid path, it has to be written `path`.

## Asset sources

`source://` selects the asset source, and the default source is used when none is given: it is a
network request on wasm, and the filesystem on the other platforms.

Every source can be given a `reader`, a `writer` and a `watcher` — they read and write the files and
report the changes (which is what hot reloading is built on). The `reader` is required, the other
two are optional.

`AssetSourceBuilders` is a resource (a zlim-core `Resource`) that lives in the main world. To add a
source of your own, the custom `builder` has to be inserted into that resource **before**
`AssetPlugin` is applied: the plugin builds the sources of every asset service out of those builders
when it is applied, and a source cannot be changed once it has been built.

When the `AssetSourceBuilders` resource does not exist, the asset plugin uses the default builders.
When those builders hold no builder for the default source, a platform-specific one is used:

- wasm makes network requests;
- android uses the application's own asset folder;
- every other platform uses the `assets` folder of the project.

Next to the default source, the asset plugin provides an `embedded` source, which is what
`include_bytes!`-style macros register their inline assets into. See the `io::embedded` module of
this crate for the details.

## Paths and labels

The `path` part is usually an ordinary path. In a filesystem source it is the path of the file, so
`image/example.png` is mapped to `assets/image/example.png` in the ordinary default source.

`#label` marks a *sub-asset*, which is what a single file that produces several assets uses. What a
read gives back follows from the source and the path, and reading a sub-asset that is not loaded yet
normally loads the whole file (and loading a whole file normally loads its sub-assets along with it).

For sub-assets and dependency assets, see the `Assets` section below and the `loader` module of this
crate.

## Asset storage

An asset is stored in the main world through `Assets<A>`, and the outside world normally holds a
reference to it through `Handle<A>`, which also keeps it alive.

A `Handle` is either a `Uuid` handle or a `Strong` handle.

A `Uuid` handle is normally chosen by the user (through this crate's `uuid_handle!` macro). Such an
asset usually has no resource path and is not loaded from a source: the user inserts it themselves
when the program starts. It is not managed by this crate's asset service either — its lifetime is
entirely the user's.

A `Strong` handle is an ordinary runtime asset, normally loaded from an asset path — that is, the
way described above. Those assets are managed by this crate's asset service: the `Handle` counts the
references, and the asset is released once none is left.

## The asset server

`AssetServer` is a resource (a `zlim_core::Resource`) that lives in the main world. It loads and
keeps assets, and maintains the asset sources and the runtime asset information.

```rust, ignore
fn system(server: Res<AssetServer>) {
    let handle: Handle<Image> = server.load("images/example.png");
    // do something
    server.save("images/copied.png", handle);
}
```

Four things matter inside `AssetServer`: the list of asset sources, the list of asset loaders, the
list of asset savers, and the list of asset infos.

The asset sources were described above, so they are not repeated here.

An *asset loader* controls how an asset is loaded from a source; see the `loader` module of this
crate. When a load is started through the asset server, it returns a `Handle` immediately and starts
an asynchronous task in the background, which picks the loader the input calls for, loads the asset,
inserts it into the main world's `Assets`, and records the related information in the asset server.

Which loader is chosen normally follows from these inputs:

- the loader type path a `.meta` names — the strict form, and authoritative: when it is given and no
  loader has it, the search stops there;
- the loader type name — the lenient form of the same thing, which a full type path resolves as well;
- the asset type, when it is known (this narrowing is skipped for a path that has a label, since a
  sub-asset may well have a different type than the loader producing it);
- the extensions of the asset path: the full extension first (`foo.tar.gz` → `tar.gz`), then each
  secondary extension (`gz`), compared case-insensitively, newest loader first.

When nothing fits, the load fails. When a name is shared by several loaders and neither the asset
type nor the extension can tell them apart, the load fails with an ambiguity error naming the
candidates; the one case that warns and continues instead is an asset type with several candidate
loaders, where the newest one is used. The `loader` module spells this out step by step.

The *metadata* is normally a `.meta` file with the same path as the asset, plus a `.meta`
extension, and it is where the loader an asset wants can be named.

An *asset saver* controls how an asset is saved to a source; see the `saver` module of this crate.
Saving is asynchronous as well, because the assets live in the world's `Assets` and the asset server
itself stores no asset data (only asset information). A save command is queued, and a job that runs
once per frame lets the `World` handle those commands.

The caller passes an asset path and a handle, and the asset type is guaranteed to be known at that
point. The choice works like the loader's:

- the asset type (known);
- the saver name (when one is given);
- the extension of the asset path.

A saver can also be given extra options, such as whether a `.meta` file is written, and which loader
that `.meta` file should name.

Today the asset savers can only handle simple assets, without a `#label`; that may be completed
later.

## Hot reloading

An asset source can register a `watcher`, which observes changes to its assets. This crate provides
the `watch` feature: when it is enabled, Windows, Linux and macOS get a default asset watcher that
watches for file changes.

The `World` handles those change events through a job that runs once per frame, and reloads the
assets of the paths involved.

`embedded` assets support hot reloading as well, but the implementation is a little different.
`embedded` is a virtual filesystem in memory: data added through `embedded_asset!` is normally stored
in it as a `&'static [u8]` directly. When watching is enabled, `embedded` instead keeps a mapping
from those embedded assets to the files they came from; when such a file changes, it is read again
and the data is stored as an `Arc<[u8]>` at runtime, which is what makes the reload work.

## Asset processing

If you look at the implementations of `AssetSource`, you will see that every source has two sets of
readers, writers and watchers: the ordinary one and the `processed` one.

The asset server can run in processed mode or in unprocessed mode. Unprocessed mode uses the
ordinary readers and writers — the ones described above. Processed mode uses the `processed` ones,
which may be mapped to different paths.

In the filesystem version of the default source, for example, the ordinary path is the `assets`
folder and the processed path is the `imported_assets/default` folder. The mode decides which path
of each source the asset server reads and observes. The asset server's saver always points at the
ordinary path.

Processing an asset usually means letting the asset plugin enable `AssetProcessServer`, which — like
`AssetServer` — is a resource of the main world.

In unprocessed mode there is only `AssetServer`, which reads and writes the contents of the ordinary
path (such as `assets`) directly: simple and straightforward.

In processed mode, `AssetProcessServer` normally reads the contents of the ordinary path (such as
`assets`), processes the assets through the built-in processors, and saves the processed assets to
the processed path (such as the `imported_assets/default` folder). The asset server `AssetServer`
then reads the contents of the processed folder directly. That is also why the asset server always
saves its files to the ordinary path.

Not every source has two paths: `embedded` and `http`, for example, use the same path for both.
Today a source is taken to be processed when it has a `processed_writer`. If a source should not be
processed, do not set one — with the reader before and after processing pointing at the same folder,
severe damage can happen, such as every file being deleted.

## Asset plugins

This crate provides three plugins today: `AssetPlugin`, `WebAssetPlugin` and
`AssetDiagnosticsPlugin`.

`AssetPlugin` is the asset plugin: it initializes the asset sources, creates the asset services, and
configures whether the processed mode is used. 

```rust, ignore
App::new()
  .add_plugins(AssetPlugin::default());
  .build();
  .init_asset::<MyAsset>()
  .register_asset_loader(MyAssetLoader)
  .run();
```

If you want custom asset sources, keep the `AssetSourceBuilder` added before this plugin is applied,
or it will be ignored. Registering asset types and loaders/savers, on the other hand, has to happen
**after** `AssetPlugin`'s `apply` — they need the asset server to exist.

An application should normally always initialize the asset system with `AssetPlugin`, because much of
the internals depends on the plugin to keep the related logic running: a wrong manual initialization
can leave some event queues without a reader, and their memory use grows without bound.

`WebAssetPlugin` initializes the network asset sources. It needs no feature itself and is always
available, but by default it has no effect at all. The `http` or `https` cargo feature has to be
enabled; then it adds the builders of those two sources during its `apply`. It applies before the
asset plugin.

`AssetDiagnosticsPlugin` provides diagnostic information, based on `zlim_diagnostic`. Today we only
provide one counter, of the load tasks the asset server has *started* (`STARTED_LOAD_COUNT`). It is
imprecise, diagnostic-only information, and it can wrap around (although a `usize` is unlikely to
overflow). Strictly speaking it does not depend on the asset plugin, because the related diagnostic
jobs are skipped when the asset server does not exist, and the program keeps running safely.

## Cargo features

- `debug` — performs many data checks and prints a lot of debug information.

- `watch` — once started, enables watching asset events on the platforms that can, by default (this
  can be overridden through `AssetPlugin`'s parameter). It pulls in the `notify-debouncer-full` crate
  and implements filesystem-based asset watching. Today this only takes effect on the Windows, Linux
  and macOS targets; on other platforms (such as `wasm`) the feature has no effect at all.

- `http` / `https` — add the implementations of the `http` and `https` sources for
  `WebAssetPlugin`, on basically every platform. Without these features `WebAssetPlugin` is a no-op.

- `web_asset_cache` — caches web assets, in the `.web-asset-cache` folder. This only takes effect on
  the Windows, Linux and macOS targets. Note that this is a development-stage feature: its caching
  strategy is very simple — it validates the `url` only, has no expiry policy, and uses a fixed hash
  algorithm so that the cached file names stay stable — which is why it must never be used in a
  release build.
