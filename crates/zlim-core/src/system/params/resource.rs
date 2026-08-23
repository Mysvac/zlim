//! Resource [`SystemParam`] implementations: `Res`, `ResMut`, `NonSend`,
//! `NonSendMut`, and their `Option` variants.
//!
//! The parameter types themselves are defined in [`crate::borrow`]; this
//! module only provides their [`SystemParam`] implementations.

use crate::borrow::{NonSend, NonSendMut, Res, ResMut};
use crate::resource::{Resource, ResourceId};
use crate::system::{AccessTable, SystemParam, SystemParamError};
use crate::tick::Tick;
use crate::world::{DeferredWorld, World, WorldCell};

#[cold]
#[inline(never)]
fn uninit_resource_error<P>() -> SystemParamError {
    SystemParamError::new::<P>("Try to fetch a uninitialized resource")
}

// -----------------------------------------------------------------------------
// Res

unsafe impl<T: Resource + Sync> SystemParam for Res<'_, T> {
    type State = ResourceId;
    type Item<'world, 'state> = Res<'world, T>;

    const DEFERRED: bool = false;
    const NON_SEND: bool = false;
    const EXCLUSIVE: bool = false;

    #[inline(never)]
    fn init_state(world: &World) -> Self::State {
        world.resources.get::<T>().id
    }

    #[inline(never)]
    fn register_access(state: &Self::State, table: &mut AccessTable, strict: bool) -> bool {
        table.register_reading_res(*state, strict)
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
            if let Some(data) = world.slots.get(*state)
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
    type State = ResourceId;
    type Item<'world, 'state> = ResMut<'world, T>;

    const DEFERRED: bool = false;
    const NON_SEND: bool = false;
    const EXCLUSIVE: bool = false;

    #[inline(never)]
    fn init_state(world: &World) -> Self::State {
        world.resources.get::<T>().id
    }

    #[inline(never)]
    fn register_access(state: &Self::State, table: &mut AccessTable, strict: bool) -> bool {
        table.register_writing_res(*state, strict)
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
            if let Some(data) = world.slots.get_mut(*state)
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
    type State = ResourceId;
    type Item<'world, 'state> = NonSend<'world, T>;

    const DEFERRED: bool = false;
    const NON_SEND: bool = true; // <--
    const EXCLUSIVE: bool = false;

    #[inline(never)]
    fn init_state(world: &World) -> Self::State {
        world.resources.get::<T>().id
    }

    #[inline(never)]
    fn register_access(state: &Self::State, table: &mut AccessTable, strict: bool) -> bool {
        table.register_reading_res(*state, strict)
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
            if let Some(data) = world.slots.get(*state)
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
    type State = ResourceId;
    type Item<'world, 'state> = NonSendMut<'world, T>;

    const DEFERRED: bool = false;
    const NON_SEND: bool = true; // <--
    const EXCLUSIVE: bool = false;

    #[inline(never)]
    fn init_state(world: &World) -> Self::State {
        world.resources.get::<T>().id
    }

    #[inline(never)]
    fn register_access(state: &Self::State, table: &mut AccessTable, strict: bool) -> bool {
        table.register_writing_res(*state, strict)
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
            if let Some(data) = world.slots.get_mut(*state)
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
// Option<Res>

unsafe impl<T: Resource + Sync> SystemParam for Option<Res<'_, T>> {
    type State = ResourceId;
    type Item<'world, 'state> = Option<Res<'world, T>>;

    const DEFERRED: bool = false;
    const NON_SEND: bool = false;
    const EXCLUSIVE: bool = false;

    #[inline(never)]
    fn init_state(world: &World) -> Self::State {
        world.resources.get::<T>().id
    }

    #[inline(never)]
    fn register_access(state: &Self::State, table: &mut AccessTable, strict: bool) -> bool {
        table.register_reading_res(*state, strict)
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
            if let Some(data) = world.slots.get(*state)
                && let Some(untyped) = data.get_ref(last_run, this_run)
            {
                Ok(Some(untyped.into_resource::<T>()))
            } else {
                Ok(None)
            }
        }
    }

    #[inline(always)]
    fn queue_deferred(_: &mut Self::State, _: DeferredWorld) {}

    #[inline(always)]
    fn apply_deferred(_: &mut Self::State, _: &mut World) {}
}

// -----------------------------------------------------------------------------
// Option<ResMut>

unsafe impl<T: Resource + Send> SystemParam for Option<ResMut<'_, T>> {
    type State = ResourceId;
    type Item<'world, 'state> = Option<ResMut<'world, T>>;

    const DEFERRED: bool = false;
    const NON_SEND: bool = false;
    const EXCLUSIVE: bool = false;

    #[inline(never)]
    fn init_state(world: &World) -> Self::State {
        world.resources.get::<T>().id
    }

    #[inline(never)]
    fn register_access(state: &Self::State, table: &mut AccessTable, strict: bool) -> bool {
        table.register_writing_res(*state, strict)
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
            if let Some(data) = world.slots.get_mut(*state)
                && let Some(untyped) = data.get_mut(last_run, this_run)
            {
                Ok(Some(untyped.into_resource::<T>()))
            } else {
                Ok(None)
            }
        }
    }

    #[inline(always)]
    fn queue_deferred(_: &mut Self::State, _: DeferredWorld) {}

    #[inline(always)]
    fn apply_deferred(_: &mut Self::State, _: &mut World) {}
}

