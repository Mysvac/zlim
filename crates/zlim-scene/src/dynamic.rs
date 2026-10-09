//! Turning a deserialized [`DynamicScene`] into [`ResolvedScene`]s.
//!
//! A [`DynamicScene`] is what reading a scene document produces: every entity the document declared,
//! with the components it carries already turned into templates. What it is *not* is a description —
//! it is flat. Its parent edges are entity ids, and nothing has been arranged into the tree a
//! [`ResolvedScene`] is.
//!
//! This module is that last step, and it is why the document format can live in [`zlim_core`]: the
//! format knows how to *read* a scene, and the crate that owns the resolved form knows how to
//! *arrange* one. The two meet here, which is the only place that has to.
//!
//! [`DynamicScene`]: zlim_core::scene::DynamicScene
//! [`ResolvedScene`]: crate::ResolvedScene

use zlim_core::entity::EntityMap;
use zlim_core::scene::{DynamicEntity, DynamicScene};
use zlim_core::template::{EntityTemplate, ErasedTemplate};
use zlim_error::{ZlimError, ZlimResult};

use crate::resolved::ResolvedScene;

// -----------------------------------------------------------------------------
// ResolvedScene::from_dynamic

impl ResolvedScene {
    /// Builds the scenes a [`DynamicScene`] describes: one per root of the document,
    /// each carrying the entities below it.
    ///
    /// A document is a flat list, so this is where it becomes a tree. The entities
    /// that name no parent — or name one the document does not declare — are the
    /// roots, in document order; every other entity becomes a child of the entity
    /// its `parent` names. A child may be listed before its parent, so the order of
    /// the list decides nothing: only the edges do.
    ///
    /// The components of an entity become its [`templates`], in the order the
    /// document listed their types. Nothing takes a canonical slot: a scene read
    /// from a document is applied once and is not patched, so
    /// [`push_erased_template`] is enough, and no component can replace another
    /// entity's.
    ///
    /// `dynamic` is taken by value because the templates move: each
    /// `Box<dyn ReflectTemplate>` becomes the `Box<dyn ErasedTemplate>` the scene
    /// stores — the same allocation and the same value, only a different vtable.
    /// Nothing is cloned, so a document is read into a scene exactly once.
    ///
    /// # Errors
    ///
    /// Returns an error if the document's entity ids cannot form a tree. The
    /// format permits both of these, but neither has a meaning to apply:
    ///
    /// - **a cycle.** An entity is reached by following `parent` edges from a
    ///   root; a set of entities that point at each other is reached by none, so
    ///   there is no tree to build.
    /// - **a duplicated id.** An id is how an edge names an entity, so giving one
    ///   id to two entities makes every edge that uses it ambiguous.
    ///
    /// Neither is a panic: the input comes from a document, and a document that
    /// cannot be arranged is a load that failed, not a bug in this code.
    ///
    /// [`templates`]: ResolvedScene::templates
    /// [`push_erased_template`]: ResolvedScene::push_erased_template
    pub fn from_dynamic(dynamic: DynamicScene) -> ZlimResult<Vec<Self>> {
        let entities = dynamic.entities;

        // The id an edge may name is the id the entity was given when it was read, which is also what
        // a component of another entity points at; `set_id` binds it at apply time.
        //
        // A document may not give one id to two entities: the id is how an edge names an entity, so
        // a duplicate makes every edge that uses it ambiguous. Rejecting it here means nothing is
        // silently dropped later.
        let mut index_of: EntityMap<usize> = EntityMap::with_capacity(entities.len());
        for (index, entity) in entities.iter().enumerate() {
            if index_of.insert(entity.id, index).is_some() {
                ::core::hint::cold_path();
                return Err(ZlimError::error(format!(
                    "the scene document declares the entity `{:?}` twice, \
                    so an edge that names it would be ambiguous: `{entity:?}`",
                    entity.id
                )));
            }
        }

        // A node's children, and the nodes that hang off nothing.
        let mut children: Vec<Vec<usize>> = vec![Vec::new(); entities.len()];
        let mut roots: Vec<usize> = Vec::new();
        for (index, entity) in entities.iter().enumerate() {
            match entity
                .parent
                .and_then(|parent| index_of.get(parent).copied())
            {
                // A parent the document does not declare, or none at all: this entity is a root.
                None => roots.push(index),
                // An entity that names itself is not a child of anything, so it is a root too.
                Some(parent) if parent == index => roots.push(index),
                Some(parent) => children[parent].push(index),
            }
        }

        // Build bottom-up, so that a node's children exist by the time it is built. The depth is what
        // decides that order, not the position in the list.
        let mut depth: Vec<usize> = vec![usize::MAX; entities.len()];
        let mut order: Vec<usize> = Vec::with_capacity(entities.len());
        let mut stack: Vec<(usize, usize)> = roots.iter().map(|&root| (root, 0)).collect();
        while let Some((index, at)) = stack.pop() {
            if depth[index] <= at {
                continue;
            }
            depth[index] = at;
            order.push(index);
            for &child in &children[index] {
                stack.push((child, at + 1));
            }
        }

        // A root reaches every node of a tree. Anything left unvisited is inside a cycle, which no
        // root leads into — and a cycle has no tree to build.
        if order.len() != entities.len() {
            ::core::hint::cold_path();
            return Err(ZlimError::error(
                "the scene document does not describe a tree: \
                some entities form a cycle in their `parent` edges",
            ));
        }

        order.sort_by_key(|&index| core::cmp::Reverse(depth[index]));

        // The entities are moved out as they are built, so the vector has to be mutable and each node
        // taken exactly once.
        let mut entities: Vec<Option<DynamicEntity>> = entities.into_iter().map(Some).collect();

        // A child is taken once: ids are unique and the graph is acyclic by the checks above, so every
        // node has exactly one parent — and so exactly one taker. A node the loop has reached has
        // already given its components away, which is what `entities[index]` being `None` means.
        let mut built: Vec<Option<ResolvedScene>> = (0..entities.len()).map(|_| None).collect();
        for index in order {
            let entity = entities[index]
                .take()
                .expect("a node is built once, after its children");
            let parent = entity.parent;

            let mut scene = ResolvedScene::new();

            // The id the document gave this entity. `apply` binds it in the same step it binds the
            // names, so a component that points at this entity resolves even when the document
            // declares it later.
            scene.set_id(EntityTemplate::Entity(entity.id));

            // Only a child carries an edge. A root of the document is left without one, which is
            // *not* the same as `Some(EntityTemplate::None)`: that is an answer, and the answer is
            // "move this entity to the root". A root of a *document* is a root of that document — it
            // says nothing about where the entity belongs once applied, so applying the scene onto an
            // existing entity leaves its place in the hierarchy alone. See `apply.rs`, where a layer
            // with no edge is skipped rather than taken as a request.
            if let Some(parent) = parent {
                scene.set_parent(EntityTemplate::Entity(parent));
            }

            for template in entity.components.into_values() {
                // The upcast is free: `ReflectTemplate` has `ErasedTemplate` as a supertrait, so the
                // two fat pointers differ only in their vtable and the same allocation carries both.
                // Taking `dynamic` by value is what makes this a move rather than a clone.
                let erased: Box<dyn ErasedTemplate> = template;
                scene.push_erased_template(erased);
            }

            for &child in &children[index] {
                let child = built[child].take().expect("a parent takes its child once");
                scene.add_child(child);
            }

            built[index] = Some(scene);
        }

        Ok(roots
            .into_iter()
            // Every root was visited above, so its slot is filled.
            .filter_map(|root| built[root].take())
            .collect())
    }
}
