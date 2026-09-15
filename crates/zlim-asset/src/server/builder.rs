//! Chained configuration for starting an asset load or save.

use core::any::TypeId;
use std::borrow::Cow;

use crate::asset::Asset;
use crate::handle::{ErasedHandle, Handle};
use crate::loaded::LoadedUntypedAsset;
use crate::loader::AssetLoader;
use crate::path::AssetPath;
use crate::saver::AssetSaver;
use crate::server::AssetServer;
use crate::server::event::SaveCommand;

// -----------------------------------------------------------------------------
// Guard

/// An opaque item held until a load or a save has finished.
///
/// Its [`Drop`] runs when the work is over (successfully or not), which is how a caller asks to
/// be notified without the server having to know what it is waiting for.
pub(crate) type Guard = Box<dyn Send + Sync + 'static>;

/// Chains `guard` onto `slot`, so that both are held and dropped together.
///
/// A builder holds one slot but any number of guards, so each new guard is wrapped around the
/// previous one: dropping the slot drops every guard that was chained into it.
#[inline(never)]
fn chain_guard(slot: &mut Option<Guard>, guard: Guard) {
    if let Some(old) = slot.take() {
        *slot = Some(Box::new(move || {
            drop(old);
            drop(guard);
        }));
    } else {
        *slot = Some(guard);
    }
}

// -----------------------------------------------------------------------------
// LoadBuilder

/// A chained description of one load, started by `AssetServer::load` and its siblings.
///
/// Nothing happens until a terminal method ([`load`], [`load_erased`] or [`load_untyped`]) is
/// called, and even that only *requests* the load: it returns the handle right away, and the
/// bytes are read by a task of the server's own.
///
/// [`load`]: Self::load
/// [`load_erased`]: Self::load_erased
/// [`load_untyped`]: Self::load_untyped
#[must_use = "a `LoadBuilder` does nothing until a terminal method is called"]
pub struct LoadBuilder<'a> {
    /// The asset server on which the load is invoked.
    asset_server: &'a AssetServer,

    /// The loader the caller forces, if any: a type path or a type name, as the registry accepts both.
    loader: Option<Cow<'static, str>>,

    /// Whether unapproved paths are allowed to be loaded.
    override_unapproved: bool,

    /// A "guard" that is held until the load has fully completed.
    guard: Option<Guard>,
}

impl<'a> LoadBuilder<'a> {
    /// Creates a builder that loads through `asset_server`.
    #[inline]
    pub(crate) fn new(asset_server: &'a AssetServer) -> Self {
        Self {
            asset_server,
            loader: None,
            override_unapproved: false,
            guard: None,
        }
    }

    /// Sets whether a path that escapes its source root is loaded anyway.
    #[inline]
    #[must_use = "the load doesn't start until LoadBuilder has been consumed"]
    pub fn override_unapproved(mut self, value: bool) -> Self {
        self.override_unapproved = value;
        self
    }

    /// Sets the guard item that is held during the load.
    ///
    /// It is dropped once the asset has been loaded or the load has failed, so its [`Drop`] can
    /// tell the caller that the load is over. Guards accumulate: every one of them is held.
    ///
    /// A load that is not started — the handle was already loading, or already settled — has
    /// nothing to hold the guard, so the guard is dropped when the terminal method returns; do not
    /// rely on it to observe a load this builder did not trigger.
    #[inline]
    #[must_use = "the load doesn't start until LoadBuilder has been consumed"]
    pub fn with_guard(mut self, guard: impl Send + Sync + 'static) -> Self {
        chain_guard(&mut self.guard, Box::new(guard));

        self
    }

    /// Forces `L` to be the loader that reads the asset.
    ///
    /// Without it the `.meta` next to the asset chooses the loader, and the path's extension after
    /// that. With it the `.meta` no longer chooses — but it is still read, and `L` deserializes its
    /// settings as its own. A `.meta` that names another loader is therefore not rejected for the
    /// name alone, the name is simply ignored; the file is still parsed, so its settings must
    /// still deserialize as `L`'s, or the load fails with a meta parse error.
    ///
    /// This is the typed form of [`with_loader_name`](Self::with_loader_name): it passes the
    /// fully-qualified type path of `L`, which is always registered under exactly that key, so the
    /// load selects exactly this loader. (The *save* builder is deliberately different: what it
    /// writes is a `.meta` name rather than a look-up key, so it records
    /// [`L::default_name()`](AssetLoader::default_name) instead.)
    #[inline]
    #[must_use = "the load doesn't start until LoadBuilder has been consumed"]
    pub fn with_loader<L: AssetLoader>(mut self) -> Self {
        self.loader = Some(Cow::Borrowed(L::type_path()));
        self
    }

