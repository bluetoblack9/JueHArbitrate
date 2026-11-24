# AGENTS.md

本文件适用于 `arbitrate-rs/`。如有子目录的 `AGENTS.md`，同时遵守该目录的具体约定。

## 项目边界

- `arbitrate` 是产品入口，`mcp-mock` 供测试使用。使用 `rust-toolchain.toml` 固定的 Rust 工具链和 workspace 的 edition。构建、测试和 Clippy 命令使用 `--locked`。
- 上游 Go 代码仅供静态参考，参照提交见 [UPSTREAM-REFERENCE.txt](UPSTREAM-REFERENCE.txt)。验证使用 Rust 测试与自有 fixture。测试期望值从规格推导。项目不兼容旧 Go 项目文件和记录，数据目录使用 `~/.arbitrate` 与 `.arbitrate`。
- 按 crate 职责放置实现。新增或调整依赖前查看 [依赖图测试](tests/dependency_graph.rs)。保留 `core` 的无网络、无 UI 边界。
- 保持冻结的 `assets/webfrontend.html`、`assets/LICENSE` 和 `assets/NOTICE` 的原始字节。品牌适配按计划限定范围，保留协议标识和 `SEEK_*` 环境变量名。
- 先核对任务所属阶段。CLI 契约见 [CLI 源码审计](docs/cli-source-audit.md)，阶段接线和已知边界见 [延后登记](docs/deferred.md)，M0 批 B+D 的实施和验收见 [计划 4 归档](docs/archive/m0-plan4/README.md)。

## 搜索与技能

- 需要定位未知实现时优先使用 Semble `search`，按任务选择 `code`、`docs`、`config` 或 `all`。定位后直接打开返回的文件和行号。列出字面引用或 Semble 不可用时可用 `rg`。
- 按任务选择 `.agents/skills/` 中相关技能，使用前阅读对应的 `SKILL.md`。修复行为回归可用 `test-driven-development`，排查异常可用 `systematic-debugging`，交付前使用 `verification-before-completion` 核对结果。技能安装或更新时同步检视 `skills-lock.json`。

## 验证

在 `arbitrate-rs/` 根目录运行。开发时先跑受影响的测试，准备合并时按 [CI 工作流](.github/workflows/ci.yml) 核对项目门禁。

```sh
cargo fmt --check --all
cargo clippy --workspace --locked -- -D warnings
cargo test --workspace --locked
```

- 测试使用合成数据、临时目录和替身。HTTP fixture 使用本机回环端口。Windows PATH 测试经替身验证，避免改动真实注册表或 PATH。常规测试保持 fixture 原样，规格变更时才显式重新生成。
- 在验收记录中注明实际运行的平台和结果。将 Windows 交叉检查与原生构建及测试结果分别登记，三平台原生 CI 以 runner 的实际结果为准。

## Git 提交规范

遵循 [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/)。本项目使用 `<type>(<scope>): <subject>` 格式，`subject` 简明描述本次变更。

| `type` | 用途 |
| --- | --- |
| `feat` | 新增接口或功能 |
| `fix` | 修复缺陷 |
| `refactor` | 调整实现结构，保持对外行为一致 |
| `docs` | 仅更新文档 |
| `style` | 仅调整格式或排版，例如运行 `rustfmt` |
| `chore` | 更新构建配置、工具链或依赖 |

`scope` 优先使用受影响的 crate 或具体组件，例如 `core`、`llm`、`state`、`tui`、`cli`、`pathop`、`keymap`。涉及依赖、CI 或项目技能时，可分别使用 `deps`、`ci` 或 `skills`。

```text
feat(keymap): 支持加载用户键位配置
fix(pathop): 避免 Windows 用户 PATH 重复写入
chore(deps): 更新 workspace 的依赖锁定版本
```
