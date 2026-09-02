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

## rotation2d

- `Rot2` — 2D 旋转（角度 + 正弦余弦缓存）。

## direction

- `Dir2`/`Dir3`/`Dir3A`/`Dir4` — 归一化方向向量（保证单位长度）。
- `InvalidDirectionError` — 构造零向量时的错误类型。

## isometry

- `Isometry2d`/`Isometry3d` — 刚体变换（旋转 + 平移）。

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

## sampling（feature `rand`）

- `FromRng` — 从随机数生成器构造（`Dir2`/`Dir3`/`Dir3A`/`Rot2`/`Quat`）。

## common_traits

- `ScalarField`、`VectorSpace`、`NormedVectorSpace` — 数学空间 trait。
- `StableInterpolate`/`TryStableInterpolate` — 稳定插值（向量/标量/区间等）。
- `HasTangent` — 切线提取。

## affine3

- `Affine3Ext` — `Affine3A` 扩展（`from_scale_rotation_translation`、`try_inverse` 等）。
