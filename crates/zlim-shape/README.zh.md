# zlim-shape

zlim 引擎的图元形状库，移植自 `bevy_shape`。

定义 2D/3D 几何图元、射线、包围体与形状采样。

## Primitive2d / Primitive3d

2D/3D 图元的标记 trait，所有图元类型均已实现；`zlim-math` 中的方向类型
`Dir2`/`Dir3`/`Dir3A` 也实现了对应 trait。

## 2D 图元

- `Circle`、`Arc2d`、`CircularSector`、`CircularSegment`
- `Ellipse`、`Annulus`、`Rhombus`
- `Plane2d`、`Line2d`、`Segment2d`、`Polyline2d`
- `Triangle2d`、`Rectangle`、`Polygon`、`ConvexPolygon`、`RegularPolygon`
- `Capsule2d`、`Ring`、`Extrusion`

## 3D 图元

- `Sphere`、`Cuboid`、`Cylinder`、`Capsule3d`、`Cone`、`ConicalFrustum`
- `Torus`、`Triangle3d`、`Tetrahedron`
- `Plane3d`、`InfinitePlane3d`、`Line3d`、`Segment3d`、`Polyline3d`
- `Extrusion`

## HalfSpace / Inset / ViewFrustum / WindingOrder

- `HalfSpace` — 由有界平面定义的开放半空间。
- `Inset` — 均匀向内缩放图元的 trait（`inset`）。
- `ViewFrustum` — 六个 `HalfSpace` 的交集（视锥体表示）。
- `WindingOrder` — 顺时针 / 逆时针 / 无效的点序。

## measure

- `Measured2d`/`Measured3d` — 度量 trait（`perimeter`/`area`、`area`/`volume`），
  由图元类型实现。

## ray

- `Ray2d`/`Ray3d` — 射线（原点 + 归一化方向），含平面相交辅助方法。

## bounding

- `BoundingVolume`/`IntersectsVolume` — 通用包围体 trait。
- `Bounded2d`/`Bounded3d` — 为图元生成包围体：
  - 2D：`BoundingCircle`、`Aabb2d`
  - 3D：`BoundingSphere`、`Aabb3d`
- 射线检测：`raycast2d`/`raycast3d`（`RayCast2d`/`RayCast3d`，
  图元/包围体相交）。

## sampling（feature `rand`）

- `ShapeSample` — 均匀采样形状内部 / 边界。
- `sampling::mesh_sampling::UniformMeshSampler` — 三角形网格采样。
