//! Resource [`SystemParam`] implementations: `Res`, `ResMut`, `NonSend`,
//! `NonSendMut`, and their `Option` variants.
//!
//! The parameter types themselves are defined in [`crate::borrow`]; this
//! module only provides their [`SystemParam`] implementations.
//!
//! Each parameter's [`State`](SystemParam::State) is a [`ResourceHandle`] —
//! the resource's prepared storage cell when it already exists at
//! initialization time, or its [`TypeId`] as a fallback otherwise.

use core::any::TypeId;
use core::cell::UnsafeCell;

use crate::borrow::{NonSend, NonSendMut, Res, ResMut};
use crate::resource::{Resource, ResourceCell, ResourceDB, ResourceId};
use crate::system::{AccessTable, SystemParam, SystemParamError};
use crate::tick::Tick;
use crate::world::{DeferredWorld, World, WorldCell};

#[cold]
#[inline(never)]
fn uninit_resource_error<P>() -> SystemParamError {
    SystemParamError::new::<P>("Try to fetch a uninitialized resource")
}

/// A handle to a resource's storage cell.
///
/// - [`Cell`](Self::Cell) — fast path: the cell was already prepared when
///   the system was initialized, so fetches dereference it directly.
///
/// - [`TypeId`](Self::TypeId) — fallback: the cell was not prepared yet
///   (e.g. the resource is inserted after the system is registered); it is
///   looked up in the world's [`Resources`] storage on every fetch.
#[derive(Clone, Copy)]
pub enum ResourceHandle {
    /// The prepared, `'static` storage cell of the resource.
    Cell(&'static UnsafeCell<ResourceCell>),
    /// The [`TypeId`] of the resource, resolved at fetch time.
    TypeId(TypeId),
}

unsafe impl Sync for ResourceHandle {}
unsafe impl Send for ResourceHandle {}

impl ResourceHandle {
    /// Initializes the handle for `T` from a world.
    #[inline(always)]
    fn prepare<T: Resource>(world: &World) -> Self {
        match world.resources.get_cell(TypeId::of::<T>()) {
            Some(cell) => Self::Cell(cell),
            None => Self::TypeId(TypeId::of::<T>()),
        }
    }

    /// Returns the resource's stable [`ResourceId`].
    #[inline(always)]
    fn id<T: Resource>(&self) -> ResourceId {
        match self {
            Self::Cell(cell) => unsafe { &*cell.get() }.database().id,
            Self::TypeId(_) => ResourceDB::of::<T>().id,
        }
    }

    /// Resolves a shared reference to the storage cell, if present.
    #[inline(always)]
    fn get<'w>(&mut self, world: &'w World) -> Option<&'w ResourceCell> {
        match self {
            Self::Cell(cell) => Some(unsafe { &*cell.get() }),
            Self::TypeId(ty) => {
                let cell = world.resources.get_cell(*ty)?;
                ::core::hint::cold_path();
                *self = Self::Cell(cell);
                Some(unsafe { &*cell.get() })
            }
        }
    }

    /// Resolves an exclusive reference to the storage cell, if present.
    #[inline(always)]
    fn get_mut<'w>(&mut self, world: &'w mut World) -> Option<&'w mut ResourceCell> {
        match self {
            Self::Cell(cell) => Some(unsafe { &mut *cell.get() }),
            Self::TypeId(ty) => {
                let cell = world.resources.get_cell(*ty)?;
                ::core::hint::cold_path();
                *self = Self::Cell(cell);
                Some(unsafe { &mut *cell.get() })
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Res

unsafe impl<T: Resource + Sync> SystemParam for Res<'_, T> {
    type State = ResourceHandle;
    type Item<'world, 'state> = Res<'world, T>;

    const DEFERRED: bool = false;
    const NON_SEND: bool = false;
    const EXCLUSIVE: bool = false;

    #[inline(never)]
    fn init_state(world: &World) -> Self::State {
        ResourceHandle::prepare::<T>(world)
    }

    #[inline(never)]
    fn register_access(state: &Self::State, table: &mut AccessTable, strict: bool) -> bool {
        table.register_reading_res(state.id::<T>(), strict)
    }

    #[inline(never)]
    unsafe fn build_param<'w, 's>(
        state: &'s mut Self::State,
        world: WorldCell<'w>,
        last_run: Tick,
        this_run: Tick,
    ) -> Result<Self::Item<'w, 's>, SystemParamError> {
        unsafe {
            let world = world.read_only();
            if let Some(data) = state.get(world)
                && let Some(untyped) = data.get_ref(last_run, this_run)
            {
                Ok(untyped.into_resource::<T>())
            } else {
                Err(uninit_resource_error::<Self>())
            }
        }
    }

    #[inline(always)]
    fn queue_deferred(_: &mut Self::State, _: DeferredWorld) {}

    #[inline(always)]
    fn apply_deferred(_: &mut Self::State, _: &mut World) {}
}

// -----------------------------------------------------------------------------
// ResMut

unsafe impl<T: Resource + Send> SystemParam for ResMut<'_, T> {
    type State = ResourceHandle;
    type Item<'world, 'state> = ResMut<'world, T>;

    const DEFERRED: bool = false;
    const NON_SEND: bool = false;
    const EXCLUSIVE: bool = false;

    #[inline(never)]
    fn init_state(world: &World) -> Self::State {
        ResourceHandle::prepare::<T>(world)
    }

    #[inline(never)]
    fn register_access(state: &Self::State, table: &mut AccessTable, strict: bool) -> bool {
        table.register_writing_res(state.id::<T>(), strict)
    }

    #[inline(never)]
    unsafe fn build_param<'w, 's>(
        state: &'s mut Self::State,
        world: WorldCell<'w>,
        last_run: Tick,
        this_run: Tick,
    ) -> Result<Self::Item<'w, 's>, SystemParamError> {
        unsafe {
            let world = world.data_mut();
            if let Some(data) = state.get_mut(world)
                && let Some(untyped) = data.get_mut(last_run, this_run)
            {
                Ok(untyped.into_resource::<T>())
            } else {
                Err(uninit_resource_error::<Self>())
            }
        }
    }

    #[inline(always)]
    fn queue_deferred(_: &mut Self::State, _: DeferredWorld) {}

    #[inline(always)]
    fn apply_deferred(_: &mut Self::State, _: &mut World) {}
}

