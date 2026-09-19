use core::any::TypeId;
use std::sync::Arc;

use zlim_app::{App, First, PostUpdate, PreUpdate, SubApp};
use zlim_core::job::{JobId, JobLabel};
use zlim_core::world::{FromWorld, World};

use crate::asset::Asset;
use crate::assets::Assets;
use crate::change::AssetChanges;
use crate::event::{AssetEvent, AssetLoadFailedEvent};
use crate::handle::{AssetHandleProvider, Handle};
use crate::ident::{AssetIndexAllocator, AssetSourceId};
use crate::loader::AssetLoader;
use crate::path::AssetPath;
use crate::processor::{AssetProcessServer, AssetProcessor};
use crate::saver::AssetSaver;
use crate::server::{AssetServer, LoadBuilder, SaveBuilder};
use crate::source::{AssetSourceBuilder, AssetSourceBuilders};

// -----------------------------------------------------------------------------

/// Adds the asset operations of the [`AssetServer`] to a [`World`].
pub trait WorldAssetExt {
    /// Adds `asset` to its [`Assets`] collection, like [`Assets::add`].
    fn add_asset<A: Asset>(&mut self, asset: impl Into<A>) -> Handle<A>;

    /// Loads the asset at `path`, like [`AssetServer::load`].
    fn load_asset<'a, A: Asset>(&self, path: impl Into<AssetPath<'a>>) -> Handle<A>;

    /// Starts a load, like [`AssetServer::load_builder`].
    fn load_builder(&self) -> LoadBuilder<'_>;

    /// Saves the asset at `handle` to `path`, like [`AssetServer::save`].
    fn save_asset<'a, A: Asset>(&self, path: impl Into<AssetPath<'a>>, handle: Handle<A>);

    /// Starts a save, like [`AssetServer::save_builder`].
    fn save_builder(&self) -> SaveBuilder<'_>;
}

impl WorldAssetExt for World {
    /// Adds `asset` to its [`Assets`] collection, like [`Assets::add`].
    ///
    /// # Panics
    ///
    /// Panics when `A` was never registered (`AppAssetExt::init_asset`) or the asset plugin
    /// is not part of the app, since then there is no [`Assets<A>`] resource to add to.
    fn add_asset<A: Asset>(&mut self, asset: impl Into<A>) -> Handle<A> {
        self.resource_mut::<Assets<A>>().add(asset)
    }

    /// Loads the asset at `path`, like [`AssetServer::load`].
    ///
    /// The operation is internally asynchronous, the function returns immediately without waiting.
    ///
    /// # Panics
    ///
    /// Panics when there is no [`AssetServer`] resource: the asset plugin must be applied first.
    fn load_asset<'a, A: Asset>(&self, path: impl Into<AssetPath<'a>>) -> Handle<A> {
        self.resource::<AssetServer>().load(path)
    }

    /// Starts a load, like [`AssetServer::load_builder`].
    ///
    /// # Panics
    ///
    /// Panics when there is no [`AssetServer`] resource: the asset plugin must be applied first.
    fn load_builder(&self) -> LoadBuilder<'_> {
        self.resource::<AssetServer>().load_builder()
    }

    /// Saves the asset at `handle` to `path`, like [`AssetServer::save`].
    ///
    /// The operation is internally asynchronous, the function returns immediately without waiting.
    ///
    /// # Panics
    ///
    /// Panics when there is no [`AssetServer`] resource: the asset plugin must be applied first.
    fn save_asset<'a, A: Asset>(&self, path: impl Into<AssetPath<'a>>, handle: Handle<A>) {
        self.resource::<AssetServer>().save(path, handle)
    }

    /// Starts a save, like [`AssetServer::save_builder`].
    ///
    /// # Panics
    ///
    /// Panics when there is no [`AssetServer`] resource: the asset plugin must be applied first.
    fn save_builder(&self) -> SaveBuilder<'_> {
        self.resource::<AssetServer>().save_builder()
    }
}

