# zlim-transform

Transform system built on the `zlim-core` ECS:

Keeps `Transform` (local) and `GlobalTransform` (world) consistent across the
entity hierarchy, with parallel propagation support.

## Public API

| Type | Description |
|---|---|
| `Transform` | Local transform (translation / rotation / scale). `require(GlobalTransform)`. |
| `GlobalTransform` | World transform. **Derived output** — only written by the propagation system. |
| `TransformPropagateStrategy` | Detection strategy: `Default` / `PropagateAll` / `PropagateAllOnce`. |
| `TransformPlugin` | Plugin: registers the two systems into `PostStartup` / `PostUpdate`. |
| `EntityTransformExt` / `EntityCommandsTransformExt` | `reparent_in_place` extensions. |

## Example

```rust
use zlim_app::{App, MainSchedulePlugin};
use zlim_transform::{GlobalTransform, Transform, TransformPlugin};

let mut app = App::new();
app.add_plugins((MainSchedulePlugin, TransformPlugin));
app.build(); // executes the plugins (build → apply → cleanup)

// Build the hierarchy: root -> child
let world = app.main_world_mut();
let mut root = world.spawn(Transform::from_xyz(1.0, 0.0, 0.0), None);
root.with_child(Transform::from_xyz(0.0, 2.0, 0.0)).unwrap();
let child_id = root.children().unwrap()[0];
let _ = root;

// Run once per frame: inside `PostUpdate`,
// `TransformChangeDetection` → `TransformPropagation`
app.update();

// child's world transform = root's world transform × child's local transform
let world = app.main_world_mut();
assert_eq!(
    world.entity(child_id).get::<GlobalTransform>().unwrap(),
    &GlobalTransform::from(Transform::from_xyz(1.0, 2.0, 0.0)),
);
```

## Algorithm

Transform updates are split into two stages, executed in order
(`TransformChangeDetection` → `TransformPropagation`).

### Stage 1: Change detection (`TransformChangeDetection`)

Goal: find every "root of a changed subtree" (an entity that needs updating
while its parent does not) and write it into the `TransformChangeRoot`
resource.

**Default strategy (`Default`):**

1. **Handle ReparentSignal messages**: read this frame's `ReparentSignal` messages (sent
   by `zlim-core`'s reparent operation). For each entity, compute
   "parent-global × own-local" and compare it against the current
   `GlobalTransform`; mark the `Transform` as changed only if they differ
   (floating-point comparison may misfire, but the impact is small).

2. **Collect changed entities**: sequentially iterate the query
   (`Query::iter_slice_mut`) and treat every entity whose
   `Transform.is_changed()` is true as a "root candidate" (together with its
   parent), written into a temporary buffer, while collecting its direct
   children into a pending queue. This step is **O(N) sequential access**
   with an excellent cache hit rate.

3. **Pollution pass (downward marking)**: pop nodes from the pending queue;
   mark unmarked nodes with `set_changed()` and enqueue their children;
   **skip already-marked nodes** (their subtree is handled by their own
   traversal, avoiding duplicates). Worst case is O(N) random access; it is a
   no-op when nothing changed.

4. **Filter the real roots**: filter the candidate set `(id, parent)`: an
   entity is a true root of a changed subtree only when its parent has no
   `Transform` component or is unchanged.

**`PropagateAll` strategy**: skips steps 1-3 and directly collects every
"root" (no parent, or whose parent has no `Transform` component). Stage 2 is
then always O(N) random access (full-tree propagation). Suitable for highly
dynamic scenes.

**`PropagateAllOnce`**: same as `PropagateAll`, but only for a single frame;
it automatically reverts to `Default` at the end of the frame. Useful for
scene switches (that frame updates the whole tree anyway, so a full
propagation skips the detection overhead).

### Stage 2: Propagation (`TransformPropagation`)

Iterate the root list collected in stage 1; for each root compute
`this.GlobalTransform = super.GlobalTransform * this.Transform` (or
`IDENTITY × this.Transform` when there is no parent), then recurse downwards
with the same operation for every child.

## Parallelism

Whether propagation is parallel depends on the `zlim_task` dependency: when
`zlim_task` is built multi-threaded, this crate enables multi-thread support
as well.

In multi-threaded mode, every step is parallelizable except stage 1-1
("handle ReparentSignal messages").

The default task pool is `zlim_task`'s `MainTaskPool`, which performs many
scheduling optimizations internally.
