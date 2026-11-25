# M0 计划 4 交付归档

十个任务已交付。M0.4 于 2026-11-25 签收，本地验收和三平台原生 CI 均通过。

- 实施基线 `4a4d73465a86b2223c3dffbc1ea1f0e625ecf0da`
- 本地验收代码 `a6f7b3da68c21807400c17d59bb89af900d8423f`
- GitHub Actions 验收提交 `527ec07b98190c4d8a4ab4ef58b5eb58faa7c705`（`main`）
- 归档标签 `archive/m0-plan4`
- [计划全文快照](2026-11-23-m0-plan4-batch-bd.md)保存任务、约束与实施变更。示例和原定测试数量保留为过程记录，最终接口以验收提交为准。

## 任务与验证对应

| 任务 | 交付 | 验收依据 |
| --- | --- | --- |
| 1–2 | i18n 消息目录、格式化、语言优先级和默认消息束 | [i18n 契约](../../../crates/arbitrate-core/tests/i18n.rs) |
| 3 | Unified diff 与上下文窗口 | [差异输出契约](../../../crates/arbitrate-core/tests/diff.rs) |
| 4–5 | 图片和文档引用、文件状态校验和 `rich:` | [图片契约](../../../crates/arbitrate-core/tests/imgrefs.rs)、[文档契约](../../../crates/arbitrate-core/tests/filerefs.rs) |
| 6 | `AGENTS.md` 查找、读取和 UTF-8 边界截断 | [项目说明契约](../../../crates/arbitrate-core/tests/projectmd.rs) |
| 7 | 子进程环境过滤、精确保留和诊断顺序 | [环境过滤契约](../../../crates/arbitrate-core/tests/childenv.rs) |
| 8 | Windows 用户 PATH 查询、写入和广播 | [替身契约](../../../crates/arbitrate-core/tests/pathop.rs)、[注册表解码测试](../../../crates/arbitrate-core/src/pathop.rs) |
| 9–10 | 键位数据层、TOML 子集、绑定冲突和诊断 | [数据层契约](../../../crates/arbitrate-core/tests/keymap.rs)、[解析契约](../../../crates/arbitrate-core/tests/keymap_parse.rs) |

## 评审与最终行为

- 原计划覆盖的模块和阶段边界均落实。Task 4 的缺模块红灯在恢复时重现，Task 5 的原始红灯顺序未能核实。两项现行专项和工作区测试均已复跑通过。
- `60e004d` 修复 HTTP fixture 的非阻塞读竞争，增加延迟请求回归。`b9a7009` 补齐非 Unicode 环境过滤、注册表值验证和 TOML 结构校验，并隔离图片与文档测试的相对路径。
- `a6f7b3d` 关闭复评的三项 P2。Windows 在修改前检查新进程 PATH 的 UTF-16 长度，进程写入错误会说明持久化是否已成功，调用方可以重试。用户 PATH 优先匹配字面 `$`。键位校验过滤解码后的控制字符，保留对应默认键位和其他有效覆盖。
- 依赖清单、`Cargo.lock`、Rust 工具链和冻结资产相对实施基线保持一致。CLI/TUI 接线及资源使用边界见[延后登记](../../deferred.md#延后登记计划-4-末)。

## 验收记录

验收环境为 Rust 1.90.0 和 `aarch64-apple-darwin`。合并前针对验收代码复跑下列门禁。

| 验证 | 结果 |
| --- | --- |
| `cargo test --workspace --locked` | 294 通过，0 失败，1 忽略。忽略项为显式 fixture 清单生成器 |
| `cargo fmt --check --all` | 通过 |
| `cargo clippy --workspace --locked -- -D warnings` | 通过 |
| `cargo check --workspace --tests --locked --target x86_64-pc-windows-gnu` | 通过 |
| `cargo clippy --workspace --locked --target x86_64-pc-windows-gnu -- -D warnings` | 通过 |
| `cargo check --workspace --locked --target x86_64-unknown-linux-musl` | 通过 |
| 基线到验收代码的空白检查、清单和锁文件核对 | 通过 |
## 三平台原生 CI 签收

2026-11-25，`main` 推送触发的 [GitHub Actions 运行 #1](https://github.com/bluetoblack9/JueHArbitrate/actions/runs/45544518702) 顺利完成。三个任务均使用工作流指定的 Rust 1.90.0。`Clippy` 使用 `-D warnings`，构建、检查和测试使用 `--workspace --locked`。

| Runner | 已通过的步骤 |
| --- | --- |
| `ubuntu-latest` | 格式检查、Clippy、Rust 工作区测试、`x86_64-unknown-linux-musl` 检查 |
| `macos-latest` | 格式检查、Clippy、Rust 工作区测试 |
| `windows-latest` | 原生构建与链接、格式检查、Clippy、Rust 工作区测试 |

归档保留原本地验收 SHA 和标签。当前历史中的 `54f741a` 与 `a6f7b3d` 的 Git patch ID 一致。两份验收提交的 Rust 源码、测试、资产、依赖清单与锁文件、工具链和 CI 工作流已逐路径比对一致。上表记录原生 CI 各步骤结论，本地 macOS 测试计数见前表。