// -----------------------------------------------------------------------------
// NonSend

unsafe impl<T: Resource> SystemParam for NonSend<'_, T> {
    type State = ResourceHandle;
    type Item<'world, 'state> = NonSend<'world, T>;

    const DEFERRED: bool = false;
    const NON_SEND: bool = true; // <--
    const EXCLUSIVE: bool = false;

    #[inline(never)]
    fn init_state(world: &World) -> Self::State {
        ResourceHandle::prepare::<T>(world)
    }

    #[inline(never)]
    fn register_access(state: &Self::State, table: &mut AccessTable, strict: bool) -> bool {
        table.register_reading_res(state.id::<T>(), strict)
    }

    #[inline(never)]
    unsafe fn build_param<'w, 's>(
        state: &'s mut Self::State,
        world: WorldCell<'w>,
        last_run: Tick,
        this_run: Tick,
    ) -> Result<Self::Item<'w, 's>, SystemParamError> {
        unsafe {
            let world = world.read_only();
            if let Some(data) = state.get(world)
                && let Some(untyped) = data.get_ref(last_run, this_run)
            {
                Ok(untyped.into_non_send::<T>())
            } else {
                Err(uninit_resource_error::<Self>())
            }
        }
    }

    #[inline(always)]
    fn queue_deferred(_: &mut Self::State, _: DeferredWorld) {}

    #[inline(always)]
    fn apply_deferred(_: &mut Self::State, _: &mut World) {}
}

// -----------------------------------------------------------------------------
// NonSendMut

unsafe impl<T: Resource> SystemParam for NonSendMut<'_, T> {
    type State = ResourceHandle;
    type Item<'world, 'state> = NonSendMut<'world, T>;

    const DEFERRED: bool = false;
    const NON_SEND: bool = true; // <--
    const EXCLUSIVE: bool = false;

    #[inline(never)]
    fn init_state(world: &World) -> Self::State {
        ResourceHandle::prepare::<T>(world)
    }

    #[inline(never)]
    fn register_access(state: &Self::State, table: &mut AccessTable, strict: bool) -> bool {
        table.register_writing_res(state.id::<T>(), strict)
    }

    #[inline(never)]
    unsafe fn build_param<'w, 's>(
        state: &'s mut Self::State,
        world: WorldCell<'w>,
        last_run: Tick,
        this_run: Tick,
    ) -> Result<Self::Item<'w, 's>, SystemParamError> {
        unsafe {
            let world = world.data_mut();
            if let Some(data) = state.get_mut(world)
                && let Some(untyped) = data.get_mut(last_run, this_run)
            {
                Ok(untyped.into_non_send::<T>())
            } else {
                Err(uninit_resource_error::<Self>())
            }
        }
    }

    #[inline(always)]
    fn queue_deferred(_: &mut Self::State, _: DeferredWorld) {}

    #[inline(always)]
    fn apply_deferred(_: &mut Self::State, _: &mut World) {}
}

// -----------------------------------------------------------------------------
