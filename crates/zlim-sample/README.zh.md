# zlim-sample

zlim 引擎的随机采样库，改自 `bevy_math` 和 `bevy_shape` 的 sampling 部分。

## `FromRng`

从 `rand` 随机数生成器均匀构造一个值（`from_rng`）。适用于任何满足
`StandardUniform: Distribution<T>` 的类型（含元组与数组）；已为 `zlim-math`
的方向类型 `Dir2`/`Dir3`/`Dir3A` 与旋转 `Rot2`/`Quat` 实现。

## `ShapeSample`

均匀采样几何图元的内部 / 边界（`sample_interior` / `sample_boundary`），
或取出 `Distribution` 以重复采样（`interior_dist` / `boundary_dist`，
由 `InteriorOf` / `BoundaryOf` 包装）：

- 2D —— `Annulus`、`Capsule2d`、`Circle`、`CircularSector`、`Rectangle`、
  `Rhombus`、`Triangle2d`
- 3D —— `Capsule3d`、`Cuboid`、`Cylinder`、`Sphere`、`Tetrahedron`、
  `Triangle3d`
- `Extrusion<P>` —— 当底面形状同样实现 `ShapeSample` 时，按体积 / 表面积
  采样。

## `UniformMeshSampler`

按面积加权的三角形网格（`Triangle3d` 集合）均匀采样 `Distribution`，
通过 `UniformMeshSampler::try_new` 构造。
