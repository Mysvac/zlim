//! System identifiers, scheduling flags, and per-system metadata.

use core::any::TypeId;
use core::fmt::{Debug, Display};
use core::hash::{Hash, Hasher};

use bitflags::bitflags;
use zlim_utils::debug::DebugName;

use crate::tick::Tick;

// -----------------------------------------------------------------------------
// SystemId

/// A unique, stable identifier for a system.
///
/// `SystemId` is derived from the compile-time identity of the value the
/// system was built from (typically a function item type), so systems built
/// from the same source value always share an id.  It is `Copy`, `Eq` and
/// `Hash`, and compares by the underlying [`TypeId`].
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// fn my_system() {}
///
/// // The same source value always produces the same id.
/// assert_eq!(my_system.system_id(), my_system.system_id());
///
/// let mut world = World::alloc();
/// let handle = world.insert_system(my_system);
/// assert_eq!(handle.id(), my_system.system_id());
/// ```
#[derive(Clone, Copy)]
pub struct SystemId {
    type_id: TypeId,
    name: DebugName,
}

impl PartialEq for SystemId {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        self.type_id == other.type_id
    }
}

impl Eq for SystemId {}

impl Hash for SystemId {
    #[inline(always)]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.type_id.hash(state);
    }
}

impl SystemId {
    /// Builds a [`SystemId`] derived from the compile-time identity of `T`,
    /// using its [`TypeId`] and type name.
    #[inline(always)]
    pub fn of<T: 'static>() -> Self {
        Self {
            name: DebugName::type_name::<T>(),
            type_id: TypeId::of::<T>(),
        }
    }

    /// Returns the [`TypeId`] identifying the system's underlying type.
    #[inline(always)]
    pub fn type_id(&self) -> TypeId {
        self.type_id
    }

    /// Returns the human-readable name of the system's type.
    #[inline(always)]
    pub fn debug_name(&self) -> DebugName {
        self.name
    }
}

impl Debug for SystemId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}#{:?}", self.name, self.type_id)
    }
}

impl Display for SystemId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}#{:?}", self.name, self.type_id)
    }
}

// -----------------------------------------------------------------------------
// SystemFlags

bitflags! {
    /// Bitflags describing a system's scheduling requirements.
    #[derive(Clone, Copy, PartialEq, Eq, Hash)]
    pub struct SystemFlags: u8 {
        /// Set if the system does not need to run (a placeholder).
        const NO_OP = 1 << 0;
        /// Set if the system needs to apply deferred commands.
        const DEFERRED = 1 << 1;
        /// Set if the system is thread-affine and cannot be sent across threads.
        const NON_SEND = 1 << 2;
        /// Set if the system requires exclusive `World` access.
        const EXCLUSIVE = 1 << 3;
    }
}

// -----------------------------------------------------------------------------
// SystemMeta

/// Per-system runtime metadata: identity, scheduling flags, and
/// change-detection tick bookkeeping.
#[derive(Clone)]
pub struct SystemMeta {
    pub(crate) id: SystemId,
    pub(crate) flags: SystemFlags,
    pub(crate) last_run: Tick,
}

impl Debug for SystemMeta {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SystemMeta")
            .field("id", &self.id)
            .field("last_run", &self.last_run)
            .field("deferred", &self.is_deferred())
            .field("non_send", &self.is_non_send())
            .field("exclusive", &self.is_exclusive())
            .finish()
    }
}

impl SystemMeta {
    /// Creates metadata for a system whose identity derives from `T`, with all
    /// flags cleared and `last_run` at tick zero.
    #[inline]
    pub fn new<T: 'static>() -> Self {
        Self {
            id: SystemId::of::<T>(),
            flags: SystemFlags::empty(),
            last_run: Tick::new(0),
        }
    }

    /// Returns the system's [`SystemId`].
    #[inline(always)]
    pub fn id(&self) -> SystemId {
        self.id
    }

    /// Returns the system's [`SystemFlags`].
    #[inline(always)]
    pub fn flags(&self) -> SystemFlags {
        self.flags
    }

    /// Returns the tick when the system last completed a run.
    #[inline(always)]
    pub fn last_run(&self) -> Tick {
        self.last_run
    }

    /// Sets the tick when the system last completed a run.
    #[inline(always)]
    pub fn set_last_run(&mut self, last_run: Tick) {
        self.last_run = last_run;
    }

    /// Clamps the recorded `last_run` tick to at most `now`.
    #[inline(always)]
    pub fn clamp_tick(&mut self, now: Tick) {
        self.last_run.clamp_with(now);
    }

    /// Returns whether the system is a no-op placeholder.
    #[inline(always)]
    pub fn is_no_op(&self) -> bool {
        self.flags.intersects(SystemFlags::NO_OP)
    }

    /// Returns whether the system requires deferred effects.
    #[inline(always)]
    pub fn is_deferred(&self) -> bool {
        self.flags.intersects(SystemFlags::DEFERRED)
    }

    /// Returns whether the system must stay on one thread.
    #[inline(always)]
    pub fn is_non_send(&self) -> bool {
        self.flags.intersects(SystemFlags::NON_SEND)
    }

    /// Returns whether the system requires exclusive world access.
    #[inline(always)]
    pub fn is_exclusive(&self) -> bool {
        self.flags.intersects(SystemFlags::EXCLUSIVE)
    }

    /// Marks the system as a no-op placeholder.
    #[inline(always)]
    pub fn set_no_op(&mut self) {
        self.flags = self.flags.union(SystemFlags::NO_OP);
    }

    /// Marks the system as requiring deferred effects.
    #[inline(always)]
    pub fn set_deferred(&mut self) {
        self.flags = self.flags.union(SystemFlags::DEFERRED);
    }

    /// Marks the system as thread-affine.
    #[inline(always)]
    pub fn set_non_send(&mut self) {
        self.flags = self.flags.union(SystemFlags::NON_SEND);
    }

    /// Marks the system as requiring exclusive world access.
    #[inline(always)]
    pub fn set_exclusive(&mut self) {
        self.flags = self.flags.union(SystemFlags::EXCLUSIVE);
    }
}

// -----------------------------------------------------------------------------
