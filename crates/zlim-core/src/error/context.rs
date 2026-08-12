use core::fmt::Display;

use zlim_utils::debug::DebugName;

/// Context for a [`ZlimError`] to aid in debugging.
///
/// [`ZlimError`]: crate::error::ZlimError
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ErrorContext {
    /// An error originated from the engine core.
    Engine {
        // TODO
    },
    /// An error originated from a script execution.
    Script {
        // TODO
    },
    /// An error originated from a command application.
    Command { name: DebugName },
    /// An error originated from an observer callback.
    Observer {
        // TODO
    },
    /// An error originated from an entity script.
    EntityScript {
        // TODO
    },
}

impl Display for ErrorContext {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "{} `{}` failed", self.kind(), self.name())
    }
}

impl ErrorContext {
    /// The name of the ECS construct that failed.
    pub fn name(&self) -> String {
        match self {
            ErrorContext::Engine {} => todo!(),
            ErrorContext::Script {} => todo!(),
            ErrorContext::Command { name } => name.to_string(),
            ErrorContext::Observer {} => todo!(),
            ErrorContext::EntityScript {} => todo!(),
        }
    }

    /// A string representation of the kind of ECS construct that failed.
    ///
    /// This helper is intended for logging and telemetry labels.
    pub fn kind(&self) -> &'static str {
        match self {
            ErrorContext::Engine { .. } => "Engine",
            ErrorContext::Script { .. } => "Script",
            ErrorContext::Command { .. } => "Command",
            ErrorContext::Observer { .. } => "Observer",
            ErrorContext::EntityScript { .. } => "EntityScript",
        }
    }
}
