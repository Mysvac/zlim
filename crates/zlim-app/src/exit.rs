use core::num::NonZeroU8;

use zlim_core::derive::ScheduleStage;
use zlim_core::message::Message;
use zlim_reflect::TypePath;

/// A [`Message`] that indicates the `App` should exit.
///
/// If one or more of these are present at the end of an update,
/// the runner will end and (maybe) return control to the caller.
#[derive(TypePath, Message)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[type_path = "zlim_app::AppExit"]
pub enum AppExit {
    #[default]
    Success,
    Error(NonZeroU8),
}

impl AppExit {
    /// Creates a [`AppExit::Error`] with an error code of 1.
    #[must_use]
    pub const fn error() -> Self {
        Self::Error(NonZeroU8::MIN)
    }

    /// Returns `true` if `self` is a [`AppExit::Success`].
    #[must_use]
    pub const fn is_success(&self) -> bool {
        matches!(self, AppExit::Success)
    }

    /// Returns `true` if `self` is a [`AppExit::Error`].
    #[must_use]
    pub const fn is_error(&self) -> bool {
        matches!(self, AppExit::Error(_))
    }

    /// Creates a [`AppExit`] from a code.
    ///
    /// When `code` is 0 a [`AppExit::Success`] is constructed
    /// otherwise a [`AppExit::Error`] is constructed.
    #[must_use]
    pub const fn from_code(code: u8) -> Self {
        match NonZeroU8::new(code) {
            Some(code) => Self::Error(code),
            None => Self::Success,
        }
    }
}

impl From<u8> for AppExit {
    fn from(value: u8) -> Self {
        Self::from_code(value)
    }
}

impl std::process::Termination for AppExit {
    fn report(self) -> std::process::ExitCode {
        use std::process::ExitCode;
        match self {
            AppExit::Success => ExitCode::SUCCESS,
            // We leave logging an error to our users
            AppExit::Error(value) => ExitCode::from(value.get()),
        }
    }
}

/// The schedule stage that handles application exit logic.
#[derive(TypePath, ScheduleStage, Debug, Clone, Copy)]
pub struct AppExitStage;
