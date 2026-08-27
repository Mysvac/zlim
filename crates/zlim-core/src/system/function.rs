//! Function-based systems: converting closures and functions into [`System`]s.

use super::IntoSystem;
use super::{AccessTable, SystemFlags, SystemMeta};
use super::{System, SystemId, SystemInput, SystemParam};
use crate::system::SystemError;
use crate::tick::Tick;
use crate::world::WorldCell;
use crate::world::{DeferredWorld, World, WorldId};

type SystemInputData<'a, P> = <P as SystemInput>::Data<'a>;
type SystemParamItem<'w, 's, P> = <P as SystemParam>::Item<'w, 's>;

/// A function-like value that can drive a [`System`]'s execution.
///
/// `Marker` disambiguates implementations for different arities and the
/// presence of an input. Each `SystemFunction` pairs a parameter bundle
/// ([`SystemParam`]) with a system input and output type.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a valid system function",
    label = "invalid system function"
)]
pub trait SystemFunction<Marker>: Send + Sync + 'static {
    /// The bundle of system parameters the function accepts.
    type Param: SystemParam;

    /// The system's input type.
    type Input: SystemInput;

    /// The value the function produces.
    type Output;

    /// Invokes the function with the resolved input and parameter values.
    fn run(
        this: &mut Self,
        input: SystemInputData<Self::Input>,
        param: SystemParamItem<Self::Param>,
    ) -> Self::Output;
}

