//! Tuple [`SystemParam`] implementations (up to 12 elements).

use crate::system::{AccessTable, SystemParam, SystemParamError};
use crate::tick::Tick;
use crate::world::{DeferredWorld, World, WorldCell};

macro_rules! impl_tuple {
    (0: []) => {
        unsafe impl SystemParam for () {
            type State = ();
            type Item<'world, 'state> = ();

            const DEFERRED: bool = false;
            const NON_SEND: bool = false;
            const EXCLUSIVE: bool = false;

            #[inline(always)]
            fn init_state(_: &World) -> Self::State {}

            #[inline(always)]
            fn register_access(_: &Self::State, _: &mut AccessTable, _: bool) -> bool { true }

            #[inline(always)]
            unsafe fn build_param<'w, 's>(
                _state: &'s mut Self::State,
                _world: WorldCell<'w>,
                _last_run: Tick,
                _this_run: Tick,
            ) -> Result<Self::Item<'w, 's>, SystemParamError> {
                Ok(())
            }

            #[inline(always)]
            fn queue_deferred(_: &mut Self::State, _: DeferredWorld) {}

            #[inline(always)]
            fn apply_deferred(_: &mut Self::State, _: &mut World) {}
        }
    };
    (1 : [ $index:tt : $name:ident ]) => {
        #[cfg_attr(docsrs, doc(fake_variadic))]
        #[cfg_attr(docsrs, doc = "This trait is implemented for tuples up to 12 items long.")]
        unsafe impl<$name: SystemParam> SystemParam for ($name,) {
            type State = <$name>::State;
            type Item<'world, 'state> = ( <$name>::Item<'world, 'state>, );

            const DEFERRED: bool = <$name>::DEFERRED;
            const NON_SEND: bool = <$name>::NON_SEND;
            const EXCLUSIVE: bool = <$name>::EXCLUSIVE;

            #[inline]
            fn init_state(world: &World) -> Self::State {
                <$name>::init_state(world)
            }

            #[inline]
            fn register_access(state: &Self::State, table: &mut AccessTable, strict: bool) -> bool {
                <$name>::register_access(state, table, strict)
            }

            #[inline]
            unsafe fn build_param<'w, 's>(
                state: &'s mut Self::State,
                world: WorldCell<'w>,
                last_run: Tick,
                this_run: Tick,
            ) -> Result<Self::Item<'w, 's>, SystemParamError> {
                unsafe { Ok(( <$name>::build_param(state, world, last_run, this_run)?, )) }
            }

            #[inline]
            fn queue_deferred(state: &mut Self::State, world: DeferredWorld) {
                <$name>::queue_deferred(state, world);
            }

            #[inline]
            fn apply_deferred(state: &mut Self::State, world: &mut World) {
                <$name>::apply_deferred(state, world);
            }
        }
    };
    ($num:literal : [$($index:tt : $name:ident),*]) => {
        #[cfg_attr(docsrs, doc(hidden))]
        unsafe impl<$($name: SystemParam),*> SystemParam for ($($name),*) {
            type State = ( $( <$name>::State ),* );
            type Item<'world, 'state> = ( $( <$name>::Item<'world, 'state> ),* );

            const DEFERRED: bool = { false $( || <$name>::DEFERRED )* };
            const NON_SEND: bool = { false $( || <$name>::NON_SEND )* };
            const EXCLUSIVE: bool = { false $( || <$name>::EXCLUSIVE )* };

            fn init_state(world: &World) -> Self::State {
                ( $( <$name>::init_state(world) ),* )
            }

            fn register_access(
                state: &Self::State,
                table: &mut AccessTable,
                mut strict: bool,
            ) -> bool {
                let mut all_ok = true;

                $(
                    all_ok &= <$name>::register_access(&state.$index, table, strict);
                    // After a conflict occurs, relax to non-strict to avoid
                    // repeating error logs.
                    strict &= all_ok;
                )*

                all_ok
            }

            unsafe fn build_param<'w, 's>(
                state: &'s mut Self::State,
                world: WorldCell<'w>,
                last_run: Tick,
                this_run: Tick,
            ) -> Result<Self::Item<'w, 's>, SystemParamError> {
                unsafe { Ok(( $( <$name>::build_param(&mut state.$index, world, last_run, this_run)? ),* )) }
            }

            fn queue_deferred(state: &mut Self::State, mut world: DeferredWorld) {
                if <Self as SystemParam>::DEFERRED {
                    $( <$name>::queue_deferred(&mut state.$index, world.reborrow()); )*
                }
            }

            fn apply_deferred(state: &mut Self::State, world: &mut World) {
                if <Self as SystemParam>::DEFERRED {
                    $( <$name>::apply_deferred(&mut state.$index, world); )*
                }
            }
        }
    };
}

zlim_utils::range_invoke!(impl_tuple, 12);
