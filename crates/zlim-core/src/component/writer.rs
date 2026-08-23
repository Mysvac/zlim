//! The [`ComponentWriter`] used to write component data into a table row
//! during spawn and insert operations.

use core::any::TypeId;

use zlim_log as log;
use zlim_ptr::OwningPtr;
use zlim_utils::ext::TypeMap;

use crate::component::Component;
use crate::table::{Column, Table, TableRow};
use crate::tick::Tick;
use crate::utils::{DebugCheckedUnwrap, Dropper};

// -----------------------------------------------------------------------------
// AbortOnDropFail

/// Drop guard that aborts the process if writing a component panics —
/// e.g. a component's `Drop` implementation or the byte copy.
struct AbortOnPanic;

impl Drop for AbortOnPanic {
    #[cold]
    #[inline(never)]
    fn drop(&mut self) {
        log::error!("Aborting due to drop component or copy bytes panicked.");
        std::process::abort();
    }
}

// -----------------------------------------------------------------------------
// ComponentWriter

struct Slot<'a> {
    data: OwningPtr<'a>,
    size: usize,
    dropper: Option<Dropper>,
    added: &'a mut Tick,
    changed: &'a mut Tick,
    initialized: bool,
}

/// Writes component data into a target table row during spawn and insert
/// operations.
///
/// A `ComponentWriter` is built for a specific table row via
/// [`from_table`](Self::from_table) and exposes one slot per component
/// type present in that row. `Bundle` implementations fill the slots
/// through [`write`](Self::write), [`write_raw`](Self::write_raw), and
/// [`write_if_uninit`](Self::write_if_uninit); slots left unwritten stay
/// uninitialised.
///
/// [`Table`]: crate::table::Table
pub struct ComponentWriter<'a> {
    now: Tick,
    mapper: TypeMap<Slot<'a>>,
}

// -----------------------------------------------------------------------------
// implementation

impl ComponentWriter<'_> {
    /// Returns `true` if the writer has a slot for the component type
    /// identified by `ty` — i.e. the type exists in the target table row.
    #[inline(never)]
    pub fn contains(&self, ty: TypeId) -> bool {
        self.mapper.contains(ty)
    }

    /// Marks the slot for `ty` as initialised without writing any data.
    ///
    /// Used when a component's value is handled outside the writer (e.g.
    /// moved into the column directly) and must not be default-filled
    /// afterwards.
    ///
    /// # Panics
    ///
    /// Panics if `ty` is not present in the writer.
    #[inline(never)]
    pub fn assume_init(&mut self, ty: TypeId) {
        self.mapper.get_mut(ty).unwrap().initialized = true;
    }

    /// Writes component data of the type identified by `ty` into the target
    /// row.
    ///
    /// If the slot was already initialised, the previous value is dropped
    /// and replaced, and only the `changed` tick is bumped; otherwise the
    /// value is initialised and both the `added` and `changed` ticks are set
    /// to the tick passed to [`from_table`](Self::from_table).
    ///
    /// # Safety
    ///
    /// - `ty` must be a component type present in the target table row
    ///   (i.e. present in the writer's internal map); otherwise the
    ///   internal lookup is undefined behaviour.
    /// - `data` must point to a valid, properly aligned, initialised
    ///   instance of the component type `ty`, with at least the component's
    ///   recorded layout size in readable bytes.
    /// - `data` must not overlap with the destination storage in the table
    ///   column; the bytes are moved with `copy_nonoverlapping`.
    /// - The writer takes ownership of `data`: the value is copied into
    ///   storage and `data` must not be used afterwards.
    ///
    /// # Panics
    ///
    /// May panic if `ty` is not present in the writer (the internal lookup
    /// is debug-checked).
    #[inline(never)]
    pub unsafe fn write_raw(&mut self, ty: TypeId, data: OwningPtr<'_>) {
        let abort_guard = AbortOnPanic;

        let slot = unsafe { self.mapper.get_mut(ty).debug_checked_unwrap() };

        if slot.initialized {
            if let Some(dropper) = slot.dropper {
                unsafe { dropper.call(slot.data.borrow_mut().promote()) };
            }

            *slot.changed = self.now;
        } else {
            *slot.added = self.now;
            *slot.changed = self.now;
        }

        unsafe {
            let src: *mut u8 = data.as_ptr();
            let dst: *mut u8 = slot.data.as_ptr();
            core::ptr::copy_nonoverlapping::<u8>(src, dst, slot.size);
            slot.initialized = true;
        }

        ::core::mem::forget(abort_guard);
    }

    /// Writes a component value into its slot, replacing any previous value.
    ///
    /// # Panics
    ///
    /// Panics if `T` is not present in the target table row.
    #[inline(always)]
    pub fn write<T: Component>(&mut self, data: T) {
        assert!(self.contains(TypeId::of::<T>()));
        zlim_ptr::into_owning!(data);
        unsafe { self.write_raw(TypeId::of::<T>(), data) };
    }

    /// Writes a component value only if its slot is not yet initialised.
    ///
    /// Used for required components: the value is produced by `ctor` and
    /// written only when the bundle did not provide one explicitly.
    ///
    /// # Panics
    ///
    /// Panics if `T` is not present in the target table row.
    #[inline(always)]
    pub fn write_if_uninit<T: Component>(&mut self, ctor: impl FnOnce() -> T) {
        if self.mapper.get(TypeId::of::<T>()).unwrap().initialized {
            return;
        }
        let data = ctor();
        zlim_ptr::into_owning!(data);
        unsafe { self.write_raw(TypeId::of::<T>(), data) };
    }
}

// -----------------------------------------------------------------------------
// CTOR

impl<'a> ComponentWriter<'a> {
    /// Creates a writer targeting every column of the given [`Table`] row,
    /// using `now` as the current change-detection [`Tick`] for the `added`
    /// / `changed` ticks written by [`write_raw`](Self::write_raw).
    ///
    /// # Panics
    ///
    /// Panics if `row` is out of bounds of the table's entities.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use zlim_core::prelude::*;
    /// use zlim_core::component::ComponentWriter;
    ///
    /// // `table` / `row` come from a real spawn or insert flow; the writer
    /// // wraps a single target row. Building one requires `&mut Table`,
    /// // which is only reachable inside the ECS internals, so this snippet
    /// // is a sketch rather than a runnable example.
    /// let mut writer = ComponentWriter::from_table(table, row, Tick::new(1));
    ///
    /// writer.write(Position { x: 0.0, y: 0.0 });
    /// // Required components are default-filled only when the bundle did
    /// // not provide them explicitly:
    /// writer.write_if_uninit(|| Health { value: 100.0 });
    /// ```
    #[inline(never)]
    pub fn from_table(table: &'a mut Table, row: TableRow, now: Tick) -> Self {
        let index = row.0 as usize;
        assert!(index < table.entities().len());

        let mut mapper = TypeMap::new();

        let ptr = table as *mut Table;
        for (ty, &col) in table.type_cols() {
            let column = unsafe { (&mut *ptr).get_column_mut(col) as *mut Column };
            let data = unsafe { (&mut *column).get_data_mut(index).promote() };
            let size = unsafe { (&*column).item_layout().size() };
            let dropper = unsafe { (&*column).dropper() };
            let added = unsafe { (&mut *column).get_added_mut(index) };
            let changed = unsafe { (&mut *column).get_changed_mut(index) };
            mapper.insert(
                ty,
                Slot {
                    data,
                    size,
                    dropper,
                    added,
                    changed,
                    initialized: false,
                },
            );
        }

        Self { now, mapper }
    }
}

// -----------------------------------------------------------------------------
