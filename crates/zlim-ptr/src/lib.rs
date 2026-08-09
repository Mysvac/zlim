#![doc = include_str!("../README.md")]
#![expect(unsafe_code, reason = "Raw pointers are inherently unsafe.")]
#![no_std]

// -----------------------------------------------------------------------------
// Modules

mod ptr;
mod slice;

// -----------------------------------------------------------------------------
// Top-level exports

pub use crate::ptr::{OwningPtr, Ptr, PtrMut};
pub use crate::slice::{Slice, SliceMut};
