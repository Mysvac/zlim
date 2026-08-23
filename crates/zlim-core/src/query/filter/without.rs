//! The `Without` query filter — excludes entities that have a component.

use super::{ArchetypeFilter, QueryFilter};
use crate::component::{Component, ComponentId};
use crate::entity::EntityId;
use crate::system::{AccessTable, ComponentAccess, FilterParamBuilder};
use crate::table::{Table, TableRow};
use crate::tick::Tick;
use crate::world::{World, WorldCell};

// -----------------------------------------------------------------------------
// InWithout

/// Marker trait for types that may appear inside [`Without`]: a [`Component`]
/// or a tuple of 1–12 components.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be used in `Without<..>`",
    label = "Expected a Component or a tuple of 1-12 Components",
    note = "If there are more than 12 elements, use `And<..>` instead."
)]
pub trait InWithout {}

// -----------------------------------------------------------------------------
// Without

/// Query filter that only matches entities **not** containing the given
/// component(s).
///
/// `Without<T>` requires the entity to lack component `T`; `Without<(A, B)>`
/// requires it to lack all of `A` and `B`.  The filter is evaluated entirely
/// at the table level, so it never performs per-entity checks and adds no
/// runtime cost during iteration.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_reflect::derive::TypePath;
///
/// #[derive(TypePath, Component, Clone)]
/// struct Position { x: f32, y: f32 }
///
/// #[derive(TypePath, Component, Clone)]
/// struct Static;
///
/// // Only entities without `Static` are visited.
/// fn move_dynamic(mut query: Query<&mut Position, Without<Static>>) {
///     for position in query.iter_mut() {
///         position.into_inner().x += 1.0;
///     }
/// }
///
/// let mut world = World::alloc();
/// world.spawn((Position { x: 1.0, y: 2.0 }, Static), None);
/// world.spawn(Position { x: 10.0, y: 0.0 }, None);
///
/// move_dynamic(world.query_mut::<&mut Position, Without<Static>>());
/// assert_eq!(world.query::<&Position, Without<Static>>().iter().next().unwrap().x, 11.0);
/// ```
pub struct Without<T: InWithout>(T);

// -----------------------------------------------------------------------------
// Without for Component

impl<T: Component> InWithout for T {}

unsafe impl<T: Component> QueryFilter for Without<T> {
    type State = ComponentId;
    type Cache<'world> = bool;

    const ENABLE_ENTITY_FILTER: bool = false;

    fn build_state(world: &World) -> Self::State {
        world.components.get::<T>().id
    }

    #[inline(always)]
    unsafe fn build_cache<'w>(
        _state: &Self::State,
        _world: WorldCell<'w>,
        _last_run: Tick,
        _this_run: Tick,
    ) -> Self::Cache<'w> {
        false
    }

    fn register_filter(state: &Self::State, out: &mut Vec<FilterParamBuilder>) {
        let mut builder = FilterParamBuilder::new();
        builder.without(*state);
        out.push(builder);
    }

    #[inline(always)]
    fn register_access(_state: &Self::State, _out: &mut ComponentAccess) {}

    #[inline(always)]
    fn modify_access_table(_: &Self::State, _: &mut AccessTable, _: bool) -> bool {
        true
    }

    unsafe fn update_table<'w>(state: &Self::State, cache: &mut Self::Cache<'w>, table: &'w Table) {
        *cache = !table.contains_component(*state);
    }

    #[inline(always)]
    unsafe fn filter<'w>(
        _state: &Self::State,
        cache: &mut Self::Cache<'w>,
        _entity: EntityId,
        _table_row: TableRow,
    ) -> bool {
        *cache
    }
}

unsafe impl<T: Component> ArchetypeFilter for Without<T> {}

// -----------------------------------------------------------------------------
// Without for Tuple

macro_rules! to_component_id {
    ($_:ident) => {
        ComponentId
    };
}

