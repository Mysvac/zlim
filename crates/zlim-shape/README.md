# zlim-shape

Primitive shape library for the zlim engine, ported from `bevy_shape`.

Defines geometric primitives (2D/3D), rays, bounding volumes, and shape sampling.

## Primitive2d / Primitive3d

Marker traits for 2D and 3D primitives, implemented by every primitive type.
The directions `Dir2`/`Dir3`/`Dir3A` from `zlim-math` also implement them.

## Primitives (2D)

- `Circle`, `Arc2d`, `CircularSector`, `CircularSegment`
- `Ellipse`, `Annulus`, `Rhombus`
- `Plane2d`, `Line2d`, `Segment2d`, `Polyline2d`
- `Triangle2d`, `Rectangle`, `Polygon`, `ConvexPolygon`, `RegularPolygon`
- `Capsule2d`, `Ring`, `Extrusion`

## Primitives (3D)

- `Sphere`, `Cuboid`, `Cylinder`, `Capsule3d`, `Cone`, `ConicalFrustum`
- `Torus`, `Triangle3d`, `Tetrahedron`
- `Plane3d`, `InfinitePlane3d`, `Line3d`, `Segment3d`, `Polyline3d`
- `Extrusion`

## HalfSpace / Inset / ViewFrustum / WindingOrder

- `HalfSpace` — an open half-space defined by a bounding plane.
- `Inset` — trait that uniformly resizes a primitive inward (`inset`).
- `ViewFrustum` — intersection of six `HalfSpace`s (view frustum
  representation).
- `WindingOrder` — clockwise / counter-clockwise / invalid point order.

## measure

- `Measured2d`/`Measured3d` — measurement traits (`perimeter`/`area`,
  `area`/`volume`) implemented by the primitives.

## ray

- `Ray2d`/`Ray3d` — rays (origin + normalized direction), with plane
  intersection helpers.

## bounding

- `BoundingVolume`/`IntersectsVolume` — generic bounding volume traits.
- `Bounded2d`/`Bounded3d` — generate bounding volumes for primitives:
  - 2D: `BoundingCircle`, `Aabb2d`
  - 3D: `BoundingSphere`, `Aabb3d`
- Ray casting: `raycast2d`/`raycast3d` (`RayCast2d`/`RayCast3d`,
  primitive/bounding-volume intersection).

## sampling (feature `rand`)

- `ShapeSample` — uniformly sample the interior/boundary of a shape.
- `sampling::mesh_sampling::UniformMeshSampler` — sample triangle meshes.
