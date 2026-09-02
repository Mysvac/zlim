# zlim-curve

zlim 引擎的曲线库，移植自 `bevy_curve`。

提供 `Curve` trait 以及构建于 `zlim-math` 类型之上的丰富曲线类型与适配器。

## Curve

- `Curve` trait — 曲线具有 [定义域](interval)（`Interval`，`f32` 的非空区间），
  并可在该定义域的每个值处被 [采样]，输出某个固定类型。
- 采样：`sample`（可失败）、`sample_unchecked`（域内）、`sample_clamped`。
- 对一切 `Deref` 目标实现，因此 `&curve`、`Box<dyn Curve<T>>` 也是曲线。

## CurveExt

尺寸化曲线的扩展 trait，提供适配器：

- `map` — 用函数映射采样输出。
- `reparametrize` / `reparametrize_linear` / `reparametrize_by_curve` — 改变
  参数化 / 定义域。
- `graph` — 输出元组 `(t, value)`。
- `zip` — 将两条曲线组合为 `(A, B)` 输出。
- `chain` / `chain_continue` — 首尾拼接曲线。
- `reverse`、`repeat`、`forever`、`ping_pong` — 定义域操作。
- `samples` / `sample_iter*` — 均匀 / 迭代采样。

## CurveResampleExt

- `resample` / `resample_auto` — 栅格化为均匀采样曲线。
- `resample_uneven` / `resample_uneven_auto` — 在显式时刻采样。

## cores

- `EvenCore`/`UnevenCore`（+ `ChunkedUnevenCore`）— 均匀 / 非均匀采样曲线的
  存储核心；`InterpolationDatum` 描述采样时刻与存储样本的关系。

## interval

- `Interval`/`interval` — 非空参数区间（`Interval::UNIT`、
  `Interval::EVERYWHERE` 等）。
- `InvalidIntervalError` — 空区间 / NaN 区间错误。

## easing

- `Ease` / `EaseVectorSpace` — 通过 `interpolating_curve_unbounded` 在值之间
  缓动。
- `EasingCurve<T>` — 由 `EaseFunction` 插值的起点/终点。
- `EaseFunction` — 缓动函数（线性、平滑、阶梯、弹性、弹跳、回退等），
  示意图位于 `images/easefunction`。

## derivatives

- `CurveWithDerivative`/`CurveWithTwoDerivatives` — 含导数的曲线。
- `SampleDerivative`/`SampleTwoDerivatives` — 采样值 +（一/二）阶导数。

## adaptors

`CurveExt` 方法产生的具体适配器类型：`ConstantCurve`、`FunctionCurve`、
`MapCurve`、`ReparamCurve`、`LinearReparamCurve`、`CurveReparamCurve`、
`GraphCurve`、`ZipCurve`、`ChainCurve`、`ReverseCurve`、`RepeatCurve`、
`ForeverCurve`、`PingPongCurve`、`ContinuationCurve`。

## sample_curves

- `SampleCurve`/`SampleAutoCurve` — 均匀采样插值。
- `UnevenSampleCurve`/`UnevenSampleAutoCurve` — 关键帧插值。

## iterable

- 曲线的可迭代采样辅助。

## cubic_splines

- `CubicBezier`、`CubicHermite`、`CubicCardinalSpline`、`CubicBSpline`、
  `CubicNurbs`（含 `CubicNurbsError`）、`LinearSpline`。
- `CubicSegment`/`CubicCurve`、`RationalSegment`/`RationalCurve`。
- 生成器 trait：`CubicGenerator`、`CyclicCubicGenerator`、`RationalGenerator`。
- 错误类型：`CubicBezierError`、`InsufficientDataError`。
- 样条类型也实现了 `Curve` / 导数采样（见 `cubic_splines::curve_impls`）。
