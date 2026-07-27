use std::env::home_dir;
use std::path::PathBuf;

/// Returns the path to the directory used for application settings.
///
/// On MacOs, this is `~/Library/Preferences` .
///
/// See <https://docs.rs/dirs/latest/dirs/fn.preference_dir.html> for details.
#[inline(never)]
pub fn preferences_dir() -> Option<PathBuf> {
    home_dir().map(|home| home.join("Library/Preferences"))
}
