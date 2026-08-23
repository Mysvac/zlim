//! The full per-system [`AccessTable`] used by scheduler conflict checks.

use core::fmt::{Debug, Display};

use fixedbitset::FixedBitSet;
use zlim_log as log;
use zlim_utils::hash::{HashMap, NoopState};

use super::{ComponentAccess, FilterParam, StringFmt};
use crate::resource::{ResourceDB, ResourceId};
use crate::system::access::BitSetFmt;

/// Full per-system access declaration used by scheduler conflict checks.
///
/// # Design pattern
///
/// `AccessTable` combines three access domains:
/// 1. world-level access (`&World` / `&mut World`),
/// 2. resource-level read/write sets,
/// 3. query-level component access grouped by [`FilterParam`].
///
/// Grouping query access by filter keys enables a stricter but less pessimistic
/// conflict test: mutable access to the same component may still be parallel if
/// filter constraints prove disjoint entity sets.
///
/// # Rule matrix (same table)
///
/// - world mut vs anything: conflict
/// - world ref vs world ref: compatible
/// - world ref vs resource/query write: conflict
/// - resource read vs resource read: compatible
/// - resource read vs resource write: conflict
/// - resource write vs resource write: conflict
/// - component access: compatible only when each overlapping filter bucket has
///   [`ComponentAccess::parallelizable`] access sets.
#[derive(Clone)]
pub enum AccessTable {
    /// Exclusive mutable world access (`&mut World`).
    WorldMut,
    /// Fully-readonly world access (`&World`).
    WorldRef,
    /// Resource read/write sets plus query-level component access.
    Normal {
        /// Resource ids being read.
        res_reading: FixedBitSet, // resource reading
        /// Resource ids being written.
        res_writing: FixedBitSet, // resource writing
        /// Component access grouped by [`FilterParam`] key.
        components: HashMap<FilterParam, ComponentAccess, NoopState>,
    },
}

impl Debug for AccessTable {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::WorldMut => f.write_str("WorldMut"),
            Self::WorldRef => f.write_str("WorldRef"),
            Self::Normal {
                res_reading,
                res_writing,
                components,
            } => f
                .debug_struct("AccessTable")
                .field("res_reading", &BitSetFmt(res_reading))
                .field("res_writing", &BitSetFmt(res_writing))
                .field("components", &components)
                .finish(),
        }
    }
}

impl AccessTable {
    /// Creates an empty [`AccessTable`] collection.
    pub const fn new() -> Self {
        Self::Normal {
            res_reading: FixedBitSet::new(),
            res_writing: FixedBitSet::new(),
            components: HashMap::with_hasher(NoopState),
        }
    }
}

