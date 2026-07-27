#![expect(unsafe_code, reason = "Raw pointers are inherently unsafe.")]

use core::mem::ManuallyDrop;
use core::sync::atomic::{AtomicUsize, Ordering};

use zlim_ptr::{OwningPtr, Ptr, PtrMut};

macro_rules! define_dc {
    ($t:ident, $s:ident) => {
        static $s: AtomicUsize = AtomicUsize::new(0);

        struct $t;

        impl Drop for $t {
            fn drop(&mut self) {
                $s.fetch_add(1, Ordering::SeqCst);
            }
        }

        $s.store(0, Ordering::SeqCst);
    };
}

#[test]
fn ptr_deref() {
    let value = 12_i32;
    let ptr = Ptr::from_ref(&value);

    ptr.debug_assert_aligned::<i32>();
    let read_back = unsafe { ptr.deref::<i32>() };
    assert_eq!(*read_back, 12);
}

#[test]
fn ptrmut_rw() {
    let mut value = 10_i32;
    let mut ptr = PtrMut::from_mut(&mut value);

    ptr.debug_assert_aligned::<i32>();
    let mut sub = ptr.reborrow();
    let slot = unsafe { sub.as_mut::<i32>() };
    *slot += 5;

    let read_back = unsafe { ptr.as_ref::<i32>() };
    assert_eq!(*read_back, 15);
}

#[test]
fn own_read() {
    let mut value = ManuallyDrop::new(21_i32);
    let ptr = OwningPtr::from_value(&mut value);
    let out = unsafe { ptr.read::<i32>() };
    assert_eq!(out, 21);
}

#[test]
fn own_drop_once() {
    define_dc!(DC, COUNTER);

    let mut value = ManuallyDrop::new(DC);
    let ptr = OwningPtr::from_value(&mut value);
    unsafe { ptr.drop_as::<DC>() };

    assert_eq!(COUNTER.load(Ordering::SeqCst), 1);
}

#[test]
fn own_read_then_drop() {
    define_dc!(DC, COUNTER);

    let mut value = ManuallyDrop::new(DC);
    let ptr = OwningPtr::from_value(&mut value);
    let out = unsafe { ptr.read::<DC>() };

    // `read` transfers ownership without dropping in place.
    assert_eq!(COUNTER.load(Ordering::SeqCst), 0);
    drop(out);
    assert_eq!(COUNTER.load(Ordering::SeqCst), 1);
}

#[test]
fn own_make_drop() {
    define_dc!(DC, COUNTER);

    OwningPtr::make(DC, |ptr| unsafe {
        ptr.drop_as::<DC>();
    });

    assert_eq!(COUNTER.load(Ordering::SeqCst), 1);
}

#[test]
fn own_macro() {
    let value = 7_i32;

    zlim_ptr::into_owning!(value as ptr);
    let out = unsafe { ptr.read::<i32>() };

    assert_eq!(out, 7);
}

#[test]
fn own_macro_drop() {
    define_dc!(DC, COUNTER);

    {
        let value = DC;
        {
            zlim_ptr::into_owning!(value as ptr);
            assert_eq!(COUNTER.load(Ordering::SeqCst), 0);
            unsafe { ptr.drop_as::<DC>() };
        }
    }

    assert_eq!(COUNTER.load(Ordering::SeqCst), 1);
}
