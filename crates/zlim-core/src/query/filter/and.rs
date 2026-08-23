//! The `And` query filter — logical conjunction of inner filters.

use super::{ArchetypeFilter, QueryFilter};
use crate::entity::EntityId;
use crate::system::{AccessTable, ComponentAccess, FilterParamBuilder};
use crate::table::{Table, TableRow};
use crate::tick::Tick;
use crate::world::{World, WorldCell};

// -----------------------------------------------------------------------------
// InAnd

/// Marker trait for tuples of 1–12 filters that may appear inside [`And`].
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be used in `And<..>`",
    label = "Expected a tuple of 1-12 elements, each implementing `QueryFilter`",
    note = "If there are more than 12 elements, nesting can be used."
)]
pub trait InAnd {}

// -----------------------------------------------------------------------------
// And

/// Query filter that matches entities satisfying **all** inner filters.
///
/// `And<(F1, F2, ...)>` is the logical conjunction of its inner filters.
/// At the table level the conjunction of several DNF branch sets is their
/// cartesian product; at the entity level every inner filter must accept
/// the entity.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_reflect::derive::TypePath;
///
/// #[derive(TypePath, Component, Clone)]
/// struct Health(u32);
///
/// #[derive(TypePath, Component, Clone)]
/// struct Player;
///
/// #[derive(TypePath, Component, Clone)]
/// struct Alive;
///
/// // Matches entities that are both `Player` and `Alive`.
/// fn heal_players(mut query: Query<&mut Health, And<(With<Player>, With<Alive>)>>) {
///     for health in query.iter_mut() {
///         let health = health.into_inner();
///         health.0 = health.0.saturating_add(1);
///     }
/// }
///
/// let mut world = World::alloc();
/// world.spawn((Health(10), Player, Alive), None);
/// world.spawn((Health(20), Player), None);
///
/// heal_players(world.query_mut::<&mut Health, And<(With<Player>, With<Alive>)>>());
/// assert_eq!(
///     world
///         .query::<&Health, And<(With<Player>, With<Alive>)>>()
///         .iter()
///         .next()
///         .unwrap()
///         .0,
///     11
/// );
/// ```
pub struct And<T: InAnd>(T);

// -----------------------------------------------------------------------------
// And for Tuple

macro_rules! impl_tuple {
    (0 : []) => {};
    (1 : [ $index:tt : $name:ident ]) => {
        #[cfg_attr(docsrs, doc(fake_variadic))]
        #[cfg_attr(docsrs, doc = "This trait is implemented for tuples up to 12 items long.")]
        impl<$name: QueryFilter> InAnd for ($name,) {}

        #[cfg_attr(docsrs, doc(fake_variadic))]
        #[cfg_attr(docsrs, doc = "This trait is implemented for tuples up to 12 items long.")]
        unsafe impl<$name: ArchetypeFilter> ArchetypeFilter for And<($name,)> {}

        #[cfg_attr(docsrs, doc(fake_variadic))]
        #[cfg_attr(docsrs, doc = "This trait is implemented for tuples up to 12 items long.")]
        unsafe impl<$name: QueryFilter> QueryFilter for And<($name,)> {
            type State = <$name>::State;
            type Cache<'world> = <$name>::Cache<'world>;

            const ENABLE_ENTITY_FILTER: bool = <$name>::ENABLE_ENTITY_FILTER;

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

            fn register_access(state: &Self::State, out: &mut ComponentAccess) {
                <$name>::register_access(state, out);
            }

            fn modify_access_table(state: &Self::State, table: &mut AccessTable, strict: bool) -> bool {
                <$name>::modify_access_table(state, table, strict)
            }

            unsafe fn update_table<'w>(
                state: &Self::State,
                cache: &mut Self::Cache<'w>,
                table: &'w Table,
            ) {
                unsafe { <$name>::update_table(state, cache, table) };
            }

            unsafe fn filter<'w>(
                state: &Self::State,
                cache: &mut Self::Cache<'w>,
                entity: EntityId,
                table_row: TableRow,
            ) -> bool {
                unsafe { <$name>::filter(state, cache, entity, table_row) }
            }
        }
    };
    ($num:literal : [$($index:tt : $name:ident),*]) => {
        #[cfg_attr(docsrs, doc(hidden))]
        impl<$($name: QueryFilter),*> InAnd for ($($name),*) {}

        #[cfg_attr(docsrs, doc(hidden))]
        unsafe impl<$($name: QueryFilter),*> ArchetypeFilter for And<($($name),*)> {}

        #[cfg_attr(docsrs, doc(hidden))]
        unsafe impl<$($name: QueryFilter),*> QueryFilter for And<($($name),*)> {
            type State = ( $( <$name>::State ),* );
            type Cache<'world> = ( $( <$name>::Cache<'world> ),* );

            const ENABLE_ENTITY_FILTER: bool = {
                false $( || <$name>::ENABLE_ENTITY_FILTER )*
            };

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
                // The conjunction of several DNF branches is itself DNF: the
                // cartesian product of every branch from each inner filter.
                let mut ret = Vec::<FilterParamBuilder>::new();
                ret.push(FilterParamBuilder::new());
                $({
                    let x = ::core::mem::take(&mut ret);
                    let mut y = Vec::<FilterParamBuilder>::new();
                    <$name>::register_filter(&state.$index, &mut y);
                    ret.reserve(x.len() * y.len());
                    x.iter().for_each(|a| {
                        y.iter().for_each(|b| {
                            if let Some(filter) = a.merge(b) {
                                ret.push(filter);
                            }
                        });
                    });
                })*

                out.append(&mut ret);
            }

            fn register_access(state: &Self::State, out: &mut ComponentAccess) {
                $( <$name>::register_access(&state.$index, out); )*
            }

            fn modify_access_table(state: &Self::State, table: &mut AccessTable, mut strict: bool) -> bool {
                let mut ok = true;
                $(
                    ok &= <$name>::modify_access_table(&state.$index, table, strict);
                    strict &= ok;
                )*

                ok
            }

            unsafe fn update_table<'w>(
                state: &Self::State,
                cache: &mut Self::Cache<'w>,
                table: &'w Table,
            ) {
                unsafe {
                    $( <$name>::update_table(&state.$index, &mut cache.$index, table); )*
                }
            }

            unsafe fn filter<'w>(
                state: &Self::State,
                cache: &mut Self::Cache<'w>,
                entity: EntityId,
                table_row: TableRow,
            ) -> bool {
                unsafe {
                    true
                    $( && <$name>::filter(&state.$index, &mut cache.$index, entity, table_row) )*
                }
            }
        }
    };
}

zlim_utils::range_invoke!(impl_tuple, 12);

// -----------------------------------------------------------------------------