macro_rules! impl_tuple {
    (0: []) => {};
    (1 : [ $index:tt : $name:ident ]) => {
        #[cfg_attr(docsrs, doc(fake_variadic))]
        #[cfg_attr(docsrs, doc = "This trait is implemented for tuples up to 12 items long.")]
        impl<$name: Component> InWithout for ($name,) {}

        #[cfg_attr(docsrs, doc(fake_variadic))]
        #[cfg_attr(docsrs, doc = "This trait is implemented for tuples up to 12 items long.")]
        unsafe impl<$name: Component> ArchetypeFilter for Without<($name,)> {}

        #[cfg_attr(docsrs, doc(fake_variadic))]
        #[cfg_attr(docsrs, doc = "This trait is implemented for tuples up to 12 items long.")]
        unsafe impl<$name: Component> QueryFilter for Without<($name,)> {
            type State = ComponentId;
            type Cache<'world> = bool;

            const ENABLE_ENTITY_FILTER: bool = false;

            fn build_state(world: &World) -> Self::State {
                world.components.get::<$name>().id
            }

            #[inline(always)]
            unsafe fn build_cache<'w>(
                _state: &Self::State,
                _world: WorldCell<'w>,
                _last_run: Tick,
                _this_run: Tick,
            ) -> Self::Cache<'w> {
                false
            }

            fn register_filter(state: &Self::State, out: &mut Vec<FilterParamBuilder>) {
                let mut builder = FilterParamBuilder::new();
                builder.without(*state);
                out.push(builder);
            }

            #[inline(always)]
            fn register_access(_state: &Self::State, _out: &mut ComponentAccess) {}

            #[inline(always)]
            fn modify_access_table(_: &Self::State, _: &mut AccessTable, _: bool) -> bool {
                true
            }

            unsafe fn update_table<'w>(
                state: &Self::State,
                cache: &mut Self::Cache<'w>,
                table: &'w Table,
            ) {
                *cache = !table.contains_component(*state);
            }

            #[inline(always)]
            unsafe fn filter<'w>(
                _state: &Self::State,
                cache: &mut Self::Cache<'w>,
                _entity: EntityId,
                _table_row: TableRow,
            ) -> bool {
                *cache
            }
        }
    };
    ($num:literal : [$($index:tt : $name:ident),*]) => {
        #[cfg_attr(docsrs, doc(hidden))]
        impl<$($name: Component),*> InWithout for ($($name),*) {}

        #[cfg_attr(docsrs, doc(hidden))]
        unsafe impl<$($name: Component),*> ArchetypeFilter for Without<($($name),*)> {}

        #[cfg_attr(docsrs, doc(hidden))]
        unsafe impl<$($name: Component),*> QueryFilter for Without<($($name),*)> {
            type State = ( $( to_component_id!{ $name } ),* );
            type Cache<'world> = bool;

            const ENABLE_ENTITY_FILTER: bool = false;

            fn build_state(world: &World) -> Self::State {
                ( $( world.components.get::<$name>().id, )* )
            }

            #[inline(always)]
            unsafe fn build_cache<'w>(
                _state: &Self::State,
                _world: WorldCell<'w>,
                _last_run: Tick,
                _this_run: Tick,
            ) -> Self::Cache<'w> {
                false
            }

            fn register_filter(state: &Self::State, out: &mut Vec<FilterParamBuilder>) {
                let mut builder = FilterParamBuilder::new();
                $( builder.without(state.$index); )*
                out.push(builder);
            }

            #[inline(always)]
            fn register_access(_state: &Self::State, _out: &mut ComponentAccess) {}

            #[inline(always)]
            fn modify_access_table(_: &Self::State, _: &mut AccessTable, _: bool) -> bool {
                true
            }

            unsafe fn update_table<'w>(
                state: &Self::State,
                cache: &mut Self::Cache<'w>,
                table: &'w Table,
            ) {
                *cache = true $( && !table.contains_component(state.$index) )*;
            }

            #[inline(always)]
            unsafe fn filter<'w>(
                _state: &Self::State,
                cache: &mut Self::Cache<'w>,
                _entity: EntityId,
                _table_row: TableRow,
            ) -> bool {
                *cache
            }
        }
    };
}

zlim_utils::range_invoke!(impl_tuple, 12);

// -----------------------------------------------------------------------------
