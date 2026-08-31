//! Transform propagation systems.
//!
//! The transform update is split into two jobs:
//!
//! - [`TransformChangeDetection`] — marks every node that needs an update
//!   (the "pollution pass").  It starts from the entities whose [`Transform`]
//!   changed (directly, or via a [`ReparentSignal`] message) and walks **down** the
//!   hierarchy, stamping every unmarked descendant; a descendant that is
//!   already marked is skipped, because its own subtree is handled by its own
//!   walk.  Each walk is a subtree starting at a marked node and stops at
//!   marked boundaries, so the marked region is exactly the union of "changed
//!   subtrees" — and parallel walks never touch the same node.
//! - [`TransformPropagation`] — recomputes [`GlobalTransform`].  It first
//!   finds the *roots* of the changed subtrees (entities that are marked but
//!   whose parent is not; the pollution pass guarantees these roots are
//!   disjoint), then recomputes each root's subtree in parallel.
//!
//! # Change-detection contract
//!
//! Only the *inputs* of the hierarchy — [`Transform`] and the parent relation
//! — are dirty signals.  [`GlobalTransform`] is a derived output: only
//! [`TransformPropagation`] writes it, so its own change state carries no
//! information and is never inspected.
//!
//! The system relies on the tick semantics that a component written by a
//! previous frame is *not* reported as changed to the current frame (the
//! system's `last_run` tick has advanced past the write).  This is what keeps
//! propagation from re-writing the whole tree every frame: the writes it made
//! last frame are invisible to it this frame.
//!
//! # Why mutable access for read-only data
//!
//! Several places read `Transform` change ticks through `Query::get_mut` /
//! `Query::iter_slice_mut` even though they never write.  Going through the
//! mutable [`QueryState`](zlim_core::query::QueryState) keeps the same
//! function pointers and code paths as the writing propagation pass;
//! converting to the read-only query state (`as_readonly`) would swap in
//! different fetch/iteration function pointers and hurt cache hit rate.  The
//! access is read-only, so sharing the mutable query across parallel tasks
//! stays race-free as long as each task only touches disjoint entities.
//!
//! [`Transform`]: crate::Transform
//! [`GlobalTransform`]: crate::GlobalTransform
//! [`ReparentSignal`]: zlim_core::message::ReparentSignal

// -----------------------------------------------------------------------------
// Types

pub use change_detection_impls::TransformChangeDetection;
pub use propagation_impls::TransformPropagation;
pub use transform_change_root::TransformChangeRoot;

/// The change-detection strategy used by the transform systems.
///
/// By default, transform updates are split into two stages:
///
/// 1. **Detection** — finds the root nodes of every subtree that needs an
///    update.
/// 2. **Propagation** — updates every subtree, descending from the roots
///    found in stage 1.
///
/// Stage 2 is O(N) in the worst case — every entity is visited once (random
/// access, not sequential); in the best case it does nothing, i.e. no entity
/// moved.
///
/// Stage 1 is split into several sub-steps by default:
///
/// 1. Reads all `ReparentSignal` messages (reparent implementations must send
///    them).
/// 2. Iterates the query sequentially and collects the ids of every
///    `Transform` that changed.
/// 3. Pushes the change down to the whole subtree for every collected id.
/// 4. Filters the nodes collected in step 2 down to the ones that really are
///    roots of changed subtrees.
///
/// Stage 1-1 usually receives very few messages — entity creation and
/// despawn are not reparents — so its cost is negligible.
///
/// Stage 1-2 is O(N) but sequential, with an excellent cache hit rate.
///
/// Stage 1-3 is O(N) in the worst case; subtrees are never traversed twice,
/// but the entity-tree access is random access. In the best case it is a
/// no-op (nothing changed).
///
/// Stage 1-4 is O(N) in the worst case; the best case is again a no-op. Also
/// random access.
///
/// In other words: for a static scene with no changes, the best case needs a
/// single O(N) sequential pass.  In the worst case the whole scene must be
/// updated, which becomes O(2·N) random access plus O(N) sequential access —
/// but when the whole scene needs updating, many of stage 1's operations are
/// unnecessary.
///
/// [`TransformPropagateStrategy`] therefore controls the logic of stage 1:
///
/// - [`Default`] — the default operations described above.
///
/// - [`PropagateUp`] — replaces the downward marking by skipping steps 2-4:
///   for each changed node, walk the ancestor chain upward; if any ancestor
///   is also changed, the node belongs to that ancestor's subtree and is not
///   a root; a node whose ancestor chain contains no changed node (or ends)
///   is a root of a changed subtree.  Each changed node pays one O(depth)
///   parent-chain query instead of marking its whole subtree, which suits
///   sparse, shallow changes; deep hierarchies can degrade toward
///   O(N×depth) in the worst case.
///
/// - [`PropagateAll`] — simplifies stage 1: it directly collects every entity
///   that is a root (no parent, or whose parent lacks a `Transform` component),
///   reducing stage 1 to O(N) random access (still needs a parent-existence
///   check).  Stage 2 then always costs O(N) because the whole tree is propagated;
///   the total is O(2·N) random access.
///
/// For highly dynamic scenes, consider using [`PropagateAll`] directly.
///
/// Please do not remove this resource; otherwise, the Transform jobs will not run
/// (and will output warnings).
///
/// [`Default`]: Self::Default
/// [`PropagateUp`]: Self::PropagateUp
/// [`PropagateAll`]: Self::PropagateAll
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[derive(zlim_core::derive::Resource)]
#[derive(zlim_reflect::derive::TypePath)]
#[type_path = "zlim_transform::TransformPropagateStrategy"]
pub enum TransformPropagateStrategy {
    #[default]
    Default,
    PropagateUp,
    PropagateAll,
}

