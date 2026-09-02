//! # zlim-curve
//!
//! The [`Curve`] trait, providing a domain-agnostic description of curves.
//!
//! ## Overview
//!
//! At a high level, [`Curve`] is a trait that abstracts away the implementation details of curves,
//! which comprise any kind of data parametrized by a single continuous variable. For example, that
//! variable could represent time, in which case a curve would represent a value that changes over
//! time, as in animation; on the other hand, it could represent something like displacement or
//! distance, as in graphs, gradients, and curves in space.
//!
//! The trait itself has two fundamental components: a curve must have a [domain], which is a nonempty
//! range of `f32` values, and it must be able to be [sampled] on every one of those values, producing
//! output of some fixed type.
//!
//! A primary goal of the trait is to allow interfaces to simply accept `impl Curve<T>` as input
//! rather than requiring for input curves to be defined in data in any particular way. This is
//! supported by a number of interface methods which allow [changing parametrizations], [mapping output],
//! and [rasterization].
//!
//! [domain]: Curve::domain
//! [sampled]: Curve::sample
//! [changing parametrizations]: CurveExt::reparametrize
//! [mapping output]: CurveExt::map
//! [rasterization]: CurveResampleExt::resample

// -----------------------------------------------------------------------------
// Modules

pub mod adaptors;
pub mod cores;
pub mod cubic_splines;
pub mod derivatives;
pub mod easing;
pub mod interval;
pub mod iterable;

pub mod sample_curves;

// zlim_curve re-exports all commonly-needed curve-related items.
pub use adaptors::*;
pub use cores::{EvenCore, UnevenCore};
pub use cubic_splines::*;
pub use easing::*;
pub use interval::{Interval, interval};
pub use sample_curves::*;

// -----------------------------------------------------------------------------
// Curve

use core::fmt::{Display, Formatter};
use core::{marker::PhantomData, ops::Deref};

use interval::InvalidIntervalError;
use zlim_math::StableInterpolate;
use zlim_math::VectorSpace;

/// A trait for a type that can represent values of type `T` parametrized over a fixed interval.
///
/// Typical examples of this are actual geometric curves where `T: VectorSpace`, but other kinds
/// of output data can be represented as well. See the [module-level documentation] for details.
///
/// [module-level documentation]: self
pub trait Curve<T> {
    /// The interval over which this curve is parametrized.
    ///
    /// This is the range of values of `t` where we can sample the curve and receive valid output.
    fn domain(&self) -> Interval;

    /// Sample a point on this curve at the parameter value `t`, extracting the associated value.
    ///
    /// This is the unchecked version of sampling, which should only be used if the sample time `t`
    /// is already known to lie within the curve's domain.
    ///
    /// Values sampled from outside of a curve's domain are generally considered invalid; data which
    /// is nonsensical or otherwise useless may be returned in such a circumstance, and extrapolation
    /// beyond a curve's domain should not be relied upon.
    fn sample_unchecked(&self, t: f32) -> T;

    /// Sample a point on this curve at the parameter value `t`, returning `None` if the point is
    /// outside of the curve's domain.
    fn sample(&self, t: f32) -> Option<T> {
        match self.domain().contains(t) {
            true => Some(self.sample_unchecked(t)),
            false => None,
        }
    }

    /// Sample a point on this curve at the parameter value `t`, clamping `t` to lie inside the
    /// domain of the curve.
    fn sample_clamped(&self, t: f32) -> T {
        let t = self.domain().clamp(t);
        self.sample_unchecked(t)
    }
}

impl<T, C, D> Curve<T> for D
where
    C: Curve<T> + ?Sized,
    D: Deref<Target = C>,
{
    fn domain(&self) -> Interval {
        <C as Curve<T>>::domain(self)
    }

    fn sample_unchecked(&self, t: f32) -> T {
        <C as Curve<T>>::sample_unchecked(self, t)
    }
}

// -----------------------------------------------------------------------------
// CurveExt

/// Extension trait implemented by [curves], allowing access to a number of adaptors and
/// convenience methods.
///
/// This trait is automatically implemented for all curves that are `Sized`. In particular,
/// it is implemented for types like `Box<dyn Curve<T>>`. `CurveExt` is not dyn-compatible
/// itself.
///
/// For more information, see the [module-level documentation].
///
/// [curves]: Curve
/// [module-level documentation]: self
pub trait CurveExt<T>: Curve<T> + Sized {
    /// Sample a collection of `n >= 0` points on this curve at the parameter values `t_n`,
    /// returning `None` if the point is outside of the curve's domain.
    ///
    /// The samples are returned in the same order as the parameter values `t_n` were provided and
    /// will include all results. This leaves the responsibility for things like filtering and
    /// sorting to the user for maximum flexibility.
    fn sample_iter(&self, iter: impl IntoIterator<Item = f32>) -> impl Iterator<Item = Option<T>> {
        iter.into_iter().map(|t| self.sample(t))
    }

    /// Sample a collection of `n >= 0` points on this curve at the parameter values `t_n`,
    /// extracting the associated values. This is the unchecked version of sampling, which should
    /// only be used if the sample times `t_n` are already known to lie within the curve's domain.
    ///
    /// Values sampled from outside of a curve's domain are generally considered invalid; data
    /// which is nonsensical or otherwise useless may be returned in such a circumstance, and
    /// extrapolation beyond a curve's domain should not be relied upon.
    ///
    /// The samples are returned in the same order as the parameter values `t_n` were provided and
    /// will include all results. This leaves the responsibility for things like filtering and
    /// sorting to the user for maximum flexibility.
    fn sample_iter_unchecked(
        &self,
        iter: impl IntoIterator<Item = f32>,
    ) -> impl Iterator<Item = T> {
        iter.into_iter().map(|t| self.sample_unchecked(t))
    }