// -----------------------------------------------------------------------------

/// Registers asset types, loaders and sources on an [`App`] (or a [`SubApp`]).
pub trait AppAssetExt {
    /// Registers the asset type `A`, making it loadable.
    ///
    /// This adopts `Assets<A>` into the server, registers the `AssetEvent<A>`
    /// and `AssetLoadFailedEvent<A>` messages and the per-type change table,
    /// and inserts the jobs that drive them.
    ///
    /// Registering the same type again keeps the existing `Assets<A>`:
    /// the server holds a clone of its handle provider (and of its slot allocator),
    /// so replacing the resource would invalidate every handle that already points
    /// into it.
    #[doc(alias = "register_asset")]
    fn init_asset<A: Asset>(&mut self) -> &mut Self;

    /// Registers an asset source.
    ///
    /// The builder is stored and consumed when [`AssetPlugin`] is applied, which is what builds
    /// the sources. Once that has happened the call is ignored and an error is logged, because
    /// the sources it would have joined are already in use.
    ///
    /// [`AssetPlugin`]: crate::plugin::AssetPlugin
    fn register_asset_source(
        &mut self,
        id: impl Into<AssetSourceId>,
        source: AssetSourceBuilder,
    ) -> &mut Self;

    /// Registers the saver `S`, built from the world.
    ///
    /// # Panics
    ///
    /// Panics when there is no [`AssetServer`] resource: the asset plugin must be applied first.
    fn init_asset_saver<S: AssetSaver + FromWorld>(&mut self) -> &mut Self;

    /// Registers `saver`, so that the assets it produces can be saved.
    ///
    /// # Panics
    ///
    /// Panics when there is no [`AssetServer`] resource: the asset plugin must be applied first.
    fn register_asset_saver<S: AssetSaver>(&mut self, saver: S) -> &mut Self;

    /// Registers the loader `L`, built from the world.
    ///
    /// # Panics
    ///
    /// Panics when there is no [`AssetServer`] resource: the asset plugin must be applied first.
    fn init_asset_loader<L: AssetLoader + FromWorld>(&mut self) -> &mut Self;

    /// Registers `loader`, so that the assets it produces can be loaded.
    ///
    /// # Panics
    ///
    /// Panics when there is no [`AssetServer`] resource: the asset plugin must be applied first.
    fn register_asset_loader<L: AssetLoader>(&mut self, loader: L) -> &mut Self;

    /// Pre-registers `L` and the extensions it declares.
    ///
    /// The extensions are [`AssetLoader::EXTENSIONS`] — the loader's own — because that is what its
    /// registration will claim later; passing them in would only be a second place to keep them in
    /// sync. They are claimed right away and every asset that resolves to `L` waits for it instead
    /// of failing, so an asset of a type another crate provides can be loaded before that crate is
    /// built. The wait ends when `L` is registered.
    ///
    /// # Panics
    ///
    /// Panics when there is no [`AssetServer`] resource: the asset plugin must be applied first.
    fn preregister_asset_loader<L: AssetLoader>(&mut self) -> &mut Self;

    /// Registers `processor` with the app's importer.
    ///
    /// The importer only exists in [`AssetServerMode::Processed`] with the processor override
    /// left at its default; without one there is nowhere to register, so the processor is
    /// reported and ignored.
    ///
    /// [`AssetServerMode::Processed`]: crate::server::AssetServerMode::Processed
    fn register_asset_processor<P: AssetProcessor>(&mut self, processor: P) -> &mut Self;

    /// Makes `P` the processor that handles `extension` by default.
    ///
    /// `P` has to be registered first with
    /// [`register_asset_processor`](Self::register_asset_processor): the default names the processor
    /// by index, and one that is not registered is reported and ignored.
    ///
    /// Like that registration, this needs an importer: without one the default is reported and
    /// ignored.
    fn register_extension<P: AssetProcessor>(&mut self, extension: &str) -> &mut Self;
}

