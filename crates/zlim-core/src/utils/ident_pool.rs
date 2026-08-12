use std::sync::{Mutex, PoisonError};
use zlim_utils::hash::HashSet;

use zlim_utils::mem::Global;

use crate::component::{ComponentHook, ComponentId};

/// Intern pool for deduplicating small immutable identifier slices.
///
/// `SlicePool` avoids repeatedly allocating equivalent `'static` slices
/// used by archetype/component metadata.  When a slice is interned for the
/// first time, it is copied into a `Global` (process-lifetime) allocation.
/// Subsequent calls with the same contents return the existing allocation.
///
/// # Design notes
///
/// - Accepted slices are intentionally **leaked** for process-lifetime
///   reuse.  In the ECS context, component-ID sets live for the entire
///   program, so this is a deliberate space-time trade-off.
/// - A `Mutex` protects the pool because `SlicePool` is only accessed
///   from the main thread; a `RwLock` would add unnecessary overhead.
pub(crate) struct SlicePool;

/// Internal helper macro that generates a typed interning method for
/// `SlicePool`.  Each method manages its own static pool.
///
/// # Generated method
///
/// - `pub(crate) fn $name(idents: &[$ty]) -> &'static [$ty]`
///
///   Returns the interned `&'static` slice.  If the input is empty, the
///   static empty slice `&[]` is returned directly without locking.
macro_rules! define_methods {
    ($name:ident, $ty:ty) => {
        pub(crate) fn $name(idents: &[$ty]) -> &'static [$ty] {
            // SlicePool is actually only used on the main thread.
            // So `Mutex` is faster then `RwLock`.
            static POOL: Mutex<HashSet<&[$ty]>> = Mutex::new(HashSet::new());

            if idents.is_empty() {
                return &[];
            }

            let guard = POOL.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(&idents) = guard.get(idents) {
                return idents;
            }
            ::core::mem::drop(guard);

            // Duplicate leak same slice is possible, but it's rare and acceptable.
            let slice: &[$ty] = Global::alloc_slice(idents);
            POOL.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(slice);
            slice
        }
    };
}

impl SlicePool {
    // Interns a slice of [`ComponentId`]s and returns a `&'static`
    // reference.  This is the primary use case — every table and bundle
    // maps to a canonical component-ID set.
    define_methods!(component, ComponentId);

    // Interns a slice of `(ComponentId, ComponentHook)` pairs.  Used by
    // the table construction path to deduplicate hook lists.
    define_methods!(component_hook, (ComponentId, ComponentHook));
}
