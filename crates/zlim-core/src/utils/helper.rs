use crate::component::ComponentId;
use crate::entity::EntityId;
use crate::tick::MAX_TICK_AGE;
use crate::tick::Tick;

/// When `true`, parallel search is enabled for large entity slices.
const MULTI_THREADED: bool = zlim_task::cfg::multi_thread!();

/// Minimum slice length before parallel search is considered.
///
/// Below this threshold the overhead of parallel work distribution
/// outweighs the benefit of multi-core scanning.
const THRESHOLD: usize = 64_000;

// -----------------------------------------------------------------------------
// Layout assertions — checked at compile time
// -----------------------------------------------------------------------------

/// Static assertions verifying that `Tick`, `EntityId`, and `ComponentId`
/// have the expected size/alignment.  These exist to support SIMD and
/// multi-threaded search helpers that rely on representation casts between
/// the wrapper types and their inner `u32`/`u64` storage.
const _: () = {
    assert!(size_of::<Tick>() == size_of::<u32>());
    assert!(size_of::<EntityId>() == size_of::<u64>());
    assert!(size_of::<ComponentId>() == size_of::<u32>());
    assert!(align_of::<Tick>() == align_of::<u32>());
    assert!(align_of::<EntityId>() == align_of::<u64>());
    assert!(align_of::<ComponentId>() == align_of::<u32>());
};

// -----------------------------------------------------------------------------
// clamp_tick_slice
// -----------------------------------------------------------------------------

/// Clamps a tick slice, optimised for bulk processing.
///
/// For each tick in the slice, if the age (current - stored) exceeds
/// [`MAX_TICK_AGE`], the tick is clamped to a fallback value to prevent
/// wrap-around from causing false positives in change detection.
///
/// # Implementation notes
///
/// The function transmutes `&mut [Tick]` to `&mut [u32]` for better
/// code generation — `u32` operations are more readily autovectorised by
/// LLVM.  See <https://godbolt.org/> for assembly comparisons.
pub(crate) fn clamp_tick_slice(this: &mut [Tick], now: Tick) {
    use core::mem::transmute;

    // `u32` is more easily optimized by compiler.
    let arr = unsafe { transmute::<&mut [Tick], &mut [u32]>(this) };
    let now: u32 = unsafe { transmute::<Tick, u32>(now) };

    let fall_back = now.wrapping_sub(MAX_TICK_AGE);

    // `for_each` can generate better code than explicit `for` loops.
    // At present, it's guaranteed that `wrapping_sub` and `>` are SIMD.
    arr.iter_mut().for_each(|x| {
        let age = now.wrapping_sub(*x);
        if age > MAX_TICK_AGE {
            *x = fall_back;
        }
    });
}

// -----------------------------------------------------------------------------
// contains_component
// -----------------------------------------------------------------------------

/// A SIMD-optimised `contains` for [`ComponentId`].
///
/// For slices under approximately 100 elements, this linear scan is
/// faster than binary search because the compiler can autovectorise the
/// comparison loop.  For larger slices, the caller should consider
/// binary search instead.
///
/// # Safety
///
/// Relies on [`ComponentId`] being layout-compatible with `u32`
/// (verified by the compile-time assertions above).
#[inline(always)]
pub(crate) fn contains_component(id: ComponentId, slice: &[ComponentId]) -> bool {
    let val = unsafe { core::mem::transmute::<ComponentId, u32>(id) };
    let arr = unsafe { core::mem::transmute::<&[ComponentId], &[u32]>(slice) };
    arr.contains(&val)
}

// -----------------------------------------------------------------------------
// contains_entity
// -----------------------------------------------------------------------------

/// A SIMD- and multi-threaded-optimised `contains` for [`EntityId`].
///
/// For slices shorter than [`THRESHOLD`] (64,000), a simple SIMD linear
/// scan is used.  Above the threshold, the search is distributed across
/// multiple threads via [`zlim_task::ParallelSlice`].
///
/// # Safety
///
/// Relies on [`EntityId`] being layout-compatible with `u64`
/// (verified by the compile-time assertions above).
#[inline(always)]
pub(crate) fn contains_entity(id: EntityId, slice: &[EntityId]) -> bool {
    #[inline(never)]
    fn par_contains(id: u64, slice: &[u64]) -> bool {
        use zlim_task::ParallelSlice;
        slice.par_contains(&id)
    }

    let val = unsafe { core::mem::transmute::<EntityId, u64>(id) };
    let arr = unsafe { core::mem::transmute::<&[EntityId], &[u64]>(slice) };

    if MULTI_THREADED && slice.len() > THRESHOLD {
        return par_contains(val, arr);
    }

    arr.contains(&val)
}

// -----------------------------------------------------------------------------
// position_entity
// -----------------------------------------------------------------------------

/// A SIMD- and multi-threaded-optimised `rposition` for [`EntityId`].
///
/// Returns the index of the first matching element, searching from the
/// **right** (highest index first).  This ordering is chosen because entity
/// removal paths benefit from finding the element closest to the end of
/// the slice.
///
/// As with [`contains_entity`], parallelism kicks in above
/// [`THRESHOLD`].
///
/// # Safety
///
/// Relies on [`EntityId`] being layout-compatible with `u64`.
#[inline(always)]
pub(crate) fn position_entity(id: EntityId, slice: &[EntityId]) -> Option<usize> {
    #[inline(never)]
    fn par_position(id: u64, slice: &[u64]) -> Option<usize> {
        use zlim_task::ParallelSlice;
        slice.par_position(|&e| e == id)
    }

    let val = unsafe { core::mem::transmute::<EntityId, u64>(id) };
    let arr = unsafe { core::mem::transmute::<&[EntityId], &[u64]>(slice) };

    if MULTI_THREADED && slice.len() > THRESHOLD {
        return par_position(val, arr);
    }

    // For the deletion of entities, the right side is faster
    arr.iter().rposition(|&e| e == val)
}
