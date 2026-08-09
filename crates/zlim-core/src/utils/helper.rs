use crate::component::ComponentId;
use crate::entity::EntityId;
use crate::tick::MAX_TICK_AGE;
use crate::tick::Tick;

const MULTI_THREADED: bool = zlim_task::cfg::multi_thread!();
const THRESHOLD: usize = 64_000;

const _: () = {
    assert!(size_of::<Tick>() == size_of::<u32>());
    assert!(size_of::<EntityId>() == size_of::<u64>());
};

/// Clamps a tick slice, optimized for bulk processing.
///
/// See: https://godbolt.org/
///
/// Internal note: this performs representation casts to `u32` for better code
/// generation and assumes `Tick` is layout-compatible with `u32`.
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

/// A SIMD-optimized `contains` for `ComponentId`.
///
/// See: https://godbolt.org/
///
/// With O3 optimization, it is faster than binary search when the number of elements is less than 100.
#[inline(always)]
pub(crate) fn contains_component(id: ComponentId, slice: &[ComponentId]) -> bool {
    let val = unsafe { core::mem::transmute::<ComponentId, u32>(id) };
    let arr = unsafe { core::mem::transmute::<&[ComponentId], &[u32]>(slice) };
    arr.contains(&val)
}

/// A SIMD and multi-threaded optimized `contains` for `EntityId`.
#[inline(always)]
pub(crate) fn contains_entity(id: EntityId, slice: &[EntityId]) -> bool {
    #[inline(never)]
    fn par_contains(id: EntityId, slice: &[EntityId]) -> bool {
        use zlim_task::ParallelSlice;
        slice.par_contains(&id)
    }

    if MULTI_THREADED && slice.len() > THRESHOLD {
        return par_contains(id, slice);
    }

    let val = unsafe { core::mem::transmute::<EntityId, u64>(id) };
    let arr = unsafe { core::mem::transmute::<&[EntityId], &[u64]>(slice) };
    arr.contains(&val)
}

/// A SIMD and multi-threaded optimized `position` for `EntityId`.
#[inline(always)]
pub(crate) fn position_entity(id: EntityId, slice: &[EntityId]) -> Option<usize> {
    #[inline(never)]
    fn par_position(id: EntityId, slice: &[EntityId]) -> Option<usize> {
        use zlim_task::ParallelSlice;
        slice.par_position(|&e| e == id)
    }

    if MULTI_THREADED && slice.len() > THRESHOLD {
        return par_position(id, slice);
    }

    let val = unsafe { core::mem::transmute::<EntityId, u64>(id) };
    let arr = unsafe { core::mem::transmute::<&[EntityId], &[u64]>(slice) };
    // For the deletion of relationship entities,
    // the right side is faster
    arr.iter().rposition(|&e| e == val)
}
