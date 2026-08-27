//! Hierarchy query data: fetching an entity's parent or children during
//! iteration.

use core::ops::Deref;

use super::{QueryData, ReadOnlyQueryData};
use crate::entity::{Entities, EntityId};
use crate::system::{ComponentAccess, FilterParamBuilder};
use crate::table::{Table, TableRow};
use crate::tick::Tick;
use crate::world::{World, WorldCell};

// -----------------------------------------------------------------------------
// Parent

/// Query data that fetches the **parent** of each matched entity.
///
/// Yields `Some(parent_id)` when the entity has a parent, and `None` when it
/// is a root entity.  Dereferences to `Option<EntityId>`.
///
/// The parent relation lives in the [`Entities`] hierarchy rather than in a
/// component, so [`Parent`] performs no component access and can be freely
/// combined with any other query data or filter.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_reflect::derive::TypePath;
///
/// #[derive(TypePath, Component, Clone)]
/// struct Node;
///
/// let mut world = World::alloc();
/// let mut root = world.spawn(Node, None);
/// let root_id = root.id();
/// root.with_child(Node).unwrap();
/// let child = world.entity(root_id).children()[0];
///
/// // Iterating `(EntityId, Parent)` gives each entity together with its
/// // parent:
/// let root_is_parent = world
///     .query::<(EntityId, Parent), ()>()
///     .iter()
///     .any(|(id, parent)| id == child && parent.0 == Some(root_id));
/// assert!(root_is_parent);
///
/// // Root entities report `None`:
/// let root_parent = world.query::<Parent, ()>().get(root_id).unwrap();
/// assert_eq!(root_parent.0, None);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parent(pub Option<EntityId>);

impl Deref for Parent {
    type Target = Option<EntityId>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

unsafe impl QueryData for Parent {
    type ReadOnly = Self;
    type State = ();
    type Cache<'world> = &'world Entities;
    type Item<'world> = Parent;

    #[inline(always)]
    fn build_state(_world: &World) -> Self::State {}

    #[inline(always)]
    unsafe fn build_cache<'w>(
        _: &Self::State,
        w: WorldCell<'w>,
        _: Tick,
        _: Tick,
    ) -> Self::Cache<'w> {
        unsafe { &w.read_only().entities }
    }

    #[inline(always)]
    fn register_filter(_state: &Self::State, _out: &mut Vec<FilterParamBuilder>) {}

    #[inline(always)]
    fn register_access(_state: &Self::State, _out: &mut ComponentAccess) -> bool {
        true // We did not access any components
    }

    #[inline(always)]
    unsafe fn update_table<'w>(_: &Self::State, _: &mut Self::Cache<'w>, _: &'w mut Table) {}

    unsafe fn fetch<'w>(
        _state: &Self::State,
        cache: &mut Self::Cache<'w>,
        entity: EntityId,
        _table_row: TableRow,
    ) -> Option<Self::Item<'w>> {
        const MSG: &str = "the input entity of `Query::fetch` should exists";
        Some(Self(cache.get(entity).expect(MSG).parent))
    }
}

unsafe impl ReadOnlyQueryData for Parent {}

// -----------------------------------------------------------------------------
// Children

/// Query data that fetches the **direct children** of each matched entity.
///
/// Yields the entity's direct children as `&[EntityId]`, ordered by
/// insertion.  Dereferences to `[EntityId]`.
///
/// Like [`Parent`], the child list lives in the [`Entities`] hierarchy and
/// performs no component access, so it can be freely combined with any other
/// query data or filter.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_reflect::derive::TypePath;
///
/// #[derive(TypePath, Component, Clone)]
/// struct Node;
///
/// let mut world = World::alloc();
/// let mut root = world.spawn(Node, None);
/// let root_id = root.id();
/// root.with_child(Node).unwrap();
/// let child = world.entity(root_id).children()[0];
///
/// // `Children` yields the whole child list of the matched entity:
/// let children = world.query::<Children<'_>, ()>().get(root_id).unwrap();
/// assert_eq!(children.0, &[child]);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Children<'w>(pub &'w [EntityId]);

impl Deref for Children<'_> {
    type Target = [EntityId];

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

unsafe impl QueryData for Children<'_> {
    type ReadOnly = Self;
    type State = ();
    type Cache<'world> = &'world Entities;
    type Item<'world> = Children<'world>;

    #[inline(always)]
    fn build_state(_world: &World) -> Self::State {}

    #[inline(always)]
    unsafe fn build_cache<'w>(
        _: &Self::State,
        w: WorldCell<'w>,
        _: Tick,
        _: Tick,
    ) -> Self::Cache<'w> {
        unsafe { &w.read_only().entities }
    }

    #[inline(always)]
    fn register_filter(_state: &Self::State, _out: &mut Vec<FilterParamBuilder>) {}

    #[inline(always)]
    fn register_access(_state: &Self::State, _out: &mut ComponentAccess) -> bool {
        true // We did not access any components
    }

    #[inline(always)]
    unsafe fn update_table<'w>(_: &Self::State, _: &mut Self::Cache<'w>, _: &'w mut Table) {}

    unsafe fn fetch<'w>(
        _state: &Self::State,
        cache: &mut Self::Cache<'w>,
        entity: EntityId,
        _table_row: TableRow,
    ) -> Option<Self::Item<'w>> {
        const MSG: &str = "the input entity of `Query::fetch` should exists";
        Some(Children(&cache.get(entity).expect(MSG).children))
    }
}

unsafe impl ReadOnlyQueryData for Children<'_> {}
