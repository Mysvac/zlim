use core::fmt::{Debug, Display, Formatter};
use core::hash::Hash;
use core::time::Duration;
use std::collections::VecDeque;

use zlim_app::{App, SubApp};
use zlim_core::derive::Resource;
use zlim_os::time::Instant;
use zlim_reflect::derive::TypePath;
use zlim_utils::hash::{HashMap, NoopState};

// -----------------------------------------------------------------------------
// hasher

/// Computes a 64-bit FNV-1a hash of the given byte slice.
///
/// See [WIKI](https://en.wikipedia.org/wiki/Fowler%E2%80%93Noll%E2%80%93Vo_hash_function)
const fn fnv1a_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325;
    let mut i = 0;
    while i < bytes.len() {
        hash ^= bytes[i] as u64;
        hash = hash.wrapping_mul(0x100000001B3);
        i += 1;
    }
    hash
}

// -----------------------------------------------------------------------------
// DiagnosticPath

/// Unique diagnostic path, separated by `/`.
///
/// In current implementation, path strings are interned in memory,
/// so this it is not suitable for creating a large number of dynamic
/// paths at runtime.
///
/// However, creating paths with the same name is cheap and will not
/// duplicate the underlying interned string.
#[derive(Debug, Clone)]
pub struct DiagnosticPath {
    path: &'static str,
    hash: u64,
}

impl Hash for DiagnosticPath {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}

impl Display for DiagnosticPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        Display::fmt(self.path, f)
    }
}

impl Eq for DiagnosticPath {}

impl PartialEq for DiagnosticPath {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash && self.path == other.path
    }
}

impl DiagnosticPath {
    /// Creates a [`DiagnosticPath`] from a static string.
    ///
    /// # Validation
    ///
    /// - The path should not be empty.
    /// - The path should not start with `/`.
    /// - The path should not end with `/`.
    /// - The path should not contain empty components (i.e., `//`).
    #[track_caller]
    pub const fn new(path: &'static str) -> Self {
        debug_assert!(!path.is_empty(), "diagnostic path should not be empty");

        let len = path.len();
        debug_assert!(
            path.as_bytes()[0] != b'/',
            "diagnostic path should not start with `/`",
        );
        debug_assert!(
            path.as_bytes()[len - 1] != b'/',
            "diagnostic path should not end with `/`",
        );

        Self {
            path,
            hash: fnv1a_hash(path.as_bytes()),
        }
    }

    /// Create a new [`DiagnosticPath`] from the specified string.
    ///
    /// This function interns the given string, promoting it to a `'static`
    /// lifetime, and then constructs the path. The interning process is
    /// automatically deduplicated, so repeated calls with the same string
    /// content will not cause additional memory allocations.
    ///
    /// # Validation
    ///
    /// - The path should not be empty.
    /// - The path should not start with `/`.
    /// - The path should not end with `/`.
    /// - The path should not contain empty components (i.e., `//`).
    #[track_caller]
    pub fn from_path(path: impl AsRef<str>) -> DiagnosticPath {
        let path: &str = path.as_ref();

        debug_assert!(!path.is_empty(), "diagnostic path should not be empty");

        debug_assert!(
            !path.starts_with('/'),
            "diagnostic path should not start with `/`"
        );
        debug_assert!(
            !path.ends_with('/'),
            "diagnostic path should not end with `/`"
        );
        debug_assert!(
            !path.contains("//"),
            "diagnostic path should not contain empty components"
        );

        Self {
            path: zlim_utils::str::intern_str(path),
            hash: fnv1a_hash(path.as_bytes()),
        }
    }

    /// Creates a path from slash-joined components.
    pub fn from_components<'a>(components: impl IntoIterator<Item = &'a str>) -> Self {
        let mut buf = String::new();

        for (i, component) in components.into_iter().enumerate() {
            if i > 0 {
                buf.push('/');
            }
            buf.push_str(component);
        }

        Self::from_path(buf.as_str())
    }

    /// Returns the full slash-separated path string.
    #[inline]
    pub const fn as_str(&self) -> &str {
        self.path
    }

    /// Returns an iterator over path components.
    #[inline]
    pub fn components(&self) -> impl Iterator<Item = &str> + '_ {
        self.path.split('/')
    }
}

