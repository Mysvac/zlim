//! Per-component access classification ([`ComponentAccess`]).

use core::fmt::{Debug, Display, Formatter};

use fixedbitset::FixedBitSet;

use super::{BitSetFmt, StringFmt};
use crate::component::{ComponentDB, ComponentId};

/// Component-level access summary for one logical query path.
#[derive(Clone)]
pub enum ComponentAccess {
    /// Exclusive access to a whole entity (equivalent to `&mut Entity`).
    EntityMut,
    /// Shared access to a whole entity (equivalent to `&Entity`).
    EntityRef,
    /// Component-level read/write sets, without whole-entity access.
    Components {
        /// Component ids being read.
        reading: FixedBitSet,
        /// Component ids being written.
        writing: FixedBitSet,
    },
}

impl Debug for ComponentAccess {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EntityMut => f.write_str("EntityMut"),
            Self::EntityRef => f.write_str("EntityRef"),
            Self::Components { reading, writing } => f
                .debug_struct("Components")
                .field("reading", &BitSetFmt(reading))
                .field("writing", &BitSetFmt(writing))
                .finish(),
        }
    }
}

impl Default for ComponentAccess {
    fn default() -> Self {
        Self::Components {
            reading: FixedBitSet::new(),
            writing: FixedBitSet::new(),
        }
    }
}

impl ComponentAccess {
    /// Creates an empty access summary.
    pub const fn new() -> Self {
        Self::Components {
            reading: FixedBitSet::new(),
            writing: FixedBitSet::new(),
        }
    }
}

