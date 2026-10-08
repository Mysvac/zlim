//! Error types produced by system construction and execution.

use zlim_error::derive::Error;
use zlim_error::{Severity, ZlimError};
use zlim_utils::debug::DebugName;

use super::SystemId;

// -----------------------------------------------------------------------------
// SystemParamError

/// An error produced when a [`SystemParam`](super::SystemParam) fails to build
/// during a system run.
#[derive(Clone, Debug, Error)]
#[error("Build system param `{name}` failed in system `{system}`: {info}.")]
pub struct SystemParamError {
    /// Type name of the parameter that failed to build.
    pub name: DebugName,
    /// Name of the system whose parameter failed to build.
    pub system: DebugName,
    /// Human-readable description of the failure.
    pub info: Box<str>, // not `String`, reduce struct size
    /// Severity classification of the failure.
    ///
    /// If it is [`Severity::Ignore`], this error will be converted
    /// to [`SystemError::Skipped`] during system call.
    pub severity: Severity,
}

impl From<SystemParamError> for ZlimError {
    #[cold]
    fn from(value: SystemParamError) -> Self {
        let severity = value.severity;
        ZlimError::new(severity, value)
    }
}

impl SystemParamError {
    /// Creates a parameter error for `Param` with the given description and
    /// default `Error` severity.
    #[cold]
    pub fn new<Param>(info: impl Into<Box<str>>) -> Self {
        Self {
            name: DebugName::type_name::<Param>(),
            system: DebugName::anonymous(),
            info: info.into(),
            severity: Severity::Error,
        }
    }

    /// Attaches the owning system's name to this error.
    pub fn with_system(self, system: DebugName) -> Self {
        Self { system, ..self }
    }

    /// Overrides this error's severity.
    pub fn with_severity(self, severity: Severity) -> Self {
        Self { severity, ..self }
    }
}

// -----------------------------------------------------------------------------
// SystemError

/// The error type produced while building or running a system.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// #[derive(Resource)]
/// struct Missing;
///
/// fn needs_resource(_: Res<Missing>) {}
///
/// let mut world = World::alloc();
/// // The `Missing` resource is never inserted, so the run fails with a
/// // `Param` error instead of panicking.
/// let result = world.invoke_once(needs_resource, ());
/// assert!(matches!(result, Err(SystemError::Param(_))));
/// ```
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SystemError {
    /// The system was skipped, usually because a run condition said so.
    ///
    /// If this error is returned, we will assume that the System has
    /// been skipped and not run, so we will not apply the delay command.
    ///
    /// Severity: Ignore
    #[error("System skipped; usually used to indicate conditional execution.")]
    Skipped,
    /// A runtime error propagated from within the system.
    ///
    /// If this error is returned, we will assume that the System *has*
    /// run, so the delay commands will still be applied (if needed).
    ///
    /// Severity: Internal ZlimError
    #[error("System runtime error: {_0}")]
    Runtime(ZlimError),
    /// A failure while building one of the system's parameters.
    ///
    /// If this error is returned, we will assume that the System has
    /// been skipped and not run, so we will not apply the delay command.
    ///
    /// Severity: Error (default)
    #[error("System param error: {_0}")]
    Param(SystemParamError),
    /// The system was not registered with the schedule.
    ///
    /// Severity: Warning
    #[error("Unregistered system: {_0}")]
    Unregistered(SystemId),
    /// The system ran before its persistent state was initialized.
    ///
    /// Severity: Panic
    #[error("Uninitialized system: {_0}")]
    Uninitialized(SystemId),
}

impl From<SystemError> for ZlimError {
    #[cold]
    #[inline(never)]
    fn from(mut value: SystemError) -> Self {
        let severity = match &value {
            SystemError::Skipped => Severity::Ignore,
            SystemError::Runtime(e) => e.severity(),
            SystemError::Param(e) => e.severity,
            SystemError::Unregistered(_) => Severity::Warning,
            SystemError::Uninitialized(_) => Severity::Panic,
        };

        let backtrace = match &mut value {
            SystemError::Runtime(e) => e.take_backtrace(),
            _ => std::backtrace::Backtrace::disabled(),
        };

        let mut location = None;

        // Try take internal Error, avoid deep nesting.
        if let SystemError::Runtime(e) = value {
            let dynerr = e.get();
            if dynerr.is::<SystemError>() {
                ::core::hint::cold_path();
                location = Some(e.location());
                let boxed = e.take();
                value = *boxed.downcast::<SystemError>().unwrap();
            } else if dynerr.is::<SystemParamError>() {
                ::core::hint::cold_path();
                location = Some(e.location());
                let boxed = e.take();
                value = SystemError::Param(*boxed.downcast::<SystemParamError>().unwrap());
            } else {
                value = SystemError::Runtime(e);
            }
        }

        // We don't want capture backtrace again.
        let error = ZlimError::new_with_backtrace(severity, value, backtrace);
        match location {
            None => error,
            Some(l) => error.with_location(l),
        }
    }
}

impl From<ZlimError> for SystemError {
    #[cold]
    #[inline(never)]
    fn from(value: ZlimError) -> Self {
        Self::Runtime(value)
    }
}

impl From<SystemParamError> for SystemError {
    #[cold]
    #[inline(always)]
    fn from(value: SystemParamError) -> Self {
        Self::Param(value)
    }
}

// -----------------------------------------------------------------------------