    /// Sample a collection of `n >= 0` points on this curve at the parameter values `t_n`,
    /// clamping `t_n` to lie inside the domain of the curve.
    ///
    /// The samples are returned in the same order as the parameter values `t_n` were provided and
    /// will include all results. This leaves the responsibility for things like filtering and
    /// sorting to the user for maximum flexibility.
    fn sample_iter_clamped(&self, iter: impl IntoIterator<Item = f32>) -> impl Iterator<Item = T> {
        iter.into_iter().map(|t| self.sample_clamped(t))
    }

    /// Create a new curve by mapping the values of this curve via a function `f`; i.e., if the
    /// sample at time `t` for this curve is `x`, the value at time `t` on the new curve will be
    /// `f(x)`.
    #[must_use]
    fn map<S, F>(self, f: F) -> MapCurve<T, S, Self, F>
    where
        F: Fn(T) -> S,
    {
        MapCurve {
            preimage: self,
            f,
            _phantom: PhantomData,
        }
    }

    /// Create a new [`Curve`] whose parameter space is related to the parameter space of this
    /// curve by `f`.
    ///
    /// For each time `t`, the sample from the new curve at time `t` is the sample from
    /// this curve at time `f(t)`. The given `domain` will be the domain of the new curve.
    /// The function `f` is expected to take `domain` into `self.domain()`.
    ///
    /// Note that this is the opposite of what one might expect intuitively; for example, if this
    /// curve has a parameter domain of `[0, 1]`, then stretching the parameter domain to
    /// `[0, 2]` would be performed as follows, dividing by what might be perceived as the scaling
    /// factor rather than multiplying.
    #[must_use]
    fn reparametrize<F>(self, domain: Interval, f: F) -> ReparamCurve<T, Self, F>
    where
        F: Fn(f32) -> f32,
    {
        ReparamCurve {
            domain,
            base: self,
            f,
            _phantom: PhantomData,
        }
    }

    /// Linearly reparametrize this [`Curve`], producing a new curve whose domain is the given
    /// `domain` instead of the current one. This operation is only valid for curves with bounded
    /// domains.
    ///
    /// # Errors
    ///
    /// If either this curve's domain or the given `domain` is unbounded, an error is returned.
    fn reparametrize_linear(
        self,
        domain: Interval,
    ) -> Result<LinearReparamCurve<T, Self>, LinearReparamError> {
        if !self.domain().is_bounded() {
            return Err(LinearReparamError::SourceCurveUnbounded);
        }

        if !domain.is_bounded() {
            return Err(LinearReparamError::TargetIntervalUnbounded);
        }

        Ok(LinearReparamCurve {
            base: self,
            new_domain: domain,
            _phantom: PhantomData,
        })
    }

    /// Reparametrize this [`Curve`] by sampling from another curve.
    ///
    /// The resulting curve samples at time `t` by first sampling `other` at time `t`, which produces
    /// another sample time `s` which is then used to sample this curve. The domain of the resulting
    /// curve is the domain of `other`.
    #[must_use]
    fn reparametrize_by_curve<C>(self, other: C) -> CurveReparamCurve<T, Self, C>
    where
        C: Curve<f32>,
    {
        CurveReparamCurve {
            base: self,
            reparam_curve: other,
            _phantom: PhantomData,
        }
    }

    /// Create a new [`Curve`] which is the graph of this one; that is, its output echoes the sample
    /// time as part of a tuple.
    ///
    /// For example, if this curve outputs `x` at time `t`, then the produced curve will produce
    /// `(t, x)` at time `t`. In particular, if this curve is a `Curve<T>`, the output of this method
    /// is a `Curve<(f32, T)>`.
    #[must_use]
    fn graph(self) -> GraphCurve<T, Self> {
        GraphCurve {
            base: self,
            _phantom: PhantomData,
        }
    }

    /// Create a new [`Curve`] by zipping this curve together with another.
    ///
    /// The sample at time `t` in the new curve is `(x, y)`, where `x` is the sample of `self` at
    /// time `t` and `y` is the sample of `other` at time `t`. The domain of the new curve is the
    /// intersection of the domains of its constituents.
    ///
    /// # Errors
    ///
    /// If the domain intersection would be empty, an error is returned instead.
    fn zip<S, C>(self, other: C) -> Result<ZipCurve<T, S, Self, C>, InvalidIntervalError>
    where
        C: Curve<S> + Sized,
    {
        let domain = self.domain().intersect(other.domain())?;
        Ok(ZipCurve {
            domain,
            first: self,
            second: other,
            _phantom: PhantomData,
        })
    }

    /// Create a new [`Curve`] by composing this curve end-to-start with another, producing another curve
    /// with outputs of the same type. The domain of the other curve is translated so that its start
    /// coincides with where this curve ends.
    ///
    /// # Errors
    ///
    /// A [`ChainError`] is returned if this curve's domain doesn't have a finite end or if
    /// `other`'s domain doesn't have a finite start.
    fn chain<C>(self, other: C) -> Result<ChainCurve<T, Self, C>, ChainError>
    where
        C: Curve<T>,
    {
        if !self.domain().has_finite_end() {
            return Err(ChainError::FirstEndInfinite);
        }
        if !other.domain().has_finite_start() {
            return Err(ChainError::SecondStartInfinite);
        }
        Ok(ChainCurve {
            first: self,
            second: other,
            _phantom: PhantomData,
        })
    }