impl Default for AccessTable {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl AccessTable {
    /// Attempts to declare exclusive mutable world access.
    ///
    /// Returns `true` if the access was accepted without conflict, `false`
    /// otherwise.
    #[inline]
    pub fn set_world_mut(&mut self) -> bool {
        match self {
            Self::WorldMut => false,
            Self::WorldRef => false,
            Self::Normal {
                res_reading,
                res_writing,
                components,
            } => {
                if components.is_empty() && res_reading.is_clear() && res_writing.is_clear() {
                    *self = Self::WorldMut;
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Attempts to declare fully-readonly world access.
    ///
    /// Returns `true` if the access was accepted without conflict, `false`
    /// otherwise.
    #[inline]
    pub fn set_world_ref(&mut self) -> bool {
        match self {
            Self::WorldMut => false,
            Self::WorldRef => true,
            Self::Normal {
                res_writing,
                components,
                ..
            } => {
                if res_writing.is_clear() && components.values().all(ComponentAccess::is_readonly) {
                    *self = Self::WorldRef;
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Attempts to declare read access to resource `id`.
    ///
    /// Returns `true` if the access was accepted without conflict, `false`
    /// otherwise.
    #[inline]
    pub fn set_reading_res(&mut self, id: ResourceId) -> bool {
        match self {
            Self::WorldMut => false,
            Self::WorldRef => true,
            Self::Normal {
                res_reading,
                res_writing,
                ..
            } => {
                if res_writing.contains(id.index()) {
                    false
                } else {
                    res_reading.grow_and_insert(id.index());
                    true
                }
            }
        }
    }

    /// Attempts to declare write access to resource `id`.
    ///
    /// Returns `true` if the access was accepted without conflict, `false`
    /// otherwise.
    #[inline]
    pub fn set_writing_res(&mut self, id: ResourceId) -> bool {
        match self {
            Self::WorldMut => false,
            Self::WorldRef => true,
            Self::Normal {
                res_reading,
                res_writing,
                ..
            } => {
                if res_reading.contains(id.index()) {
                    false
                } else {
                    res_reading.grow_and_insert(id.index());
                    res_writing.grow_and_insert(id.index());
                    true
                }
            }
        }
    }

    /// Attempts to declare `data` component access under each `filter`.
    ///
    /// Returns `true` if the access was accepted without conflict, `false`
    /// otherwise.
    #[inline]
    pub fn set_component_access(&mut self, data: &ComponentAccess, filter: &[FilterParam]) -> bool {
        match self {
            Self::WorldMut => false,
            Self::WorldRef => data.is_readonly(),
            Self::Normal { components, .. } => {
                let ok = filter.iter().all(|param| {
                    components
                        .iter()
                        .all(|(k, v)| k.is_disjoint(param) || data.parallelizable(v))
                });
                if !ok {
                    return ok;
                }

                filter.iter().for_each(|param| {
                    if let Some(item) = components.get_mut(param) {
                        item.merge_with(data);
                    } else {
                        components.insert(param.clone(), data.clone());
                    }
                });

                true
            }
        }
    }

    /// Force-declares exclusive mutable world access, discarding all other access.
    #[inline]
    pub fn merge_world_mut(&mut self) {
        *self = Self::WorldMut;
    }

    /// Force-declares world access, widening to mutable if any write exists.
    #[inline]
    pub fn merge_world_ref(&mut self) {
        match self {
            Self::WorldMut | Self::WorldRef => (),
            Self::Normal {
                res_writing,
                components,
                ..
            } => {
                if res_writing.is_clear() && components.values().all(ComponentAccess::is_readonly) {
                    *self = Self::WorldRef;
                } else {
                    *self = Self::WorldMut;
                }
            }
        }
    }

    /// Force-declares read access to resource `id`.
    #[inline]
    pub fn merge_reading_res(&mut self, id: ResourceId) {
        match self {
            Self::WorldMut | Self::WorldRef => (),
            Self::Normal { res_reading, .. } => res_reading.grow_and_insert(id.index()),
        }
    }

    /// Force-declares write access to resource `id`.
    #[inline]
    pub fn merge_writing_res(&mut self, id: ResourceId) {
        match self {
            Self::WorldMut => (),
            Self::WorldRef => *self = Self::WorldMut,
            Self::Normal {
                res_reading,
                res_writing,
                ..
            } => {
                res_reading.grow_and_insert(id.index());
                res_writing.grow_and_insert(id.index());
            }
        }
    }

    /// Force-declares `data` component access under each `filter`, resolving
    /// conflicts by widening.
    #[inline]
    pub fn merge_component_access(&mut self, data: &ComponentAccess, filter: &[FilterParam]) {
        match self {
            Self::WorldMut => (),
            Self::WorldRef => *self = Self::WorldMut,
            Self::Normal { components, .. } => {
                filter.iter().for_each(|param| {
                    if let Some(item) = components.get_mut(param) {
                        item.merge_with(data);
                    } else {
                        components.insert(param.clone(), data.clone());
                    }
                });
            }
        }
    }
}

impl AccessTable {
    /// Logs `msg` together with a human-readable dump of the current access table.
    #[cold]
    #[inline(never)]
    fn log_error(&self, msg: &str) {
        log::error!("{msg}\nCurrent AccessTable: \n{}", self.display());
    }

    /// Registers exclusive mutable world access, logging a conflict error when
    /// `strict` is `true` before force-merging.
    ///
    /// Returns whether the access was accepted without conflict.
    #[inline(never)]
    pub fn register_world_mut(&mut self, strict: bool) -> bool {
        let ok = self.set_world_mut();
        if !ok {
            ::core::hint::cold_path();
            if strict {
                self.log_error("Find a Access conflict, try set `WorldMut` (exclusive mutable).");
            }
            self.merge_world_mut();
        }
        ok
    }

    /// Registers fully-readonly world access, logging a conflict error when
    /// `strict` is `true` before force-merging.
    ///
    /// Returns whether the access was accepted without conflict.
    #[inline(never)]
    pub fn register_world_ref(&mut self, strict: bool) -> bool {
        let ok = self.set_world_ref();
        if !ok {
            ::core::hint::cold_path();
            if strict {
                self.log_error("Find a Access conflict, try set `WorldRef` (fully readonly).");
            }
            self.merge_world_ref();
        }
        ok
    }

    /// Registers read access to resource `id`, logging a conflict error when
    /// `strict` is `true` before force-merging.
    ///
    /// Returns whether the access was accepted without conflict.
    #[inline(never)]
    pub fn register_reading_res(&mut self, id: ResourceId, strict: bool) -> bool {
        let ok = self.set_reading_res(id);
        if !ok {
            ::core::hint::cold_path();
            if strict {
                let name = ResourceDB::get_by_id(id).type_name;
                self.log_error(&format!(
                    "Find a Access conflict, try read resource `{name}`."
                ));
            }
            self.merge_reading_res(id);
        }
        ok
    }

    /// Registers write access to resource `id`, logging a conflict error when
    /// `strict` is `true` before force-merging.
    ///
    /// Returns whether the access was accepted without conflict.
    #[inline(never)]
    pub fn register_writing_res(&mut self, id: ResourceId, strict: bool) -> bool {
        let ok = self.set_writing_res(id);
        if !ok {
            ::core::hint::cold_path();
            if strict {
                let name = ResourceDB::get_by_id(id).type_name;
                self.log_error(&format!(
                    "Find a Access conflict, try write resource `{name}`."
                ));
            }
            self.merge_writing_res(id);
        }
        ok
    }

    /// Registers `data` component access under each `filter`, logging a conflict
    /// error when `strict` is `true` before force-merging.
    ///
    /// Returns whether the access was accepted without conflict.
    #[inline(never)]
    pub fn register_component_access(
        &mut self,
        data: &ComponentAccess,
        filter: &[FilterParam],
        strict: bool,
    ) -> bool {
        let ok = self.set_component_access(data, filter);
        if !ok {
            ::core::hint::cold_path();
            if strict {
                let mut err = String::with_capacity(400);
                err.push_str("Find a Access conflict, try access: ");
                err.push_str(&data.display().to_string());
                err.push_str("Filters:");
                for f in filter {
                    err.push_str("\n- ");
                    err.push_str(&f.display().to_string());
                }
                err.push('\n');
                self.log_error(&err);
            }
            self.merge_component_access(data, filter);
        }
        ok
    }
}

impl AccessTable {
    /// Returns whether two full system access tables are parallel-compatible.
    ///
    /// This method is the scheduler-facing predicate used to build conflict
    /// graphs between systems.
    #[must_use]
    pub fn parallelizable(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::WorldMut, _) => false,
            (_, Self::WorldMut) => false,
            (Self::WorldRef, Self::WorldRef) => true,
            (
                Self::WorldRef,
                Self::Normal {
                    res_writing,
                    components,
                    ..
                },
            ) => res_writing.is_clear() && components.values().all(ComponentAccess::is_readonly),
            (
                Self::Normal {
                    res_writing,
                    components,
                    ..
                },
                Self::WorldRef,
            ) => res_writing.is_clear() && components.values().all(ComponentAccess::is_readonly),
            (
                Self::Normal {
                    res_reading: r1,
                    res_writing: w1,
                    components: c1,
                },
                Self::Normal {
                    res_reading: r2,
                    res_writing: w2,
                    components: c2,
                },
            ) => {
                if !w1.is_disjoint(r2) || !w2.is_disjoint(r1) {
                    return false;
                }
                c1.iter().all(|(k, v)| {
                    c2.iter()
                        .all(|(x, y)| k.is_disjoint(x) || v.parallelizable(y))
                })
            }
        }
    }

    /// Merges `other` into `self`, widening access as necessary to combine
    /// both tables.
    pub fn merge(&mut self, other: Self) {
        match (&mut *self, other) {
            (Self::WorldMut, _) => (),
            (_, Self::WorldMut) => *self = Self::WorldMut,
            (Self::WorldRef, Self::WorldRef) => (),
            (
                Self::WorldRef,
                Self::Normal {
                    res_writing,
                    components,
                    ..
                },
            ) => {
                if !res_writing.is_clear() || !components.values().all(ComponentAccess::is_readonly)
                {
                    *self = Self::WorldMut;
                }
            }
            (
                Self::Normal {
                    res_writing,
                    components,
                    ..
                },
                Self::WorldRef,
            ) => {
                if res_writing.is_clear() && components.values().all(ComponentAccess::is_readonly) {
                    *self = Self::WorldRef;
                } else {
                    *self = Self::WorldMut;
                }
            }
            (
                Self::Normal {
                    res_reading: r1,
                    res_writing: w1,
                    components: c1,
                },
                Self::Normal {
                    res_reading: r2,
                    res_writing: w2,
                    components: c2,
                },
            ) => {
                r1.union_with(&r2);
                w1.union_with(&w2);
                c2.into_iter().for_each(|(param, data)| {
                    if let Some(item) = c1.get_mut(&param) {
                        item.merge_with(&data);
                    } else {
                        c1.insert(param, data);
                    }
                });
            }
        }
    }
}

impl AccessTable {
    /// Creates a human-readable description of this access table, including
    /// the names of all read/written resources and component accesses.
    #[inline(never)]
    pub fn display(&self) -> impl Display {
        fn format_resource(iter: impl Iterator<Item = usize>) -> String {
            let mut msg = String::new();
            let mut is_first = true;
            for index in iter {
                let id = ResourceId::without_provenance(index);
                let name = ResourceDB::get_by_id(id).type_name;
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
            Self::WorldMut => StringFmt(String::from("WorldMut")),
            Self::WorldRef => StringFmt(String::from("WorldRef")),
            Self::Normal {
                res_reading,
                res_writing,
                components,
            } => {
                let mut msg = String::new();
                msg.push_str("AccessTable: {");
                msg.push_str("\n\treading_resource: [");
                msg.push_str(&format_resource(res_reading.ones()));
                msg.push_str("],");
                msg.push_str("\n\twriting_resource: [");
                msg.push_str(&format_resource(res_writing.ones()));
                msg.push_str("],");
                msg.push_str("\n\tquery: {");

                for (f, d) in components.iter() {
                    msg.push_str("\n\t\t");
                    msg.push_str(&f.display().to_string());
                    msg.push_str(": ");
                    msg.push_str(&d.display().to_string());
                }

                msg.push_str("\n\t},");
                msg.push_str("\n}\n");
                StringFmt(msg)
            }
        }
    }
}