// -----------------------------------------------------------------------------
// Option<NonSend>

unsafe impl<T: Resource> SystemParam for Option<NonSend<'_, T>> {
    type State = ResourceId;
    type Item<'world, 'state> = Option<NonSend<'world, T>>;

    const DEFERRED: bool = false;
    const NON_SEND: bool = true; // <--
    const EXCLUSIVE: bool = false;

    #[inline(never)]
    fn init_state(world: &World) -> Self::State {
        world.resources.get::<T>().id
    }

    #[inline(never)]
    fn register_access(state: &Self::State, table: &mut AccessTable, strict: bool) -> bool {
        table.register_reading_res(*state, strict)
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
            if let Some(data) = world.slots.get(*state)
                && let Some(untyped) = data.get_ref(last_run, this_run)
            {
                Ok(Some(untyped.into_non_send::<T>()))
            } else {
                Ok(None)
            }
        }
    }

    #[inline(always)]
    fn queue_deferred(_: &mut Self::State, _: DeferredWorld) {}

    #[inline(always)]
    fn apply_deferred(_: &mut Self::State, _: &mut World) {}
}

// -----------------------------------------------------------------------------
// Option<NonSendMut>

unsafe impl<T: Resource> SystemParam for Option<NonSendMut<'_, T>> {
    type State = ResourceId;
    type Item<'world, 'state> = Option<NonSendMut<'world, T>>;

    const DEFERRED: bool = false;
    const NON_SEND: bool = true; // <--
    const EXCLUSIVE: bool = false;

    #[inline(never)]
    fn init_state(world: &World) -> Self::State {
        world.resources.get::<T>().id
    }

    #[inline(never)]
    fn register_access(state: &Self::State, table: &mut AccessTable, strict: bool) -> bool {
        table.register_writing_res(*state, strict)
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
            if let Some(data) = world.slots.get_mut(*state)
                && let Some(untyped) = data.get_mut(last_run, this_run)
            {
                Ok(Some(untyped.into_non_send::<T>()))
            } else {
                Ok(None)
            }
        }
    }

    #[inline(always)]
    fn queue_deferred(_: &mut Self::State, _: DeferredWorld) {}

    #[inline(always)]
    fn apply_deferred(_: &mut Self::State, _: &mut World) {}
}

// -----------------------------------------------------------------------------
