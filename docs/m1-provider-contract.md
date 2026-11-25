# M1 Provider 契约与验收

M0.4 签收提交为 `527ec07b98190c4d8a4ab4ef58b5eb58faa7c705`。当前分支包含该提交。M1.0 启动时复跑 `types`、`pricing`、`budget`、`cache`，69 项通过。缓存跨模型和跨费率时段的成本锁定回归通过。[M0 签收记录](archive/m0-plan4/README.md)

## 上游参照

`UPSTREAM-REFERENCE.txt` 锁定 `cb27b90d2979f5f840526100b28ade586d034df9`。只读核对 `pkg/deepseek`、`pkg/llm`、`internal/opencatalog`、`cmd/seek/main.go` 和 `pkg/agent`。参考 checkout 的 HEAD 与指定 SHA 一致，这些路径没有已提交差异、工作区差异或未追踪文件。

| 领域字段 | DeepSeek（批 A/B/C） | 通用视图（M2 投影，批 E 适配） |
| --- | --- | --- |
| role、content、name | 文本形或带附件的 content 数组。保留适用的 name | 保留角色和正文。工具结果的 name 成为 tool name |
| tool_calls 的 id、kind、name、arguments 和 tool_call_id | 保留工具标识和结果关联。wire 流式 index 用于分片重组 | 投影 id、name、原始 arguments 字符串和结果关联 |
| reasoning_content、predicted_next | 准备请求副本时，仅带工具调用的 assistant 保留 reasoning，所有消息清除 prediction | 两字段留在领域历史 |
| 图片 asset/url、文件 asset/file_id/expires_at/purpose | asset 用于解析，URL/file_id 用于发送。批 C 验证文本、图片、文件依次编码 | 原正文按图片数、文件数依次追加提示。数量为零时跳过该类提示，附件留在领域历史 |
| id、object、created、model、choices、Usage | 响应 DTO 映射到领域值，保留 choice 顺序、index、message、finish_reason 和 Usage 五项计数 | OpenAI 保留最后非空结束原因，Anthropic 透传原值，Gemini 按批 E 规则归一化 |

M2 投影用例以正文「资料」、两张图片和一个文件为输入，断言下列 Rust 字符串及原消息未变。无附件保留原正文，单类附件仅追加对应提示。

```rust
"资料\n\n[image: 2 张图片未透传 — 当前 provider 不支持图片输入]\n\n[file: 1 个文档未透传 — 当前 provider 不支持文件输入]"
```

OpenAI 和兼容适配器向配置的完整 URL 发送 POST。M2 的 lane 构造负责选择 endpoint 和 session。目录收到空快照时保留当前状态。有效快照过滤所得空覆盖表清除对应旧表，安装层保护 deepseek-flash、deepseek-v4-pro、glm-5.3 三个一等模型。目录的 HTTP 和安装测试归批 F。

OpenCode 真实网关计费资格为未验证。离线 UA 测试验证请求头，真实计费证据按 M1 的独立门控获取。

## 传输依赖与审计

Rust 1.90.0 的锁文件解析选定 reqwest 0.12.28（rustls 后端）及 Tokio 1.54.1。两个 provider 的本地依赖各为 core，依赖图的正反例通过。九个自有 package 继承根 MIT 声明。

全工作区 cargo-deny 0.20.2 审计包含 arbitrate-deepseek、arbitrate-llm、reqwest 和 rustls。0.18.4 无法解析当前 RustSec 数据库中的 CVSS 4.0 公告，因此固定较新的二进制版本。四类检查均通过。webpki-roots 1.0.12 的 CDLA-Permissive-2.0 例外限于该版本，依据该包的 Cargo.toml 许可字段。

## TTS 字符表

