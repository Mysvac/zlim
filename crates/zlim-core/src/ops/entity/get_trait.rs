use core::any::TypeId;

use crate::borrow::{Mut, Ref};
use crate::component::Component;
use crate::table::{Table, TableRow};
use crate::tick::Tick;

/// # Safety
/// Internal Trait
pub unsafe trait GetComponents {
    /// Raw shared output (no change wrapper).
    type Raw<'a>;
    /// Change-aware shared output.
    type Ref<'a>;
    /// Change-aware mutable output.
    type Mut<'a>;

    /// Returns whether `table` can satisfy this component pattern.
    fn contains(table: &Table) -> bool;

    /// Gets the raw shared form of this component pattern.
    ///
    /// # Safety
    /// - TableRow must in bound.
    unsafe fn get<'a>(table: &'a Table, table_row: TableRow) -> Option<Self::Raw<'a>>;

    /// Gets the change-aware shared form of this component pattern.
    ///
    /// # Safety
    /// - TableRow must in bound.
    unsafe fn get_ref<'a>(
        table: &'a Table,
        table_row: TableRow,
        last_run: Tick,
        this_run: Tick,
    ) -> Option<Self::Ref<'a>>;

    /// Gets the change-aware mutable form of this component pattern.
    ///
    /// # Safety
    /// - TableRow must in bound.
    /// - Types should not be duplicated (mutable references to individual
    ///   types should not be obtained repeatedly).
    unsafe fn get_mut<'a>(
        table: &'a mut Table,
        table_row: TableRow,
        last_run: Tick,
        this_run: Tick,
    ) -> Option<Self::Mut<'a>>;
}

unsafe impl<T: Component> GetComponents for T {
    type Raw<'a> = &'a T;
    type Ref<'a> = Ref<'a, T>;
    type Mut<'a> = Mut<'a, T>;

    fn contains(table: &Table) -> bool {
        table.get_type_col(TypeId::of::<T>()).is_some()
    }

    unsafe fn get<'a>(table: &'a Table, table_row: TableRow) -> Option<Self::Raw<'a>> {
        let table_col = table.get_type_col(TypeId::of::<T>())?;
        unsafe {
            let ptr = table.get_data(table_row, table_col);
            ptr.debug_assert_aligned::<T>();
            Some(ptr.deref::<T>())
        }
    }

    unsafe fn get_ref<'a>(
        table: &'a Table,
        table_row: TableRow,
        last_run: Tick,
        this_run: Tick,
    ) -> Option<Self::Ref<'a>> {
        let table_col = table.get_type_col(TypeId::of::<T>())?;
        unsafe {
            let untyped = table.get_ref(table_row, table_col, last_run, this_run);
            Some(untyped.with_type::<T>())
        }
    }

    unsafe fn get_mut<'a>(
        table: &'a mut Table,
        table_row: TableRow,
        last_run: Tick,
        this_run: Tick,
    ) -> Option<Self::Mut<'a>> {
        let table_col = table.get_type_col(TypeId::of::<T>())?;
        unsafe {
            let untyped = table.get_mut(table_row, table_col, last_run, this_run);
            Some(untyped.with_type::<T>())
        }
    }
}

macro_rules! impl_tuple {
    (0: []) => {};
    (1 : [ $index:tt : $name:ident ]) => {
        #[cfg_attr(docsrs, doc(fake_variadic))]
        #[cfg_attr(docsrs, doc = "This trait is implemented for tuples up to 12 items long.")]
        unsafe impl<$name: Component> GetComponents for ($name,) {
            type Raw<'a> = ( &'a $name, );
            type Ref<'a> = ( Ref<'a, $name>, );
            type Mut<'a> = ( Mut<'a, $name>, );

            fn contains(table: &Table) -> bool {
                <$name as GetComponents>::contains(table)
            }

            unsafe fn get<'a>(
                table: &'a Table,
                table_row: TableRow,
            ) -> Option<Self::Raw<'a>> {
                unsafe {
                    Some((
                        <$name as GetComponents>::get(table, table_row)?,
                    ))
                }
            }

            unsafe fn get_ref<'a>(
                table: &'a Table,
                table_row: TableRow,
                last_run: Tick,
                this_run: Tick,
            ) -> Option<Self::Ref<'a>> {
                unsafe {
                    Some((
                        <$name as GetComponents>::get_ref(table, table_row, last_run, this_run)?,
                    ))
                }
            }

            unsafe fn get_mut<'a>(
                table: &'a mut Table,
                table_row: TableRow,
                last_run: Tick,
                this_run: Tick,
            ) -> Option<Self::Mut<'a>> {
                unsafe {
                    Some((
                        <$name as GetComponents>::get_mut(table, table_row, last_run, this_run)?,
                    ))
                }
            }
        }
    };
    ($num:literal : [$($index:tt : $name:ident),*]) => {
        #[cfg_attr(docsrs, doc(hidden))]
        unsafe impl<$($name: Component),*> GetComponents for ($($name,)*) {
            type Raw<'a> = ( $( &'a $name, )* );
            type Ref<'a> = ( $( Ref<'a, $name>, )* );
            type Mut<'a> = ( $( Mut<'a, $name>, )* );

            fn contains(table: &Table) -> bool {
                true $( && <$name as GetComponents>::contains(table) )*
            }

            unsafe fn get<'a>(table: &'a Table, table_row: TableRow) -> Option<Self::Raw<'a>> {
                unsafe {
                    Some((
                        $( <$name as GetComponents>::get(table, table_row)?, )*
                    ))
                }
            }

            unsafe fn get_ref<'a>(
                table: &'a Table,
                table_row: TableRow,
                last_run: Tick,
                this_run: Tick,
            ) -> Option<Self::Ref<'a>> {
                unsafe {
                    Some((
                        $( <$name as GetComponents>::get_ref(table, table_row, last_run, this_run)?, )*
                    ))
                }
            }

            unsafe fn get_mut<'a>(
                table: &'a mut Table,
                table_row: TableRow,
                last_run: Tick,
                this_run: Tick,
            ) -> Option<Self::Mut<'a>> {
                unsafe {
                    let table_ptr = table as *mut Table;
                    Some((
                        $( <$name as GetComponents>::get_mut(&mut *table_ptr, table_row, last_run, this_run)?, )*
                    ))
                }
            }
        }
    };
}

zlim_utils::range_invoke!(impl_tuple, 12);
