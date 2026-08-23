//! Access declaration and conflict analysis model for ECS systems.
//!
//! # Design pattern overview
//!
//! The access model is intentionally split into three layers:
//! 1. [`ComponentAccess`]: fine-grained component access for one logical query path.
//! 2. [`FilterParam`]: a canonical key describing query `with` / `without` constraints.
//! 3. [`AccessTable`]: full per-system access summary consumed by scheduler conflict checks.
//!
//! This layering keeps conflict analysis both strict and practical:
//! - component-level conflicts are tracked with explicit read/write sets,
//! - query filter disjointness reduces false-positive conflicts,
//! - world/resource/query accesses are merged into one schedulable table.

// -----------------------------------------------------------------------------
// Modules & Exports

mod data;
mod filter;
mod table;

pub use data::ComponentAccess;
pub use filter::{FilterParam, FilterParamBuilder};
pub use table::AccessTable;

// -----------------------------------------------------------------------------
// Internal Helper

struct BitSetFmt<'a>(&'a fixedbitset::FixedBitSet);

impl core::fmt::Debug for BitSetFmt<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_list().entries(self.0.ones()).finish()
    }
}

struct StringFmt(String);

impl core::fmt::Display for StringFmt {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

// -----------------------------------------------------------------------------