macro_rules! impl_tuple {
    (0: []) => {
        impl<O, Func> SystemFunction<fn() -> O> for Func
        where
            O: 'static,
            Func: Send + Sync + 'static,
            for<'a> &'a mut Func: FnMut() -> O
        {
            type Param = ();
            type Input = ();
            type Output = O;

            #[inline]
            fn run(
                this: &mut Self,
                _input: (),
                _param: (),
            ) -> Self::Output {
                #[inline(always)]
                fn call<O>(mut func: impl FnMut() -> O) -> O {
                    func()
                }

                call(this)
            }
        }

        impl<I, O, Func> SystemFunction<(I, fn() -> O)> for Func
        where
            O: 'static,
            I: SystemInput + 'static,
            Func: Send + Sync + 'static,
            for<'a> &'a mut Func:
                FnMut(I) -> O +
                FnMut(I::Item<'_>) -> O +
        {
            type Param = ();
            type Input = I;
            type Output = O;

            #[inline]
            fn run(
                this: &mut Self,
                input: I::Data<'_>,
                _param: (),
            ) -> Self::Output {
                #[inline(always)]
                fn call<I, O>(
                    mut func: impl FnMut(I) -> O,
                    input: I,
                ) -> O {
                    func(input)
                }

                call(this, I::wrap(input))
            }
        }
    };
    (1 : [ $index:tt : $name:ident ]) => {
        #[cfg_attr(docsrs, doc(fake_variadic))]
        #[cfg_attr(docsrs, doc = "This trait is implemented for tuples up to 12 items long.")]
        impl<O, $name, Func> SystemFunction<fn($name) -> O> for Func
        where
            O: 'static,
            $name: SystemParam + 'static,
            Func: Send + Sync + 'static,
            for<'a> &'a mut Func:
                FnMut($name) -> O +
                FnMut(<$name>::Item<'_, '_>) -> O
        {
            type Param = ( $name, );
            type Input = ();
            type Output = O;

            #[inline]
            fn run(
                this: &mut Self,
                _input: (),
                param: ( <$name>::Item<'_,'_> ,),
            ) -> Self::Output {
                #[inline(always)]
                fn call<O, $name>(
                    mut func: impl FnMut($name) -> O,
                    param: ( $name , ),
                ) -> O {
                    func(param.0)
                }

                call(this, param)
            }
        }

        #[cfg_attr(docsrs, doc(fake_variadic))]
        #[cfg_attr(docsrs, doc = "This trait is implemented for tuples up to 12 items long.")]
        impl<I, O, $name, Func> SystemFunction<(I, fn($name) -> O)> for Func
        where
            O: 'static,
            I: SystemInput + 'static,
            $name: SystemParam + 'static,
            Func: Send + Sync + 'static,
            for<'a> &'a mut Func:
                FnMut(I, $name) -> O +
                FnMut(I::Item<'_>, <$name>::Item<'_, '_>) -> O
        {
            type Param = ( $name, );
            type Input = I;
            type Output = O;

            #[inline]
            fn run(
                this: &mut Self,
                input: I::Data<'_>,
                param: ( <$name>::Item<'_,'_> ,),
            ) -> Self::Output {
                #[inline(always)]
                fn call<I, O, $name>(
                    mut func: impl FnMut(I, $name) -> O,
                    input: I,
                    param: ( $name , ),
                ) -> O {
                    func(input, param.0)
                }

                call(this, I::wrap(input), param)
            }
        }
    };
    ($num:literal : [$($index:tt : $name:ident),*]) => {
        #[cfg_attr(docsrs, doc(hidden))]
        impl<O, $($name,)* Func> SystemFunction<fn($($name),*) -> O> for Func
        where
            O: 'static,
            $($name: SystemParam + 'static,)*
            Func: Send + Sync + 'static,
            for<'a> &'a mut Func:
                FnMut($($name),*) -> O +
                FnMut($(<$name>::Item<'_, '_>),*) -> O
        {
            type Param = ( $($name),* );
            type Input = ();
            type Output = O;

            #[inline]
            fn run(
                this: &mut Self,
                _input: (),
                param: ( $(<$name>::Item<'_,'_>, )* ),
            ) -> Self::Output {
                #[inline(always)]
                fn call<O, $($name),*>(
                    mut func: impl FnMut($($name),*) -> O,
                    param: ( $($name),* ),
                ) -> O {
                    func($(param.$index),*)
                }

                call(this, param)
            }
        }

        #[cfg_attr(docsrs, doc(hidden))]
        impl<I, O, $($name,)* Func> SystemFunction<(I, fn($($name),*) -> O)> for Func
        where
            O: 'static,
            I: SystemInput + 'static,
            $($name: SystemParam + 'static,)*
            Func: Send + Sync + 'static,
            for<'a> &'a mut Func:
                FnMut(I, $($name),*) -> O +
                FnMut(I::Item<'_>, $(<$name>::Item<'_, '_>),*) -> O
        {
            type Param = ( $($name),* );
            type Input = I;
            type Output = O;

            #[inline]
            fn run(
                this: &mut Self,
                input: I::Data<'_>,
                param: ( $(<$name>::Item<'_,'_>, )* ),
            ) -> Self::Output {
                #[inline(always)]
                fn call<I, O, $($name),*>(
                    mut func: impl FnMut(I, $($name),*) -> O,
                    input: I,
                    param: ( $($name),* ),
                ) -> O {
                    func(input, $(param.$index),*)
                }

                call(this, I::wrap(input), param)
            }
        }
    }
}

zlim_utils::range_invoke!(impl_tuple, 12);

// -----------------------------------------------------------------------------
// FunctionSystem

struct FunctionState<P: SystemParam> {
    param: P::State,
    #[cfg(any(debug_assertions, feature = "debug"))]
    world_id: WorldId,
}

/// A [`System`] wrapper around a function-like value.
///
/// `M` is the function marker selecting the `SystemFunction` implementation;
/// `F` is the concrete function/closure type.
///
/// Users rarely name this type directly: calling [`IntoSystem::into_system`]
/// on a function or closure produces a `FunctionSystem`, and the scheduler
/// drives it through the [`System`] trait.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// fn hello() {
///     println!("Hello, world!");
/// }
///
/// let mut world = World::alloc();
///
/// // `IntoSystem::into_system` wraps the function in a `FunctionSystem`,
/// // and `World::invoke_once` builds, initializes, and runs a fresh instance.
/// let result = world.invoke_once(hello, ());
/// assert!(result.is_ok());
/// ```
pub struct FunctionSystem<M, F: SystemFunction<M>> {
    func: F,
    meta: SystemMeta,
    state: Option<FunctionState<F::Param>>,
}

impl<M, F: SystemFunction<M>> FunctionSystem<M, F> {
    /// Builds a runtime system wrapper from a function-like implementation.
    ///
    /// The wrapper derives scheduling flags (`DEFERRED`, `EXCLUSIVE`,
    /// `NON_SEND`) from the parameter type and stores per-system runtime state
    /// after initialization.
    pub fn new(func: F) -> Self {
        let mut meta = SystemMeta::new::<F>();

        if <F::Param as SystemParam>::DEFERRED {
            meta.set_deferred();
        }
        if <F::Param as SystemParam>::NON_SEND {
            meta.set_non_send();
        }
        if <F::Param as SystemParam>::EXCLUSIVE {
            meta.set_exclusive();
        }

        Self {
            func,
            meta,
            state: None,
        }
    }
}

impl<M: 'static, F: SystemFunction<M> + 'static> System for FunctionSystem<M, F> {
    type Input = F::Input;
    type Output = F::Output;

    #[inline]
    fn id(&self) -> SystemId {
        self.meta.id
    }

    #[inline]
    fn flags(&self) -> SystemFlags {
        self.meta.flags
    }

    #[inline]
    fn last_run(&self) -> Tick {
        self.meta.last_run
    }

    #[inline]
    fn set_last_run(&mut self, last_run: Tick) {
        self.meta.last_run = last_run;
    }

    #[inline]
    fn clamp_ticks(&mut self, now: Tick) {
        self.meta.last_run.clamp_with(now);
    }

    fn initialize(&mut self, world: &World) {
        if self.state.is_none() {
            self.state = Some(FunctionState {
                param: <F::Param as SystemParam>::init_state(world),
                #[cfg(any(debug_assertions, feature = "debug"))]
                world_id: world.id,
            });
            self.meta.last_run = world.last_run;
        }
    }

    fn register_access(&self, table: &mut AccessTable, strict: bool) {
        let state = &self
            .state
            .as_ref()
            .unwrap_or_else(|| uninitialized_system(self.meta.id))
            .param;
        <F::Param as SystemParam>::register_access(state, table, strict);
    }

    unsafe fn run_raw(
        &mut self,
        input: <Self::Input as SystemInput>::Data<'_>,
        world: WorldCell<'_>,
    ) -> Result<Self::Output, SystemError> {
        #[cfg(feature = "trace")]
        let _span_guard = self.meta.span.enter();

        let Some(state) = &mut self.state else {
            core::hint::cold_path();
            return Err(SystemError::Uninitialized(self.meta.id));
        };

        let last_run = self.meta.last_run();
        let this_run = unsafe { world.read_only().advance_tick() };

        #[cfg(any(debug_assertions, feature = "debug"))]
        if state.world_id != unsafe { world.read_only().id } {
            let run = unsafe { world.read_only().id };
            mismatched_world(self.meta.id, state.world_id, run);
        }

        let param = unsafe {
            match <F::Param as SystemParam>::build_param(
                &mut state.param,
                world,
                last_run,
                this_run,
            ) {
                Ok(p) => p,
                Err(e) => {
                    core::hint::cold_path();
                    let debug_name = self.meta.id.debug_name();
                    return Err(SystemError::Param(e.with_system(debug_name)));
                }
            }
        };

        let output = <F as SystemFunction<M>>::run(&mut self.func, input, param);

        #[cfg(feature = "trace")]
        ::core::mem::drop(_span_guard);

        self.meta.set_last_run(this_run);

        Ok(output)
    }

    fn queue_deferred(&mut self, world: DeferredWorld) {
        if <F::Param as SystemParam>::DEFERRED {
            let opt_state = &mut self.state;
            if let Some(state) = opt_state {
                #[cfg(any(debug_assertions, feature = "debug"))]
                if state.world_id != world.id {
                    mismatched_world(self.meta.id, state.world_id, world.id);
                }

                <F::Param as SystemParam>::queue_deferred(&mut state.param, world);
            }
        }
    }

    fn apply_deferred(&mut self, world: &mut World) {
        if <F::Param as SystemParam>::DEFERRED {
            let opt_state = &mut self.state;
            if let Some(state) = opt_state {
                #[cfg(any(debug_assertions, feature = "debug"))]
                if state.world_id != world.id {
                    mismatched_world(self.meta.id, state.world_id, world.id);
                }

                <F::Param as SystemParam>::apply_deferred(&mut state.param, world);
            }
        }
    }
}

#[cold]
#[inline(never)]
fn uninitialized_system(system_id: SystemId) -> ! {
    panic!("Run `System::register_access` for a uninitialized system {system_id}.")
}

#[cold]
#[inline(never)]
#[cfg(any(debug_assertions, feature = "debug"))]
fn mismatched_world(id: SystemId, init: WorldId, run: WorldId) -> ! {
    panic!("System {id} is initialized in world {init}, but runs in world {run}.")
}

// -----------------------------------------------------------------------------
// FunctionSystemMarker

/// Marker type distinguishing the `FunctionSystem` [`IntoSystem`] implementation.
pub struct FunctionSystemMarker;

impl<M: 'static, F: SystemFunction<M>> IntoSystem<F::Input, F::Output, (FunctionSystemMarker, M)>
    for F
{
    type System = FunctionSystem<M, F>;

    #[inline]
    fn into_system(this: Self) -> Self::System {
        FunctionSystem::new(this)
    }

    #[inline]
    fn system_id(&self) -> SystemId {
        SystemId::of::<F>()
    }
}

// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::IntoSystem;
    use super::System;

    #[test]
    fn system_id() {
        fn func() {}

        let id = IntoSystem::system_id(&func);
        let s = IntoSystem::into_system(func);
        assert_eq!(s.id(), id);
    }
}