    /// Create a new [`Curve`] inverting this curve on the x-axis, producing another curve with
    /// outputs of the same type, effectively playing backwards starting at `self.domain().end()`
    /// and transitioning over to `self.domain().start()`. The domain of the new curve is still the
    /// same.
    ///
    /// # Errors
    ///
    /// A [`ReverseError`] is returned if this curve's domain isn't bounded.
    fn reverse(self) -> Result<ReverseCurve<T, Self>, ReverseError> {
        self.domain()
            .is_bounded()
            .then(|| ReverseCurve {
                curve: self,
                _phantom: PhantomData,
            })
            .ok_or(ReverseError::SourceDomainEndInfinite)
    }

    /// Create a new [`Curve`] repeating this curve `N` times, producing another curve with outputs
    /// of the same type. The domain of the new curve will be bigger by a factor of `n + 1`.
    ///
    /// # Notes
    ///
    /// - this doesn't guarantee a smooth transition from one occurrence of the curve to its next
    ///   iteration. The curve will make a jump if `self.domain().start() != self.domain().end()`!
    /// - for `count == 0` the output of this adaptor is basically identical to the previous curve
    /// - the value at the transitioning points (`domain.end() * n` for `n >= 1`) in the results is the
    ///   value at `domain.end()` in the original curve
    ///
    /// # Errors
    ///
    /// A [`RepeatError`] is returned if this curve's domain isn't bounded.
    fn repeat(self, count: usize) -> Result<RepeatCurve<T, Self>, RepeatError> {
        self.domain()
            .is_bounded()
            .then(|| {
                let domain = self.domain();
                let end = domain.end() + domain.length() * count as f32;
                // This unwrap always succeeds because `curve` has a valid Interval as its domain and the
                // length of `curve` cannot be NAN. It's still fine if it's infinity.
                let domain = Interval::new(domain.start(), end).unwrap();
                RepeatCurve {
                    domain,
                    curve: self,
                    _phantom: PhantomData,
                }
            })
            .ok_or(RepeatError::SourceDomainUnbounded)
    }

    /// Create a new [`Curve`] repeating this curve forever, producing another curve with
    /// outputs of the same type. The domain of the new curve will be unbounded.
    ///
    /// # Notes
    ///
    /// - this doesn't guarantee a smooth transition from one occurrence of the curve to its next
    ///   iteration. The curve will make a jump if `self.domain().start() != self.domain().end()`!
    /// - the value at the transitioning points (`domain.end() * n` for `n >= 1`) in the results is the
    ///   value at `domain.end()` in the original curve
    ///
    /// # Errors
    ///
    /// A [`RepeatError`] is returned if this curve's domain isn't bounded.
    fn forever(self) -> Result<ForeverCurve<T, Self>, RepeatError> {
        self.domain()
            .is_bounded()
            .then(|| ForeverCurve {
                curve: self,
                _phantom: PhantomData,
            })
            .ok_or(RepeatError::SourceDomainUnbounded)
    }

    /// Create a new [`Curve`] chaining the original curve with its inverse, producing
    /// another curve with outputs of the same type. The domain of the new curve will be twice as
    /// long. The transition point is guaranteed to not make any jumps.
    ///
    /// # Errors
    ///
    /// A [`PingPongError`] is returned if this curve's domain isn't right-finite.
    fn ping_pong(self) -> Result<PingPongCurve<T, Self>, PingPongError> {
        self.domain()
            .has_finite_end()
            .then(|| PingPongCurve {
                curve: self,
                _phantom: PhantomData,
            })
            .ok_or(PingPongError::SourceDomainEndInfinite)
    }

    /// Create a new [`Curve`] by composing this curve end-to-start with another, producing another
    /// curve with outputs of the same type. The domain of the other curve is translated so that
    /// its start coincides with where this curve ends.
    ///
    ///
    /// Additionally the transition of the samples is guaranteed to make no sudden jumps. This is
    /// useful if you really just know about the shapes of your curves and don't want to deal with
    /// stitching them together properly when it would just introduce useless complexity. It is
    /// realized by translating the other curve so that its start sample point coincides with the
    /// current curves' end sample point.
    ///
    /// # Errors
    ///
    /// A [`ChainError`] is returned if this curve's domain doesn't have a finite end or if
    /// `other`'s domain doesn't have a finite start.
    fn chain_continue<C>(self, other: C) -> Result<ContinuationCurve<T, Self, C>, ChainError>
    where
        T: VectorSpace,
        C: Curve<T>,
    {
        if !self.domain().has_finite_end() {
            return Err(ChainError::FirstEndInfinite);
        }
        if !other.domain().has_finite_start() {
            return Err(ChainError::SecondStartInfinite);
        }

        let offset = self.sample_unchecked(self.domain().end())
            - other.sample_unchecked(self.domain().start());

        Ok(ContinuationCurve {
            first: self,
            second: other,
            offset,
            _phantom: PhantomData,
        })
    }

    /// Extract an iterator over evenly-spaced samples from this curve.
    ///
    /// # Errors
    ///
    /// If `samples` is less than 2 or if this curve has unbounded domain, a [`ResamplingError`]
    /// is returned.
    fn samples(&self, samples: usize) -> Result<impl Iterator<Item = T>, ResamplingError> {
        if samples < 2 {
            return Err(ResamplingError::NotEnoughSamples(samples));
        }
        if !self.domain().is_bounded() {
            return Err(ResamplingError::UnboundedDomain);
        }

        // Unwrap on `spaced_points` always succeeds because its error conditions are handled
        // above.
        Ok(self
            .domain()
            .spaced_points(samples)
            .unwrap()
            .map(|t| self.sample_unchecked(t)))
    }

