# M1 计划 1 · M1.0 领域类型、传输测试设施与依赖门禁 Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans or superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal**　让 DeepSeek 客户端和通用 provider 可以从同一份领域样例开始开发，并能在本地重现请求、断流、重试等待和取消。

**Architecture**　共享领域值放在 core。两个 provider 各自直接依赖 core，各自拥有传输和协议实现。复用工作区根目录的 Rust 测试辅助文件，供不同 crate 的集成测试编译。Han 属性表归首个生产消费者 deepseek。

**Tech Stack**　Rust 1.90.0、edition 2024、Cargo resolver 3、reqwest 0.12（rustls）、Tokio 1、cargo-deny。计划日期 2026-11-25。命令从产品仓库 `arbitrate-rs/` 执行，示例 shell 为 Bash。本快照保存 M1.0 的执行步骤及验收标准。

**执行状态**　八项任务已签收。初次三平台 CI 验证提交为 `c735b50`，交接记录提交为 `041ba0b`。复评修复提交为 `eb2f243`。[验收记录](../../../docs/m1-provider-contract.md#m10-签收)

## Global Constraints

- M0.4 已签收。以 `527ec07b98190c4d8a4ab4ef58b5eb58faa7c705` 为三平台 CI 验收基线。实施前确认工作分支包含该基线，并把归档链接记入任务 1 的审计记录。[M0 归档](../../../docs/archive/m0-plan4/README.md) · [GitHub Actions 运行 #1](https://github.com/bluetoblack9/JueHArbitrate/actions/runs/45544518702)
- 只读参考锁定上游。所有功能测试、fixture 校验和数据生成用例均使用 Rust；本地 HTTP 仅用回环端口和虚构凭据。无需历史 Go 文件、Go 工具链、真实 HOME 或 provider key。
- 保留 9 个 workspace package、版本 0.1.0 和 core 无网络边界。两个 provider 的本地依赖各限 core，包括 dev/build 依赖。使用 M0 既有 Usage、pricing、budget、cache 和依赖图检查。
- 以下每个涉及 Rust 的任务在专项测试获得预期红灯后实施，提交前依次运行 `cargo fmt --all`、`cargo fmt --check --all`、`cargo clippy --workspace --locked -- -D warnings`、该任务的专项测试及 `git diff --check`。命令链任一步失败即停。更新依赖时仅首次解析允许写锁文件，之后使用 `--locked`。只暂存该任务列出的文件。
- 保留协议头 `x-opencode-session`、缓存字段和 `SEEK_*` 环境变量。M1.0 交付离线 UA 契约；真实网关计费资格单独登记。M1.1–M1.3 接手端点、事件解析、重试策略、端点计费接线及目录安装的相应集成工作。

## 交付位置与依赖

| 任务 | 产品仓库内的文件 | 下游如何使用 |
| --- | --- | --- |
| 1 基线审计 | 新增 `docs/m1-provider-contract.md` | 记录准入证据、源码差异、字段去向和后续验证责任 |
| 2 领域类型 | 修改 `crates/arbitrate-core/src/types.rs`，新增 `tests/support/provider_messages.rs`、`crates/arbitrate-core/tests/provider_messages.rs` | 批 A/C/E 编译同一 Rust 样例 |
| 3 传输依赖与 CI | 根 `Cargo.toml` 与八个 `crates/*/Cargo.toml`（全部 package 继承许可证，依赖调整限于根、core 和两个 provider）、`Cargo.lock`、`tests/dependency_graph.rs`、`deny.toml`、`.github/workflows/ci.yml`、`docs/m1-provider-contract.md` | 两个 crate 取得真实依赖边和固定的工具链检查 |
| 4 HTTP/SSE 替身 | 修改 `tests/support/mock_http.rs`；新增两个 provider 各自的 `tests/http_fixture.rs` | 批 A–F 使用同一记录器；根 `tests/mock_http_selftest.rs` 持续回归 |
| 5 UA 与超时 | 新增 `crates/arbitrate-llm/src/transport.rs`、`crates/arbitrate-deepseek/src/transport.rs`，各自修改 `src/lib.rs`，扩展任务 4 的测试 | 批 A/E/F 复用真实请求构造入口 |
| 6 退避与取消 | 扩展 deepseek 的 `src/transport.rs` 与 `tests/http_fixture.rs` | 批 A 的单次调用、批 B 的读流使用同一取消语义 |
| 7 Han 资产 | deepseek 的 `src/han.rs`、`src/han_ranges.rs`、`src/lib.rs`；根 `tests/fixtures/unicode/`、`tests/fixture_manifest.rs`、`tests/fixtures/manifest.json`、`docs/m1-provider-contract.md` | 批 D 直接调用计费字符函数 |
| 8 签收 | 更新任务 1 的审计记录，以及已关闭条目所在的 `docs/deferred.md` | 批 A、E 可开始；批 F 还需 llm HTTP 设施 |

执行顺序为 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8。每个任务独立提交。签收记录中分别列出本地运行、原生 CI 和线上资格的状态。

### Task 1 · 确认基线和消息字段去向

- [x] 核对 M0.4 验收清单及原生 CI，按 Global Constraints 登记。复跑原有权益与计价测试，预期四个测试目标均通过，尤其是 `cost_locked_at_record_model_switch` 和 `cost_locked_at_record_tier_transition`。

```sh
set -e
git merge-base --is-ancestor 527ec07b98190c4d8a4ab4ef58b5eb58faa7c705 HEAD
cargo test -p arbitrate-core --locked --test types --test pricing --test budget --test cache
```

- [x] 从 `UPSTREAM-REFERENCE.txt` 取本地路径和提交。针对指定目录运行只读 diff，并从提交对象读取实际源码。若 checkout 有差异，在审计中分别记录已提交差异、工作区差异与影响的行为；采用锁定版本。若锁定源码与 M1 规格冲突，先回写行为及判据，再开展受影响的任务。

```sh
set -e
src=$(sed -n 's/^local_reference: //p' UPSTREAM-REFERENCE.txt)
pin=$(sed -n 's/^upstream_commit: //p' UPSTREAM-REFERENCE.txt)
git -C "$src" cat-file -e "$pin^{commit}"
paths=(pkg/deepseek pkg/llm internal/opencatalog cmd/seek/main.go pkg/agent)
git -C "$src" diff --name-status "$pin" HEAD -- "${paths[@]}"
git -C "$src" diff --name-status HEAD -- "${paths[@]}"
git -C "$src" grep -n -E 'buildProvider|llm\.Message\{|StripReasoningContent' "$pin" -- "${paths[@]}"
git -C "$src" show "$pin:pkg/deepseek/types.go"
```

- [x] 在 `docs/m1-provider-contract.md` 写入固定 SHA、实际差异和下表。审阅时对照 `pkg/deepseek/{types,stream,vision,files}.go`、`pkg/llm/llm.go`、各 adapter、agent 投影与 main 的 lane 构造。对照 `internal/opencatalog` 登记空快照忽略、有效快照过滤后空覆盖表清除，以及 Rust 对三个一等模型的保护。目录网络测试归批 F。

| 领域字段 | DeepSeek（批 A/B/C） | 通用视图（M2 投影，批 E 适配） |
| --- | --- | --- |
| role、content、name | 文本形或含附件的 content 数组；保留适用的 name | role/text，工具结果的 name 用于 tool name；各 adapter 映射到对应角色 |
| tool_calls 的 id、type、函数名、arguments；tool_call_id | 标识、函数载荷及工具结果关联；流式 index 用于分片重组 | 使用 id/name/arguments 和工具结果关联；arguments 保留字符串到 adapter 边界 |
| reasoning_content、predicted_next | 在副本上准备发送；仅带工具调用的 assistant 保留 reasoning；全部清除 prediction | 两字段均留在领域历史 |
| images 的 asset/url；files 的 asset/file_id/expires_at/purpose | asset 用于解析，URL/file_id 用于发送；到期时间、用途供上传及复用判定；批 C 验证文本→图片→文件次序 | M2 将每条消息的原正文复制到通用视图，按图片数、文件数依次追加各自的提示。数量为零时跳过该类提示；附件引用保留在领域历史中 |
| id、object、created、model、choices 的 index/message/finish_reason、Usage 五项 | 响应 DTO 逐字段解码到领域值 | 流式事件另行映射；OpenAI 最后非空结束原因、Anthropic 原值、Gemini 按批 E 归一化 |

M2 投影验收使用 Task 2 的共享样例。以正文 `资料`、两张图片和一个文件为例，预期正文如下（Rust 字符串字面量）。无附件时预期为原正文；仅有一种附件时仅追加对应提示。M1.0 将该期望和计数规则写入审计，M2 在 agent 投影入口逐字断言并检查原消息未变。[锁定源码 msgsToLLM](https://github.com/whyiyhw/seek/blob/cb27b90d2979f5f840526100b28ade586d034df9/pkg/agent/translate.go#L20-L35)

```rust
"资料\n\n[image: 2 张图片未透传 — 当前 provider 不支持图片输入]\n\n[file: 1 个文档未透传 — 当前 provider 不支持文件输入]"
```

- [x] 审计写入 OpenCode 在线资格「未验证」或已有证据，写明 UA 离线测试无法验证计费。提交 `docs(provider): 记录 M1 源码基线与字段映射`。

### Task 2 · 实现可由三个 crate 复用的领域样例

- [x] 在 `tests/support/provider_messages.rs` 定义测试函数 `pub fn sample_turn() -> Vec<Message>`。构造三个合法连续角色：user 的正文为「资料」，含两个不同的图片 asset 和一个文件 asset（doc.pdf）；assistant 带 reasoning、prediction 和 id 为 `call-1` 的函数调用；tool 带同一关联 ID 和工具名。另构造图片 URL、已解析 file_id、过期时间及 purpose，用于请求副本。通过下面的模块引用从 core 集成测试取样；两个 provider 的 tests 使用相同相对路径。测试辅助文件只依赖 `arbitrate_core::types`。

```rust
#[path = "../../../tests/support/provider_messages.rs"]
mod provider_messages;

#[test]
fn tool_result_keeps_association_and_history_owns_attachments() {
    let history = provider_messages::sample_turn();
    assert_eq!(history[1].tool_calls[0].id, history[2].tool_call_id);
    assert_eq!(history[1].tool_calls[0].arguments, r#"{"path":"a"}"#);
    let mut copy = history.clone();
    copy[0].files[0].file_id = "file-123".into();
    assert!(history[0].files[0].file_id.is_empty());
    assert_eq!(history[0].files[0].asset, "doc.pdf");
}
```

- [x] 新增响应样例：两个 choice 分别以 `stop`、`tool_calls` 结束；断言顺序、index、model、created、object、id 和消息，Usage 用命中 8、未命中 2 验证现有 `hit_ratio()` 得 0.8。运行 `cargo test -p arbitrate-core --locked --test provider_messages`，预期因 core 缺少新类型而编译失败。
- [x] 在现有 `types.rs` 增加以下公开值类型和字段（字段均 pub，derive Clone、Debug、PartialEq、Eq；样例需要默认值的结构 derive Default）。`Usage` 保留原定义。附件采用可同时携带本地身份与传输缓存的值，空串为缺席；Unix 秒 0 表示未知或永久。API/disk serde 映射由各自所有者实现。

```text
Message     { role: String, content: String, name: String, tool_call_id: String,
              tool_calls: Vec<ToolCall>, images: Vec<ImagePart>, files: Vec<FilePart>,
              reasoning_content: String, predicted_next: String }
ToolCall    { index: Option<usize>, id: String, kind: String, name: String, arguments: String }
ImagePart   { asset: String, url: String }
FilePart    { asset: String, file_id: String, expires_at: i64, purpose: String }
Choice      { index: usize, message: Message, finish_reason: String }
ChatResponse{ id: String, object: String, created: i64, model: String,
              choices: Vec<Choice>, usage: Usage }
```

- [x] 执行全局格式/Clippy 门禁及 `cargo test -p arbitrate-core --locked --test provider_messages --test types --test cache`。确认旧 Usage 行为和新样例均通过。提交 `feat(core): 建立 provider 共享消息与响应类型`。

### Task 3 · 在真实依赖图上启用传输与审计门禁

- [x] 在现有 `tests/dependency_graph.rs` 用已有 `local_edges` 和 Cargo metadata 新增正向断言：deepseek、llm 各自声明的本地边集合恰为 {core}。保留既有直接/间接违规、rename、dev/build/target 等测试。运行 `cargo test -p arbitrate --locked --test dependency_graph`，新增断言应在当前两个空 manifest 上失败。
- [x] 在根 `[workspace.dependencies]` 定义以下依赖。两个 provider 的 `[dependencies]` 继承 arbitrate-core、reqwest、tokio；仅 deepseek 的 reqwest 启用 multipart。两 crate 的 dev-dependencies 继承 tokio 并开启 rt、test-util。按 M0 延后登记删除 core 冗余的 serde_json dev-dependency 并补尾换行，保留原 normal dependencies。[reqwest feature 表](https://docs.rs/crate/reqwest/0.12.24/features)

```toml
reqwest = { version = "0.12.24", default-features = false, features = ["rustls-tls", "json", "stream"] }
tokio = { version = "1", features = ["macros", "sync", "time", "rt"] }
```

- [x] 在 Rust 1.90.0 运行一次 `cargo check --workspace` 解析并审查 lockfile。确认 `cargo tree -p arbitrate-deepseek -e features --locked` 和 llm 对应命令显示 rustls。若锁定的传递包超出 MSRV，选择满足依赖约束且支持 1.90 的版本，再执行 `cargo check --workspace --locked`。保留 M0 图检查负责 workspace 内部边。
- [x] 新增 `deny.toml`，用版本固定的 cargo-deny 检查 advisories、licenses、bans、sources。初始策略如下；依据实际锁定依赖逐项核实许可证，若需新增 SPDX 条目，记录包名及许可证来源。依据产品根目录的 MIT 许可证，在根 `[workspace.package]` 设置 `license = "MIT"`。根 binary 与八个成员在各自的 `[package]` 设置 `license.workspace = true`。运行 `cargo metadata --locked --no-deps --format-version 1`，确认九个本地 package 的 `license` 均为 `MIT`。外部依赖按各自许可证审计。[产品 LICENSE](../../../LICENSE)

```toml
[advisories]
version = 2
[licenses]
allow = ["MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Unicode-3.0", "Zlib", "OpenSSL"]
[bans]
multiple-versions = "warn"
wildcards = "warn"
[sources]
unknown-registry = "deny"
unknown-git = "deny"
```

- [x] 在现有 CI 的 Linux job 安装 `musl-tools`（TLS 原生依赖的交叉编译器），在原 musl 检查前确认 `musl-gcc --version`。增加 Linux 步骤 `uses: taiki-e/install-action@v2`、`with: { tool: cargo-deny@0.20.2 }`，随后运行 `cargo deny --workspace --locked check`。保留三平台原 CI。工具获取或 advisory 库不可用时记为基础设施失败并复跑。0.18.4 无法解析当前 RustSec 的 CVSS 4.0 公告，实施时固定为 0.20.2。[工具版本](https://github.com/EmbarkStudios/cargo-deny/releases/tag/0.20.2) · [安装 action](https://github.com/taiki-e/install-action)
- [x] 执行全局门禁、`cargo build --workspace --locked`、图检查及 `cargo deny --workspace --locked check`。首次签收时运行 `cargo deny --workspace --locked list --layout crate`，在输出中核实 `arbitrate-deepseek`、`arbitrate-llm`、`reqwest` 和 `rustls` 四项，并将对应条目写入任务 1 的审计记录。[cargo-deny list 参数](https://github.com/EmbarkStudios/cargo-deny/blob/0.20.2/src/cargo-deny/list.rs#L39-L44)

  审查 diff 的 manifest/锁文件/CI/配置和测试。提交 `chore(deps): 锁定 provider 传输依赖并启用依赖审计`。

### Task 4 · HTTP/SSE 替身记录原始字节并制造确定的中断

- [x] 两个 provider 的 `tests/http_fixture.rs` 顶层引入以下模块。在 deepseek 专项测试用 `reqwest::multipart::Form` 先加 text("purpose", "user_data")，再加已知长度的 Part::bytes(vec![0, 255, 1]).file_name("a.bin")；发送时带虚构的 Authorization 和两条同名自定义头。llm 用无 Authorization 的 GET 读取一个返回 `[0, 255, 1]` 的二进制响应。两个测试都实际连接各自的 `MockHttp::start` 地址。使用 `Client::builder().no_proxy()` 隔离机器代理。各自构造不同实例，避免请求串线。

```rust
#[allow(dead_code)]
#[path = "../../../tests/support/mock_http.rs"]
mod mock_http;
```

- [x] 新增三个响应脚本及客户端断言。对截断响应先断言收到 200，再用 `response.bytes().await.is_err()` 区别于正常 EOF；对暂停脚本取得首段后挂起下一次读取，供 Task 6 取消。跑 `cargo test -p arbitrate-deepseek -p arbitrate-llm --locked --test http_fixture`，应因缺少响应变体或记录字段失败。
可直接放入 llm 的测试目标的读错误用例如下。响应状态与 body 读取必须分别断言。

```rust
#[tokio::test]
async fn truncated_body_is_an_error_after_success_headers() {
    use mock_http::{MockHttp, Reply};
    let server = MockHttp::start(vec![Reply::Truncated {
        prefix: b"data: one".to_vec(),
        missing: 3,
    }]);
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let response = client.get(format!("http://{}/events", server.addr))
        .send().await.unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert!(response.bytes().await.is_err());
    assert_eq!(server.requests.lock().unwrap().len(), 1);
}
```

- [x] 在 `Recorded` 增加 `raw_body: Vec<u8>` 和 `headers: Vec<(String, Vec<u8>)>`。头名统一 ASCII 小写，保留重复头及值，逐行按字节读取，移除 CRLF 和头值两端的 HTTP 空白。继续提供 M0 的 method、path、body 字符串视图。已知长度请求 read_exact；对 chunked 请求逐块解帧、消费分隔 CRLF 与终止 trailers，再返回载荷。非法长度或提前 EOF 返回 I/O 错误。增加原始 TCP 用例送入两个 chunk（含 0xff），断言记录的载荷无损。
- [x] 在原 handle 的 match 扩展下列 Reply。保留原来的 Json、Sse、Hangup 和 macOS 接受连接后显式恢复阻塞模式的逻辑。

| Reply 变体 | 写入 socket 的行为和验收 |
| --- | --- |
| `Bytes { content_type: String, body: Vec<u8> }` | 200 与准确 Content-Length，原样 write_all；客户端读出 00 ff 01 |
| `Truncated { prefix: Vec<u8>, missing: usize }` | 测试传 missing=3，Content-Length=prefix.len()+3，flush 前缀后关闭连接；确保错误发生在读 body 时 |
| `PausedSse { first: String, release: Arc<(Mutex<bool>, Condvar)> }` | 写并 flush 200 头和第一条完整 chunk。用 wait_timeout_while 等待显式 release 或 5 秒清理上界，随后写终止 chunk；客户端测试发出清理信号并 notify_all |

- [x] 从记录的 Content-Type 解析 boundary，按该 boundary 和 CRLF 取每个 part 的 Content-Disposition 与原始载荷。断言字段序恰为 `["purpose","file"]`、文件名和二进制内容；搜索整个 body 中的单词无法区分头与载荷。检查重复头逐项存在，llm 对照请求中 Authorization 缺席。执行全局门禁、两个新目标及 `cargo test -p arbitrate --locked --test mock_http_selftest`。提交 `feat(provider): 扩展共享 HTTP 替身的字节与断流场景`。

### Task 5 · 固定 UA、超时及可注入的请求入口

- [x] 两个 crate 各导出 `pub mod transport`。在各自 `http_fixture` 先写请求测试。以回环 mock 提供的完整 `/v1/chat/completions` URL 调用 llm 的 `openai_request` 两次，body 相同、session 分别为空和 `test-session`；断言两份原始 body 相同，首个请求无 `x-opencode-session`，后一请求恰有一个且值为 `test-session`。两次均断言 method 为 POST，记录的请求目标恰为 `/v1/chat/completions`。再传入同一 mock 的完整自定义 endpoint，断言 POST 目标恰为 `/gateway/custom-infer?api-version=test`，验证路径末尾没有多出的斜线，且查询串保留。UA 对开发和发布版本分别为 `arbitrate/dev`、`arbitrate/0.1.0`。deepseek 的基础请求不附加 session。运行两个专项目标，先确认缺模块/入口的红灯。
- [x] DeepSeek 的 `Transport::new` 接收 `reqwest::Client`、`reqwest::Url` 和 `Duration`。llm 的 `Transport::new` 使用相同的 base 和超时参数，客户端类型为下述 `TransportClient`。两端均提供 `request(&self, method: reqwest::Method, relative: &str) -> Result<reqwest::RequestBuilder, String>`。URL 拼接失败映射为固定错误文案。此相对路径入口供 DeepSeek 等使用目录 base 的调用方。base 以目录 URL 解释，规范化尾斜线，对 relative 移除前导斜线，保留 base 路径前缀。测试用 base `http://127.0.0.1:<port>/inference/go/v1/` + `/chat/completions`，断言完整 path。通过 builder 的 `.timeout(self.timeout)` 施加每请求超时。两个 crate 独立实现薄入口。relative 仅接收调用方提供的端点路径；拒绝 scheme、authority、查询和片段，经 URL API 分别构造查询参数，避免覆盖注入的 host。
- [x] 两端分别定义聊天默认 `Duration::from_secs(300)`，llm 的目录抓取默认 `Duration::from_secs(5)`。用测试读取构建后的 `Request::timeout()` 比较默认值、目录值和注入的 30ms。llm 添加纯函数 `user_agent(version: Option<&str>) -> String`，空/缺版本为 dev。默认版本来源取 `if cfg!(debug_assertions) { None } else { Some(env!("CARGO_PKG_VERSION")) }`，在文档记录这一构建策略。llm 在默认构造时计算 UA，另提供消费 self 的 builder 方法 `with_user_agent(self, value: String) -> Self` 供测试注入两种版本，OpenAI 与 compatible 共用 `openai_request(&self, endpoint: reqwest::Url, session: &str) -> reqwest::RequestBuilder`，将完整 endpoint 直接传给 `self.client.post(endpoint)` 并应用同一 UA 和超时，保留配置的路径及查询串。该入口仅对非空 session 添加协议头；批 F 使用目录 UA，其他 provider 的头遵照各自协议。mock 必须断言实际传输值，包括默认构建入口的 UA。
- [x] 复评要求 llm 使用可复用的 `TransportClient::new(default_headers: HeaderMap, use_system_proxy: bool) -> Result<TransportClient, reqwest::Error>`。它在构建 reqwest 客户端前删除默认头中的 `x-opencode-session`，保留其他默认头。由每次 OpenAI 请求的 session 参数决定该头是否出现。通过注入的客户端调用原 builder 的 `.send()`，在 mock 端断言空值时没有该头，非空值时仅有指定值，同时确认无关默认头仍被发送。

- [x] 执行全局门禁、`cargo test -p arbitrate-deepseek -p arbitrate-llm --locked --test http_fixture` 及 `cargo test -p arbitrate-llm --release --locked --test http_fixture`。提交 `feat(provider): 建立可注入的请求入口与 UA 契约`。

### Task 6 · 证明退避和读流都能立即取消

- [x] 在 deepseek 的 `tests/http_fixture.rs` 写三个 Rust 场景。测试驱动只串联“一次 HTTP → 等待 → 再一次 HTTP”；状态码、次数预算、空流判定及错误注释由批 A/B 实现。以首个响应已收到作为同步点。

| 场景 | 触发顺序 | 必须观察到的结果 |
| --- | --- | --- |
| 时间推进 | 回环 mock 先 503 后 200；收到 503 后，将注入的测试时钟置于 0ms，把同一个退避 future pin 一次，在 0ms 和 499ms 各显式 poll 一次，再推进 1ms | 前两次均为 Pending，且请求数为 1。500ms 时显式 poll 得到 Ready(Ok(()))，随即发送第二次，请求数为 2 |
| 退避中取消 | 收到 503，保持测试时钟为 0ms，首次 poll 至 Pending，发送 true，在时钟仍为 0ms 时立即 poll 同一个 future | 当次必须得到 `Ready(Err(Cancelled))`。驱动只在成功分支重发；取消返回后再用测试时钟的 `send_replace` 推进 1s，确认请求数仍为 1 |
| 读流中取消 | 实时时钟下向 PausedSse 发起请求，读到首段，将第二次 `response.chunk()` 与取消组合并首次 poll 至 Pending，然后发出取消 | 读任务结束，响应和事件发送端在任务返回时释放；接收端先获得首段、再获得 None。服务器清理闩在观察任务结束后才放行；总请求数为 1 |

- [x] 先运行 `cargo test -p arbitrate-deepseek --locked --test http_fixture`，确认缺少取消/等待 API。测试中用 `tokio::select! { biased; result = &mut future => panic!("提前完成: {result:?}"), _ = std::future::ready(()) => {} }` 对已 pin 的 future 分别在计时起点和推进 499ms 后做 Pending 检查。推进到 500ms 后用相同的 biased select，完成分支断言 `Ok(())`，兜底分支报错。使用 `watch::channel(Duration::ZERO)` 控制测试时钟，由注入的 sleeper 等待 `elapsed >= delay`，两项退避测试均调用下述生产入口 `wait_retry(&mut cancel, sleeper)`，sleeper 接收入口传入的 delay，返回输出为 `()` 的等待 future。三次边界 poll 共用同一 future。取消场景在 0ms 发出取消后使用 biased select 检查完成分支为 `Err(Cancelled)`，若仍为 Pending 则立即失败。真实 TCP 继续使用实时时钟。
- [x] 在 deepseek transport 定义以下两个函数。批 A 在生产重试分支调用 `wait_retry(&mut cancel, tokio::time::sleep).await`。由 `wait_retry` 选择时长，调用方仅提供等待函数。每次操作持有独立 watch receiver，发送方存活到请求结束；已取消或控制端消失均退出。读流端将相同取消原语包住下一次 chunk。取消分支优先处理同时就绪情形。释放 future 即结束被取消的等待。

```rust
use std::future::Future;
use std::time::Duration;
use tokio::sync::watch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

pub const RETRY_DELAY: Duration = Duration::from_millis(500);

pub async fn wait_retry<S, F>(
    cancel: &mut watch::Receiver<bool>,
    sleep: S,
) -> Result<(), Cancelled>
where
    S: FnOnce(Duration) -> F,
    F: Future<Output = ()>,
{
    until_cancelled(cancel, sleep(RETRY_DELAY)).await
}

pub async fn until_cancelled<F: Future>(
    cancel: &mut watch::Receiver<bool>,
    operation: F,
) -> Result<F::Output, Cancelled> {
    tokio::select! {
        biased;
        _ = cancel.wait_for(|value| *value) => Err(Cancelled),
        result = operation => Ok(result),
    }
}
```

- [x] 断言时使用独立的规格值 499ms 和 500ms。将 `wait_retry` 的传入时长临时调为 400ms 或 600ms，时间推进用例应分别在 499ms 或 500ms 失败；还原实现后复跑。用单调虚拟时间检验前两行，用有限的真实超时给读流场景兜底。执行全局门禁和上述专项命令，重复运行三场景以排除调度竞争。提交 `feat(deepseek): 支持可推进退避与读流取消`。

### Task 7 · 锁定 Unicode 17.0.0 Han 数据和计数

- [x] 在 `crates/arbitrate-deepseek/src/han.rs` 写单元测试，导出供批 D 复用的 `pub fn tts_billable_chars(text: &str) -> usize` 和 `pub const TTS_MAX_BILLABLE_CHARS: usize = 10_000`。先仅声明模块与测试，运行 `cargo test -p arbitrate-deepseek --locked --lib han`，确认缺少计数实现而失败。至少包括下表；10,000 的请求发送判定由批 D 使用同一常量测试。

| 输入 | 手算期望 |
| --- | --- |
| 空串、ASCII、U+3001、U+3006 | 依次为 0、1、1、1 |
| U+3005、U+2E80、U+2F00、U+3021、U+3038、U+20000 | 各为 2 |
| 4,999 个「汉」加 1、2、3 个 ASCII 字符 | 9,999、10,000、10,001 |

- [x] 仅维护资产时获取固定版本原始字节，保留来源中的版权声明及随附许可；常规构建与测试完全离线。自固定版本 UCD 的 ReadMe 和 Scripts 文件头核对版本号；按下列地址下载 Scripts，另存 Unicode 数据许可于同一 fixture 目录。将两个文件分别登记到根清单生成器的 META（`path`、含来源 URL 与版本的 `spec_source`、`schema_version`），由原生成器写入 SHA-256。审阅时对比实际源文件摘要与清单。[Unicode 17.0.0 Scripts](https://www.unicode.org/Public/17.0.0/ucd/Scripts.txt) · [Unicode 数据许可](https://www.unicode.org/license.txt)

```sh
set -e
mkdir -p tests/fixtures/unicode
curl -fLsS https://www.unicode.org/Public/17.0.0/ucd/Scripts.txt -o tests/fixtures/unicode/Scripts-17.0.0.txt
curl -fLsS https://www.unicode.org/license.txt -o tests/fixtures/unicode/LICENSE.txt
cargo test -p arbitrate --locked --test fixture_manifest regenerate_fixture_manifest -- --ignored --exact
cargo test -p arbitrate --locked --test fixture_manifest
```

- [x] 在 han 模块的 `#[cfg(test)]` 中实现纯 Rust 生成器。包含原文 `include_str!("../../../tests/fixtures/unicode/Scripts-17.0.0.txt")`；逐行去掉 `#` 注释、按 `;` 拆字段，仅保留属性恰为 `Han` 的行。把单码点或 `start..end` 按十六进制解析成闭区间，按起点排序，断言无交叠，可合并相邻区间。用独立的、仅显式运行的 ignored test 将结果写成 `src/han_ranges.rs` 的 `const HAN_RANGES: &[(u32, u32)]`。对 `src/han.rs` 使用同目录 `include!("han_ranges.rs")` 引入表；初次从空切片声明引导编译。固定十六进制格式和换行，生成文件使用 `#[rustfmt::skip]` 标注该数组。运行 `cargo test -p arbitrate-deepseek --locked --lib han::tests::regenerate_han_ranges -- --ignored --exact`。生产端仅编译该静态区间表。
- [x] 在生产函数中遍历 `text.chars()`，每个标量计 1，命中静态表再加 1。用 `partition_point(|&(_, end)| end < cp)` 找候选，再检查候选起点。普通测试直接断言从固定原文解析的**全部区间**等于编译表，以覆盖扩展区及属性空洞；另逐项跑手算边界。执行全局门禁、han 单测和 fixture 清单测试。在审计中登记 Unicode 版本可能与上游运行时不同，线上账单另验。提交 `feat(deepseek): 固定 Unicode Han 字符计费表`。

### Task 8 · 验收并移交下游

- [x] 从产品仓库根目录运行以下门禁。linux-musl 的命令在具备任务 3 工具的 Linux runner 执行；Windows 原生 runner 同时执行 `cargo build --workspace --locked`。记录实际执行环境、提交、通过/失败/忽略数和 job 链接。核对两个显式再生成测试均带 ignore，常规测试不写文件。

```sh
set -e
cargo fmt --check --all
cargo clippy --workspace --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
cargo deny --workspace --locked check
git diff --exit-code -- Cargo.lock tests/fixtures crates/arbitrate-deepseek/src/han_ranges.rs
git diff --check
```

```sh
set -e
rustup target add x86_64-unknown-linux-musl
cargo check --workspace --locked --target x86_64-unknown-linux-musl
```

- [x] 在审计记录按下表逐项附测试目标或证据。更新 deferred 内 Message/ChatResponse、Han 计数条目的交付提交。若最终接口签名与主计划不同，一并更新外层 M1 §3；涉及 M0 已公布契约时同步 M0 §3 和总纲。工作区外层计划与产品仓库归档分别定位。

| 签收项 | 证据 | 后续批次的第一项消费验证 |
| --- | --- | --- |
| core 字段/Usage、价格和窗口 | `provider_messages`、`types`、`pricing`、`budget`、`cache` | A 验响应和文本，C 验附件副本，E 验适用字段的请求翻译，F 验整表安装，M2 验通用消息正文中的附件数量、顺序和原文 |
| 真实依赖边、MSRV、锁文件 | `dependency_graph`、九个本地包的 MIT 元数据、workspace 级 cargo-deny 成功记录与两端及传输依赖的清单、三平台构建及 musl | A/E 加入实际 codec 后复跑 |
| 记录头与原始载荷、截断、暂停 | 两个 `http_fixture` 和根 `mock_http_selftest` | B 验中途错误/终态，C 验真实上传，E 验流事件 |
| UA、session、超时与取消 | 两种完整 endpoint 的 POST 目标、目录 base 路径、debug/release UA、无/有 session、可注入超时、同一 wait_retry 入口在 499ms 为 Pending、500ms 完成；0ms 取消立即返回且推进后请求数仍为 1；读流中取消 | A/B 验请求层和泵层各自的尝试预算；F 验目录客户端 5s |
| Han 资产、生成和门限 | 清单摘要、全区间一致性、手算边界 | D 验超限在发送前拒绝、TTS 成本消费 |
| 线上资格 | OpenCode 计费证据或明确的「未验证」 | M1.4 按独立门控记录真实结果 |

- [x] 提交 `docs(provider): 归档 M1.0 验收与下游交接`。核对 `git status --short` 和执行前记录的工作区状态。M1.0 签收后，批 A 与批 E 接口层可分别开工；批 F 可复用 core 注册表及已交付的 llm HTTP 入口。
