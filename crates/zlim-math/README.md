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

## rotation2d

- `Rot2` — 2D rotation (angle with cached sine/cosine).

## direction

- `Dir2`/`Dir3`/`Dir3A`/`Dir4` — normalized direction vectors (unit length
  guaranteed).
- `InvalidDirectionError` — error returned when constructing from a zero
  vector.

## isometry

- `Isometry2d`/`Isometry3d` — rigid transforms (rotation + translation).

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

## sampling (feature `rand`)

- `FromRng` — construction from a random number generator (`Dir2`/`Dir3`/
  `Dir3A`/`Rot2`/`Quat`).

## common_traits

- `ScalarField`, `VectorSpace`, `NormedVectorSpace` — math space traits.
- `StableInterpolate`/`TryStableInterpolate` — stable interpolation
  (vectors, scalars, intervals, …).
- `HasTangent` — tangent extraction.

## affine3

- `Affine3Ext` — `Affine3A` extensions (`from_scale_rotation_translation`,
  `try_inverse`, …).
