use core::fmt::Display;

use zlim_utils::str::SmolStr;

/// Context for a [`ZlimError`] to aid in debugging.
///
/// [`ZlimError`]: crate::error::ZlimError
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ErrorContext {
    Engine {
        // TODO
    },
    Script {
        // TODO
    },
    Command {
        // TODO
    },
    Observer {
        // TODO
    },
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
    pub fn name(&self) -> SmolStr {
        todo!()
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
