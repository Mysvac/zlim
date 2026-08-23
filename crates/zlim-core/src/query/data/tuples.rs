use super::{QueryData, QuerySlice, ReadOnlyQueryData};
use crate::entity::EntityId;
use crate::system::{ComponentAccess, FilterParamBuilder};
use crate::table::{Table, TableRow};
use crate::tick::Tick;
use crate::world::{World, WorldCell};

macro_rules! impl_tuple {
    (0: []) => {
        unsafe impl ReadOnlyQueryData for () {}

        unsafe impl QuerySlice for () {
            type SliceItem<'world> = ();

            type ReadOnlySlice = ();

            #[inline(always)]
            unsafe fn fetch_slice<'w>(
                _state: &Self::State,
                _cache: &mut Self::Cache<'w>,
                _entities: &'w [EntityId],
            ) -> Option<Self::SliceItem<'w>> {
                Some(())
            }
        }

        unsafe impl QueryData for () {
            type ReadOnly = ();
            type State = ();
            type Cache<'world> = ();
            type Item<'world> = ();

            #[inline(always)]
            fn build_state(_world: &World) -> Self::State {}

            #[inline(always)]
            unsafe fn build_cache<'w>(
                _state: &Self::State,
                _world: WorldCell<'w>,
                _last_run: Tick,
                _this_run: Tick,
            ) -> Self::Cache<'w> {}

            #[inline(always)]
            fn register_filter(_state: &Self::State, _out: &mut Vec<FilterParamBuilder>) {}

            #[inline(always)]
            fn register_access(_state: &Self::State, _out: &mut ComponentAccess) -> bool { true }

            #[inline(always)]
            unsafe fn update_table<'w>(
                _state: &Self::State,
                _cache: &mut Self::Cache<'w>,
                _table: &'w mut Table,
            ) {}

            #[inline(always)]
            unsafe fn fetch<'w>(
                _state: &Self::State,
                _cache: &mut Self::Cache<'w>,
                _entity: EntityId,
                _table_row: TableRow,
            ) -> Option<Self::Item<'w>> {
                Some(())
            }
        }
    };
    (1 : [ $index:tt : $name:ident ]) => {
        #[cfg_attr(docsrs, doc(fake_variadic))]
        #[cfg_attr(docsrs, doc = "This trait is implemented for tuples up to 12 items long.")]
        unsafe impl<$name: ReadOnlyQueryData> ReadOnlyQueryData for ($name,) {}

        #[cfg_attr(docsrs, doc(fake_variadic))]
        #[cfg_attr(docsrs, doc = "This trait is implemented for tuples up to 12 items long.")]
        unsafe impl<$name: QuerySlice> QuerySlice for ($name,) {
            type SliceItem<'world> = (<$name>::SliceItem<'world>,);

            type ReadOnlySlice = (<$name as QuerySlice>::ReadOnlySlice,);

            unsafe fn fetch_slice<'w>(
                state: &Self::State,
                cache: &mut Self::Cache<'w>,
                entities: &'w [EntityId],
            ) -> Option<Self::SliceItem<'w>> {
                unsafe { Some(( <$name>::fetch_slice(state, cache, entities)?, )) }
            }
        }

        #[cfg_attr(docsrs, doc(fake_variadic))]
        #[cfg_attr(docsrs, doc = "This trait is implemented for tuples up to 12 items long.")]
        unsafe impl<$name: QueryData> QueryData for ($name,) {
            type ReadOnly = (<$name as QueryData>::ReadOnly,);
            type State = <$name>::State;
            type Cache<'world> = <$name>::Cache<'world>;
            type Item<'world> = ( <$name>::Item<'world>, );

            fn build_state(world: &World) -> Self::State {
                <$name>::build_state(world)
            }

            unsafe fn build_cache<'w>(
                state: &Self::State,
                world: WorldCell<'w>,
                last_run: Tick,
                this_run: Tick,
            ) -> Self::Cache<'w> {
                unsafe { <$name>::build_cache(state, world, last_run, this_run) }
            }

            fn register_filter(state: &Self::State, out: &mut Vec<FilterParamBuilder>) {
                <$name>::register_filter(state, out);
            }

            fn register_access(state: &Self::State, out: &mut ComponentAccess) -> bool {
                <$name>::register_access(state, out)
            }

            unsafe fn update_table<'w>(
                state: &Self::State,
                cache: &mut Self::Cache<'w>,
                table: &'w mut Table,
            ) {
                unsafe { <$name>::update_table(state, cache, table); }
            }

            unsafe fn fetch<'w>(
                state: &Self::State,
                cache: &mut Self::Cache<'w>,
                entity: EntityId,
                table_row: TableRow,
            ) -> Option<Self::Item<'w>> {
                unsafe { Some(( <$name>::fetch(state, cache, entity, table_row)?, )) }
            }
        }
    };
    ($num:literal : [$($index:tt : $name:ident),*]) => {
        #[cfg_attr(docsrs, doc(hidden))]
        unsafe impl<$($name: ReadOnlyQueryData),*> ReadOnlyQueryData for ($($name),*) {}

        #[cfg_attr(docsrs, doc(hidden))]
        unsafe impl<$($name: QuerySlice),*> QuerySlice for ($($name),*) {
            type SliceItem<'world> = ( $( <$name>::SliceItem<'world> ),* );

            type ReadOnlySlice = ( $(<$name as QuerySlice>::ReadOnlySlice),* );

            unsafe fn fetch_slice<'w>(
                state: &Self::State,
                cache: &mut Self::Cache<'w>,
                entities: &'w [EntityId],
            ) -> Option<Self::SliceItem<'w>> {
                unsafe {
                    Some(( $( <$name>::fetch_slice(&state.$index, &mut cache.$index, entities)?, )* ))
                }
            }
        }

        #[cfg_attr(docsrs, doc(hidden))]
        unsafe impl<$($name: QueryData),*> QueryData for ($($name),*) {
            type ReadOnly = ( $(<$name as QueryData>::ReadOnly),* );
            type State = ( $( <$name>::State ),* );
            type Cache<'world> = ( $( <$name>::Cache<'world> ),* );
            type Item<'world> = ( $( <$name>::Item<'world> ),* );

            fn build_state(world: &World) -> Self::State {
                ( $( <$name>::build_state(world), )* )
            }

            unsafe fn build_cache<'w>(
                state: &Self::State,
                world: WorldCell<'w>,
                last_run: Tick,
                this_run: Tick,
            ) -> Self::Cache<'w> {
                unsafe {
                    ( $( <$name>::build_cache(&state.$index, world, last_run, this_run), )* )
                }
            }

            fn register_filter(state: &Self::State, out: &mut Vec<FilterParamBuilder>) {
                $( <$name>::register_filter(&state.$index, out); )*
            }

            fn register_access(state: &Self::State, out: &mut ComponentAccess) -> bool {
                let mut all_ok = true;

                $(
                    all_ok &= <$name>::register_access(&state.$index, out);
                    // After a conflict occurs, relax to non-strict to avoid
                    // repeating error logs.
                )*

                all_ok
            }

            unsafe fn update_table<'w>(
                state: &Self::State,
                cache: &mut Self::Cache<'w>,
                table: &'w mut Table,
            ) {
                unsafe {
                    let ptr = table as *mut Table;
                    $( <$name>::update_table(&state.$index, &mut cache.$index, &mut *ptr); )*
                }
            }

            unsafe fn fetch<'w>(
                state: &Self::State,
                cache: &mut Self::Cache<'w>,
                entity: EntityId,
                table_row: TableRow,
            ) -> Option<Self::Item<'w>> {
                unsafe {
                    Some(( $( <$name>::fetch(&state.$index, &mut cache.$index, entity, table_row)?, )* ))
                }
            }
        }
    };
}

zlim_utils::range_invoke!(impl_tuple, 12);
