//! Canonical query-filter descriptors ([`FilterParam`]).

use core::fmt::{Debug, Display};
use core::hash::{Hash, Hasher};
use std::collections::BTreeSet;

use zlim_utils::hash::FixedState;

use super::StringFmt;
use crate::component::{ComponentDB, ComponentId};

// -----------------------------------------------------------------------------
// FilterParam

/// Builder for canonical query filter descriptors used by access analysis.
///
/// `FilterParamBuilder` normalizes user query filter constraints into a stable
/// representation so identical logical filters map to the same scheduler key.
///
/// # Invariants
///
/// - `with` and `without` sets are individually ordered (`BTreeSet`).
/// - A valid build requires `with ∩ without = empty`.
/// - Built output is deterministic for equivalent filter expressions.
#[derive(Debug, Default, Clone)]
pub struct FilterParamBuilder {
    // We use BTreeSet to ensure it's ordering.
    with: BTreeSet<ComponentId>,
    without: BTreeSet<ComponentId>,
}

impl FilterParamBuilder {
    /// Creates an empty filter builder with no constraints.
    pub const fn new() -> Self {
        Self {
            with: BTreeSet::new(),
            without: BTreeSet::new(),
        }
    }

    /// Adds a required component to the positive filter set.
    pub fn with(&mut self, id: ComponentId) {
        self.with.insert(id);
    }

    /// Adds a forbidden component to the negative filter set.
    pub fn without(&mut self, id: ComponentId) {
        self.without.insert(id);
    }

    /// Merges two filter builders when constraints are logically compatible.
    ///
    /// Returns `None` when constraints are contradictory, for example:
    /// - left requires `A` while right forbids `A`, or
    /// - right requires `B` while left forbids `B`.
    pub fn merge(&self, other: &Self) -> Option<FilterParamBuilder> {
        if self.with.is_disjoint(&other.without) && other.with.is_disjoint(&self.without) {
            let mut with = self.with.clone();
            with.extend(&other.with);
            let mut without = self.without.clone();
            without.extend(&other.without);
            Some(FilterParamBuilder { with, without })
        } else {
            None
        }
    }

    /// Builds an immutable, hash-stable [`FilterParam`].
    ///
    /// Returns `None` when contradictory constraints exist in one builder.
    pub fn build(self) -> Option<FilterParam> {
        use crate::utils::SlicePool;

        if self.with.is_disjoint(&self.without) {
            let with_len = self.with.len();
            let without_len = self.without.len();
            // ComponentId <= u32::MAX, ↓ length overflow is impossible
            let mut vec = Vec::with_capacity(with_len + without_len);
            vec.extend(self.with);
            vec.extend(self.without);

            let params = SlicePool::component(&vec);

            let mut hasher = FixedState::HASHER;
            with_len.hash(&mut hasher);
            params.hash(&mut hasher);
            let hash = hasher.finish();

            Some(FilterParam {
                hash,
                with_len,
                params,
            })
        } else {
            None
        }
    }
}

/// Canonical, hashable representation of component filter requirements.
///
/// A `FilterParam` is used as a bucketing key in [`AccessTable`](super::AccessTable).
/// Systems only need component-level conflict checks when their filter keys are
/// not provably disjoint.
#[derive(Clone, PartialEq, Eq)]
pub struct FilterParam {
    hash: u64,
    with_len: usize,
    params: &'static [ComponentId],
}

impl FilterParam {
    /// Components that must be present.
    pub fn with(&self) -> &[ComponentId] {
        &self.params[..self.with_len]
    }

    /// Components that must be absent.
    pub fn without(&self) -> &[ComponentId] {
        &self.params[self.with_len..]
    }

    /// Returns whether two filters describe disjoint entity sets.
    ///
    /// If this returns `true`, scheduler-level access checks may treat matching
    /// query branches as non-overlapping even when they access the same
    /// component ids mutably.
    pub fn is_disjoint(&self, other: &Self) -> bool {
        use crate::utils::contains_component;

        let x_without = self.without();
        let y_without = other.without();
        let x_with = self.with();
        let y_with = other.with();

        // Although the slice is sorted, we assume the params
        // is usually small, so the `contains` is faster.
        x_without.iter().any(|&id| contains_component(id, y_with))
            || y_without.iter().any(|&id| contains_component(id, x_with))
    }
}

impl Hash for FilterParam {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}

impl Debug for FilterParam {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FilterParam")
            .field("with", &self.with())
            .field("without", &self.without())
            .finish()
    }
}

impl FilterParam {
    /// Creates a human-readable description of this filter.
    #[inline(never)]
    pub fn display(&self) -> impl Display {
        fn format_component(iter: &[ComponentId]) -> String {
            let mut msg = String::new();
            let mut is_first = true;
            for &id in iter {
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

        let mut msg = String::new();
        let with = format_component(self.with());
        let without = format_component(self.without());
        msg.push_str("Filter { ");
        msg.push_str(&format!("With: [{with}], "));
        msg.push_str(&format!("Without: [{without}] "));
        msg.push_str("} ");
        StringFmt(msg)
    }
}
