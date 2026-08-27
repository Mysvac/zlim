use core::ops::{Deref, DerefMut};

use crate::error::Severity;
use crate::system::SystemParam;
use crate::system::{AccessTable, SystemParamError};
use crate::tick::Tick;
use crate::world::{DeferredWorld, World, WorldCell};

/// A wrapper for [`SystemParam`].
///
/// If the target `SystemParam` is successfully constructed
///  the system will be executed.
///
/// If construction fails, the [`SystemParamError`] is downgraded
/// to [`Severity::Ignore`], and the system is skipped.
///
/// By default, errors at the `Ignore` severity level are silently
/// discarded.
///
/// Unlike the [`Option`] container, which returns [`None`] upon
/// construction failure but still allows the system to proceed,
/// this wrapper prevents system execution entirely when the param
/// is unavailable.
///
/// ```ignore
/// fn system(logger: If<Res<Logger>>) {
///     // ......
/// }
/// ```
///
/// In this example, the system runs if the `Logger` resource exists,
/// and is skipped otherwise.
///
/// Note: The system will return `SystemError::SystemParamError` upon
/// skipping, but its severity is set to `Ignore`.
#[derive(Debug)]
pub struct If<T>(pub T);

impl<T> Deref for If<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for If<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

unsafe impl<T: SystemParam> SystemParam for If<T> {
    type State = T::State;
    type Item<'world, 'state> = If<T::Item<'world, 'state>>;

    const DEFERRED: bool = T::DEFERRED;
    const NON_SEND: bool = T::NON_SEND;
    const EXCLUSIVE: bool = T::EXCLUSIVE;

    fn init_state(world: &World) -> Self::State {
        T::init_state(world)
    }

    fn register_access(state: &Self::State, table: &mut AccessTable, strict: bool) -> bool {
        T::register_access(state, table, strict)
    }

    unsafe fn build_param<'w, 's>(
        state: &'s mut Self::State,
        world: WorldCell<'w>,
        last_run: Tick,
        this_run: Tick,
    ) -> Result<Self::Item<'w, 's>, SystemParamError> {
        unsafe {
            match T::build_param(state, world, last_run, this_run) {
                Ok(value) => Ok(If(value)),
                Err(e) => Err(e.with_severity(Severity::Ignore)),
            }
        }
    }

    fn queue_deferred(state: &mut Self::State, world: DeferredWorld) {
        T::queue_deferred(state, world);
    }

    fn apply_deferred(state: &mut Self::State, world: &mut World) {
        T::apply_deferred(state, world);
    }
}

unsafe impl<T: SystemParam> SystemParam for Option<T> {
    type State = T::State;
    type Item<'world, 'state> = Option<T::Item<'world, 'state>>;

    const DEFERRED: bool = T::DEFERRED;
    const NON_SEND: bool = T::NON_SEND;
    const EXCLUSIVE: bool = T::EXCLUSIVE;

    fn init_state(world: &World) -> Self::State {
        T::init_state(world)
    }

    fn register_access(state: &Self::State, table: &mut AccessTable, strict: bool) -> bool {
        T::register_access(state, table, strict)
    }

    unsafe fn build_param<'w, 's>(
        state: &'s mut Self::State,
        world: WorldCell<'w>,
        last_run: Tick,
        this_run: Tick,
    ) -> Result<Self::Item<'w, 's>, SystemParamError> {
        unsafe { Ok(T::build_param(state, world, last_run, this_run).ok()) }
    }

    fn queue_deferred(state: &mut Self::State, world: DeferredWorld) {
        T::queue_deferred(state, world);
    }

    fn apply_deferred(state: &mut Self::State, world: &mut World) {
        T::apply_deferred(state, world);
    }
}
