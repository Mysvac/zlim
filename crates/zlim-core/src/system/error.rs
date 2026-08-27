//! Error types produced by system construction and execution.

use zlim_core_derive::Error;
use zlim_utils::debug::DebugName;

use super::SystemId;
use crate::error::{Severity, ZlimError};

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
/// use zlim_reflect::derive::TypePath;
///
/// #[derive(TypePath, Resource)]
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
    /// Not an error; usually used to indicate conditional execution.
    ///
    /// Severity: Ignore
    #[error("Not an error; usually used to indicate conditional execution.")]
    None,
    /// A runtime error propagated from within the system.
    ///
    /// Severity: Internal ZlimError
    #[error("System runtime error: {_0}")]
    Runtime(ZlimError),
    /// A failure while building one of the system's parameters.
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
        while let SystemError::Runtime(e) = value {
            let dynerr = e.get();
            if dynerr.is::<SystemError>() {
                ::core::hint::cold_path();
                let boxed = e.take();
                value = *boxed.downcast::<SystemError>().unwrap();
            } else if dynerr.is::<SystemParamError>() {
                ::core::hint::cold_path();
                let boxed = e.take();
                value = SystemError::Param(*boxed.downcast::<SystemParamError>().unwrap());
                break;
            } else {
                value = SystemError::Runtime(e);
                break;
            }
        }

        let severity = match &value {
            SystemError::None => Severity::Ignore,
            SystemError::Runtime(e) => e.severity(),
            SystemError::Param(e) => e.severity,
            SystemError::Unregistered(_) => Severity::Warning,
            SystemError::Uninitialized(_) => Severity::Panic,
        };

        ZlimError::new(severity, value)
    }
}

impl From<ZlimError> for SystemError {
    #[cold]
    #[inline(never)]
    fn from(value: ZlimError) -> Self {
        let dynerr = value.get();
        if dynerr.is::<SystemError>() {
            ::core::hint::cold_path();
            *value.take().downcast::<SystemError>().unwrap()
        } else if dynerr.is::<SystemParamError>() {
            ::core::hint::cold_path();
            SystemError::Param(*value.take().downcast::<SystemParamError>().unwrap())
        } else {
            Self::Runtime(value)
        }
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