    /// Borrow this curve rather than taking ownership of it. This is essentially an alias for a
    /// prefix `&`; the point is that intermediate operations can be performed while retaining
    /// access to the original curve.
    fn by_ref(&self) -> &Self {
        self
    }

    /// Flip this curve so that its tuple output is arranged the other way.
    #[must_use]
    fn flip<U, V>(self) -> impl Curve<(V, U)>
    where
        Self: CurveExt<(U, V)>,
    {
        self.map(|(u, v)| (v, u))
    }
}

impl<C, T> CurveExt<T> for C where C: Curve<T> {}

// -----------------------------------------------------------------------------
// CurveResampleExt

/// Extension trait implemented by [curves], allowing access to generic resampling methods as
/// well as those based on [stable interpolation].
///
/// This trait is automatically implemented for all curves.
///
/// For more information, see the [module-level documentation].
///
/// [curves]: Curve
/// [stable interpolation]: zlim_math::StableInterpolate
/// [module-level documentation]: self
pub trait CurveResampleExt<T>: Curve<T> {
    /// Resample this [`Curve`] to produce a new one that is defined by interpolation over equally
    /// spaced sample values, using the provided `interpolation` to interpolate between adjacent samples.
    /// The curve is interpolated on `segments` segments between samples. For example, if `segments` is 1,
    /// only the start and end points of the curve are used as samples; if `segments` is 2, a sample at
    /// the midpoint is taken as well, and so on.
    ///
    /// The interpolation takes two values by reference together with a scalar parameter and
    /// produces an owned value. The expectation is that `interpolation(&x, &y, 0.0)` and
    /// `interpolation(&x, &y, 1.0)` are equivalent to `x` and `y` respectively.
    ///
    /// # Errors
    ///
    /// If `segments` is zero or if this curve has unbounded domain, then a [`ResamplingError`] is
    /// returned.
    fn resample<I>(
        &self,
        segments: usize,
        interpolation: I,
    ) -> Result<SampleCurve<T, I>, ResamplingError>
    where
        I: Fn(&T, &T, f32) -> T,
    {
        let samples = self.samples(segments + 1)?.collect();
        Ok(SampleCurve {
            core: EvenCore {
                domain: self.domain(),
                samples,
            },
            interpolation,
        })
    }

    /// Resample this [`Curve`] to produce a new one that is defined by interpolation over equally
    /// spaced sample values, using [automatic interpolation] to interpolate between adjacent samples.
    /// The curve is interpolated on `segments` segments between samples. For example, if `segments` is 1,
    /// only the start and end points of the curve are used as samples; if `segments` is 2, a sample at
    /// the midpoint is taken as well, and so on.
    ///
    /// # Errors
    ///
    /// If `segments` is zero or if this curve has unbounded domain, a [`ResamplingError`] is returned.
    ///
    /// [automatic interpolation]: zlim_math::common_traits::StableInterpolate
    fn resample_auto(&self, segments: usize) -> Result<SampleAutoCurve<T>, ResamplingError>
    where
        T: StableInterpolate,
    {
        let samples = self.samples(segments + 1)?.collect();
        Ok(SampleAutoCurve {
            core: EvenCore {
                domain: self.domain(),
                samples,
            },
        })
    }

    /// Resample this [`Curve`] to produce a new one that is defined by interpolation over samples
    /// taken at a given set of times. The given `interpolation` is used to interpolate adjacent
    /// samples, and the `sample_times` are expected to contain at least two valid times within the
    /// curve's domain interval.
    ///
    /// Redundant sample times, non-finite sample times, and sample times outside of the domain
    /// are filtered out. With an insufficient quantity of data, a [`ResamplingError`] is
    /// returned.
    ///
    /// The domain of the produced curve stretches between the first and last sample times of the
    /// iterator.
    ///
    /// The interpolation takes two values by reference together with a scalar parameter and
    /// produces an owned value. The expectation is that `interpolation(&x, &y, 0.0)` and
    /// `interpolation(&x, &y, 1.0)` are equivalent to `x` and `y` respectively.
    ///
    /// # Errors
    ///
    /// If `sample_times` doesn't contain at least two distinct times after filtering, a
    /// [`ResamplingError`] is returned.
    fn resample_uneven<I>(
        &self,
        sample_times: impl IntoIterator<Item = f32>,
        interpolation: I,
    ) -> Result<UnevenSampleCurve<T, I>, ResamplingError>
    where
        I: Fn(&T, &T, f32) -> T,
    {
        let domain = self.domain();
        let mut times: Vec<f32> = sample_times
            .into_iter()
            .filter(|t| t.is_finite() && domain.contains(*t))
            .collect();
        times.sort_by(f32::total_cmp);
        times.dedup();
        if times.len() < 2 {
            return Err(ResamplingError::NotEnoughSamples(times.len()));
        }
        let samples = times.iter().map(|t| self.sample_unchecked(*t)).collect();
        Ok(UnevenSampleCurve {
            core: UnevenCore { times, samples },
            interpolation,
        })
    }

