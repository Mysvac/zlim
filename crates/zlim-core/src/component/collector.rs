//! The [`ComponentCollector`] used to collect component IDs during bundle
//! registration.

use std::collections::BTreeSet;

use super::{Component, Components};
use super::{ComponentDB, ComponentId};
use crate::utils::SlicePool;

/// Collects component type IDs during bundle registration.
///
/// `ComponentCollector` is used by [`Bundle::collect_explicit`] /
/// [`Bundle::collect_required`] to register every component type that a
/// bundle needs.  After collection, the sorted component ID list determines
/// the target archetype for entity spawning.
///
/// # Static vs. world-bound collection
///
/// When `components` is `Some(&Components)`, component types are
/// registered into the specific world's component registry.  When `None`,
/// only the process-global `ComponentDB` registry is used — this is
/// appropriate for static analysis or early validation.
///
/// [`Bundle::collect_explicit`]: crate::bundle::Bundle::collect_explicit
/// [`Bundle::collect_required`]: crate::bundle::Bundle::collect_required
pub struct ComponentCollector<'a> {
    components: Option<&'a Components>,
    collected: BTreeSet<ComponentId>,
}

impl<'a> ComponentCollector<'a> {
    /// Creates a new collector.
    ///
    /// Pass `Some(world.components())` to register types into a specific
    /// world, or `None` to use only the global registry.
    #[inline(always)]
    pub fn new(components: Option<&'a Components>) -> Self {
        ComponentCollector {
            components,
            collected: BTreeSet::new(),
        }
    }

    /// Insert a [`ComponentId`] explicitly.
    #[inline(always)]
    pub fn insert(&mut self, id: ComponentId) {
        self.collected.insert(id);
    }

    /// Collects a component type, registering it if necessary, **without**
    /// following its required components.
    ///
    /// If a world-bound `Components` registry was provided at construction,
    /// the type is registered into that world.  Otherwise, only the global
    /// `ComponentDB` is consulted.
    ///
    /// This method is marked `#[inline(never)]` to reduce code bloat — it
    /// is called once per component type per bundle variant, which is cold
    /// relative to the hot spawn path.
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_core::component::ComponentCollector;
    ///
    /// #[derive(TypePath, Component, Clone)]
    /// struct Position;
    ///
    /// let mut collector = ComponentCollector::new(None);
    ///
    /// collector.collect_explicit::<Position>();
    ///
    /// let ids = collector.finish();
    ///
    /// assert_eq!(ids, &[ComponentDB::of::<Position>().id]);
    /// ```
    #[inline(never)]
    pub fn collect_explicit<C: Component>(&mut self) {
        if let Some(c) = &mut self.components {
            self.collected.insert(c.get::<C>().id);
        } else {
            self.collected.insert(ComponentDB::of::<C>().id);
        }
    }

    /// Collects a component type **and** its required components, recursively.
    ///
    /// Registers the type if necessary, then follows its required
    /// components through [`Component::REQUIRED`].
    ///
    /// This method is marked `#[inline(never)]` to reduce code bloat.
    #[inline(never)]
    pub fn collect_required<C: Component>(&mut self) {
        let id = if let Some(c) = &mut self.components {
            c.get::<C>().id
        } else {
            ComponentDB::of::<C>().id
        };

        if self.collected.insert(id)
            && let Some(required) = C::REQUIRED
        {
            required.collect(self);
        }
    }

    /// Finalises collection and returns the sorted, deduplicated component
    /// ID list.
    ///
    /// The returned slice is interned via `SlicePool` and has `'static`
    /// lifetime.
    #[inline(never)]
    pub fn finish(self) -> &'static [ComponentId] {
        let buf: Vec<ComponentId> = self.collected.into_iter().collect();
        debug_assert!(buf.is_sorted());
        SlicePool::component(&buf)
    }
}
