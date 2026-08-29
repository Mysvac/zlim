# zlim-math

Math library for the zlim engine, ported from `bevy_math` and built on `glam`.

## glam

Re-exports all of `glam`:

- `Vec2`/`Vec3`/`Vec3A`/`Vec4` and the `vec2`/`vec3`/`vec3a`/`vec4`
  constructors (`f32`)
- `f64` vectors (`DVec2`/`DVec3`/`DVec4`, …), integer vectors
  (`IVec*`/`UVec*`), boolean vectors (`BVec*`)
- `Mat2`/`Mat3`/`Mat3A`/`Mat4`, `Quat`, `EulerRot`, `FloatExt`
- swizzles (`Vec2Swizzles`/`Vec3Swizzles`/`Vec4Swizzles`)
- camera projection helpers: `proj`, `dproj` (double precision).
  DirectX and WebGPU NDC convention: Z range **[0, 1]**, Y-up.

## ops

- `FloatPow` — floating-point power extensions (`powf`/`powi`, … with
  selectable std/libm backend).

## Primitive2d / Primitive3d

- Marker traits for 2D and 3D primitives.

## rotation2d

- `Rot2` — 2D rotation (angle with cached sine/cosine).

## direction

- `Dir2`/`Dir3`/`Dir3A`/`Dir4` — normalized direction vectors (unit length
  guaranteed).
- `InvalidDirectionError` — error returned when constructing from a zero
  vector.

## isometry

- `Isometry2d`/`Isometry3d` — rigid transforms (rotation + translation).

## ray

- `Ray2d`/`Ray3d` — rays (origin + direction).

## measure

- `Measured2d`/`Measured3d` — measured values with a unit (Aabb, circle,
  sphere, …).

## primitives

- 2D primitives: `Circle`, `Arc2d`, `CircularSector`, `CircularSegment`,
  `Ellipse`, `Annulus`, `Rhombus`, `Plane2d`, `Line2d`, `Segment2d`,
  `Polyline2d`, `Triangle2d`, `Rectangle`, `Polygon`, `ConvexPolygon`,
  `RegularPolygon`, `Capsule2d`, `Ring`, …
- 3D primitives: spheres, boxes, cylinders, cones, pyramids, prisms, tori,
  capsules, infinite planes, line segments, …
- `HalfSpace`, `Inset` (shrink), `ViewFrustum` (with `corners`/
  `from_camera_origin`, …).

## matrix

- `reflection_matrix` — plane reflection matrix.

## float_ord

- `FloatOrd` — total-order wrapper for floats (usable as a `HashMap`/
  `HashSet` key).

## aspect_ratio

- `AspectRatio` — aspect ratio (construction, validation, pixel-size
  conversion).
- `AspectRatioError` — aspect ratio validation error.

## compass

- `CompassOctant`/`CompassQuadrant` — compass octant/quadrant enums.

## rects

- `Rect`/`IRect`/`URect` — float / signed-int / unsigned-int rectangles
  (inset, inflate, intersection, `from_center_half_size`, …).

## bounding

- Bounding volumes: `bounded2d` (`BoundingCircle`, `Aabb2d`, …),
  `bounded3d` (`BoundingSphere`, `Aabb3d`, …).
- Ray casting: `raycast2d`/`raycast3d` (`RayCast2d`/`RayCast3d`,
  primitive/bounding-volume intersection).

## curve

- `Curve` trait (`sample`/`sample_clamped`, …) and its implementations:
  - `cores`: `EvenCore`/`UnevenCore` (even/uneven sampling cores).
  - `adaptors`: mapping, chaining, splicing, reversing, resampling, …
  - `derivatives`: derivative curves.
  - `easing`: easing functions (linear, smooth, stepped, …).
  - `interval`: `Interval`/`interval` (parameter domain:
    `Normalized`/`Unit`/`Everything`, …).
  - `iterable`: iterative sampling.
  - `sample_curves`: common sampling curves (exponential, power, logistic,
    noise, …).

## cubic_splines

- `CubicBezier`, `CubicHermite`, `CubicCardinalSpline`, `CubicBSpline`,
  `CubicNurbs` (with `CubicNurbsError`), `LinearSpline`.
- `CubicSegment`/`CubicCurve`, `RationalSegment`/`RationalCurve`.
- Generator traits: `CubicGenerator`, `CyclicCubicGenerator`,
  `RationalGenerator`.
- Errors: `CubicBezierError`, `InsufficientDataError`.

## sampling (feature `rand`)

- `FromRng` — construction from a random number generator.
- `ShapeSample` — sampling the surface/interior of primitives and bounding
  volumes.
- Submodules: `shape_sampling`, `mesh_sampling`, `standard`.

## common_traits

- `ScalarField`, `VectorSpace`, `NormedVectorSpace` — math space traits.
- `StableInterpolate`/`TryStableInterpolate` — stable interpolation
  (vectors, scalars, intervals, …).
- `HasTangent` — tangent extraction.

## affine3

- `Affine3Ext` — `Affine3A` extensions (`from_scale_rotation_translation`,
  `try_inverse`, …).