// -----------------------------------------------------------------------------
// DiagnosticMeasurement

/// A single measurement of a [`Diagnostic`].
#[derive(Debug, Clone)]
pub struct DiagnosticMeasurement {
    /// When this measurement was taken.
    pub time: Instant,
    /// Value of the measurement.
    pub value: f64,
}

// -----------------------------------------------------------------------------
// Diagnostic

/// A timeline of sampled values for a single diagnostic metric.
#[derive(Debug)]
pub struct Diagnostic {
    path: DiagnosticPath,
    // Optional textual suffix, e.g. `%` or `ms`.
    suffix: &'static str,
    sum: f64,
    ema: f64,
    smoothing_factor: f64,
    lastest: Option<DiagnosticMeasurement>,
    history: VecDeque<DiagnosticMeasurement>,
    max_history_length: usize,
    /// Disabled diagnostics are ignored for logging and measurement updates.
    pub is_enabled: bool,
}

impl Diagnostic {
    /// Creates a new diagnostic without history.
    ///
    /// - `max_history_length` is `0`, [`smoothed`] and [`average`] use the latest values directly.
    /// - `smoothing_factor` is `2.0 / 21.0` but unused because `max_history_length` is zero.
    ///
    /// [`smoothed`]: Self::smoothed
    /// [`average`]: Self::average
    pub const fn new(path: DiagnosticPath) -> Self {
        Self {
            path,
            suffix: "",
            sum: 0.0,
            ema: 0.0,
            smoothing_factor: 2.0 / 21.0,
            lastest: None,
            history: VecDeque::new(),
            max_history_length: 0,
            is_enabled: true,
        }
    }

    /// Configures a display suffix for this diagnostic.
    #[inline]
    #[must_use]
    pub fn with_suffix(mut self, suffix: &'static str) -> Self {
        self.suffix = suffix;
        self
    }

    /// The smoothing factor used for the exponential smoothing used for
    /// [`smoothed`](Self::smoothed).
    ///
    /// If measurements come in less frequently than `smoothing_factor` seconds
    /// apart, no smoothing will be applied. As measurements come in more
    /// frequently, the smoothing takes a greater effect such that it takes
    /// approximately `smoothing_factor` seconds for 83% of an instantaneous
    /// change in measurement to be reflected in the smoothed value.
    ///
    /// A smoothing factor of 0.0 will effectively disable smoothing.
    ///
    /// Default smoothing factor is `2.0 / 21.0` (by [`Diagnostic::new`]).
    #[inline]
    #[must_use]
    pub fn with_smoothing_factor(mut self, smoothing_factor: f64) -> Self {
        self.smoothing_factor = smoothing_factor;
        self
    }

    /// Configures the history length used for averaging.
    #[must_use]
    pub fn with_max_history_length(mut self, max_history_length: usize) -> Self {
        self.max_history_length = max_history_length;

        if self.history.capacity() == max_history_length {
            return self;
        }

        let mut history = VecDeque::with_capacity(max_history_length);

        for _ in 0..max_history_length {
            if let Some(item) = self.history.pop_back() {
                history.push_front(item);
            } else {
                break;
            }
        }

        self.history = history;

        self
    }

    /// Get the [`DiagnosticPath`] that identifies this [`Diagnostic`].
    #[inline]
    pub fn path(&self) -> &DiagnosticPath {
        &self.path
    }

