use std::env::home_dir;
use std::ffi::OsString;
use std::path::PathBuf;

/// The path if it's absolute or [`None`]. Empty paths are not absolute.
///
/// [XDG Base Directory Specification] requires that the path specified in environment variables
/// must be absolute. If it's not, we should ignore it and fallback to the default path.
///
/// [XDG Base Directory Specification]: https://specifications.freedesktop.org/basedir/latest/
#[inline(always)]
fn is_absolute_path(path: OsString) -> Option<PathBuf> {
    let path: PathBuf = PathBuf::from(path);
    path.is_absolute().then_some(path)
}

/// Returns the path to the directory used for application settings.
///
/// On Linux, this value is default to `XDG_CONFIG_HOME`.
/// When unset, empty, or invalid is `~/.config/` .
///
/// See <https://docs.rs/dirs/latest/dirs/fn.preference_dir.html> for details.
#[inline(never)]
pub fn preferences_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .and_then(is_absolute_path)
        .or_else(|| home_dir().map(|home| home.join(".config")))
}