    /// Forces the loader registered under `name` to be the loader that reads the asset.
    ///
    /// The name is the lenient form: a fully-qualified type path and a short type name both resolve,
    /// so a string read out of a `.meta` file — or one written by hand — can be passed as it is.
    /// Everything else behaves as in [`with_loader`](Self::with_loader).
    ///
    /// **An empty `name` is not a name.** It is treated exactly like not specifying a loader at all —
    /// the look-up proceeds as if this method had not been called — which is the same convention
    /// [`AssetMeta`](crate::meta::AssetMeta) uses: an empty loader name in a config means "this config
    /// names no loader" (it is not even serialized), so the two spell the same thing. If there is no
    /// name to force, simply do not call this.
    #[inline]
    #[must_use = "the load doesn't start until LoadBuilder has been consumed"]
    pub fn with_loader_name(mut self, name: impl Into<Cow<'static, str>>) -> Self {
        self.loader = Some(name.into());
        self
    }

    /// Begins loading the asset of type `A` at `path` and returns its handle without waiting.
    ///
    /// # Panics
    ///
    /// Panics when `A` was never initialized — `app.init_asset::<A>()` was not called — because
    /// there is no handle provider to allocate from. The message names that missing call.
    #[inline]
    #[must_use = "not using the returned strong handle may result in the unexpected release of the asset"]
    pub fn load<'b, A: Asset>(self, path: impl Into<AssetPath<'b>>) -> Handle<A> {
        #[inline(never)]
        fn internal<A: Asset>(builder: LoadBuilder, path: AssetPath<'static>) -> Handle<A> {
            builder.asset_server.load_typed_asset_impl(
                builder.loader,
                path,
                builder.override_unapproved,
                builder.guard,
            )
        }

        internal(self, path.into().into_owned())
    }

    /// Type-erased counterpart of [`load`](Self::load).
    ///
    /// # Panics
    ///
    /// Panics when no handle provider is registered for `type_id` — the asset type was never
    /// initialized with `init_asset`. Only the id is known here, so the message shows the id
    /// instead of a type name.
    #[inline]
    #[must_use = "not using the returned strong handle may result in the unexpected release of the asset"]
    pub fn load_erased<'b>(self, type_id: TypeId, path: impl Into<AssetPath<'b>>) -> ErasedHandle {
        #[inline(never)]
        fn internal(
            builder: LoadBuilder,
            type_id: TypeId,
            path: AssetPath<'static>,
        ) -> ErasedHandle {
            builder.asset_server.load_erased_asset_impl(
                builder.loader,
                path,
                type_id,
                None,
                builder.override_unapproved,
                builder.guard,
            )
        }

        internal(self, type_id, path.into().into_owned())
    }

    /// Loads the asset at `path` without knowing its type.
    ///
    /// The returned handle refers to a [`LoadedUntypedAsset`] that carries the handle of the asset
    /// that was actually loaded; the concrete asset is registered under `path` as usual.
    ///
    /// # Panics
    ///
    /// Panics when [`LoadedUntypedAsset`] was never initialized with
    /// `app.init_asset::<LoadedUntypedAsset>()`. [`AssetPlugin`](crate::plugin::AssetPlugin) does
    /// that for an app, so this only happens on a server that is driven without the plugin.
    #[inline]
    #[must_use = "not using the returned strong handle may result in the unexpected release of the asset"]
    pub fn load_untyped<'b>(self, path: impl Into<AssetPath<'b>>) -> Handle<LoadedUntypedAsset> {
        #[inline(never)]
        fn internal(builder: LoadBuilder, path: AssetPath<'static>) -> Handle<LoadedUntypedAsset> {
            builder.asset_server.load_untyped_asset_impl(
                builder.loader,
                path,
                builder.override_unapproved,
                builder.guard,
            )
        }

        internal(self, path.into().into_owned())
    }
}

