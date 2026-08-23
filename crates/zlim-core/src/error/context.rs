//! Error context metadata describing where an error originated.

use core::fmt::Display;

use zlim_utils::debug::DebugName;

use crate::system::SystemId;

use crate::tick::Tick;

/// Context for a [`ZlimError`] to aid in debugging.
///
/// [`ZlimError`]: crate::error::ZlimError
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ErrorContext {
    /// An error originated from a job execution.
    Job {
        name: &'static str,
        group: &'static str,
        tick: Tick,
    },
    /// An error originated from an System.
    System { id: SystemId, tick: Tick },
    /// An error originated from a command application.
    Command { name: DebugName },
}

impl Display for ErrorContext {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "{} `{}` failed", self.kind(), self.name())
    }
}

impl ErrorContext {
    /// The name of the ECS construct that failed.
    ///
    /// Variants that do not carry a name yet return an empty string.
    pub fn name(&self) -> String {
        match self {
            ErrorContext::System { id, .. } => id.to_string(),
            ErrorContext::Job { name, .. } => name.to_string(),
            ErrorContext::Command { name } => name.to_string(),
        }
    }

    /// A string representation of the kind of ECS construct that failed.
    ///
    /// This helper is intended for logging and telemetry labels.
    pub fn kind(&self) -> &'static str {
        match self {
            ErrorContext::Job { .. } => "Job",
            ErrorContext::System { .. } => "System",
            ErrorContext::Command { .. } => "Command",
        }
    }
}
