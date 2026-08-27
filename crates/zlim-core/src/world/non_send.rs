//! Non-`Send` world access.

use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};

use crate::world::World;

/// A main-thread-only handle to a [`World`] that permits access to `!Send` resources.
///
/// Resources of `!Send` / `!Sync` types (e.g. thread local data, raw pointers,
/// or platform handles) must only be touched from the main thread. They share
/// the same storage as regular resources, but the accessors are gated behind this type:
///
/// - Obtain one from [`World::with_non_send`] or [`World::with_non_send_mut`],
///   which run your closure on the main thread and hand it a `NonSendWorld`.
///
/// - Or fetch it as a system parameter — `&NonSendWorld` / `&mut NonSendWorld`
///   are `NON_SEND` parameters, so systems using them are scheduled on the main
///   thread only.
///
/// `NonSendWorld` is `#[repr(transparent)]` over [`World`] (the layout is
/// asserted at compile time) and derefs to it, so every `&World` API — queries,
/// entity lookups, `Send` resource access, and so on — works through it; only
/// the NonSend resource methods ([`insert_non_send`](Self::insert_non_send),
/// [`get_non_send`](Self::get_non_send), [`non_send_mut`](Self::non_send_mut),
/// …) are added on top.
///
/// The type is intentionally `!Send + !Sync` (via a [`PhantomData`] marker):
/// moving a `NonSendWorld` to another thread would defeat its purpose.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// // `Cell` is `Send` but not `Sync`, so this resource can only be touched
/// // from the main thread.
/// #[derive(TypePath, Resource)]
/// struct FrameStats {
///     frames: core::cell::Cell<u32>,
/// }
///
/// let mut world = World::alloc();
/// world.with_non_send_mut(|w| {
///     w.insert_non_send(FrameStats { frames: core::cell::Cell::new(0) });
/// });
///
/// world.with_non_send_mut(|w| {
///     w.non_send_mut::<FrameStats>().frames.set(42);
/// });
///
/// world.with_non_send(|w| {
///     assert_eq!(w.non_send::<FrameStats>().frames.get(), 42);
/// });
/// ```
#[repr(transparent)]
pub struct NonSendWorld {
    world: World,
    _marker: PhantomData<*const ()>,
}

const _: () = const {
    assert!(size_of::<NonSendWorld>() == size_of::<World>());
    assert!(align_of::<NonSendWorld>() == align_of::<World>());
};

impl Deref for NonSendWorld {
    type Target = World;

    fn deref(&self) -> &Self::Target {
        &self.world
    }
}

impl DerefMut for NonSendWorld {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.world
    }
}

impl World {
    /// Runs `f` on the main thread with read access to the world's `!Send` resources.
    ///
    /// The closure is sent to the main thread via [`zlim_task::invoke_on_main`],
    /// which blocks the calling thread until it finishes.
    ///
    /// If the current thread **is** the main thread, the closure runs
    /// synchronously with no extra hop.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// #[derive(TypePath, Resource)]
    /// struct FrameStats {
    ///     frames: core::cell::Cell<u32>,
    /// }
    ///
    /// let mut world = World::alloc();
    /// world.with_non_send_mut(|w| {
    ///     w.insert_non_send(FrameStats { frames: core::cell::Cell::new(1) });
    /// });
    ///
    /// world.with_non_send(|w| {
    ///     assert_eq!(w.non_send::<FrameStats>().frames.get(), 1);
    /// });
    /// ```
    #[inline]
    pub fn with_non_send<T: Send, F>(&self, f: F) -> T
    where
        F: FnOnce(&NonSendWorld) -> T + Send,
    {
        use core::mem::transmute;
        // SAFETY: `NonSendWorld` is `#[repr(transparent)]` over `World` — the
        // layout asserts above guarantee the reference cast is valid and
        // preserves the original lifetime and aliasing rules.
        zlim_task::invoke_on_main(|| unsafe { f(transmute::<&World, &NonSendWorld>(self)) })
    }

    /// Runs `f` on the main thread with mutable access to the world's `!Send` resources.
    ///
    /// The closure is sent to the main thread via [`zlim_task::invoke_on_main`],
    /// which blocks the calling thread until it finishes.
    ///
    /// If the current thread **is** the main thread, the closure runs
    /// synchronously with no extra hop.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// #[derive(TypePath, Resource)]
    /// struct FrameStats {
    ///     frames: core::cell::Cell<u32>,
    /// }
    ///
    /// let mut world = World::alloc();
    ///
    /// world.with_non_send_mut(|w| {
    ///     w.insert_non_send(FrameStats { frames: core::cell::Cell::new(0) });
    ///     w.non_send_mut::<FrameStats>().frames.set(60);
    /// });
    ///
    /// world.with_non_send(|w| {
    ///     assert_eq!(w.non_send::<FrameStats>().frames.get(), 60);
    /// });
    /// ```
    #[inline]
    pub fn with_non_send_mut<T: Send, F>(&mut self, f: F) -> T
    where
        F: FnOnce(&mut NonSendWorld) -> T + Send,
    {
        use core::mem::transmute;
        // SAFETY: `NonSendWorld` is `#[repr(transparent)]` over `World` — the
        // layout asserts above guarantee the reference cast is valid and
        // preserves the original lifetime and aliasing rules.
        zlim_task::invoke_on_main(|| unsafe { f(transmute::<&mut World, &mut NonSendWorld>(self)) })
    }
}
