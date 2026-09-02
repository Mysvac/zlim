# zlim-curve

Curve library for the zlim engine, ported from `bevy_curve`.

Provides the `Curve` trait and a rich set of curve types and adaptors built on
`zlim-math` types.

## Curve

- `Curve` trait — a curve has a [domain](interval) (`Interval`, a nonempty
  range of `f32`) and can be [sampled] at every value of that domain, producing
  output of some fixed type.
- Sampling: `sample` (fallible), `sample_unchecked` (in-domain), `sample_clamped`.
- Implemented for all `Deref` targets, so `&curve` / `Box<dyn Curve<T>>` are
  curves too.

## CurveExt

Extension trait for sized curves, providing adaptors:

- `map` — map the sampled output through a function.
- `reparametrize` / `reparametrize_linear` / `reparametrize_by_curve` — change
  the parameterization / domain.
- `graph` — tuple output `(t, value)`.
- `zip` — combine two curves into a `(A, B)` output.
- `chain` / `chain_continue` — concatenate curves end-to-start.
- `reverse`, `repeat`, `forever`, `ping_pong` — domain manipulations.
- `samples` / `sample_iter*` — collect evenly-spaced / iterated samples.

## CurveResampleExt

- `resample` / `resample_auto` — rasterize into evenly-spaced sampled curves.
- `resample_uneven` / `resample_uneven_auto` — sample at explicit times.

## cores

- `EvenCore`/`UnevenCore` (+ `ChunkedUnevenCore`) — storage cores for
  evenly / unevenly sampled curves; `InterpolationDatum` describes how a sample
  time relates to stored samples.

## interval

- `Interval`/`interval` — nonempty parameter domains
  (`Interval::UNIT`, `Interval::EVERYWHERE`, …).
- `InvalidIntervalError` — error for empty / NaN intervals.

## easing

- `Ease` / `EaseVectorSpace` — ease between values via
  `interpolating_curve_unbounded`.
- `EasingCurve<T>` — start/end values interpolated by an `EaseFunction`.
- `EaseFunction` — easing functions (linear, smooth, stepped, elastic, bounce,
  back, …) with plot images in `images/easefunction`.

## derivatives

- `CurveWithDerivative`/`CurveWithTwoDerivatives` — derivative-aware curves.
- `SampleDerivative`/`SampleTwoDerivatives` — sample value + (first/two)
  derivative(s).

## adaptors

Concrete adaptor types produced by `CurveExt` methods:
`ConstantCurve`, `FunctionCurve`, `MapCurve`, `ReparamCurve`,
`LinearReparamCurve`, `CurveReparamCurve`, `GraphCurve`, `ZipCurve`,
`ChainCurve`, `ReverseCurve`, `RepeatCurve`, `ForeverCurve`, `PingPongCurve`,
`ContinuationCurve`.

## sample_curves

- `SampleCurve`/`SampleAutoCurve` — evenly-spaced sample interpolation.
- `UnevenSampleCurve`/`UnevenSampleAutoCurve` — keyframe interpolation.

## iterable

- Iterative sampling helpers over curves.

## cubic_splines

- `CubicBezier`, `CubicHermite`, `CubicCardinalSpline`, `CubicBSpline`,
  `CubicNurbs` (with `CubicNurbsError`), `LinearSpline`.
- `CubicSegment`/`CubicCurve`, `RationalSegment`/`RationalCurve`.
- Generator traits: `CubicGenerator`, `CyclicCubicGenerator`,
  `RationalGenerator`.
- Errors: `CubicBezierError`, `InsufficientDataError`.
- Spline types also implement `Curve`/derivative sampling (see
  `cubic_splines::curve_impls`).
