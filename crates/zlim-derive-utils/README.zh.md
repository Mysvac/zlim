proc-macro crate 的辅助工具。

本库提供：基于 `Cargo.toml` 的 crate 路径解析。

## crate_path 规则

解析顺序：

1. 名称以 `zlim_` 开头且 `zlim` 在 `[dependencies]` 中，返回 `::zlim::<module>`。
2. 名称完全匹配 `[dependencies]` 中的项，返回 `::<name>`。
3. 名称以 `zlim_` 开头且 `zlim` 在 `[dev-dependencies]` 中，返回 `::zlim::<module>`。
4. 否则，回退返回 `::<name>`。

## 注意事项

- `crate_path` 的参数是 Rust **模块**路径，而非 Cargo crate 名——请使用下划线 `_` 而非连字符 `-`。
  例如 `zlim_core` 对应 `Cargo.toml` 中的 `zlim-core`。

- 当名称以 `zlim_` 开头且 manifest 中存在 `zlim` crate 时，`zlim_` 前缀会被重映射为 `::zlim::`，
  生成 `::zlim::<module>` 形式的路径。

- manifest 会按路径和修改时间缓存，同一进程内的重复解析不会重新读取文件。

- 当 `CARGO_MANIFEST_DIR` 缺失、`Cargo.toml` 无法读取或解析失败时会 panic。
