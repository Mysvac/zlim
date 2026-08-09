use core::any::TypeId;

use crate::borrow::{Mut, Ref};
use crate::component::Component;
use crate::table::Table;
use crate::table::TableRow;
use crate::tick::Tick;

/// # Safety
/// Internal Trait
pub unsafe trait FetchComponents {
    /// The fetched output type.
    type Item<'a>;

    /// # Safety
    /// Given `TableRow` must be valid (in bound).
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
