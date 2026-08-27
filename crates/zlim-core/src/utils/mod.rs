//! Internal utilities shared across the ECS core.

mod debug_unwrap;
mod dropper;
mod forgetter;
mod helper;
mod ident;
mod ident_pool;

pub(crate) use debug_unwrap::DebugCheckedUnwrap;
pub(crate) use forgetter::ForgetEntityOnPanic;
pub(crate) use helper::*;
pub(crate) use ident::define_ident;

pub use dropper::Dropper;
pub use ident_pool::SlicePool;
