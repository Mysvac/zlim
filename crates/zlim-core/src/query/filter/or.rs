//! The `Or` query filter — logical disjunction of inner filters.

use super::{ArchetypeFilter, QueryFilter};
use crate::entity::EntityId;
use crate::system::{AccessTable, ComponentAccess, FilterParamBuilder};
use crate::table::{Table, TableRow};
use crate::tick::Tick;
use crate::world::{World, WorldCell};

// -----------------------------------------------------------------------------
// InOr

/// Marker trait for tuples of 1–12 filters that may appear inside [`Or`].
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be used in `Or<..>`",
    label = "Expected a tuple of 1-12 elements, each implementing `QueryFilter`",
    note = "If there are more than 12 elements, nesting can be used."
)]
pub trait InOr {}

// -----------------------------------------------------------------------------
// Or

/// Query filter that matches entities satisfying **at least one** inner
/// filter.
///
/// `Or<(F1, F2, ...)>` is the logical disjunction of its inner filters.
/// At the table level the disjunction of several DNF branch sets is their
/// union; at the entity level any inner filter accepting the entity suffices.
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
/// struct Poisoned;
///
/// #[derive(TypePath, Component, Clone)]
/// struct Burning;
///
/// // Matches entities with either `Poisoned` or `Burning`.
/// fn tick_damage(mut query: Query<&mut Health, Or<(With<Poisoned>, With<Burning>)>>) {
///     for health in query.iter_mut() {
///         let health = health.into_inner();
///         health.0 = health.0.saturating_sub(1);
///     }
/// }
///
/// let mut world = World::alloc();
/// world.spawn((Health(10), Poisoned), None);
/// world.spawn((Health(20), Burning), None);
/// world.spawn(Health(30), None);
///
/// tick_damage(world.query_mut::<&mut Health, Or<(With<Poisoned>, With<Burning>)>>());
/// let total: u32 = world.query::<&Health, ()>().iter().map(|h| h.0).sum();
/// assert_eq!(total, 9 + 19 + 30);
/// ```
pub struct Or<T: InOr>(T);

// -----------------------------------------------------------------------------
// Or for Tuple

macro_rules! impl_tuple {
    (0 : []) => {};
    (1 : [ $index:tt : $name:ident ]) => {
        #[cfg_attr(docsrs, doc(fake_variadic))]
        #[cfg_attr(docsrs, doc = "This trait is implemented for tuples up to 12 items long.")]
        impl<$name: QueryFilter> InOr for ($name,) {}

        #[cfg_attr(docsrs, doc(fake_variadic))]
        #[cfg_attr(docsrs, doc = "This trait is implemented for tuples up to 12 items long.")]
        unsafe impl<$name: ArchetypeFilter> ArchetypeFilter for Or<($name,)> {}

        #[cfg_attr(docsrs, doc(fake_variadic))]
        #[cfg_attr(docsrs, doc = "This trait is implemented for tuples up to 12 items long.")]
        unsafe impl<$name: QueryFilter> QueryFilter for Or<($name,)> {
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
        impl<$($name: QueryFilter),*> InOr for ($($name),*) {}

        #[cfg_attr(docsrs, doc(hidden))]
        unsafe impl<$($name: ArchetypeFilter),*> ArchetypeFilter for Or<($($name),*)> {}

        #[cfg_attr(docsrs, doc(hidden))]
        unsafe impl<$($name: QueryFilter),*> QueryFilter for Or<($($name),*)> {
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
                // The disjunction of several DNF branches is the union of
                // their branch sets — matching any branch satisfies the Or.
                let mut ret = Vec::<FilterParamBuilder>::new();
                $( <$name>::register_filter(&state.$index, &mut ret); )*
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
                    false
                    $( || <$name>::filter(&state.$index, &mut cache.$index, entity, table_row) )*
                }
            }
        }
    };
}

zlim_utils::range_invoke!(impl_tuple, 12);

// -----------------------------------------------------------------------------
