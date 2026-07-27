//! APIs that return the location of standard user directories.

use std::path::PathBuf;

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "macos")]
mod macos;

/// Returns the path to the directory used for application settings.
///
/// - On Windows, this is: `C:\Users\{user}\AppData\Roaming`
///
/// - On Linux, this is: `XDG_CONFIG_HOME` or `/home/{user}/.config`
///
/// - On MacOS, this is: `/Users/{user}/Library/Preferences`
///
/// - For other platform (wasm, andoird, ...), this function always return `None`.
#[inline]
pub fn preferences_dir() -> Option<PathBuf> {
    crate::cfg::switch! {
        #[cfg(target_os = "windows")] => { windows::preferences_dir() },
        #[cfg(target_os = "linux")] => { linux::preferences_dir() },
        #[cfg(target_os = "macos")] => { macos::preferences_dir() },
        _ => { None },
    }
}