fn init_asset_impl<A: Asset>(world: &mut World) {
    const E: &str = "`AssetServer` does not exist yet: `AssetPlugin` has to be \
        applied first — call `App::build` before this, or order the current plugin \
        after `AssetPlugin` with `AssetPlugin::apply_before::<Self>` in its `build`";

    if !world.contains_resource::<Assets<A>>() {
        let assets = Assets::<A>::default();

        world
            .get_resource::<AssetServer>()
            .expect(E)
            .register_asset(&assets);

        world.insert_resource(assets);
    }

    // The importer's server is a server of its own, and it runs the loaders of the assets it
    // processes: a loader that hands out a handle while it runs (a sub-asset, a nested load)
    // allocates that handle there, so that server needs a provider for `A` as well. It gets a
    // provider of its own instead of the one `Assets<A>` owns: the importer's id space is meant to
    // be separate from the app's, which is why there is no `Assets<A>` behind it.
    if let Some(importer) = world.get_resource::<AssetProcessServer>() {
        let provider =
            AssetHandleProvider::new(TypeId::of::<A>(), Arc::new(AssetIndexAllocator::new()));
        importer.server().register_handle_provider(provider);
    }

    world.register_message::<AssetEvent<A>>();
    world.register_message::<AssetLoadFailedEvent<A>>();
    world.init_resource::<AssetChanges<A>>();

    use crate::assets::jobs::{HandleAssetDropEvents, HandleAssetEvents};
    use crate::change::ClampAssetChangesTick;
    use crate::server::jobs::HandleAssetSeverEvents;

    world
        .schedule_entry(PostUpdate)
        .insert::<HandleAssetEvents<A>>(());

    world
        .schedule_entry(PreUpdate)
        .insert::<HandleAssetDropEvents<A>>(());

    world
        .schedule_entry(First)
        .insert::<ClampAssetChangesTick<A>>(());

    // Dropped handles are processed after the load results of this frame have been applied.
    world.schedule_entry(PreUpdate).insert_order(&[
        JobId::isolated(HandleAssetSeverEvents::name()),
        JobId::isolated(HandleAssetDropEvents::<A>::name()),
    ]);
}

fn register_asset_source_impl(world: &mut World, id: AssetSourceId, source: AssetSourceBuilder) {
    if world.contains_resource::<AssetServer>() {
        zlim_log::error!(
            "The asset source '{id}' has to be registered before `AssetPlugin` builds the \
             sources; it is ignored."
        );
    } else {
        world
            .resource_mut_or_init::<AssetSourceBuilders>()
            .insert(id, source);
    }
}

fn register_asset_processor_impl<P: AssetProcessor>(world: &mut World, processor: P) {
    match world.get_resource::<AssetProcessServer>() {
        Some(server) => server.register_processor(processor),
        None => {
            ::core::hint::cold_path();
            zlim_log::error!(
                "`register_asset_processor` needs an `AssetProcessServer`, which `AssetPlugin` \
                 only builds in `AssetServerMode::Processed` with the importer enabled; the \
                 processor `{}` is ignored.",
                <P as zlim_path::TypePath>::type_path()
            );
        }
    }
}

fn register_extension_impl<P: AssetProcessor>(world: &mut World, extension: &str) {
    match world.get_resource::<AssetProcessServer>() {
        Some(server) => server.register_extension::<P>(extension),
        None => {
            ::core::hint::cold_path();
            zlim_log::error!(
                "`register_extension` needs an `AssetProcessServer`, which `AssetPlugin` \
                 only builds in `AssetServerMode::Processed` with the importer enabled; the default \
                 for `.{extension}` is ignored."
            );
        }
    }
}

impl AppAssetExt for App {
    #[inline]
    fn init_asset<A: Asset>(&mut self) -> &mut Self {
        init_asset_impl::<A>(self.main_world_mut());
        self
    }

    #[inline]
    fn register_asset_source(
        &mut self,
        id: impl Into<AssetSourceId>,
        source: AssetSourceBuilder,
    ) -> &mut Self {
        register_asset_source_impl(self.main_world_mut(), id.into(), source);
        self
    }