    /// Get the `suffix` textual suffix.
    #[inline]
    pub fn suffix(&self) -> &'static str {
        self.suffix
    }

    /// Get the latest measurement from this diagnostic.
    #[inline]
    pub fn measurement(&self) -> Option<&DiagnosticMeasurement> {
        self.lastest.as_ref()
    }

    /// Get the latest value from this diagnostic.
    #[inline]
    pub fn value(&self) -> Option<f64> {
        self.lastest.as_ref().map(|m| m.value)
    }

    /// Return the simple moving average of this diagnostic's recent values.
    ///
    /// This is a cheap operation as the sum is cached.
    #[inline]
    pub fn average(&self) -> Option<f64> {
        if self.lastest.is_some() {
            Some(self.sum / (1 + self.history.len()) as f64)
        } else {
            None
        }
    }

    /// Return the exponential moving average of this diagnostic.
    ///
    /// This is by default tuned to behave reasonably well for a typical
    /// measurement that changes every frame such as frametime. This can be
    /// adjusted using [`with_smoothing_factor`](Self::with_smoothing_factor).
    #[inline]
    pub fn smoothed(&self) -> Option<f64> {
        if self.lastest.is_some() {
            Some(self.ema)
        } else {
            None
        }
    }

    /// Return the duration between the oldest and most recent values for this diagnostic.
    pub fn duration(&self) -> Option<Duration> {
        if self.history.is_empty() {
            return None;
        }

        let newest = self.lastest.as_ref()?;
        let oldest = self.history.front()?;
        Some(newest.time.duration_since(oldest.time))
    }

    /// Clear all measurements in this diagnostic.
    pub fn clear(&mut self) {
        self.lastest = None;
        self.history.clear();
        self.sum = 0.0;
        self.ema = 0.0;
    }

    /// Return `true` if the diagnotic is empty (without any measurement).
    pub fn is_empty(&self) -> bool {
        self.lastest.is_none()
    }

    /// Return the number of elements for this diagnostic.
    ///
    /// As same as `is_empty() as usize + history_len()`.
    pub fn len(&self) -> usize {
        self.history.len() + (self.lastest.is_some() as usize)
    }

    /// Return the number of history elements for this diagnostic.
    ///
    /// This does not include the latest element.
    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    /// Returns the configured maximum history length.
    pub fn max_history_length(&self) -> usize {
        self.max_history_length
    }

    /// All measured values from this [`Diagnostic`], up to the configured maximum history length.
    pub fn values(&self) -> impl Iterator<Item = f64> {
        self.history.iter().map(|x| x.value).chain(self.value())
    }

    /// All measurements from this [`Diagnostic`], up to the configured maximum history length.
    pub fn measurements(&self) -> impl Iterator<Item = &DiagnosticMeasurement> {
        self.history.iter().chain(self.measurement())
    }

    /// Appends a new measurement and updates moving statistics.
    pub fn add_measurement(&mut self, measurement: DiagnosticMeasurement) {
        if let Some(previous) = self.lastest.take() {
            if !measurement.value.is_nan() {
                let delta = (measurement.time - previous.time).as_secs_f64();
                let alpha = (delta / self.smoothing_factor).clamp(0.0, 1.0);
                self.ema += alpha * (measurement.value - self.ema);
                self.sum += measurement.value;
            }
            self.lastest = Some(measurement);

            if self.max_history_length > 0 {
                while self.history.len() >= self.max_history_length {
                    let removed = self.history.pop_front().unwrap();
                    if !removed.value.is_nan() {
                        self.sum -= removed.value;
                    }
                }
                self.history.push_back(previous);
            }
        } else {
            if !measurement.value.is_nan() {
                self.ema = measurement.value;
                self.sum = measurement.value;
            }
            self.lastest = Some(measurement);
        }
    }
}

// -----------------------------------------------------------------------------
// Diagnostics

/// A collection of [`Diagnostic`]s.
///
/// This resource can be accessed via [`Res<Diagnostics>`] or [`ResMut<Diagnostics>`].
///
/// [`Res<Diagnostics>`]: zlim_core::borrow::Res
/// [`ResMut<Diagnostics>`]: zlim_core::borrow::ResMut
#[derive(TypePath, Resource, Default)]
pub struct Diagnostics {
    diagnostics: HashMap<DiagnosticPath, Diagnostic, NoopState>,
}

impl Diagnostics {
    /// Add a new [`Diagnostic`].
    ///
    /// If possible, prefer calling
    /// [`AppDiagnosticExt::register_diagnostic`].
    pub fn add(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.insert(diagnostic.path.clone(), diagnostic);
    }

    /// Get the [`Diagnostic`] with the given [`DiagnosticPath`], if it exists.
    pub fn get(&self, path: &DiagnosticPath) -> Option<&Diagnostic> {
        self.diagnostics.get(path)
    }

    /// Mutably get the [`Diagnostic`] with the given [`DiagnosticPath`], if it exists.
    pub fn get_mut(&mut self, path: &DiagnosticPath) -> Option<&mut Diagnostic> {
        self.diagnostics.get_mut(path)
    }

