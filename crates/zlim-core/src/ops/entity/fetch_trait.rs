//! The `FetchComponents` access trait and its implementations.

use core::any::TypeId;

use crate::borrow::{Mut, Ref};
use crate::component::Component;
use crate::table::Table;
use crate::table::TableRow;
use crate::tick::Tick;

/// Internal trait describing an arbitrary component access pattern that can
/// be fetched from a [`Table`] row.
///
/// Implemented for component references (`&T`, `&mut T`), change-aware
/// wrappers ([`Ref`], [`Mut`]), optional components, and tuples of the
/// above.  This is the pattern used by the entity-handle `fetch` methods
/// ([`Entity::fetch`], [`EntityMut::fetch`], [`EntityOwned::fetch`]).
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// #[derive(TypePath, Component, Clone, PartialEq, Debug)]
/// struct Hp(u32);
///
/// #[derive(TypePath, Component, Clone, PartialEq, Debug)]
/// struct Speed(f32);
///
/// let mut world = World::alloc();
/// let mut entity = world.spawn((Hp(100), Speed(3.0)), None);
///
/// // `fetch` accepts any pattern that implements `FetchComponents`:
/// // references, change-aware wrappers, options, and tuples.
/// let (hp, speed) = entity.fetch::<(&Hp, Mut<Speed>)>().unwrap();
/// assert_eq!(hp, &Hp(100));
/// assert_eq!(speed.into_inner(), &Speed(3.0));
/// ```
///
/// # Safety
///
/// Implementors must ensure that every reference produced by [`fetch`] is
/// derived exclusively from the passed `table` borrow and the requested
/// `table_row`, and that the same component type is never fetched mutably
/// more than once.
///
/// [`Table`]: crate::table::Table
/// [`fetch`]: FetchComponents::fetch
/// [`Entity::fetch`]: crate::ops::Entity::fetch
/// [`EntityMut::fetch`]: crate::ops::EntityMut::fetch
/// [`EntityOwned::fetch`]: crate::ops::EntityOwned::fetch
pub unsafe trait FetchComponents {
    /// The fetched output type.
    type Item<'a>;

    /// Fetches this component access pattern from a table row.
    ///
    /// # Safety
    /// The given `TableRow` must be in bounds for `table`.
    unsafe fn fetch<'a>(
        mutable: bool,
        table: &'a mut Table,
        table_row: TableRow,
        last_run: Tick,
        this_run: Tick,
    ) -> Option<Self::Item<'a>>;
}

unsafe impl<T: Component> FetchComponents for &T {
    type Item<'a> = &'a T;

    unsafe fn fetch<'a>(
        _mutable: bool,
        table: &'a mut Table,
        table_row: TableRow,
        _last_run: Tick,
        _this_run: Tick,
    ) -> Option<Self::Item<'a>> {
        let table_col = table.get_type_col(TypeId::of::<T>())?;
        unsafe {
            let ptr = table.get_data(table_row, table_col);
            ptr.debug_assert_aligned::<T>();
            Some(ptr.deref::<T>())
        }
    }
}

unsafe impl<T: Component> FetchComponents for &mut T {
    type Item<'a> = &'a mut T;

    unsafe fn fetch<'a>(
        mutable: bool,
        table: &'a mut Table,
        table_row: TableRow,
        last_run: Tick,
        this_run: Tick,
    ) -> Option<Self::Item<'a>> {
        if !mutable {
            return None;
        }
        let table_col = table.get_type_col(TypeId::of::<T>())?;
        unsafe {
            let untyped = table.get_mut(table_row, table_col, last_run, this_run);
            Some(untyped.with_type::<T>().into_inner())
        }
    }
}

unsafe impl<T: Component> FetchComponents for Ref<'_, T> {
    type Item<'a> = Ref<'a, T>;

    unsafe fn fetch<'a>(
        _mutable: bool,
        table: &'a mut Table,
        table_row: TableRow,
        last_run: Tick,
        this_run: Tick,
    ) -> Option<Self::Item<'a>> {
        let table_col = table.get_type_col(TypeId::of::<T>())?;
        unsafe {
            let untyped = table.get_ref(table_row, table_col, last_run, this_run);
            Some(untyped.with_type::<T>())
        }
    }
}

unsafe impl<T: Component> FetchComponents for Mut<'_, T> {
    type Item<'a> = Mut<'a, T>;

    unsafe fn fetch<'a>(
        mutable: bool,
        table: &'a mut Table,
        table_row: TableRow,
        last_run: Tick,
        this_run: Tick,
    ) -> Option<Self::Item<'a>> {
        if !mutable {
            return None;
        }

        let table_col = table.get_type_col(TypeId::of::<T>())?;
        unsafe {
            let untyped = table.get_mut(table_row, table_col, last_run, this_run);
            Some(untyped.with_type::<T>())
        }
    }
}

unsafe impl<T: FetchComponents> FetchComponents for Option<T> {
    type Item<'a> = Option<T::Item<'a>>;

    unsafe fn fetch<'a>(
        mutable: bool,
        table: &'a mut Table,
        table_row: TableRow,
        last_run: Tick,
        this_run: Tick,
    ) -> Option<Self::Item<'a>> {
        unsafe { Some(T::fetch(mutable, table, table_row, last_run, this_run)) }
    }
}

macro_rules! impl_tuple {
    (0: []) => {};
    (1 : [ $index:tt : $name:ident ]) => {
        #[cfg_attr(docsrs, doc(fake_variadic))]
        #[cfg_attr(docsrs, doc = "This trait is implemented for tuples up to 12 items long.")]
        unsafe impl<$name: FetchComponents> FetchComponents for ($name,) {
            type Item<'a> = (<$name>::Item<'a>,);

            unsafe fn fetch<'a>(
                mutable: bool,
                table: &'a mut Table,
                table_row: TableRow,
                last_run: Tick,
                this_run: Tick,
            ) -> Option<Self::Item<'a>> {
                unsafe {
                    Some((
                        <$name>::fetch(mutable, table, table_row, last_run, this_run)?,
                    ))
                }
            }
        }
    };
    ($num:literal : [$($index:tt : $name:ident),*]) => {
        #[cfg_attr(docsrs, doc(hidden))]
        unsafe impl<$($name: FetchComponents),*> FetchComponents for ($($name,)*) {
            type Item<'a> = ( $( <$name>::Item<'a>, )* );

            unsafe fn fetch<'a>(
                mutable: bool,
                table: &'a mut Table,
                table_row: TableRow,
                last_run: Tick,
                this_run: Tick,
            ) -> Option<Self::Item<'a>> {
                unsafe {
                    let table_ptr = table as *mut Table;
                    Some((
                        $( <$name>::fetch(mutable, &mut *table_ptr, table_row, last_run, this_run)?, )*
                    ))
                }
            }
        }
    };
}

zlim_utils::range_invoke!(impl_tuple, 12);