    /// Resample this [`Curve`] to produce a new one that is defined by [automatic interpolation] over
    /// samples taken at the given set of times. The given `sample_times` are expected to contain at least
    /// two valid times within the curve's domain interval.
    ///
    /// Redundant sample times, non-finite sample times, and sample times outside of the domain
    /// are simply filtered out. With an insufficient quantity of data, a [`ResamplingError`] is
    /// returned.
    ///
    /// The domain of the produced [`UnevenSampleAutoCurve`] stretches between the first and last
    /// sample times of the iterator.
    ///
    /// # Errors
    ///
    /// If `sample_times` doesn't contain at least two distinct times after filtering, a
    /// [`ResamplingError`] is returned.
    ///
    /// [automatic interpolation]: zlim_math::common_traits::StableInterpolate
    fn resample_uneven_auto(
        &self,
        sample_times: impl IntoIterator<Item = f32>,
    ) -> Result<UnevenSampleAutoCurve<T>, ResamplingError>
    where
        T: StableInterpolate,
    {
        let domain = self.domain();
        let mut times: Vec<f32> = sample_times
            .into_iter()
            .filter(|t| t.is_finite() && domain.contains(*t))
            .collect();
        times.sort_by(f32::total_cmp);
        times.dedup();
        if times.len() < 2 {
            return Err(ResamplingError::NotEnoughSamples(times.len()));
        }
        let samples = times.iter().map(|t| self.sample_unchecked(*t)).collect();
        Ok(UnevenSampleAutoCurve {
            core: UnevenCore { times, samples },
        })
    }
}

impl<C, T> CurveResampleExt<T> for C where C: Curve<T> + ?Sized {}

// -----------------------------------------------------------------------------
// LinearReparamError

/// An error indicating that a linear reparameterization couldn't be performed because of
/// malformed inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinearReparamError {
    /// The source curve that was to be reparametrized had unbounded domain.
    SourceCurveUnbounded,

    /// The target interval for reparameterization was unbounded.
    TargetIntervalUnbounded,
}

impl Display for LinearReparamError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::SourceCurveUnbounded => f.write_str("This curve has unbounded domain"),
            Self::TargetIntervalUnbounded => {
                f.write_str("The target interval for reparameterization is unbounded")
            }
        }
    }
}

impl core::error::Error for LinearReparamError {}

// -----------------------------------------------------------------------------
// ReverseError

/// An error indicating that a reversion of a curve couldn't be performed because of
/// malformed inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReverseError {
    /// The source curve that was to be reversed had unbounded domain end.
    SourceDomainEndInfinite,
}

impl Display for ReverseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.write_str("This curve has an unbounded domain end")
    }
}

impl core::error::Error for ReverseError {}

// -----------------------------------------------------------------------------
// RepeatError

/// An error indicating that a repetition of a curve couldn't be performed because of malformed
/// inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatError {
    /// The source curve that was to be repeated had unbounded domain.
    SourceDomainUnbounded,
}

impl Display for RepeatError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.write_str("This curve has an unbounded domain")
    }
}

impl core::error::Error for RepeatError {}

// -----------------------------------------------------------------------------
// PingPongError

/// An error indicating that a ping ponging of a curve couldn't be performed because of
/// malformed inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PingPongError {
    /// The source curve that was to be ping ponged had unbounded domain end.
    SourceDomainEndInfinite,
}

impl Display for PingPongError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.write_str("This curve has an unbounded domain end")
    }
}

impl core::error::Error for PingPongError {}

// -----------------------------------------------------------------------------
// ChainError

/// An error indicating that an end-to-end composition couldn't be performed because of
/// malformed inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainError {
    /// The right endpoint of the first curve was infinite.
    FirstEndInfinite,

    /// The left endpoint of the second curve was infinite.
    SecondStartInfinite,
}

impl Display for ChainError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::FirstEndInfinite => f.write_str("The first curve's domain has an infinite end"),
            Self::SecondStartInfinite => {
                f.write_str("The second curve's domain has an infinite start")
            }
        }
    }
}

impl core::error::Error for ChainError {}

// -----------------------------------------------------------------------------
// ResamplingError

/// An error indicating that a resampling operation could not be performed because of
/// malformed inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResamplingError {
    /// This resampling operation was not provided with enough samples to have well-formed output.
    NotEnoughSamples(usize),

    /// This resampling operation failed because of an unbounded interval.
    UnboundedDomain,
}

impl Display for ResamplingError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotEnoughSamples(_) => {
                f.write_str("Not enough unique samples to construct resampled curve")
            }
            Self::UnboundedDomain => {
                f.write_str("Could not resample because this curve has unbounded domain")
            }
        }
    }
}