// -----------------------------------------------------------------------------
// Single Threaded Implementations

zlim_task::cfg::single_thread! {
    mod transform_change_root {
        use zlim_core::derive::Resource;
        use zlim_core::entity::EntityId;
        use zlim_reflect::derive::TypePath;

        /// Internal data pipeline for Transform Propagation. Users should not use this.
        ///
        /// Please do not remove this resource; otherwise, the Transform jobs will not run
        /// (and will output warnings).
        #[derive(TypePath, Resource, Default)]
        #[type_path = "zlim_transform::TransformChangeRoot"]
        #[repr(transparent)]
        pub struct TransformChangeRoot(pub(crate) Vec<(EntityId, Option<EntityId>)>);
    }

    #[expect(unsafe_code, reason = "for better performance")]
    mod change_detection_impls {
        use core::ptr::NonNull;
        use zlim_core::borrow::{Mut, ResMut, Res};
        use zlim_core::entity::EntityId;
        use zlim_core::job_fn;
        use zlim_core::message::{MessageReader, ReparentSignal};
        use zlim_core::query::Query;
        use zlim_core::system::{HierarchyQuery, Local};
        use zlim_core::tick::{DetectChanges, DetectChangesMut};

        use super::TransformPropagateStrategy;
        use super::TransformChangeRoot;
        use crate::{GlobalTransform, Transform};

        /// Performs transform change detection.
        ///
        /// 1. Collects all changed "root" nodes.
        /// 2. Marks all nodes (entities) that need change propagation as changed.
        ///
        /// This enables culling of unchanged subtrees.
        ///
        /// If [`TransformPropagateStrategy`] is set to `PropagateAll`,
        /// subtree marking is skipped, and all root nodes with a [`Transform`] component
        /// are collected directly.
        ///
        /// If it is set to `PropagateUp`, steps 2-4 of the default strategy
        /// are skipped and replaced by an upward query: each changed node
        /// walks its ancestor chain and becomes a root only when no ancestor
        /// is changed.
        #[job_fn(type = TransformChangeDetection, name = "zlim_transform::TransformChangeDetection")]
        fn transform_change_detection(
            hierarchy: HierarchyQuery,
            global: Query<&GlobalTransform>,
            mut query: Query<(Mut<Transform>, EntityId)>,
            mut reparent: MessageReader<ReparentSignal>,
            strategy: Res<TransformPropagateStrategy>,
            mut tcroot: ResMut<TransformChangeRoot>,
            mut tcroot_buf: Local<TransformChangeRoot>,
            mut buffer: Local<Vec<EntityId>>,
        ) {
            let tcroot: &mut Vec<(EntityId, Option<EntityId>)> = &mut tcroot.0;
            // SAFETY: EntityId no needs drop
            unsafe { tcroot.set_len(0) };

            // If propagating all, collect all "root" entities directly
            if matches!(*strategy, TransformPropagateStrategy::PropagateAll) {
                let ptr: NonNull<Query<(Mut<Transform>, EntityId)>> = NonNull::from_mut(&mut query);
                let q2 = unsafe { &*ptr.as_ptr() };

                // Note: `Query::iter_slice_mut + iter` is faster than `Query::iter`
                for (_, entities) in query.iter_slice_mut() {
                    for &id in entities {
                        // SAFETY: `id` comes from the query, so the entity is alive.
                        let parent = unsafe { hierarchy.get_parent_unchecked(id) };

                        let Some(parent) = parent else {
                            tcroot.push((id, None));
                            continue;
                        };

                        // A parent without a `Transform` component is not part
                        // of the transform tree, so `id` is a root.
                        // `contains_weak` skips the filter-cache update.
                        if !q2.contains_weak(parent) {
                            tcroot.push((id, Some(parent)));
                        }
                    }
                }

                return; // <---
            }

            // Non-full propagation: collect all root nodes of changed subtrees.

            // Mark all root nodes that have undergone structural changes (reparent)
            for r in reparent.read() {
                let id = r.entity;
                if let Ok((mut transform, _)) = query.get_mut(id)
                    && let Ok(gt) = global.get(id)
                {
                    // If the GlobalTransform hasn't actually changed, don't mark as changed.
                    if let Some(p) = unsafe { hierarchy.get_parent_unchecked(id) }
                        && let Ok(pgt) = global.get(p)
                    {
                        if pgt.mul_transform(*transform) != *gt {
                            // NOTE: the float comparison may cause an unnecessary
                            // `set_changed` due to precision issues, but the impact is small.
                            transform.set_changed();
                        }
                    } else {
                        if GlobalTransform::IDENTITY.mul_transform(*transform) != *gt {
                            transform.set_changed();
                        }
                    }
                }
            }

            // PropagateUp: Checks each node against the change root by querying upward.
            if matches!(*strategy, TransformPropagateStrategy::PropagateUp) {
                let ptr: NonNull<Query<(Mut<Transform>, EntityId)>> = NonNull::from_mut(&mut query);
                let q2 = unsafe { &mut *ptr.as_ptr() };

                for (transforms, entities) in query.iter_slice_mut() {
                    debug_assert_eq!(transforms.len(), entities.len());

                    'out: for (transform, entity) in transforms.into_iter().zip(entities) {
                        if !transform.is_changed() {
                            continue;
                        }
                        ::core::hint::cold_path();

                        let id = *entity;
                        let parent = unsafe { hierarchy.get_parent_unchecked(id) };

                        let mut iter_item = parent;
                        'inn: while let Some(node) = iter_item {
                            let Ok((transform, _)) = q2.get_mut(node) else {
                                break 'inn; // without parent, it's root node
                            };
                            if transform.is_changed() {
                                continue 'out; // ancestor is changed, it's not root
                            }
                            iter_item = unsafe { hierarchy.get_parent_unchecked(node) };
                        }

                        tcroot.push((id, parent))
                    }
                }

                return;
            }

            // TransformPropagateStrategy::Default: propagate down

            let tcroot_buf: &mut Vec<(EntityId, Option<EntityId>)> = &mut tcroot_buf.0;
            unsafe { tcroot_buf.set_len(0) };
            unsafe { buffer.set_len(0) };

            // Step-1: Collect all changed "root" nodes, and collect all
            // child nodes that need change propagation detection.
            for (transforms, entities) in query.iter_slice_mut() {
                debug_assert_eq!(transforms.len(), entities.len());
                for (transform, entity) in transforms.into_iter().zip(entities) {
                    if !transform.is_changed() {
                        continue;
                    }
                    ::core::hint::cold_path();
                    let id = *entity;
                    let parent = unsafe { hierarchy.get_parent_unchecked(id) };
                    let children = unsafe { hierarchy.get_children_unchecked(id) };

                    tcroot_buf.push((id, parent));

                    if !children.is_empty() {
                        buffer.reserve(children.len());
                        buffer.extend_from_slice(children);
                    }
                }
            }

            // Step-2: Propagate down change detection
            while let Some(node) = buffer.pop() {
                let Ok((mut transform, _)) = query.get_mut(node) else {
                    continue;
                };

                // Early stop. If the node is already marked as changed,
                // its children are already in the queue.
                if transform.is_changed() {
                    continue;
                }

                transform.set_changed();

                let children: &[EntityId] = unsafe { hierarchy.get_children_unchecked(node) };
                buffer.reserve(children.len());
                buffer.extend_from_slice(children);
            }

            // Step-3: Filter out invalid changed "root" nodes
            tcroot.reserve(tcroot_buf.len() >> 2);
            for &(id, px) in tcroot_buf.iter() {
                let Some(p) = px else {
                    tcroot.push((id, None));
                    continue;
                };

                let Ok((t, _)) = query.get_mut(p) else {
                    tcroot.push((id, px));
                    continue;
                };

                if !t.is_changed() {
                    tcroot.push((id, px));
                }
            }
        }
    }

    #[expect(unsafe_code, reason = "for better performance")]
    mod propagation_impls {
        use super::TransformChangeRoot;
        use crate::{GlobalTransform, Transform};
        use core::ptr::NonNull;
        use zlim_core::borrow::{Mut, Res};
        use zlim_core::entity::EntityId;
        use zlim_core::job_fn;
        use zlim_core::query::Query;
        use zlim_core::system::{HierarchyQuery, Local};

        /// Propagates transforms and updates `GlobalTransform`.
        ///
        /// Must run after [`TransformChangeDetection`](crate::TransformChangeDetection).
        #[job_fn(type = TransformPropagation, name = "zlim_transform::TransformPropagation")]
        fn propagate_transform(
            hierarchy: HierarchyQuery,
            mut query: Query<(Mut<Transform>, Mut<GlobalTransform>)>,
            tcroot: Res<TransformChangeRoot>,
            mut buffer: Local<Vec<(EntityId, GlobalTransform)>>,
        ) {
            let tcroot: &Vec<(EntityId, Option<EntityId>)> = &tcroot.0;

            let buf: &mut Vec<(EntityId, GlobalTransform)> = &mut buffer;
            unsafe { buf.set_len(0) };

            let ptr = NonNull::from_mut(&mut query);
            let q: &mut Query<(Mut<Transform>, Mut<GlobalTransform>)> = unsafe { &mut *ptr.as_ptr() };

            // Iterate over all root nodes of changed subtrees
            for &(root, parent) in tcroot.iter() {
                let (transform, global) = query.get_mut(root).expect("should exist");

                // If the node has a parent, use its GlobalTransform as the base;
                // otherwise use the identity transform.
                let root_gt = if let Some(parent) = parent
                    && let Ok((_, parent_global)) = q.get_mut(parent)
                {
                    parent_global.mul_transform(*transform)
                } else {
                    GlobalTransform::IDENTITY.mul_transform(*transform)
                };
                *global.into_inner() = root_gt;

                // SAFETY: Query::get_mut can skip invalid entities
                let children = unsafe { hierarchy.get_children_unchecked(root) };
                propagate_recursive(children, hierarchy, root_gt, q, buf);
            }
        }

        #[inline]
        fn propagate_recursive(
            children: &[EntityId],
            hierarchy: HierarchyQuery,
            parent_gt: GlobalTransform,
            query: &mut Query<(Mut<Transform>, Mut<GlobalTransform>)>,
            buffer: &mut Vec<(EntityId, GlobalTransform)>,
        ) {
            buffer.reserve(children.len());
            children.iter().for_each(|&x| buffer.push((x, parent_gt)));

            // Recursively update all child nodes.
            while let Some((entity, base_gt)) = buffer.pop() {
                let Ok((transform, mut global)) = query.get_mut(entity) else {
                    continue; // Node without Transform, disconnect
                };

                let new_gt = base_gt.mul_transform(*transform);
                *global = new_gt;
                // SAFETY: Query::get_mut can skip invalid entities
                let children = unsafe { hierarchy.get_children_unchecked(entity) };
                buffer.reserve(children.len());
                children.iter().for_each(|&x| buffer.push((x, new_gt)));
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Multi Threaded Implementations

zlim_task::cfg::multi_thread! {
    mod transform_change_root {
        use core::cell::RefCell;
        use zlim_core::derive::Resource;
        use zlim_core::entity::EntityId;
        use zlim_reflect::derive::TypePath;
        use zlim_utils::ext::ThreadLocal;

        /// Internal data pipeline for Transform Propagation. Users should not use this.
        ///
        /// Please do not remove this resource; otherwise, the Transform jobs will not run
        /// (and will output warnings).
        #[derive(TypePath, Resource, Default)]
        #[type_path = "zlim_transform::TransformChangeRoot"]
        #[repr(transparent)]
        pub struct TransformChangeRoot(
            pub(crate) ThreadLocal<RefCell<Vec<(EntityId, Option<EntityId>)>>>,
        );
    }

    #[expect(unsafe_code, reason = "for better performance")]
    mod change_detection_impls {
        use core::cell::RefCell;
        use core::mem::transmute;
        use core::ptr::NonNull;
        use zlim_core::borrow::{Mut, ResMut, Res};
        use zlim_core::entity::EntityId;
        use zlim_core::job_fn;
        use zlim_core::message::{MessageReader, ReparentSignal};
        use zlim_core::query::Query;
        use zlim_core::system::{HierarchyQuery, Local};
        use zlim_core::tick::{DetectChanges, DetectChangesMut};
        use zlim_task::Scope;
        use zlim_utils::ext::{ArrayDeque, ThreadLocal};

        use super::TransformPropagateStrategy;
        use super::TransformChangeRoot;
        use crate::{GlobalTransform, Transform};

        /// Performs transform change detection.
        ///
        /// 1. Collects all changed "root" nodes.
        /// 2. Marks all nodes (entities) that need change propagation as changed.
        ///
        /// This enables culling of unchanged subtrees.
        ///
        /// If [`TransformPropagateStrategy`] is set to `PropagateAll`,
        /// subtree marking is skipped, and all root nodes with a [`Transform`] component
        /// are collected directly.
        ///
        /// If it is set to `PropagateUp`, steps 2-4 of the default strategy
        /// are skipped and replaced by an upward query: each changed node
        /// walks its ancestor chain and becomes a root only when no ancestor
        /// is changed.
        #[job_fn(type = TransformChangeDetection, name = "zlim_transform::TransformChangeDetection")]
        fn transform_change_detection(
            hierarchy: HierarchyQuery,
            global: Query<&GlobalTransform>,
            mut query: Query<(Mut<Transform>, EntityId)>,
            mut reparent: MessageReader<ReparentSignal>,
            strategy: Res<TransformPropagateStrategy>,
            mut tcroot: ResMut<TransformChangeRoot>,
            mut tcroot_buf: Local<TransformChangeRoot>,
        ) {
            let tcroot: &mut ThreadLocal<RefCell<Vec<(EntityId, Option<EntityId>)>>> = &mut tcroot.0;
            // SAFETY: EntityId no needs drop
            unsafe { tcroot.iter_mut().for_each(|x| x.get_mut().set_len(0)) };

            // If propagating all, collect all "root" entities directly
            if matches!(*strategy, TransformPropagateStrategy::PropagateAll) {
                const CHUNK_SIZE: usize = 2048; // need benchmark
                const ONE_HALF_CHUNK: usize = CHUNK_SIZE + (CHUNK_SIZE >> 1);

                zlim_task::MainTaskPool::get().scope(|s| {
                    let ptr: NonNull<Query<(Mut<Transform>, EntityId)>> = NonNull::from_mut(&mut query);
                    let tcroot_ref: &ThreadLocal<RefCell<Vec<(EntityId, Option<EntityId>)>>> = tcroot;

                    for (_, mut entities) in query.iter_slice_mut() {
                        // split large blocks
                        while entities.len() > ONE_HALF_CHUNK {
                            let (e1, e2) = unsafe { entities.split_at_unchecked(CHUNK_SIZE) };
                            entities = e2;

                            let q2: &Query<(Mut<Transform>, EntityId)> = unsafe { &*ptr.as_ptr() };

                            s.spawn(async move {
                                let mut local = tcroot_ref.get_or_default().borrow_mut();
                                let local_tcroot: &mut Vec<(EntityId, Option<EntityId>)> = &mut local;
                                for &id in e1 {
                                    let parent = unsafe { hierarchy.get_parent_unchecked(id) };

                                    let Some(parent) = parent else {
                                        local_tcroot.push((id, None));
                                        continue;
                                    };

                                    if !q2.contains_weak(parent) {
                                        local_tcroot.push((id, Some(parent)));
                                    }
                                }
                            });
                        }

                        if !entities.is_empty() {
                            let q2: &Query<(Mut<Transform>, EntityId)> = unsafe { &*ptr.as_ptr() };
                            s.spawn(async move {
                                let mut local = tcroot_ref.get_or_default().borrow_mut();
                                let local_tcroot: &mut Vec<(EntityId, Option<EntityId>)> = &mut local;
                                for &id in entities {
                                    let parent = unsafe { hierarchy.get_parent_unchecked(id) };

                                    let Some(parent) = parent else {
                                        local_tcroot.push((id, None));
                                        continue;
                                    };

                                    if !q2.contains_weak(parent) {
                                        local_tcroot.push((id, Some(parent)));
                                    }
                                }
                            });
                        }
                    }
                });

                return;
            }

            // Non-full propagation: collect all root nodes of changed subtrees.

            // Mark all root nodes that have undergone structural changes (reparent)
            for r in reparent.read() {
                let id = r.entity;
                if let Ok((mut transform, _)) = query.get_mut(id)
                    && let Ok(gt) = global.get(id)
                {
                    // If the GlobalTransform hasn't actually changed, don't mark as changed.
                    if let Some(p) = unsafe { hierarchy.get_parent_unchecked(id) }
                        && let Ok(pgt) = global.get(p)
                    {
                        if pgt.mul_transform(*transform) != *gt {
                            // NOTE: the float comparison may cause an
                            // unnecessary `set_changed` due to precision
                            // issues, but the impact is small.
                            transform.set_changed();
                        }
                    } else {
                        if GlobalTransform::IDENTITY.mul_transform(*transform) != *gt {
                            transform.set_changed();
                        }
                    }
                }
            }

            // PropagateUp: Checks each node against the change root by querying upward.
            if matches!(*strategy, TransformPropagateStrategy::PropagateUp) {
                const CHUNK_SIZE: usize = 512; // need benchmark
                const ONE_HALF_CHUNK: usize = CHUNK_SIZE + (CHUNK_SIZE >> 1);

                zlim_task::MainTaskPool::get().scope(|s| {
                    let ptr: NonNull<Query<(Mut<Transform>, EntityId)>> = NonNull::from_mut(&mut query);
                    let tcroot_ref: &ThreadLocal<RefCell<Vec<(EntityId, Option<EntityId>)>>> = tcroot;

                    for (mut transforms, mut entities) in query.iter_slice_mut() {
                        // split large blocks
                        while entities.len() > ONE_HALF_CHUNK {
                            let (t1, t2) = transforms.split_at(CHUNK_SIZE);
                            let (e1, e2) = unsafe { entities.split_at_unchecked(CHUNK_SIZE) };
                            transforms = t2;
                            entities = e2;

                            let q2: &mut Query<(Mut<Transform>, EntityId)> = unsafe { &mut *ptr.as_ptr() };

                            s.spawn(async move {
                                let mut local = tcroot_ref.get_or_default().borrow_mut();
                                let local_tcroot: &mut Vec<(EntityId, Option<EntityId>)> = &mut local;
                                'out: for (transform, entity) in t1.into_iter().zip(e1) {
                                    if !transform.is_changed() {
                                        continue;
                                    }
                                    ::core::hint::cold_path();

                                    let id = *entity;
                                    let parent = unsafe { hierarchy.get_parent_unchecked(id) };

                                    let mut iter_item = parent;
                                    'inn: while let Some(node) = iter_item {
                                        let Ok((transform, _)) = q2.get_mut(node) else {
                                            break 'inn; // without parent, it's root node
                                        };
                                        if transform.is_changed() {
                                            continue 'out; // ancestor is changed, it's not root
                                        }
                                        iter_item = unsafe { hierarchy.get_parent_unchecked(node) };
                                    }

                                    local_tcroot.push((id, parent))
                                }
                            });

                        }

                        if !entities.is_empty() {
                            debug_assert_eq!(transforms.len(), entities.len());
                            let q2: &mut Query<(Mut<Transform>, EntityId)> = unsafe { &mut *ptr.as_ptr() };

                            s.spawn(async move {
                                let mut local = tcroot_ref.get_or_default().borrow_mut();
                                let local_tcroot: &mut Vec<(EntityId, Option<EntityId>)> = &mut local;
                                'out: for (transform, entity) in transforms.into_iter().zip(entities) {
                                    if !transform.is_changed() {
                                        continue;
                                    }
                                    ::core::hint::cold_path();

                                    let id = *entity;
                                    let parent = unsafe { hierarchy.get_parent_unchecked(id) };

                                    let mut iter_item = parent;
                                    'inn: while let Some(node) = iter_item {
                                        let Ok((transform, _)) = q2.get_mut(node) else {
                                            break 'inn; // without parent, it's root node
                                        };
                                        if transform.is_changed() {
                                            continue 'out; // ancestor is changed, it's not root
                                        }
                                        iter_item = unsafe { hierarchy.get_parent_unchecked(node) };
                                    }

                                    local_tcroot.push((id, parent))
                                }
                            });
                        }
                    }
                });

                return;
            }

            // TransformPropagateStrategy::Default: propagate down

            let tcroot_buf: &mut ThreadLocal<RefCell<Vec<(EntityId, Option<EntityId>)>>> =
                &mut tcroot_buf.0;
            // SAFETY: EntityId no needs drop
            unsafe { tcroot_buf.iter_mut().for_each(|x| x.get_mut().set_len(0)) };

            // Step-1: Collect all changed "root" nodes, and collect all child nodes
            // that need change propagation detection.
            zlim_task::MainTaskPool::get().scope(|s| {
                const CHUNK_SIZE: usize = 2048; // need benchmark
                const ONE_HALF_CHUNK: usize = CHUNK_SIZE + (CHUNK_SIZE >> 1);

                let tcroot_buf_ref: &ThreadLocal<RefCell<Vec<(EntityId, Option<EntityId>)>>> =
                    tcroot_buf;
                for (mut transforms, mut entities) in query.iter_slice_mut() {
                    while entities.len() > ONE_HALF_CHUNK {
                        let (t1, t2) = transforms.split_at(CHUNK_SIZE);
                        let (e1, e2) = unsafe { entities.split_at_unchecked(CHUNK_SIZE) };
                        debug_assert_eq!(t1.len(), e1.len());

                        transforms = t2;
                        entities = e2;

                        s.spawn(async move {
                            let mut local = tcroot_buf_ref.get_or_default().borrow_mut();
                            let local_buf: &mut Vec<(EntityId, Option<EntityId>)> = &mut local;

                            for (transform, entity) in t1.into_iter().zip(e1) {
                                if !transform.is_changed() {
                                    continue;
                                }
                                ::core::hint::cold_path();
                                let id = *entity;
                                let parent = unsafe { hierarchy.get_parent_unchecked(id) };

                                local_buf.push((id, parent));
                            }
                        });
                    }

                    debug_assert_eq!(transforms.len(), entities.len());
                    if !entities.is_empty() {
                        s.spawn(async move {
                            let mut local = tcroot_buf_ref.get_or_default().borrow_mut();
                            let local_buf: &mut Vec<(EntityId, Option<EntityId>)> = &mut local;

                            for (transform, entity) in transforms.into_iter().zip(entities) {
                                if !transform.is_changed() {
                                    continue;
                                }
                                ::core::hint::cold_path();
                                let id = *entity;
                                let parent = unsafe { hierarchy.get_parent_unchecked(id) };

                                local_buf.push((id, parent));
                            }
                        });
                    }
                }
            });

            // Step-2: Propagate change detection
            zlim_task::MainTaskPool::get().scope(|s: &Scope<'_, '_, ()>| {
                // SAFETY: the lifetime is ensured by `iterate` function
                let s2: &'static Scope<'static, 'static, ()> =
                    unsafe { transmute::<&Scope<()>, &Scope<()>>(s) };
                let h2: HierarchyQuery<'static> =
                    unsafe { transmute::<HierarchyQuery, HierarchyQuery>(hierarchy) };
                let ptr = NonNull::from_mut(&mut query);

                for local_buf in tcroot_buf.iter_mut() {
                    let q: &mut Query<(Mut<Transform>, EntityId)> = unsafe { &mut *ptr.as_ptr() };

                    let local_buf: &[(EntityId, Option<EntityId>)] = local_buf.get_mut().as_slice();
                    if local_buf.is_empty() {
                        continue;
                    }

                    s.spawn(async move {
                        let ptr = NonNull::from_mut(q);

                        for &(id, _) in local_buf {
                            let mut children: &[EntityId] =
                                unsafe { hierarchy.get_children_unchecked(id) };

                            if children.is_empty() {
                                continue;
                            }

                            let q1: &mut Query<(Mut<Transform>, EntityId)> =
                                unsafe { &mut *ptr.as_ptr() };

                            // ```
                            // Parent:
                            //   - Child[0] -> Node[?] // single chain
                            //   - Child[1] -> Node[?]
                            //   - Child[2] -> Node[?]
                            //   - ...
                            // ```
                            //
                            // Inline processing of single-chain node to optimize cache speed.
                            if children.len() == 1 {
                                let child: EntityId = unsafe { *children.get_unchecked(0) };
                                let Ok((mut transform, _)) = q1.get_mut(child) else {
                                    continue;
                                };
                                if transform.is_changed() {
                                    continue;
                                }
                                transform.set_changed();
                                let children_c: &[EntityId] =
                                    unsafe { hierarchy.get_children_unchecked(child) };

                                if children_c.is_empty() {
                                    continue;
                                } else {
                                    // inline process at most once; push remaining children
                                    // to the task queue to avoid starvation
                                    children = children_c;
                                }
                            }

                            // SAFETY: the lifetime is ensured by `iterate` function
                            let c2: &[EntityId] =
                                unsafe { transmute::<&[EntityId], &[EntityId]>(children) };
                            let q2: &mut Query<(Mut<Transform>, EntityId)> = unsafe {
                                transmute::<
                                    &mut Query<(Mut<Transform>, EntityId)>,
                                    &mut Query<(Mut<Transform>, EntityId)>,
                                >(q1)
                            };

                            s.spawn(async move {
                                iterate(s2, c2, h2, q2);
                            });
                        }
                    });
                }
            });

            // Step-3: Filter out invalid changed "root" nodes
            zlim_task::MainTaskPool::get().scope(|s: &Scope<'_, '_, ()>| {
                const CHUNK_SIZE: usize = 1024; // need benchmark
                const ONE_HALF_CHUNK: usize = CHUNK_SIZE + (CHUNK_SIZE >> 1);

                let ptr = NonNull::from_mut(&mut query);
                let tcroot_ref: &ThreadLocal<RefCell<Vec<(EntityId, Option<EntityId>)>>> = tcroot;
                for local_buf in tcroot_buf.iter_mut() {
                    let mut local_buf: &[(EntityId, Option<EntityId>)] = local_buf.get_mut().as_slice();

                    while local_buf.len() > ONE_HALF_CHUNK {
                        let (t1, t2) = local_buf.split_at(CHUNK_SIZE);
                        local_buf = t2;

                        let q: &mut Query<(Mut<Transform>, EntityId)> = unsafe { &mut *ptr.as_ptr() };
                        s.spawn(async move {
                            let mut local = tcroot_ref.get_or_default().borrow_mut();
                            let local_tcroot: &mut Vec<(EntityId, Option<EntityId>)> = &mut local;

                            for &(id, px) in t1 {
                                let Some(p) = px else {
                                    local_tcroot.push((id, None));
                                    continue;
                                };
                                let Ok((t, _)) = q.get_mut(p) else {
                                    local_tcroot.push((id, px));
                                    continue;
                                };
                                if !t.is_changed() {
                                    local_tcroot.push((id, px));
                                }
                            }
                        });
                    }

                    if !local_buf.is_empty() {
                        let q: &mut Query<(Mut<Transform>, EntityId)> = unsafe { &mut *ptr.as_ptr() };
                        s.spawn(async move {
                            let mut local = tcroot_ref.get_or_default().borrow_mut();
                            let local_tcroot: &mut Vec<(EntityId, Option<EntityId>)> = &mut local;

                            for &(id, px) in local_buf {
                                let Some(p) = px else {
                                    local_tcroot.push((id, None));
                                    continue;
                                };
                                let Ok((t, _)) = q.get_mut(p) else {
                                    local_tcroot.push((id, px));
                                    continue;
                                };
                                if !t.is_changed() {
                                    local_tcroot.push((id, px));
                                }
                            }
                        });
                    }
                }
            });
        }

        fn iterate<'sco, 'env>(
            s: &'sco Scope<'sco, 'env, ()>,
            mut children: &'env [EntityId],
            hierarchy: HierarchyQuery<'env>,
            query: &'env mut Query<'env, 'env, (Mut<'static, Transform>, EntityId)>,
        ) {
            const CHUNK_SIZE: usize = 1024; // need benchmark
            const ONE_HALF_CHUNK: usize = CHUNK_SIZE + (CHUNK_SIZE >> 1);

            let ptr: NonNull<Query<(Mut<Transform>, EntityId)>> = NonNull::from_mut(query);

            while children.len() > ONE_HALF_CHUNK {
                let (c1, c2) = unsafe { children.split_at_unchecked(CHUNK_SIZE) };
                children = c2;
                let q: &mut Query<(Mut<Transform>, EntityId)> = unsafe { &mut *ptr.as_ptr() };
                s.spawn(async move {
                    iterate(s, c1, hierarchy, q);
                });
            }

            let mut buffer: ArrayDeque<&[EntityId], 4> = ArrayDeque::new();
            // SAFETY: buffer.is_empty()
            unsafe { buffer.push_back_unchecked(children) };

            while let Some(children) = buffer.pop_front() {
                for &child in children {
                    let Ok((mut transform, _)) = query.get_mut(child) else {
                        continue;
                    };
                    if transform.is_changed() {
                        continue;
                    }
                    transform.set_changed();

                    let children_c: &[EntityId] = unsafe { hierarchy.get_children_unchecked(child) };

                    if children_c.is_empty() {
                        continue;
                    }

                    // Normal node
                    if children_c.len() > 1 {
                        if buffer.is_full() {
                            // SAFETY: !buffer.is_empty()
                            let old = unsafe { buffer.pop_front().unwrap_unchecked() };
                            let q: &mut Query<(Mut<Transform>, EntityId)> =
                                unsafe { &mut *ptr.as_ptr() };
                            s.spawn(async move {
                                iterate(s, old, hierarchy, q);
                            });
                        }

                        // SAFETY: !buffer.is_full()
                        unsafe { buffer.push_back_unchecked(children_c) };
                        continue;
                    }

                    // ```
                    // Parent:
                    //   - Child[0] -> Node[?] // single chain
                    //   - Child[1] -> Node[?]
                    //   - Child[2] -> Node[?]
                    //   - ...
                    // ```
                    //
                    // Inline processing of single-chain node to optimize cache speed.
                    ::core::hint::cold_path();
                    let &child_c = unsafe { children_c.get_unchecked(0) };
                    let Ok((mut transform, _)) = query.get_mut(child_c) else {
                        continue;
                    };

                    if transform.is_changed() {
                        continue;
                    }
                    transform.set_changed();

                    let children_cc: &[EntityId] = unsafe { hierarchy.get_children_unchecked(child_c) };

                    if children_cc.is_empty() {
                        continue;
                    }

                    if buffer.is_full() {
                        // SAFETY: !buffer.is_empty()
                        let old = unsafe { buffer.pop_front().unwrap_unchecked() };
                        let q: &mut Query<(Mut<Transform>, EntityId)> = unsafe { &mut *ptr.as_ptr() };
                        s.spawn(async move {
                            iterate(s, old, hierarchy, q);
                        });
                    }

                    // SAFETY: !buffer.is_full()
                    unsafe { buffer.push_back_unchecked(children_cc) };
                    continue;
                }
            }
        }
    }

    #[expect(unsafe_code, reason = "for better performance")]
    mod propagation_impls {
        use core::cell::RefCell;
        use core::mem::transmute;
        use core::ptr::NonNull;
        use zlim_core::borrow::{Mut, ResMut};
        use zlim_core::entity::EntityId;
        use zlim_core::job_fn;
        use zlim_core::query::Query;
        use zlim_core::system::HierarchyQuery;
        use zlim_task::Scope;
        use zlim_utils::ext::{ArrayDeque, ThreadLocal};

        use super::TransformChangeRoot;
        use crate::{GlobalTransform, Transform};

        /// Propagates transforms and updates `GlobalTransform`.
        ///
        /// Must run after [`TransformChangeDetection`](crate::TransformChangeDetection).
        #[job_fn(type = TransformPropagation, name = "zlim_transform::TransformPropagation")]
        fn propagate_transform(
            hierarchy: HierarchyQuery,
            mut query: Query<(Mut<Transform>, Mut<GlobalTransform>)>,
            mut tcroot: ResMut<TransformChangeRoot>,
        ) {
            let tcroot: &mut ThreadLocal<RefCell<Vec<(EntityId, Option<EntityId>)>>> = &mut tcroot.0;

            zlim_task::MainTaskPool::get().scope(|s: &Scope<'_, '_, ()>| {
                // SAFETY: the lifetime is ensured by `iterate` function
                let s2: &'static Scope<'static, 'static, ()> =
                    unsafe { transmute::<&Scope<()>, &Scope<()>>(s) };
                let h2: HierarchyQuery<'static> =
                    unsafe { transmute::<HierarchyQuery, HierarchyQuery>(hierarchy) };
                let ptr = NonNull::from_mut(&mut query);

                for local in tcroot.iter_mut() {
                    let q: &mut Query<(Mut<Transform>, Mut<GlobalTransform>)> =
                        unsafe { &mut *ptr.as_ptr() };

                    let local: &[(EntityId, Option<EntityId>)] = local.get_mut().as_slice();
                    if local.is_empty() {
                        continue;
                    }

                    s.spawn(async move {
                        let ptr = NonNull::from_mut(q);

                        for &(root, parent) in local {
                            let (transform, global) = q.get_mut(root).expect("should exist");
                            let children = unsafe { hierarchy.get_children_unchecked(root) };

                            let q1: &mut Query<(Mut<Transform>, Mut<GlobalTransform>)> =
                                unsafe { &mut *ptr.as_ptr() };
                            let c2: &[EntityId] =
                                unsafe { transmute::<&[EntityId], &[EntityId]>(children) };
                            let q2: &mut Query<(Mut<Transform>, Mut<GlobalTransform>)> = unsafe {
                                transmute::<
                                    &mut Query<(Mut<Transform>, Mut<GlobalTransform>)>,
                                    &mut Query<(Mut<Transform>, Mut<GlobalTransform>)>,
                                >(q1)
                            };

                            let root_gt: GlobalTransform = if let Some(parent) = parent
                                && let Ok((_, parent_global)) = q1.get_mut(parent)
                            {
                                parent_global.mul_transform(*transform)
                            } else {
                                GlobalTransform::IDENTITY.mul_transform(*transform)
                            };

                            *global.into_inner() = root_gt;

                            if !children.is_empty() {
                                s.spawn(async move {
                                    iterate(s2, c2, h2, root_gt, q2);
                                });
                            }
                        }
                    });
                }
            });
        }

        fn iterate<'sco, 'env>(
            s: &'sco Scope<'sco, 'env, ()>,
            mut entities: &'env [EntityId],
            hierarchy: HierarchyQuery<'env>,
            base_gt: GlobalTransform,
            query: &'env mut Query<
                'env,
                'env,
                (Mut<'static, Transform>, Mut<'static, GlobalTransform>),
            >,
        ) {
            const CHUNK_SIZE: usize = 256; // need benchmark
            const ONE_HALF_CHUNK: usize = CHUNK_SIZE + (CHUNK_SIZE >> 1);

            let ptr: NonNull<Query<(Mut<Transform>, Mut<GlobalTransform>)>> = NonNull::from_mut(query);

            while entities.len() > ONE_HALF_CHUNK {
                let (c1, c2) = unsafe { entities.split_at_unchecked(CHUNK_SIZE) };
                entities = c2;
                let q: &mut Query<(Mut<Transform>, Mut<GlobalTransform>)> =
                    unsafe { &mut *ptr.as_ptr() };
                s.spawn(async move {
                    iterate(s, c1, hierarchy, base_gt, q);
                });
            }

            // A small local queue, to optimize small entity tree.
            let mut buffer: ArrayDeque<(&[EntityId], GlobalTransform), 4> = ArrayDeque::new();
            // SAFETY: buffer.is_empty()
            unsafe { buffer.push_back_unchecked((entities, base_gt)) };

            while let Some((children, parent_gt)) = buffer.pop_front() {
                for &child in children {
                    let Ok((transform, global)) = query.get_mut(child) else {
                        continue;
                    };

                    let new_gt = parent_gt.mul_transform(*transform);
                    *global.into_inner() = new_gt;

                    let children_c: &'env [EntityId] =
                        unsafe { hierarchy.get_children_unchecked(child) };

                    // ```
                    // Parent:
                    //   - Child[0] (Simple Node, without children)
                    //   - Child[1] (Simple Node)
                    //   - Child[2] (Simple Node)
                    //   - ...
                    // ```
                    //
                    // Skip Simple Node
                    if children_c.is_empty() {
                        continue;
                    }

                    // Normal node
                    if children_c.len() > 1 {
                        if buffer.is_full() {
                            // SAFETY: !buffer.is_empty()
                            let (old, old_gt) = unsafe { buffer.pop_front().unwrap_unchecked() };
                            let q: &mut Query<(Mut<Transform>, Mut<GlobalTransform>)> =
                                unsafe { &mut *ptr.as_ptr() };
                            s.spawn(async move {
                                iterate(s, old, hierarchy, old_gt, q);
                            });
                        }

                        // SAFETY: !buffer.is_full()
                        unsafe { buffer.push_back_unchecked((children_c, new_gt)) };
                        continue;
                    }

                    // ```
                    // Parent:
                    //   - Child[0] -> Node[?] // single chain
                    //   - Child[1] -> Node[?]
                    //   - Child[2] -> Node[?]
                    //   - ...
                    // ```
                    //
                    // Inline processing of single-chain node to optimize cache speed.
                    ::core::hint::cold_path();
                    let &child_c = unsafe { children_c.get_unchecked(0) };
                    let Ok((transform, global)) = query.get_mut(child_c) else {
                        continue;
                    };

                    let new_gt_c = new_gt.mul_transform(*transform);
                    *global.into_inner() = new_gt_c;
                    let children_cc: &'env [EntityId] =
                        unsafe { hierarchy.get_children_unchecked(child_c) };

                    if children_cc.is_empty() {
                        continue;
                    }

                    // direct forwarding multi-layer single-linked node.

                    if buffer.is_full() {
                        // SAFETY: !buffer.is_empty()
                        let (old, old_gt) = unsafe { buffer.pop_front().unwrap_unchecked() };
                        let q: &mut Query<(Mut<Transform>, Mut<GlobalTransform>)> =
                            unsafe { &mut *ptr.as_ptr() };
                        s.spawn(async move {
                            iterate(s, old, hierarchy, old_gt, q);
                        });
                    }

                    // SAFETY: !buffer.is_full()
                    unsafe { buffer.push_back_unchecked((children_cc, new_gt_c)) };
                }
            }
        }
    }
}

// -----------------------------------------------------------------------------