    /// Return an iterator over all [`Diagnostic`]s.
    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.values()
    }

    /// Return an iterator over all [`Diagnostic`]s, by mutable reference.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Diagnostic> {
        self.diagnostics.values_mut()
    }

    /// Get the latest [`DiagnosticMeasurement`] from an enabled [`Diagnostic`].
    pub fn get_measurement(&self, path: &DiagnosticPath) -> Option<&DiagnosticMeasurement> {
        self.diagnostics
            .get(path)
            .filter(|diagnostic| diagnostic.is_enabled)
            .and_then(|diagnostic| diagnostic.measurement())
    }

    /// Add a measurement to an **enabled** [`Diagnostic`].
    ///
    /// The measurement is passed as a function so that it will
    /// be evaluated only if the [`Diagnostic`] is enabled.
    ///
    /// This can be useful if the value is costly to calculate.
    pub fn add_measurement<F>(&mut self, path: &DiagnosticPath, value: F)
    where
        F: FnOnce() -> f64,
    {
        if let Some(diagnostic) = self.diagnostics.get_mut(path)
            && diagnostic.is_enabled
        {
            let measurement = DiagnosticMeasurement {
                time: Instant::now(),
                value: value(),
            };
            diagnostic.add_measurement(measurement);
        }
    }
}

impl Debug for Diagnostics {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        Debug::fmt(&self.diagnostics, f)
    }
}

// -----------------------------------------------------------------------------
// AppDiagnosticExt

/// Extends app builders with `register_diagnostic`.
pub trait AppDiagnosticExt {
    /// Registers a diagnostic in the app's [`Diagnostics`].
    fn register_diagnostic(&mut self, diagnostic: Diagnostic) -> &mut Self;
}

impl AppDiagnosticExt for SubApp {
    fn register_diagnostic(&mut self, diagnostic: Diagnostic) -> &mut Self {
        self.world_mut()
            .resource_mut_or_init::<Diagnostics>()
            .add(diagnostic);
        self
    }
}

impl AppDiagnosticExt for App {
    fn register_diagnostic(&mut self, diagnostic: Diagnostic) -> &mut Self {
        self.main_world_mut()
            .resource_mut_or_init::<Diagnostics>()
            .add(diagnostic);
        self
    }
}

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_history() {
        const MEASUREMENT: f64 = 20.0;

        let path = DiagnosticPath::new("test");
        let mut diagnostic = Diagnostic::new(path).with_max_history_length(5);

        let mut now = Instant::now();

        for _ in 0..3 {
            for _ in 0..5 {
                diagnostic.add_measurement(DiagnosticMeasurement {
                    time: now,
                    value: MEASUREMENT,
                });
                now += Duration::from_secs(1);
            }
            assert!((diagnostic.average().expect("average") - MEASUREMENT).abs() < 0.1);
            assert!((diagnostic.smoothed().expect("smoothed") - MEASUREMENT).abs() < 0.1);
            diagnostic.clear();
        }
    }

    /// Verifies that disabled diagnostics are ignored by
    /// [`Diagnostics::add_measurement`]: the measurement closure is only
    /// evaluated for an **enabled** diagnostic, and disabling one stops
    /// further measurements from being recorded.
    #[test]
    fn disabled_diagnostics_are_ignored() {
        let path = DiagnosticPath::new("test");

        let mut diagnostics = Diagnostics::default();
        let mut diagnostic = Diagnostic::new(path.clone());
        diagnostic.is_enabled = false;
        diagnostics.add(diagnostic);

        // A disabled diagnostic ignores measurements: the closure is not
        // even evaluated, and no measurement is recorded.
        let mut evaluated = false;
        diagnostics.add_measurement(&path, || {
            evaluated = true;
            1.0
        });
        assert!(!evaluated);
        assert!(diagnostics.get(&path).unwrap().measurement().is_none());

        // Re-enabling the diagnostic records measurements again.
        diagnostics.get_mut(&path).unwrap().is_enabled = true;
        diagnostics.add_measurement(&path, || 2.0);
        assert_eq!(diagnostics.get_measurement(&path).unwrap().value, 2.0);
    }
}
