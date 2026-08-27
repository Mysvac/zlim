use std::sync::{Mutex, PoisonError};
use zlim_utils::hash::HashSet;

use crate::component::{ComponentHook, ComponentId};
use crate::job::{Job, JobId};
use zlim_utils::mem::Global;

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
pub struct SlicePool;

/// Internal helper macro that generates a typed interning method for
/// `SlicePool`.  Each method manages its own static pool.
///
/// # Generated method
///
/// - `pub fn $name(idents: &[$ty]) -> &'static [$ty]`
///
///   Returns the interned `&'static` slice.  If the input is empty, the
///   static empty slice `&[]` is returned directly without locking.
macro_rules! define_methods {
    ($name:ident, $ty:ty) => {
        pub fn $name(idents: &[$ty]) -> &'static [$ty] {
            // SlicePool is actually only used on the main thread.
            // So `Mutex` is faster than `RwLock`.
            static POOL: Mutex<HashSet<&[$ty]>> = Mutex::new(HashSet::new());

            if idents.is_empty() {
                return &[];
            }

            let mut guard = POOL.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(&idents) = guard.get(idents) {
                return idents;
            }

            ::core::hint::cold_path();
            let slice: &'static [$ty] = Global::alloc_slice(idents);
            guard.insert(slice);

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

    // Interns a slice of `(u16, u16)` pairs.
    define_methods!(u16x2, (u16, u16));

    // Interns a slice of `JobId`.
    define_methods!(job_id, JobId);

    // Interns a slice of run-condition constructors, each taking the job's
    // group name.
    define_methods!(run_if, fn(&'static str) -> Box<dyn Job>);
}
