# zlim-math

zlim 引擎的数学库，移植自 `bevy_math`，基于 `glam` 。

## glam

重新导出 `glam` 的大部分内容：

- `Vec2`/`Vec3`/`Vec3A`/`Vec4` 及 `vec2`/`vec3`/`vec3a`/`vec4` 构造函数（`f32`）
- `DVec2`/`DVec3`/`DVec4` 等 `f64` 向量，`IVec*`/`UVec*` 整型向量，`BVec*` 布尔向量
- `Mat2`/`Mat3`/`Mat3A`/`Mat4`、`Quat`、`EulerRot`、`FloatExt`
- swizzles（`Vec2Swizzles`/`Vec3Swizzles`/`Vec4Swizzles`）
- 相机投影辅助：`proj` 、`dproj`（双精度）。
  DirectX & WebGPU NDC convention：Z **[0, 1]**, Y-up 。

## ops

- `FloatPow` — 浮点幂运算扩展（`powf`/`powi` 等，可指定 std/libm 实现）。

## Primitive2d / Primitive3d

- 2D 与 3D 图元的标记 trait。

## rotation2d

- `Rot2` — 2D 旋转（角度 + 正弦余弦缓存）。

## direction

- `Dir2`/`Dir3`/`Dir3A`/`Dir4` — 归一化方向向量（保证单位长度）。
- `InvalidDirectionError` — 构造零向量时的错误类型。

## isometry

- `Isometry2d`/`Isometry3d` — 刚体变换（旋转 + 平移）。

## ray

- `Ray2d`/`Ray3d` — 射线（原点 + 方向）。

## measure

- `Measured2d`/`Measured3d` — 带单位的度量值（Aabb/圆/球等）。

## primitives

- 2D 图元：`Circle`、`Arc2d`、`CircularSector`、`CircularSegment`、`Ellipse`、
  `Annulus`、`Rhombus`、`Plane2d`、`Line2d`、`Segment2d`、`Polyline2d`、
  `Triangle2d`、`Rectangle`、`Polygon`、`ConvexPolygon`、`RegularPolygon`、
  `Capsule2d`、`Ring` 等。
- 3D 图元：球、盒、圆柱、圆锥、棱锥、棱柱、环面、胶囊体、无限平面、线段等。
- `HalfSpace`、`Inset`（内缩）、`ViewFrustum`（视锥体，含 `corners`/`from_camera_origin` 等）。

## matrix

- `reflection_matrix` — 平面反射矩阵。

## float_ord

- `FloatOrd` — 浮点数的全序包装（可作为 HashMap/HashSet 键）。

## aspect_ratio

- `AspectRatio` — 宽高比（构造/验证/换算像素尺寸）。
- `AspectRatioError` — 宽高比验证错误。

## compass

- `CompassOctant`/`CompassQuadrant` — 罗盘八方位/四方位枚举。

## rects

- `Rect`/`IRect`/`URect` — 浮点/有符号整型/无符号整型矩形（含 inset/膨胀、交集、`from_center_half_size` 等）。

## bounding

- 包围体：`bounded2d`（`BoundingCircle`、`Aabb2d` 等）、`bounded3d`（`BoundingSphere`、`Aabb3d` 等）。
- 射线检测：`raycast2d`/`raycast3d`（`RayCast2d`/`RayCast3d`，图元/包围体相交）。

## curve

- `Curve` trait（`sample`/`sample_clamped` 等）及其实现：
  - `cores`：`EvenCore`/`UnevenCore`（均匀/非均匀采样核心）。
  - `adaptors`：映射、链式、拼接、反向、重采样等适配器。
  - `derivatives`：导数曲线。
  - `easing`：缓动函数（线性、平滑、阶梯等）。
  - `interval`：`Interval`/`interval`（参数区间，`Normalized`/`Unit`/`Everything` 等）。
  - `iterable`：可迭代采样。
  - `sample_curves`：常用采样曲线（指数、幂、logistic、噪声等）。

## cubic_splines

- `CubicBezier`、`CubicHermite`、`CubicCardinalSpline`、`CubicBSpline`、
  `CubicNurbs`（含 `CubicNurbsError`）、`LinearSpline`。
- `CubicSegment`/`CubicCurve`、`RationalSegment`/`RationalCurve`。
- 生成器 trait：`CubicGenerator`、`CyclicCubicGenerator`、`RationalGenerator`。
- 错误类型：`CubicBezierError`、`InsufficientDataError`。

## sampling（feature `rand`）

- `FromRng` — 从随机数生成器构造。
- `ShapeSample` — 图元/包围体表面与内部采样。
- 子模块：`shape_sampling`、`mesh_sampling`、`standard`。

## common_traits

- `ScalarField`、`VectorSpace`、`NormedVectorSpace` — 数学空间 trait。
- `StableInterpolate`/`TryStableInterpolate` — 稳定插值（向量/标量/区间等）。
- `HasTangent` — 切线提取。

## affine3

- `Affine3Ext` — `Affine3A` 扩展（`from_scale_rotation_translation`、`try_inverse` 等）。