// -----------------------------------------------------------------------------
// SaveBuilder

/// A chained description of one save, started by `AssetServer::save`.
///
/// Nothing happens until [`save`](Self::save) / [`save_erased`](Self::save_erased) is called, and even
/// that only *queues* the request: the bytes are written when the server applies its save commands,
/// which is once a frame, not inside the call.
///
/// # The loader name and the saver are chosen independently
///
/// A save resolves two things that only *look* like one decision:
///
/// - the **saver**, from the asset type and the path — or from
///   [`with_saver`](Self::with_saver) / [`with_saver_name`](Self::with_saver_name), when the caller
///   names one;
/// - the **loader name** written into a meta, which the caller names (or leaves out, in which case the
///   meta records none).
///
/// Nothing here checks them against each other: the saver is not filtered by the loader name, and the
/// loader name is not checked against the settings the saver produces (the saver's
/// [`LoaderSettings`](AssetSaver::LoaderSettings) is written as it is, and only the loader that is
/// named later decides whether it can read them back).
///
/// So when an asset type has several usable savers, naming a loader is usually only half the
/// decision: name the saver as well, so that the pair written out is the one that was meant.
#[must_use = "a `SaveBuilder` does nothing until a terminal method is called"]
pub struct SaveBuilder<'a> {
    /// The asset server the save is invoked on.
    asset_server: &'a AssetServer,

    /// The loader to record in the `.meta`, if the caller named one.
    loader: Option<Cow<'static, str>>,

    /// The saver that writes the bytes, if the caller named one.
    saver: Option<Cow<'static, str>>,

    /// Whether a `.meta` is written next to the asset.
    save_meta: bool,

    /// Whether unapproved paths are allowed to be written.
    override_unapproved: bool,

    /// A "guard" that is held until the save has fully completed.
    guard: Option<Guard>,
}

impl<'a> SaveBuilder<'a> {
    /// Creates a builder that saves through `asset_server`.
    #[inline]
    pub(crate) fn new(asset_server: &'a AssetServer) -> Self {
        Self {
            asset_server,
            loader: None,
            saver: None,
            save_meta: false,
            override_unapproved: false,
            guard: None,
        }
    }

    /// Sets whether a path that escapes its source root is written anyway.
    #[inline]
    #[must_use = "the save doesn't start until SaveBuilder has been consumed"]
    pub fn override_unapproved(mut self, value: bool) -> Self {
        self.override_unapproved = value;
        self
    }

    /// Sets the guard item that is held during the save.
    ///
    /// It is dropped once the save is over, so its [`Drop`] can tell the caller about it. Guards
    /// accumulate: every one of them is held.
    #[inline]
    #[must_use = "the save doesn't start until SaveBuilder has been consumed"]
    pub fn with_guard(mut self, guard: impl Send + Sync + 'static) -> Self {
        chain_guard(&mut self.guard, Box::new(guard));

        self
    }

    /// Sets whether a `.meta` is written next to the asset.
    ///
    /// Defaults to `false`, which writes the asset bytes alone. With `true` the meta the chosen saver
    /// builds is written as well, which is what records the loader those bytes are read back with.
    ///
    /// That is the only thing a loader name is carried by, so **naming one (with
    /// [`with_loader`](Self::with_loader) or [`with_loader_name`](Self::with_loader_name)) without
    /// `with_meta(true)` is a no-op**: there is no meta to write it into, and the server logs a
    /// warning when the save runs instead of failing.
    #[inline]
    #[must_use = "the save doesn't start until SaveBuilder has been consumed"]
    pub fn with_meta(mut self, save_meta: bool) -> Self {
        self.save_meta = save_meta;
        self
    }

