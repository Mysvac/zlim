# zlim-sample

Random sampling library for the zlim engine, adapted from the `sampling` pieces
of `bevy_math` and `bevy_shape`.

## `FromRng`

Construct a value uniformly at random from a `rand` RNG (`from_rng`).  Works
for any type with `StandardUniform: Distribution<T>`, including tuples and
arrays; implemented for the `zlim-math` directions `Dir2`/`Dir3`/`Dir3A` and
rotations `Rot2`/`Quat`.

## `ShapeSample`

Uniformly sample the interior / boundary of a shape primitive
(`sample_interior` / `sample_boundary`), or extract a `Distribution` for
repeated sampling (`interior_dist` / `boundary_dist`, wrapped by
`InteriorOf` / `BoundaryOf`):

- 2D — `Annulus`, `Capsule2d`, `Circle`, `CircularSector`, `Rectangle`,
  `Rhombus`, `Triangle2d`
- 3D — `Capsule3d`, `Cuboid`, `Cylinder`, `Sphere`, `Tetrahedron`,
  `Triangle3d`
- `Extrusion<P>` — sampled as a volume / surface when its base shape also
  implements `ShapeSample`.

## `UniformMeshSampler`

A `Distribution` that samples triangle meshes (`Triangle3d` collections)
uniformly by area, built with `UniformMeshSampler::try_new`.
