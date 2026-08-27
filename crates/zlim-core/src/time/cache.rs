use core::any::TypeId;
use core::cell::UnsafeCell;
use core::panic::RefUnwindSafe;
use core::panic::UnwindSafe;

use zlim_utils::mem::Global;

use super::{Fixed, Real, Time, Virtual};
use super::{TimeSnapshot, TimeState};
use crate::borrow::ResMut;
use crate::resource::Resource;
use crate::resource::ResourceCell;
use crate::resource::Resources;
use crate::tick::Tick;
use crate::utils::DebugCheckedUnwrap;

/// A cache used to accelerate access to the time API.
pub(crate) struct TimeCache {
    pub time: &'static UnsafeCell<ResourceCell>,
    pub real: &'static UnsafeCell<ResourceCell>,
    pub virt: &'static UnsafeCell<ResourceCell>,
    pub fixed: &'static UnsafeCell<ResourceCell>,
    pub state: &'static UnsafeCell<ResourceCell>,
    pub snapshot: &'static UnsafeCell<ResourceCell>,
}

unsafe impl Sync for TimeCache {}
unsafe impl Send for TimeCache {}
impl UnwindSafe for TimeCache {}
impl RefUnwindSafe for TimeCache {}

impl TimeCache {
    pub(crate) fn new() -> Self {
        let time = UnsafeCell::new(ResourceCell::new(<Time<()>>::register()));
        let real = UnsafeCell::new(ResourceCell::new(<Time<Real>>::register()));
        let virt = UnsafeCell::new(ResourceCell::new(<Time<Virtual>>::register()));
        let fixed = UnsafeCell::new(ResourceCell::new(<Time<Fixed>>::register()));
        let state = UnsafeCell::new(ResourceCell::new(<TimeState>::register()));
        let snapshot = UnsafeCell::new(ResourceCell::new(<TimeSnapshot>::register()));
        unsafe {
            Self {
                time: Global::alloc_unchecked(time),
                real: Global::alloc_unchecked(real),
                virt: Global::alloc_unchecked(virt),
                fixed: Global::alloc_unchecked(fixed),
                state: Global::alloc_unchecked(state),
                snapshot: Global::alloc_unchecked(snapshot),
            }
        }
    }

    pub(crate) fn apply(&self, resources: &mut Resources) {
        let mut valid: bool = true;
        valid &= resources.insert(TypeId::of::<Time<()>>(), self.time);
        valid &= resources.insert(TypeId::of::<Time<Real>>(), self.real);
        valid &= resources.insert(TypeId::of::<Time<Fixed>>(), self.fixed);
        valid &= resources.insert(TypeId::of::<Time<Virtual>>(), self.virt);
        valid &= resources.insert(TypeId::of::<TimeState>(), self.state);
        valid &= resources.insert(TypeId::of::<TimeSnapshot>(), self.snapshot);
        assert!(valid);
    }
}

macro_rules! impl_getter {
    ($func:ident, $ty:ty, $field:ident) => {
        #[inline]
        pub fn $func(&self) -> Option<&'_ $ty> {
            let cell = unsafe { &mut *self.$field.get() };
            debug_assert_eq!(cell.database().type_id, TypeId::of::<$ty>());
            let untyped = cell.get_data()?;
            unsafe { Some(untyped.deref::<$ty>()) }
        }
    };
}

macro_rules! impl_getter_mut {
    ($func:ident, $ty:ty, $field:ident) => {
        #[inline]
        pub fn $func<const INIT: bool>(
            &mut self,
            last_run: Tick,
            this_run: Tick,
        ) -> ResMut<'_, $ty> {
            let cell = unsafe { &mut *self.$field.get() };
            debug_assert_eq!(cell.database().type_id, TypeId::of::<$ty>());

            #[cold]
            #[inline(never)]
            fn inner<'a>(
                cell: &UnsafeCell<ResourceCell>,
                last_run: Tick,
                this_run: Tick,
            ) -> ResMut<'a, $ty> {
                let cell = unsafe { &mut *cell.get() };
                unsafe {
                    cell.insert(<$ty>::default(), this_run);
                    cell.get_mut(last_run, this_run)
                        .debug_checked_unwrap()
                        .into_resource::<$ty>()
                }
            }

            unsafe {
                match cell.get_mut(last_run, this_run) {
                    Some(untyped) => untyped.into_resource::<$ty>(),
                    None => {
                        if INIT {
                            inner(self.$field, last_run, this_run)
                        } else {
                            const NAME: &str = stringify!($ty);
                            panic!("Missing resource `{}`, has it been removed?", NAME)
                        }
                    }
                }
            }
        }
    };
}

impl TimeCache {
    impl_getter!(time, Time, time);
    impl_getter!(real_time, Time<Real>, real);
    impl_getter!(fixed_time, Time<Fixed>, fixed);
    impl_getter!(virtual_time, Time<Virtual>, virt);
    impl_getter!(state, TimeState, state);
    impl_getter!(snapshot, TimeSnapshot, snapshot);

    impl_getter_mut!(time_mut, Time<()>, time);
    impl_getter_mut!(real_time_mut, Time<Real>, real);
    impl_getter_mut!(fixed_time_mut, Time<Fixed>, fixed);
    impl_getter_mut!(virtual_time_mut, Time<Virtual>, virt);
    impl_getter_mut!(state_mut, TimeState, state);
    impl_getter_mut!(snapshot_mut, TimeSnapshot, snapshot);
}
