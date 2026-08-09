# Platform Abstraction Layer

Platform-layer bridge that handles cross-platform differences and
exports platform-specific interfaces for the rest of the runtime.

## dirs

Standard user directory paths, resolved per-platform.

| Platform | `preferences_dir()` |
|----------|---------------------|
| Windows  | `C:\Users\{user}\AppData\Roaming` |
| Linux    | `$XDG_CONFIG_HOME` or `~/.config` |
| macOS    | `~/Library/Preferences` |
| WASM / Android | `None` |

## time

Re-exports `std::time` on native targets; switches to `web_time` on WASM.
This is a direct re-export with zero wrapping overhead on native.
On WASM the crate avoids a hard `std::time` dependency.

## sys

Hidden re-exports of platform system crates (`windows-sys`, `wasm-bindgen`, `android-activity`, etc.).

Marked `#[doc(hidden)]` — not part of the public API.

Exists only to consolidate platform dependency versions in one place
so downstream code does not need to specify them individually.