Han 判定使用 Unicode 17.0.0 的 Script 属性完整区间表。Rust 生成器从仓库内 Scripts.txt 提取区间，普通测试逐区间比对并检查 10,000 的边界。Unicode 数据版本与上游运行时内置表可能不同，线上账单另验。[数据来源](https://www.unicode.org/Public/17.0.0/ucd/Scripts.txt) · [fixture 摘要与许可](../tests/fixtures/manifest.json)

## M1.0 签收

验收对象为功能分支 `feat/m1-provider-foundation` 的 `c735b50ef23b9ff0dcf34c7857453f102b2b37ff`。本机使用 Rust 1.90.0，目标为 `aarch64-apple-darwin`。工作区测试 310 项通过、0 项失败、2 项忽略。两个忽略项是显式 fixture 和 Han 区间表生成器，均以单独的生成命令运行通过。常规门禁运行后，Cargo.lock、fixture 和生成表均通过 Git 差异检查。

| 执行环境 | 通过项 |
| --- | --- |
| 本机 macOS | 格式检查、严格 Clippy、锁定构建和全工作区测试。两个 provider 的 all-targets Clippy 与 llm 发布构建的 5 项 HTTP 测试也通过 |
| Linux runner | 格式检查、严格 Clippy、锁定测试、cargo-deny 审计和 musl 目标检查。[Linux job](https://github.com/bluetoblack9/JueHArbitrate/actions/runs/45556651228/job/139010209726) |
| macOS runner | 格式检查、严格 Clippy 和锁定测试。[macOS job](https://github.com/bluetoblack9/JueHArbitrate/actions/runs/45556651228/job/139010209489) |
| Windows runner | 原生构建与链接、格式检查、严格 Clippy 和锁定测试。[Windows job](https://github.com/bluetoblack9/JueHArbitrate/actions/runs/45556651228/job/139010209756) |

额外运行的全工作区 all-targets Clippy 报告了 M0 测试文件 `i18n.rs`、`pathop.rs` 和 `config.rs` 中的既有 lint。M1.0 计划和 CI 规定的全工作区严格 Clippy 已通过。

| 签收项 | 证据与下游验收 |
| --- | --- |
| 领域字段与定价 | `provider_messages` 验证共享样例，`types`、`pricing`、`budget`、`cache` 验证 M0 契约。批 A 核对响应和文本，批 C 核对附件副本，批 E 核对通用字段映射，批 F 核对覆盖表安装。M2 验证正文中的附件数量、提示顺序和原文 |
| 依赖图与锁文件 | `dependency_graph` 的六项测试通过，九个本地包继承 MIT。固定版本的 cargo-deny 覆盖两个 provider、reqwest 和 rustls。Linux 原生审计和 musl 目标检查通过 |
| HTTP/SSE fixture | 两端 `http_fixture` 与原 `mock_http_selftest` 验证重复头、multipart 顺序、二进制数据、200 后断流和 SSE 暂停。批 B/C/E 分别补充实际 decoder、上传和适配器事件测试 |
| 请求与取消 | `Transport::request` 接收目录相对路径，`openai_request` 接收完整 URL。测试验证 session 有无、默认与注入超时、debug/release UA。批 A/B 使用 `wait_retry` 和 `until_cancelled`。退避测试在 499ms 仍等待、500ms 完成，在 0ms 取消后立即返回。400ms 和 600ms 的临时变更均被捕获，还原后连续三次专项通过。批 F 使用 5 秒目录超时 |
| Han 字符与限额 | Han 专项和 fixture SHA-256 检查通过。批 D 使用 `tts_billable_chars` 和 `TTS_MAX_BILLABLE_CHARS` 验证发送前的上限及成本计算 |
| 线上资格 | OpenCode 网关计费仍待真实凭据下的独立验证。M1.4 记录实际证据 |

M1.0 已签收。批 A 和批 E 的接口层可分别推进，批 F 可使用现有 core 注册表和 llm HTTP 入口。[全部 CI 任务](https://github.com/bluetoblack9/JueHArbitrate/actions/runs/45556651228)

## 归档复评

复评在 Task 5 发现 reqwest 在发送时会补入默认头。修复提交 `eb2f243a7cf3e244e91712abfc0ffe69b944910e` 让 llm 的 `Transport::new` 接收 `TransportClient`。`TransportClient::new(default_headers, use_system_proxy)` 在创建客户端前过滤 `x-opencode-session`，请求的 session 值由单次调用提供。回归测试经注入客户端的 builder 实际发送，断言空 session 省略该头、非空 session 发出唯一的指定值、其他默认头仍可传输。

修复前回环回归因出现默认 session 而失败。修复后工作区测试 310 项通过、0 项失败、2 项显式生成器忽略，llm 发布构建的 5 项 HTTP 测试全部通过。格式检查、工作区严格 Clippy 和 llm all-targets 严格 Clippy 均通过。修复提交的 Linux、macOS、Windows CI 复验成功。[复评修复 CI](https://github.com/bluetoblack9/JueHArbitrate/actions/runs/45564046570)

八项任务的执行步骤和验收标准已随仓库保存。快照同期更新 llm 客户端签名和线上请求头断言。[M1.0 执行计划快照](archive/m1-provider-foundation/2026-11-25-m1-plan1-provider-foundation.md)
