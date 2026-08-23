//! World-access [`SystemParam`] implementations: `&World`, `&mut World`, and
//! `DeferredWorld`.

use crate::system::{AccessTable, SystemParam, SystemParamError};
use crate::tick::Tick;
use crate::world::{DeferredWorld, World, WorldCell};

// -----------------------------------------------------------------------------
// World

unsafe impl SystemParam for &World {
    type State = ();
    type Item<'world, 'state> = &'world World;

    const DEFERRED: bool = false;
    const NON_SEND: bool = false;
    const EXCLUSIVE: bool = false;

    #[inline(always)]
    fn init_state(_: &World) -> Self::State {}

    #[inline(always)]
    fn register_access(_: &Self::State, table: &mut AccessTable, strict: bool) -> bool {
        table.register_world_ref(strict)
    }

    #[inline(always)]
    unsafe fn build_param<'w, 's>(
        _: &'s mut Self::State,
        world: WorldCell<'w>,
        _: Tick,
        _: Tick,
    ) -> Result<Self::Item<'w, 's>, SystemParamError> {
        unsafe { Ok(world.read_only()) }
    }

    #[inline(always)]
    fn queue_deferred(_: &mut Self::State, _: DeferredWorld) {}

    #[inline(always)]
    fn apply_deferred(_: &mut Self::State, _: &mut World) {}
}

unsafe impl SystemParam for &mut World {
    type State = ();
    type Item<'world, 'state> = &'world mut World;

    const DEFERRED: bool = false;
    const NON_SEND: bool = true;
    const EXCLUSIVE: bool = true;

    #[inline(always)]
    fn init_state(_: &World) -> Self::State {}

    #[inline(always)]
    fn register_access(_: &Self::State, table: &mut AccessTable, strict: bool) -> bool {
        table.register_world_mut(strict)
    }

    #[inline(always)]
    unsafe fn build_param<'w, 's>(
        _: &'s mut Self::State,
        world: WorldCell<'w>,
        _: Tick,
        _: Tick,
    ) -> Result<Self::Item<'w, 's>, SystemParamError> {
        unsafe { Ok(world.full_mut()) }
    }

    #[inline(always)]
    fn queue_deferred(_: &mut Self::State, _: DeferredWorld) {}

    #[inline(always)]
    fn apply_deferred(_: &mut Self::State, _: &mut World) {}
}

unsafe impl SystemParam for DeferredWorld<'_> {
    type State = ();
    type Item<'world, 'state> = DeferredWorld<'world>;

    const DEFERRED: bool = true;
    const NON_SEND: bool = false; // <-- should be false.
    const EXCLUSIVE: bool = true;

    #[inline(always)]
    fn init_state(_: &World) -> Self::State {}

    #[inline(always)]
    fn register_access(_: &Self::State, table: &mut AccessTable, strict: bool) -> bool {
        table.register_world_mut(strict)
    }

    #[inline(always)]
    unsafe fn build_param<'w, 's>(
        _: &'s mut Self::State,
        world: WorldCell<'w>,
        _: Tick,
        _: Tick,
    ) -> Result<Self::Item<'w, 's>, SystemParamError> {
        unsafe { Ok(world.deferred()) }
    }

    #[inline(always)]
    fn queue_deferred(_: &mut Self::State, _: DeferredWorld) {}

    #[inline(never)] // `flush` is inlined, so we `inline(never)` to avoid bloating.
    fn apply_deferred(_: &mut Self::State, world: &mut World) {
        world.flush();
    }
}
