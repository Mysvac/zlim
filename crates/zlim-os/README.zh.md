平台层桥接层，处理跨平台差异。

## time

在原生目标上重新导出 `std::time`；在 WASM 上切换为 `web_time`。

## dirs

标准的用户目录路径，按平台解析。

| 平台 | `preferences_dir()` |
|------|---------------------|
| Windows | `C:\Users\{user}\AppData\Roaming` |
| Linux | `$XDG_CONFIG_HOME` 或 `~/.config` |
| macOS | `~/Library/Preferences` |
| WASM / Android | `None` |

## sys

平台系统 crate（`windows-sys`、`wasm-bindgen`、`android-activity` 等）的重新导出。

标记为 `#[doc(hidden)]` —— 不属于公共 API。

仅用于将平台依赖的版本统一收敛在一处，使下游代码无需逐一指定。
