//! Hierarchy query data: fetching an entity's parent or children during
//! iteration.

use core::iter::FusedIterator;
use core::ops::Deref;

use zlim_ptr::ThinSlice;

use super::{QueryData, ReadOnlyQueryData};
use crate::entity::{EntityId, EntityNode};
use crate::query::QuerySlice;
use crate::system::{ComponentAccess, FilterParamBuilder};
use crate::table::{Table, TableRow};
use crate::tick::Tick;
use crate::world::{World, WorldCell};

// -----------------------------------------------------------------------------
// Parent

/// Query data that fetches the **parent** of each matched entity.
///
/// Yields `Some(parent_id)` when the entity has a parent, and `None` when it
/// is a root entity.  Dereferences to [`Option<EntityId>`](EntityId).
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
///
/// [`Entities`]: crate::entity::Entities
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
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
    type Cache<'world> = ThinSlice<'world, EntityNode>;
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
        unsafe { ThinSlice::from_ref(&w.read_only().entities.entities) }
    }

    #[inline(always)]
    fn register_filter(_state: &Self::State, _out: &mut Vec<FilterParamBuilder>) {}

    #[inline(always)]
    fn register_access(_state: &Self::State, _out: &mut ComponentAccess) -> bool {
        true // We did not access any components
    }

    #[inline(always)]
    unsafe fn update_table<'w>(_: &Self::State, _: &mut Self::Cache<'w>, _: &'w mut Table) {}

    #[cfg_attr(not(debug_assertions), inline)]
    unsafe fn fetch<'w>(
        _state: &Self::State,
        cache: &mut Self::Cache<'w>,
        entity: EntityId,
        _table_row: TableRow,
    ) -> Option<Self::Item<'w>> {
        let node = unsafe { cache.get(entity.index() as usize) };

        debug_assert_eq!(node.generation, entity.generation());
        debug_assert!(node.location.is_some());

        Some(Parent(node.parent))
    }
}

unsafe impl ReadOnlyQueryData for Parent {}

// -----------------------------------------------------------------------------
// ParentSlice

/// Efficient iterators for parents.
///
/// Returned by [`Query<Parent>::iter_slice`].
///
/// [`Query<Parent>::iter_slice`]: crate::query::Query::iter_slice
#[derive(Debug, Clone)]
pub struct ParentSlice<'w> {
    entities: ::core::slice::Iter<'w, EntityId>,
    inventory: ThinSlice<'w, EntityNode>,
}

unsafe impl QuerySlice for Parent {
    type SliceItem<'world> = ParentSlice<'world>;
    type ReadOnlySlice = Parent;

    unsafe fn fetch_slice<'w>(
        _state: &Self::State,
        cache: &mut Self::Cache<'w>,
        entities: &'w [EntityId],
    ) -> Option<Self::SliceItem<'w>> {
        Some(ParentSlice {
            entities: entities.iter(),
            inventory: *cache,
        })
    }
}

impl Iterator for ParentSlice<'_> {
    type Item = Parent;

    #[cfg_attr(not(debug_assertions), inline)]
    fn next(&mut self) -> Option<Self::Item> {
        let &entity = self.entities.next()?;
        let node = unsafe { self.inventory.get(entity.index() as usize) };
        debug_assert_eq!(node.generation, entity.generation());
        Some(Parent(node.parent))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.entities.size_hint()
    }

    #[inline]
    fn count(self) -> usize {
        self.entities.len()
    }
}

impl ExactSizeIterator for ParentSlice<'_> {
    #[inline]
    fn len(&self) -> usize {
        self.entities.len()
    }
}

impl FusedIterator for ParentSlice<'_> {}

// -----------------------------------------------------------------------------
// Children

/// Query data that fetches the **direct children** of each matched entity.
///
/// Yields the entity's direct children as `&[EntityId]`, ordered by
/// insertion.  Dereferences to [`EntityId`].
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
///
/// [`Entities`]: crate::entity::Entities
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
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
    type Cache<'world> = ThinSlice<'world, EntityNode>;
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
        unsafe { ThinSlice::from_ref(&w.read_only().entities.entities) }
    }

    #[inline(always)]
    fn register_filter(_state: &Self::State, _out: &mut Vec<FilterParamBuilder>) {}

    #[inline(always)]
    fn register_access(_state: &Self::State, _out: &mut ComponentAccess) -> bool {
        true // We did not access any components
    }

    #[inline(always)]
    unsafe fn update_table<'w>(_: &Self::State, _: &mut Self::Cache<'w>, _: &'w mut Table) {}

    #[cfg_attr(not(debug_assertions), inline)]
    unsafe fn fetch<'w>(
        _state: &Self::State,
        cache: &mut Self::Cache<'w>,
        entity: EntityId,
        _table_row: TableRow,
    ) -> Option<Self::Item<'w>> {
        let node = unsafe { cache.get(entity.index() as usize) };

        debug_assert_eq!(node.generation, entity.generation());
        debug_assert!(node.location.is_some());

        Some(Children(&node.children))
    }
}

unsafe impl ReadOnlyQueryData for Children<'_> {}

// -----------------------------------------------------------------------------
// ChildrenSlice

/// Efficient iterators for Children.
///
/// Returned by [`Query<Children>::iter_slice`].
///
/// [`Query<Children>::iter_slice`]: crate::query::Query::iter_slice
#[derive(Debug, Clone)]
pub struct ChildrenSlice<'w> {
    entities: ::core::slice::Iter<'w, EntityId>,
    inventory: ThinSlice<'w, EntityNode>,
}

unsafe impl QuerySlice for Children<'_> {
    type SliceItem<'world> = ChildrenSlice<'world>;
    type ReadOnlySlice = Self;

    unsafe fn fetch_slice<'w>(
        _state: &Self::State,
        cache: &mut Self::Cache<'w>,
        entities: &'w [EntityId],
    ) -> Option<Self::SliceItem<'w>> {
        Some(ChildrenSlice {
            entities: entities.iter(),
            inventory: *cache,
        })
    }
}

impl<'w> Iterator for ChildrenSlice<'w> {
    type Item = Children<'w>;

    #[cfg_attr(not(debug_assertions), inline)]
    fn next(&mut self) -> Option<Self::Item> {
        let &entity = self.entities.next()?;
        let node = unsafe { self.inventory.get(entity.index() as usize) };
        debug_assert_eq!(node.generation, entity.generation());
        Some(Children(&node.children))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.entities.size_hint()
    }

    #[inline]
    fn count(self) -> usize {
        self.entities.len()
    }
}

impl ExactSizeIterator for ChildrenSlice<'_> {
    #[inline]
    fn len(&self) -> usize {
        self.entities.len()
    }
}

impl FusedIterator for ChildrenSlice<'_> {}

// -----------------------------------------------------------------------------
