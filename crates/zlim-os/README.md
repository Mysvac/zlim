# zlim-os

Platform-layer bridge that handles cross-platform differences and
exports platform-specific interfaces for the rest of the runtime.

## time

Re-exports `std::time` on native targets; switches to `web_time` on WASM.

## dirs

Standard user directory paths, resolved per-platform.

| Platform | `preferences_dir()` |
|----------|---------------------|
| Windows  | `C:\Users\{user}\AppData\Roaming` |
| Linux    | `$XDG_CONFIG_HOME` or `~/.config` |
| macOS    | `~/Library/Preferences` |
| WASM / Android | `None` |

## sys

Hidden re-exports of platform system crates (`windows-sys`, `wasm-bindgen`, `android-activity`, etc.).

Marked `#[doc(hidden)]` — not part of the public API.

Exists only to consolidate platform dependency versions in one place
so downstream code does not need to specify them individually.