impl ComponentAccess {
    /// Declares shared-entity access.
    #[must_use]
    #[inline(never)]
    pub fn set_entity_ref(&mut self) -> bool {
        match self {
            Self::EntityMut => false,
            Self::EntityRef => true,
            Self::Components { writing, .. } => {
                if writing.is_clear() {
                    *self = Self::EntityRef;
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Declares exclusive-entity access.
    #[must_use]
    pub fn set_entity_mut(&mut self) -> bool {
        match self {
            Self::EntityMut => false,
            Self::EntityRef => true,
            Self::Components { reading, .. } => {
                if reading.is_clear() {
                    *self = Self::EntityMut;
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Declares component read access.
    #[must_use]
    pub fn set_reading(&mut self, id: ComponentId) -> bool {
        match self {
            Self::EntityMut => false,
            Self::EntityRef => true,
            Self::Components {
                reading, writing, ..
            } => {
                if writing.contains(id.index()) {
                    false
                } else {
                    reading.grow_and_insert(id.index());
                    true
                }
            }
        }
    }

    /// Declares component read access without conflict reporting.
    ///
    /// Used by read-only query filters (e.g. `Added<T>` / `Changed<T>`) that
    /// need change-tick metadata reads not covered by the query's data
    /// access.  Tick reads never conflict with component data access, so this
    /// registers a read even when the component is also written.
    pub fn force_reading(&mut self, id: ComponentId) {
        if let Self::Components { reading, .. } = self {
            reading.grow_and_insert(id.index());
        }
    }

    /// Declares component write access.
    #[must_use]
    pub fn set_writing(&mut self, id: ComponentId) -> bool {
        match self {
            Self::EntityMut => false,
            Self::EntityRef => false,
            Self::Components {
                reading, writing, ..
            } => {
                if reading.contains(id.index()) {
                    false
                } else {
                    reading.grow_and_insert(id.index());
                    writing.grow_and_insert(id.index());
                    true
                }
            }
        }
    }

    /// Force-declares exclusive-entity access, discarding any component sets.
    pub fn merge_entity_mut(&mut self) {
        *self = Self::EntityMut;
    }

    /// Force-declares shared-entity access, widening to exclusive if components
    /// are being written.
    pub fn merge_entity_ref(&mut self) {
        match self {
            Self::EntityMut | Self::EntityRef => (),
            Self::Components { writing, .. } => {
                if writing.is_clear() {
                    *self = Self::EntityRef;
                } else {
                    *self = Self::EntityMut;
                }
            }
        }
    }

    /// Force-declares component read access for `id`.
    pub fn merge_reading(&mut self, id: ComponentId) {
        match self {
            Self::EntityMut | Self::EntityRef => (),
            Self::Components { reading, .. } => reading.grow_and_insert(id.index()),
        }
    }

    /// Force-declares component write access for `id`.
    pub fn merge_writing(&mut self, id: ComponentId) {
        match self {
            Self::EntityMut => (),
            Self::EntityRef => *self = Self::EntityMut,
            Self::Components { reading, writing } => {
                reading.grow_and_insert(id.index());
                writing.grow_and_insert(id.index());
            }
        }
    }

    /// Merges `other`'s access into `self` without conflict checking.
    pub fn merge_with(&mut self, other: &Self) {
        match (&mut *self, other) {
            (Self::EntityMut, _) => (),
            (_, Self::EntityMut) => (),
            (Self::EntityRef, Self::EntityRef) => (),
            (Self::EntityRef, Self::Components { writing, .. }) => {
                if !writing.is_clear() {
                    *self = Self::EntityMut
                }
            }
            (Self::Components { writing, .. }, Self::EntityRef) => {
                if writing.is_clear() {
                    *self = Self::EntityRef
                } else {
                    *self = Self::EntityMut
                }
            }
            (
                Self::Components {
                    reading: r1,
                    writing: w1,
                },
                Self::Components {
                    reading: r2,
                    writing: w2,
                },
            ) => {
                r1.union_with(r2);
                w1.union_with(w2);
            }
        }
    }
}

impl ComponentAccess {
    /// Registers exclusive mutable entity access.
    ///
    /// Returns whether the access was accepted without conflict.
    #[inline(never)]
    pub fn register_entity_mut(&mut self) -> bool {
        let ok = self.set_entity_mut();
        if !ok {
            ::core::hint::cold_path();
            self.merge_entity_mut();
        }
        ok
    }

    /// Registers fully-readonly entity access.
    ///
    /// Returns whether the access was accepted without conflict.
    #[inline(never)]
    pub fn register_entity_ref(&mut self) -> bool {
        let ok = self.set_entity_ref();
        if !ok {
            ::core::hint::cold_path();
            self.merge_entity_ref();
        }
        ok
    }

    /// Registers read access to component `id`.
    ///
    /// Returns whether the access was accepted without conflict.
    #[inline(never)]
    pub fn register_reading(&mut self, id: ComponentId) -> bool {
        let ok = self.set_reading(id);
        if !ok {
            ::core::hint::cold_path();
            self.merge_reading(id);
        }
        ok
    }

    /// Registers write access to component `id`.
    ///
    /// Returns whether the access was accepted without conflict.
    #[inline(never)]
    pub fn register_writing(&mut self, id: ComponentId) -> bool {
        let ok = self.set_writing(id);
        if !ok {
            ::core::hint::cold_path();
            self.merge_writing(id);
        }
        ok
    }
}

impl ComponentAccess {
    /// Returns whether this access performs no writes.
    #[must_use]
    pub fn is_readonly(&self) -> bool {
        match self {
            Self::EntityMut => false,
            Self::EntityRef => true,
            Self::Components { writing, .. } => writing.is_clear(),
        }
    }

    /// Returns whether this access can run in parallel with `other`.
    ///
    /// Only meaningful when both access entries have been validated.
    #[inline]
    #[must_use]
    pub fn parallelizable(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::EntityMut, _) => false,
            (_, Self::EntityMut) => false,
            (Self::EntityRef, Self::EntityRef) => true,
            (Self::EntityRef, Self::Components { writing, .. }) => writing.is_clear(),
            (Self::Components { writing, .. }, Self::EntityRef) => writing.is_clear(),
            (
                Self::Components {
                    reading: r1,
                    writing: w1,
                },
                Self::Components {
                    reading: r2,
                    writing: w2,
                },
            ) => w1.is_disjoint(r2) && w2.is_disjoint(r1),
        }
    }
}

impl ComponentAccess {
    /// Creates a human-readable description of this access.
    #[inline(never)]
    pub fn display(&self) -> impl Display {
        fn format_component(iter: impl Iterator<Item = usize>) -> String {
            let mut msg = String::new();
            let mut is_first = true;
            for index in iter {
                let id = ComponentId::without_provenance(index);
                let name = ComponentDB::get_by_id(id).type_name;
                if is_first {
                    msg.push_str(&format!("{}#{}", name, id));
                    is_first = false;
                } else {
                    msg.push_str(&format!(", {}#{}", name, id));
                }
            }
            msg
        }

        match self {
            Self::EntityMut => StringFmt(String::from("EntityMut")),
            Self::EntityRef => StringFmt(String::from("EntityRef")),
            Self::Components { reading, writing } => {
                let mut msg = String::new();
                msg.push_str("Components { ");
                msg.push_str("reading: [");
                msg.push_str(&format_component(reading.ones()));
                msg.push_str("], ");
                msg.push_str("writing: [");
                msg.push_str(&format_component(writing.ones()));
                msg.push_str("] }");
                StringFmt(msg)
            }
        }
    }
}