    #[inline]
    fn init_asset_saver<S: AssetSaver + FromWorld>(&mut self) -> &mut Self {
        let server = self.main_world_mut().resource::<AssetServer>();
        if !server.contains_saver::<S>() {
            let saver = S::from_world(self.main_world());
            self.register_asset_saver(saver);
        }
        self
    }

    #[inline]
    fn register_asset_saver<S: AssetSaver>(&mut self, saver: S) -> &mut Self {
        self.main_world_mut()
            .resource::<AssetServer>()
            .register_saver(saver);
        self
    }

    #[inline]
    fn init_asset_loader<L: AssetLoader + FromWorld>(&mut self) -> &mut Self {
        let server = self.main_world_mut().resource::<AssetServer>();
        if !server.contains_loader::<L>() {
            let loader = L::from_world(self.main_world());
            self.register_asset_loader(loader);
        }
        self
    }

    #[inline]
    fn register_asset_loader<L: AssetLoader>(&mut self, loader: L) -> &mut Self {
        self.main_world_mut()
            .resource::<AssetServer>()
            .register_loader(loader);
        self
    }

    #[inline]
    fn preregister_asset_loader<L: AssetLoader>(&mut self) -> &mut Self {
        self.main_world_mut()
            .resource::<AssetServer>()
            .preregister_loader::<L>();
        self
    }

    #[inline]
    fn register_asset_processor<P: AssetProcessor>(&mut self, processor: P) -> &mut Self {
        register_asset_processor_impl(self.main_world_mut(), processor);
        self
    }

    #[inline]
    fn register_extension<P: AssetProcessor>(&mut self, extension: &str) -> &mut Self {
        register_extension_impl::<P>(self.main_world_mut(), extension);
        self
    }
}

impl AppAssetExt for SubApp {
    #[inline]
    fn init_asset<A: Asset>(&mut self) -> &mut Self {
        init_asset_impl::<A>(self.world_mut());
        self
    }

    #[inline]
    fn register_asset_source(
        &mut self,
        id: impl Into<AssetSourceId>,
        source: AssetSourceBuilder,
    ) -> &mut Self {
        register_asset_source_impl(self.world_mut(), id.into(), source);
        self
    }

    #[inline]
    fn init_asset_saver<S: AssetSaver + FromWorld>(&mut self) -> &mut Self {
        let server = self.world_mut().resource::<AssetServer>();
        if !server.contains_saver::<S>() {
            let saver = S::from_world(self.world());
            self.register_asset_saver(saver);
        }
        self
    }

    #[inline]
    fn register_asset_saver<S: AssetSaver>(&mut self, saver: S) -> &mut Self {
        self.world_mut()
            .resource::<AssetServer>()
            .register_saver(saver);
        self
    }

    #[inline]
    fn init_asset_loader<L: AssetLoader + FromWorld>(&mut self) -> &mut Self {
        let server = self.world_mut().resource::<AssetServer>();
        if !server.contains_loader::<L>() {
            let loader = L::from_world(self.world());
            self.register_asset_loader(loader);
        }
        self
    }

    #[inline]
    fn register_asset_loader<L: AssetLoader>(&mut self, loader: L) -> &mut Self {
        self.world_mut()
            .resource::<AssetServer>()
            .register_loader(loader);
        self
    }

    #[inline]
    fn preregister_asset_loader<L: AssetLoader>(&mut self) -> &mut Self {
        self.world_mut()
            .resource::<AssetServer>()
            .preregister_loader::<L>();
        self
    }

    #[inline]
    fn register_asset_processor<P: AssetProcessor>(&mut self, processor: P) -> &mut Self {
        register_asset_processor_impl(self.world_mut(), processor);
        self
    }

    #[inline]
    fn register_extension<P: AssetProcessor>(&mut self, extension: &str) -> &mut Self {
        register_extension_impl::<P>(self.world_mut(), extension);
        self
    }
}