    /// Names the loader the saved bytes are read back with — the one a `.meta` written by
    /// [`with_meta(true)`](Self::with_meta) records.
    ///
    /// It records [`L::default_name()`](AssetLoader::default_name), the name `L` uses for itself in
    /// the `.meta` files it creates: the short type name when `L` sets
    /// [`SHORT_NAME`](AssetLoader::SHORT_NAME), its fully-qualified type path otherwise. Both resolve
    /// when the asset is loaded again, so this only decides what the written `.meta` looks like.
    ///
    /// The *load* builder is deliberately stricter: [`LoadBuilder::with_loader`] passes the type path
    /// there, because a look-up has to select exactly that loader rather than one that reads the same
    /// way.
    ///
    /// Naming a loader does not narrow the saver — nothing checks that the saver's settings are the
    /// ones this loader reads back — so when the asset type has several usable savers, name the saver
    /// too; see [`SaveBuilder`] for the whole note.
    ///
    /// Without [`with_meta(true)`](Self::with_meta) the name is ignored — there is no meta to carry
    /// it — and the server logs a warning when the save runs.
    #[inline]
    #[must_use = "the save doesn't start until SaveBuilder has been consumed"]
    pub fn with_loader<L: AssetLoader>(mut self) -> Self {
        self.loader = Some(Cow::Borrowed(L::default_name()));
        self
    }

    /// Names the loader the saved bytes are read back with — the one a `.meta` written by
    /// [`with_meta(true)`](Self::with_meta) records.
    ///
    /// This is the untyped form of [`with_loader`](Self::with_loader): the name is the lenient form,
    /// so a fully-qualified type path and a short type name both resolve when the asset is loaded
    /// again.
    ///
    /// **Do not call this with an empty name.** An empty one is currently skipped — the meta side
    /// never serializes a loader name that is empty, so the `.meta` ends up naming no loader — but
    /// relying on that is not recommended: if there is no name to record, do not call this at all.
    ///
    /// Naming a loader does not narrow the saver — nothing checks that the saver's settings are the
    /// ones this loader reads back — so when the asset type has several usable savers, name the saver
    /// too; see [`SaveBuilder`] for the whole note.
    ///
    /// Without [`with_meta(true)`](Self::with_meta) the name is ignored — there is no meta to carry
    /// it — and the server logs a warning when the save runs.
    #[inline]
    #[must_use = "the save doesn't start until SaveBuilder has been consumed"]
    pub fn with_loader_name(mut self, name: impl Into<Cow<'static, str>>) -> Self {
        self.loader = Some(name.into());
        self
    }

    /// Forces `S` to be the saver that writes the asset.
    ///
    /// Without it the saver is chosen from the asset type and the path, the way a save resolves one.
    /// This is the typed form of [`with_saver_name`](Self::with_saver_name): it passes the
    /// fully-qualified type path of `S`, which is always registered under exactly that key.
    #[inline]
    #[must_use = "the save doesn't start until SaveBuilder has been consumed"]
    pub fn with_saver<S: AssetSaver>(mut self) -> Self {
        self.saver = Some(Cow::Borrowed(S::type_path()));
        self
    }

    /// Forces the saver registered under `name` to be the saver that writes the asset.
    ///
    /// The name is the lenient form, exactly as in
    /// [`LoadBuilder::with_loader_name`](LoadBuilder::with_loader_name).
    ///
    /// **`name` should not be empty.** An empty one counts as no name at all: the saver is then
    /// resolved from the asset type and the path, as if this had not been called — which is not
    /// recommended, so pass a name or do not call this.
    ///
    /// Forcing the saver says nothing about the loader name that ends up next to its settings in the
    /// `.meta` — the two are not checked against each other; see [`SaveBuilder`].
    #[inline]
    #[must_use = "the save doesn't start until SaveBuilder has been consumed"]
    pub fn with_saver_name(mut self, name: impl Into<Cow<'static, str>>) -> Self {
        self.saver = Some(name.into());
        self
    }

    /// Writes the asset `handle` refers to to `path`.
    #[inline]
    pub fn save<'b, A: Asset>(self, path: impl Into<AssetPath<'b>>, handle: Handle<A>) {
        self.save_erased(path.into(), handle.erased())
    }

    /// Type-erased counterpart of [`save`](Self::save).
    #[inline]
    pub fn save_erased<'b>(self, path: impl Into<AssetPath<'b>>, handle: ErasedHandle) {
        let path = path.into().into_owned();

        let cmd = SaveCommand {
            handle,
            path,
            loader: self.loader,
            saver: self.saver,
            save_meta: self.save_meta,
            override_unapproved: self.override_unapproved,
            guard: self.guard,
        };

        self.asset_server.push_save_command(cmd);
    }
}
