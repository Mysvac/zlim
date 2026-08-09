use std::collections::BTreeSet;
use std::sync::{Mutex, PoisonError};

use zlim_utils::mem::Global;

use crate::component::{ComponentHook, ComponentId};

/// Intern pool for deduplicating small immutable identifier slices.
///
/// This avoids repeatedly allocating equivalent `'static` slices used by
/// archetype/component metadata. Identical slice contents are reused whenever
/// possible.
///
/// The pool intentionally leaks accepted slices for process-lifetime reuse.
pub struct SlicePool;

macro_rules! define_methods {
    ($name:ident, $ty:ty) => {
        pub fn $name(idents: &[$ty]) -> &'static [$ty] {
            // SlicePool is actually only used on the main thread.
            // So `Mutex` is faster then `RwLock`.
            static POOL: Mutex<BTreeSet<&[$ty]>> = Mutex::new(BTreeSet::new());

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
    define_methods!(component, ComponentId);
    define_methods!(component_hook, (ComponentId, ComponentHook));
}