impl core::error::Error for ResamplingError {}

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use approx::{AbsDiffEq, assert_abs_diff_eq};
    use core::f32::consts::TAU;
    use zlim_math::{Quat, Vec2, ops};

    #[test]
    fn curve_can_be_made_into_an_object() {
        let curve = ConstantCurve::new(Interval::UNIT, 42.0);
        let curve: &dyn Curve<f64> = &curve;

        assert_eq!(curve.sample(1.0), Some(42.0));
        assert_eq!(curve.sample(2.0), None);
    }

    #[test]
    fn constant_curves() {
        let curve = ConstantCurve::new(Interval::EVERYWHERE, 5.0);
        assert!(curve.sample_unchecked(-35.0) == 5.0);

        let curve = ConstantCurve::new(Interval::UNIT, true);
        assert!(curve.sample_unchecked(2.0));
        assert!(curve.sample(2.0).is_none());
    }

    #[test]
    fn function_curves() {
        let curve = FunctionCurve::new(Interval::EVERYWHERE, |t| t * t);
        assert!(curve.sample_unchecked(2.0).abs_diff_eq(&4.0, f32::EPSILON));
        assert!(curve.sample_unchecked(-3.0).abs_diff_eq(&9.0, f32::EPSILON));

        let curve = FunctionCurve::new(interval(0.0, f32::INFINITY).unwrap(), ops::log2);
        assert_eq!(curve.sample_unchecked(3.5), ops::log2(3.5));
        assert!(curve.sample_unchecked(-1.0).is_nan());
        assert!(curve.sample(-1.0).is_none());
    }

    #[test]
    fn linear_curve() {
        let start = Vec2::ZERO;
        let end = Vec2::new(1.0, 2.0);
        let curve = EasingCurve::new(start, end, EaseFunction::Linear);

        let mid = (start + end) / 2.0;

        [(0.0, start), (0.5, mid), (1.0, end)]
            .into_iter()
            .for_each(|(t, x)| {
                assert!(curve.sample_unchecked(t).abs_diff_eq(x, f32::EPSILON));
            });
    }

    #[test]
    fn easing_curves_step() {
        let start = Vec2::ZERO;
        let end = Vec2::new(1.0, 2.0);

        let curve = EasingCurve::new(start, end, EaseFunction::Steps(4, JumpAt::End));
        [
            (0.0, start),
            (0.249, start),
            (0.250, Vec2::new(0.25, 0.5)),
            (0.499, Vec2::new(0.25, 0.5)),
            (0.500, Vec2::new(0.5, 1.0)),
            (0.749, Vec2::new(0.5, 1.0)),
            (0.750, Vec2::new(0.75, 1.5)),
            (1.0, end),
        ]
        .into_iter()
        .for_each(|(t, x)| {
            assert!(curve.sample_unchecked(t).abs_diff_eq(x, f32::EPSILON));
        });
    }

    #[test]
    fn easing_curves_quadratic() {
        let start = Vec2::ZERO;
        let end = Vec2::new(1.0, 2.0);

        let curve = EasingCurve::new(start, end, EaseFunction::QuadraticIn);
        [
            (0.0, start),
            (0.25, Vec2::new(0.0625, 0.125)),
            (0.5, Vec2::new(0.25, 0.5)),
            (1.0, end),
        ]
        .into_iter()
        .for_each(|(t, x)| {
            assert!(curve.sample_unchecked(t).abs_diff_eq(x, f32::EPSILON),);
        });
    }

    #[expect(
        clippy::neg_multiply,
        reason = "Clippy doesn't like this, but it's correct"
    )]
    #[test]
    fn mapping() {
        let curve = FunctionCurve::new(Interval::EVERYWHERE, |t| t * 3.0 + 1.0);
        let mapped_curve = curve.map(|x| x / 7.0);
        assert_eq!(mapped_curve.sample_unchecked(3.5), (3.5 * 3.0 + 1.0) / 7.0);
        assert_eq!(
            mapped_curve.sample_unchecked(-1.0),
            (-1.0 * 3.0 + 1.0) / 7.0
        );
        assert_eq!(mapped_curve.domain(), Interval::EVERYWHERE);

        let curve = FunctionCurve::new(Interval::UNIT, |t| t * TAU);
        let mapped_curve = curve.map(Quat::from_rotation_z);
        assert_eq!(mapped_curve.sample_unchecked(0.0), Quat::IDENTITY);
        assert!(mapped_curve.sample_unchecked(1.0).is_near_identity());
        assert_eq!(mapped_curve.domain(), Interval::UNIT);
    }

    #[test]
    fn reverse() {
        let curve = FunctionCurve::new(Interval::new(0.0, 1.0).unwrap(), |t| t * 3.0 + 1.0);
        let rev_curve = curve.reverse().unwrap();
        assert_eq!(rev_curve.sample(-0.1), None);
        assert_eq!(rev_curve.sample(0.0), Some(1.0 * 3.0 + 1.0));
        assert_eq!(rev_curve.sample(0.5), Some(0.5 * 3.0 + 1.0));
        assert_eq!(rev_curve.sample(1.0), Some(0.0 * 3.0 + 1.0));
        assert_eq!(rev_curve.sample(1.1), None);

        let curve = FunctionCurve::new(Interval::new(-2.0, 1.0).unwrap(), |t| t * 3.0 + 1.0);
        let rev_curve = curve.reverse().unwrap();
        assert_eq!(rev_curve.sample(-2.1), None);
        assert_eq!(rev_curve.sample(-2.0), Some(1.0 * 3.0 + 1.0));
        assert_eq!(rev_curve.sample(-0.5), Some(-0.5 * 3.0 + 1.0));
        assert_eq!(rev_curve.sample(1.0), Some(-2.0 * 3.0 + 1.0));
        assert_eq!(rev_curve.sample(1.1), None);
    }

    #[test]
    fn repeat() {
        let curve = FunctionCurve::new(Interval::new(0.0, 1.0).unwrap(), |t| t * 3.0 + 1.0);
        let repeat_curve = curve.by_ref().repeat(1).unwrap();
        assert_eq!(repeat_curve.sample(-0.1), None);
        assert_eq!(repeat_curve.sample(0.0), Some(0.0 * 3.0 + 1.0));
        assert_eq!(repeat_curve.sample(0.5), Some(0.5 * 3.0 + 1.0));
        assert_eq!(repeat_curve.sample(0.99), Some(0.99 * 3.0 + 1.0));
        assert_eq!(repeat_curve.sample(1.0), Some(1.0 * 3.0 + 1.0));
        assert_eq!(repeat_curve.sample(1.01), Some(0.01 * 3.0 + 1.0));
        assert_eq!(repeat_curve.sample(1.5), Some(0.5 * 3.0 + 1.0));
        assert_eq!(repeat_curve.sample(1.99), Some(0.99 * 3.0 + 1.0));
        assert_eq!(repeat_curve.sample(2.0), Some(1.0 * 3.0 + 1.0));
        assert_eq!(repeat_curve.sample(2.01), None);

        let repeat_curve = curve.by_ref().repeat(3).unwrap();
        assert_eq!(repeat_curve.sample(2.0), Some(1.0 * 3.0 + 1.0));
        assert_eq!(repeat_curve.sample(3.0), Some(1.0 * 3.0 + 1.0));
        assert_eq!(repeat_curve.sample(4.0), Some(1.0 * 3.0 + 1.0));
        assert_eq!(repeat_curve.sample(5.0), None);

        let repeat_curve = curve.by_ref().forever().unwrap();
        assert_eq!(repeat_curve.sample(-1.0), Some(1.0 * 3.0 + 1.0));
        assert_eq!(repeat_curve.sample(2.0), Some(1.0 * 3.0 + 1.0));
        assert_eq!(repeat_curve.sample(3.0), Some(1.0 * 3.0 + 1.0));
        assert_eq!(repeat_curve.sample(4.0), Some(1.0 * 3.0 + 1.0));
        assert_eq!(repeat_curve.sample(5.0), Some(1.0 * 3.0 + 1.0));
    }

    #[test]
    fn ping_pong() {
        let curve = FunctionCurve::new(Interval::new(0.0, 1.0).unwrap(), |t| t * 3.0 + 1.0);
        let ping_pong_curve = curve.ping_pong().unwrap();
        assert_eq!(ping_pong_curve.sample(-0.1), None);
        assert_eq!(ping_pong_curve.sample(0.0), Some(0.0 * 3.0 + 1.0));
        assert_eq!(ping_pong_curve.sample(0.5), Some(0.5 * 3.0 + 1.0));
        assert_eq!(ping_pong_curve.sample(1.0), Some(1.0 * 3.0 + 1.0));
        assert_eq!(ping_pong_curve.sample(1.5), Some(0.5 * 3.0 + 1.0));
        assert_eq!(ping_pong_curve.sample(2.0), Some(0.0 * 3.0 + 1.0));
        assert_eq!(ping_pong_curve.sample(2.1), None);

        let curve = FunctionCurve::new(Interval::new(-2.0, 2.0).unwrap(), |t| t * 3.0 + 1.0);
        let ping_pong_curve = curve.ping_pong().unwrap();
        assert_eq!(ping_pong_curve.sample(-2.1), None);
        assert_eq!(ping_pong_curve.sample(-2.0), Some(-2.0 * 3.0 + 1.0));
        assert_eq!(ping_pong_curve.sample(-0.5), Some(-0.5 * 3.0 + 1.0));
        assert_eq!(ping_pong_curve.sample(2.0), Some(2.0 * 3.0 + 1.0));
        assert_eq!(ping_pong_curve.sample(4.5), Some(-0.5 * 3.0 + 1.0));
        assert_eq!(ping_pong_curve.sample(6.0), Some(-2.0 * 3.0 + 1.0));
        assert_eq!(ping_pong_curve.sample(6.1), None);
    }

    #[test]
    fn continue_chain() {
        let first = FunctionCurve::new(Interval::new(0.0, 1.0).unwrap(), |t| t * 3.0 + 1.0);
        let second = FunctionCurve::new(Interval::new(0.0, 1.0).unwrap(), |t| t * t);
        let c0_chain_curve = first.chain_continue(second).unwrap();
        assert_eq!(c0_chain_curve.sample(-0.1), None);
        assert_eq!(c0_chain_curve.sample(0.0), Some(0.0 * 3.0 + 1.0));
        assert_eq!(c0_chain_curve.sample(0.5), Some(0.5 * 3.0 + 1.0));
        assert_eq!(c0_chain_curve.sample(1.0), Some(1.0 * 3.0 + 1.0));
        assert_eq!(c0_chain_curve.sample(1.5), Some(1.0 * 3.0 + 1.0 + 0.25));
        assert_eq!(c0_chain_curve.sample(2.0), Some(1.0 * 3.0 + 1.0 + 1.0));
        assert_eq!(c0_chain_curve.sample(2.1), None);
    }

    #[test]
    fn reparameterization() {
        let curve = FunctionCurve::new(interval(1.0, f32::INFINITY).unwrap(), ops::log2);
        let reparametrized_curve = curve
            .by_ref()
            .reparametrize(interval(0.0, f32::INFINITY).unwrap(), ops::exp2);
        assert_abs_diff_eq!(reparametrized_curve.sample_unchecked(3.5), 3.5);
        assert_abs_diff_eq!(reparametrized_curve.sample_unchecked(100.0), 100.0);
        assert_eq!(
            reparametrized_curve.domain(),
            interval(0.0, f32::INFINITY).unwrap()
        );

        let reparametrized_curve = curve.by_ref().reparametrize(Interval::UNIT, |t| t + 1.0);
        assert_abs_diff_eq!(reparametrized_curve.sample_unchecked(0.0), 0.0);
        assert_abs_diff_eq!(reparametrized_curve.sample_unchecked(1.0), 1.0);
        assert_eq!(reparametrized_curve.domain(), Interval::UNIT);
    }

    #[test]
    fn multiple_maps() {
        // Make sure these actually happen in the right order.
        let curve = FunctionCurve::new(Interval::UNIT, ops::exp2);
        let first_mapped = curve.map(ops::log2);
        let second_mapped = first_mapped.map(|x| x * -2.0);
        assert_abs_diff_eq!(second_mapped.sample_unchecked(0.0), 0.0);
        assert_abs_diff_eq!(second_mapped.sample_unchecked(0.5), -1.0);
        assert_abs_diff_eq!(second_mapped.sample_unchecked(1.0), -2.0);
    }

    #[test]
    fn multiple_reparams() {
        // Make sure these happen in the right order too.
        let curve = FunctionCurve::new(Interval::UNIT, ops::exp2);
        let first_reparam = curve.reparametrize(interval(1.0, 2.0).unwrap(), ops::log2);
        let second_reparam = first_reparam.reparametrize(Interval::UNIT, |t| t + 1.0);
        assert_abs_diff_eq!(second_reparam.sample_unchecked(0.0), 1.0);
        assert_abs_diff_eq!(second_reparam.sample_unchecked(0.5), 1.5);
        assert_abs_diff_eq!(second_reparam.sample_unchecked(1.0), 2.0);
    }

    #[test]
    fn resampling() {
        let curve = FunctionCurve::new(interval(1.0, 4.0).unwrap(), ops::log2);

        // Need at least one segment to sample.
        let nice_try = curve.by_ref().resample_auto(0);
        assert!(nice_try.is_err());

        // The values of a resampled curve should be very close at the sample points.
        // Because of denominators, it's not literally equal.
        // (This is a tradeoff against O(1) sampling.)
        let resampled_curve = curve.by_ref().resample_auto(100).unwrap();
        for test_pt in curve.domain().spaced_points(101).unwrap() {
            let expected = curve.sample_unchecked(test_pt);
            assert_abs_diff_eq!(
                resampled_curve.sample_unchecked(test_pt),
                expected,
                epsilon = 1e-6
            );
        }

        // Another example.
        let curve = FunctionCurve::new(interval(0.0, TAU).unwrap(), ops::cos);
        let resampled_curve = curve.by_ref().resample_auto(1000).unwrap();
        for test_pt in curve.domain().spaced_points(1001).unwrap() {
            let expected = curve.sample_unchecked(test_pt);
            assert_abs_diff_eq!(
                resampled_curve.sample_unchecked(test_pt),
                expected,
                epsilon = 1e-6
            );
        }
    }

    #[test]
    fn uneven_resampling() {
        let curve = FunctionCurve::new(interval(0.0, f32::INFINITY).unwrap(), ops::exp);

        // Need at least two points to resample.
        let nice_try = curve.by_ref().resample_uneven_auto([1.0; 1]);
        assert!(nice_try.is_err());

        // Uneven sampling should produce literal equality at the sample points.
        // (This is part of what you get in exchange for O(log(n)) sampling.)
        let sample_points = (0..100).map(|idx| idx as f32 * 0.1);
        let resampled_curve = curve.by_ref().resample_uneven_auto(sample_points).unwrap();
        for idx in 0..100 {
            let test_pt = idx as f32 * 0.1;
            let expected = curve.sample_unchecked(test_pt);
            assert_eq!(resampled_curve.sample_unchecked(test_pt), expected);
        }
        assert_abs_diff_eq!(resampled_curve.domain().start(), 0.0);
        assert_abs_diff_eq!(resampled_curve.domain().end(), 9.9, epsilon = 1e-6);

        // Another example.
        let curve = FunctionCurve::new(interval(1.0, f32::INFINITY).unwrap(), ops::log2);
        let sample_points = (0..10).map(|idx| ops::exp2(idx as f32));
        let resampled_curve = curve.by_ref().resample_uneven_auto(sample_points).unwrap();
        for idx in 0..10 {
            let test_pt = ops::exp2(idx as f32);
            let expected = curve.sample_unchecked(test_pt);
            assert_eq!(resampled_curve.sample_unchecked(test_pt), expected);
        }
        assert_abs_diff_eq!(resampled_curve.domain().start(), 1.0);
        assert_abs_diff_eq!(resampled_curve.domain().end(), 512.0);
    }

    #[test]
    fn sample_iterators() {
        let times = [-0.5, 0.0, 0.5, 1.0, 1.5];

        let curve = FunctionCurve::new(Interval::EVERYWHERE, |t| t * 3.0 + 1.0);
        let samples = curve.sample_iter_unchecked(times).collect::<Vec<_>>();
        let [y0, y1, y2, y3, y4] = samples.try_into().unwrap();

        assert_eq!(y0, -0.5 * 3.0 + 1.0);
        assert_eq!(y1, 0.0 * 3.0 + 1.0);
        assert_eq!(y2, 0.5 * 3.0 + 1.0);
        assert_eq!(y3, 1.0 * 3.0 + 1.0);
        assert_eq!(y4, 1.5 * 3.0 + 1.0);

        let finite_curve = FunctionCurve::new(Interval::new(0.0, 1.0).unwrap(), |t| t * 3.0 + 1.0);
        let samples = finite_curve.sample_iter(times).collect::<Vec<_>>();
        let [y0, y1, y2, y3, y4] = samples.try_into().unwrap();

        assert_eq!(y0, None);
        assert_eq!(y1, Some(0.0 * 3.0 + 1.0));
        assert_eq!(y2, Some(0.5 * 3.0 + 1.0));
        assert_eq!(y3, Some(1.0 * 3.0 + 1.0));
        assert_eq!(y4, None);

        let samples = finite_curve.sample_iter_clamped(times).collect::<Vec<_>>();
        let [y0, y1, y2, y3, y4] = samples.try_into().unwrap();

        assert_eq!(y0, 0.0 * 3.0 + 1.0);
        assert_eq!(y1, 0.0 * 3.0 + 1.0);
        assert_eq!(y2, 0.5 * 3.0 + 1.0);
        assert_eq!(y3, 1.0 * 3.0 + 1.0);
        assert_eq!(y4, 1.0 * 3.0 + 1.0);
    }
}
