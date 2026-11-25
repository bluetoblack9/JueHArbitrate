# M1 计划 2 · M1.1 批 A DeepSeek 协议类型、客户端与重试 Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans or superpowers:subagent-driven-development to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal**　让 `arbitrate-deepseek::Client::chat` 能发送非流式请求、解码为 core 响应，并在可恢复错误时按固定策略重试。向批 B/C/D 提供同一套请求编码、HTTP 发送和单层重试入口。

**Architecture**　core 保留领域消息与 Usage。DeepSeek crate 拥有协议编解码、端点、错误类型和客户端。重试只包住一次端点操作，整次操作含发送、读响应和解码。请求准备在历史副本或借用的 wire 视图上完成。

**Tech Stack**　Rust 1.90.0、edition 2024、workspace 现有 serde/serde_json、reqwest 0.12.28（rustls）和 Tokio。复用 M1.0 的 `Transport`、`wait_retry`、`until_cancelled` 和 HTTP fixture。[M1 Provider 契约](../../../docs/m1-provider-contract.md)

批 A 已完成本地与三平台验收，实施验收基线为 `3471226`。[批 A CI](https://github.com/bluetoblack9/JueHArbitrate/actions/runs/45603344276) · [批 A 验收记录](../../../docs/m1-provider-contract.md#m11-批-a-验收)

## Global Constraints

- 从 M1.0 已验收的 `dd4bfd4502bfd17d7c7436b5c29cdd92268c097f` 派生工作分支。开始前运行 `git merge-base --is-ancestor dd4bfd4502bfd17d7c7436b5c29cdd92268c097f HEAD`、`git status --short` 和 `cargo test --workspace --locked`。[M1.0 归档](../../../docs/m1-provider-contract.md)
- 锁定上游 `cb27b90d2979f5f840526100b28ade586d034df9` 只读。对照 `pkg/deepseek/{types,client,retry,vision}.go`，其中 vision 仅供核对既有纯文本 wire 形状。全部验证使用 Rust 和回环地址，使用合成凭据和手写预期。
- 保留本地依赖边 deepseek → core。仅在 deepseek 的 manifest 增加已有 workspace serde 和 serde_json 直接依赖。首次更新 manifest 后执行 `cargo check --workspace` 生成锁文件并核对差异，后续构建和测试使用 `--locked`。
- 批 A 交付纯文本 codec、非流式 Chat 和单层重试。批 B 交付流式事件、`strip_reasoning_content` 和独立的泵重试；批 C 扩展同一个 codec 以处理附件；批 D 实现 FIM、Search、ASR、TTS。M2 按工具和 lane 接线。
- 每项用例先确认预期失败，再实施。提交前依次执行 `cargo fmt --all`、`cargo fmt --check --all`、`cargo clippy --workspace --locked -- -D warnings`、任务专项测试、`git diff --check`。从 `arbitrate-rs/` 运行命令。

## 交付文件与任务顺序

以下代码文件路径以 `crates/arbitrate-deepseek/` 为根。

| 任务 | 产品文件 | 验收测试 |
| --- | --- | --- |
| 1 纯文本 wire codec | `src/wire.rs`、`src/lib.rs`、本 crate 的 `Cargo.toml`、根 `Cargo.lock` | `tests/wire.rs` |
| 2 客户端与单次 Chat | `src/client.rs`、`src/error.rs`、`src/lib.rs`、根 `tests/support/mock_http.rs`、llm 的 `crates/arbitrate-llm/tests/http_fixture.rs` | `src/client.rs` 的单次尝试单测、`tests/chat.rs` 的公开入口测试及 llm 的截断响应回归 |
| 3 单层重试 | `src/retry.rs`、`src/lib.rs`、`src/client.rs`、`src/error.rs` | `tests/chat.rs` 的重试矩阵和 `src/client.rs` 的读 body 取消单测 |
| 4 签收与交接 | 产品仓库 `docs/m1-provider-contract.md` 和 `docs/archive/m1-deepseek-batch-a/` | 完整工作区、三平台 CI 和审计证据 |

### Task 1 · 编解码纯文本请求与响应

- [x] 创建 `tests/wire.rs`，以 `#[path = "../../../tests/support/provider_messages.rs"]` 引用共享样例。同一测试文件使用 `sample_turn()` 和 `sample_response()`。构造纯文本发送样例时保留共享 assistant/tool，用自有文本 user 替换含附件的 user。另将已有的 asset 样例原样送入编码器，验证返回本地错误和原样例未变。
- [x] 编写以下断言，运行 `cargo test -p arbitrate-deepseek --locked --test wire`，先确认缺失模块/类型的失败。示例展示最小请求，矩阵覆盖其余字段。

```rust
use arbitrate_core::types::Message;
use arbitrate_deepseek::wire::{ChatRequest, MODEL_V41_FLASH, encode_chat, should_enable_thinking};

#[test]
fn minimal_text_request_has_stable_wire_shape() {
    let input = ChatRequest {
        model: MODEL_V41_FLASH.into(),
        messages: vec![Message {
            role: "user".into(),
            content: "hi".into(),
            ..Message::default()
        }],
        ..ChatRequest::default()
    };
    let expected = br#"{"model":"deepseek-flash","messages":[{"role":"user","content":"hi"}]}"#;
    assert_eq!(encode_chat(&input).unwrap(), expected);
    assert_eq!(encode_chat(&input).unwrap(), expected);
    assert!(should_enable_thinking(MODEL_V41_FLASH));
    assert!(!should_enable_thinking("deepseek-v4.1-flash"));
}
```

| 用例 | 字段断言 |
| --- | --- |
| 精简请求与空消息 | `model`、`messages` 恒存在，空集合为 `[]`。`stream=false`、`max_tokens=0`、空 tools、空 reasoning_effort 和缺席的可选对象省略。空正文省略 `content` |
| 参数全集 | `Some(0.0)` 不省略。显式填写 `temperature`、`top_p`、`max_tokens`、`tools`、`tool_choice`、`response_format`、`stream_options`、`thinking`、`reasoning_effort`、`safety_check`，逐字段断言 JSON。保留 `include_usage: false` |
| 工具续轮 | core 的 `kind/name/arguments` → `type/function.name/function.arguments`，arguments 是原始字符串，工具结果保留 `tool_call_id` 和 name。用两个工具调用分别设置 `index: Some(0)` 和 `Some(1)`，断言两条出站 JSON 都省略 index，且原消息仍保留各自的 index。已准备的 reasoning 文本原样编码，`predicted_next` 永远缺席 |
| 本地附件字段 | A 阶段遇到含图片或文件的消息返回本地编码错误，连已经解析的附件也由批 C 支持。批 C 必须替换此测试判据，增加双形 `content` 的成功用例 |
| 缓存和响应 | 完整样例检验 id/object/created/model、两个 choice 的 index/message/finish_reason 和全部五项 token；收到明确的工具调用 index 时保留其值，缺省为 None。缺省或 null usage 为零，缺省的缓存计数为零。正文 null 视为空。无效 JSON、非对象根和字段类型错误返回 decode error |
| 模型判定 | `deepseek-flash` 唯一 true。空模型、`deepseek-v4-pro`、旧 ID、大小写变体和带后缀 ID 均 false |

- [x] 新增公开 `wire::ChatRequest`（`Clone, Debug, Default`），字段如下。请求类型持有 core 的 Message。serde 派生落在私有 DTO，按表声明顺序序列化。消息发送形固定为 role → content → name → tool_call_id → tool_calls → reasoning_content。附件检测在此 DTO 转换处完成，批 C 扩展同一位置。入站工具调用 DTO 可将 index 解码成 core 的 `Option<usize>`。用于出站历史的工具调用 DTO 无 index 字段。批 B 定义流式 delta 的 index 语义。

```text
ChatRequest { model: String, messages: Vec<Message>, stream: bool,
              temperature: Option<f64>, top_p: Option<f64>, max_tokens: i64,
              tools: Vec<Tool>, tool_choice: Option<serde_json::Value>,
              response_format: Option<ResponseFormat>, stream_options: Option<StreamOptions>,
              thinking: Option<ThinkingMode>, reasoning_effort: String,
              safety_check: Option<SafetyCheck> }
Tool { kind: String, function: ToolFunction }
ToolFunction { name: String, description: String, parameters: Option<serde_json::Value> }
ResponseFormat { kind: String }
StreamOptions { include_usage: bool }
ThinkingMode { kind: String }
SafetyCheck { level: String }
```

- [x] 实现 `wire::encode_chat(&ChatRequest) -> Result<Vec<u8>, EncodeError>`、`wire::decode_chat(&[u8]) -> Result<core::types::ChatResponse, serde_json::Error>`，其中 core 指 `arbitrate_core`。本地编码错误区分附件暂未支持、数值非有限（拒绝 NaN/∞）、serde_json 序列化。将 `kind` 映射为 wire `type`；单独定义含可选字段的入站 Usage DTO，再整合为 core Usage，保留 `format_hit_ratio` 的既有实现并从 deepseek 再导出。发布 `MODEL_V41_FLASH` 和 `SAFETY_CHECK_HIGH`，`should_enable_thinking` 仅进行精确字符串比较。
- [x] 按全局门禁运行 `cargo test -p arbitrate-deepseek --locked --test wire` 和 `cargo test -p arbitrate --locked --test dependency_graph`。检查锁文件仅改变 DeepSeek 对既有 serde/serde_json 的依赖边。提交 `feat(deepseek): 定义纯文本聊天协议编解码`。

### Task 2 · 发送非流式 Chat 并保留错误分类

- [x] 在 `tests/chat.rs` 编写公开 `Client::chat` 用例。在 `src/client.rs` 的 `#[cfg(test)]` 模块编写单次尝试用例，直接调用私有 `chat_once`。两处均通过 `#[path = "../../../tests/support/mock_http.rs"]` 复用根 fixture。单测所用的 `mod mock_http;` 声明放在 `client.rs` 文件顶层并加 `#[cfg(test)]`。统一使用回环地址、`reqwest::Client::builder().no_proxy()` 和合成 key。公开入口测试保留 watch sender 到请求结束。
- [x] 运行 `cargo test -p arbitrate-deepseek --locked --test chat`，确认缺少公开入口的失败。新增单次尝试测试时运行 `cargo test -p arbitrate-deepseek --locked --lib`，确认缺少或尚未接通 `chat_once` 的失败。两层用例按下表放置，Task 3 接入重试后保留单次尝试断言。

| 测试入口 | 用例 | 网络和结果断言 |
| --- | --- | --- |
| 公开 `chat` | 成功与客户端注入 | `POST /proxy/chat/completions`，Bearer 合成 key、Content-Type 与 Accept 为 application/json。源请求 `stream=true`，线上 stream 缺席，原请求保持 true。响应取回两条 choice 与 Usage。注入 builder 时设置合成默认头 `X-Fixture-Client: injected`，mock 断言线上收到这一值 |
| 单次 `chat_once` | API 错误 | 400 和 `{"error":{"type":"invalid_request_error","code":"bad_input","message":"bad input"}}` → status 400、type、code、message 均保留，Display 为 `deepseek api error: invalid_request_error: bad input` |
| 单次 `chat_once` | API 错误兜底 | 空 message、非法 JSON、纯文本分别用完整响应体和 status 构造 `status 4xx: <body>`，再经 `ApiError` 的 Display 前缀呈现 |
| 单次 `chat_once` | 传输和解码失败 | Hangup → Transport。收到 200 或 400 响应头后 body 截断均为 Transport。完整 200 且非法 JSON 为 Decode |
| 公开构造器与 `chat` | 本地失败与取消 | 空 key 指示 `DEEPSEEK_API_KEY`。未支持的附件、无效数值和预先取消的有效请求均为零次 HTTP 请求 |

- [x] 将共享 fixture 的 `Reply::Truncated` 扩展为 `{ status: u16, prefix: Vec<u8>, missing: usize }`。让 `write_bytes` 使用传入的状态。其 `Reply::Bytes` 调用继续传 200。llm 原有截断用例显式传 200，DeepSeek 的 `chat_once` 单测用 200 和 400 验证读取失败的分类。发送头和前缀后关闭连接，声明的 Content-Length 保持大于实发 body。

- [x] 在 `client.rs` 暴露 `Client::new(api_key: String) -> Result<Self, Error>` 和 `Client::with_options(api_key: String, base: Url, timeout: Duration, builder: reqwest::ClientBuilder) -> Result<Self, Error>`。`new` 使用 `reqwest::Client::builder()`、默认 URL `https://api.deepseek.com` 和 M1.0 的 `DEFAULT_REQUEST_TIMEOUT` 调用 `with_options`。由上层传入 key。调用方通过 builder 设置代理、默认头等客户端选项。DeepSeek 构造器在 `build()` 前统一设置 `.retry(reqwest::retry::never())` 和 `.redirect(reqwest::redirect::Policy::none())`，覆盖 builder 上已有的对应策略，以保证应用层重试预算。构建错误归 `Error::RequestConfig`。将构建好的客户端传给 M1.0 `Transport`，3xx 按 API 错误处理。
- [x] 登记端点常量 `ENDPOINT_CHAT="/chat/completions"`、`ENDPOINT_FIM="/beta/completions"`、`ENDPOINT_FILES="/files"`、`ENDPOINT_SEARCH="/v1/tools/search"`、`ENDPOINT_ASR="/v1/tools/asr"`、`ENDPOINT_TTS="/v1/tools/tts"`。本批只在代码中加入 `pub(crate) ENDPOINT_CHAT`。其余常量由批 C/D 在首次消费时加入。
- [x] 在 `error.rs` 定义公开 `ApiError { status: u16, kind: String, code: String, message: String }` 和 `Error`。变体定为 `MissingApiKey`、`RequestConfig(String)`、`Encode(EncodeError)`、`Transport(reqwest::Error)`、`Api(ApiError)`、`Decode(serde_json::Error)`、`Cancelled`。Task 3 追加 `Exhausted`。用 `RequestBuilder::build_split` 取得客户端和已构建请求，构建错误归 `RequestConfig`，执行错误归 `Transport`。对传输错误保留 reqwest 来源，报告前移除其 URL；使用 serde 解析非 2xx 的 error 信封，以非空 message 为有效判据，其他情况以 body 的 UTF-8 lossy 文本构造 ApiError。最终签收时保证可通过变体或访问器取得 API 状态。
- [x] 实现以下接口。构造器为空 key 返回 `Error::MissingApiKey`，默认 URL 的错误为配置失败。Chat 将输入克隆并强制 `stream = false`，只编码一次。用 M1.0 `until_cancelled` 覆盖发送和完整 body 读取。`read_chat_response` 记录状态，先读取完整 body。读取错误直接归 `Error::Transport`，包括收到非 2xx 响应头后的断流。完整 body 到达后，按状态构造 `Error::Api` 或解码成功响应。端点请求复用现有 `Transport` 的前缀校验。批 B 单独定义开流阶段非 2xx 的读取策略，可复用 `ApiError::from_body(status, body)` 构造 API 错误。

```text
pub async fn chat(&self, request: &ChatRequest, cancel: &mut watch::Receiver<bool>)
    -> Result<arbitrate_core::types::ChatResponse, Error>
pub(crate) async fn send_json_once(&self, endpoint: &str, payload: reqwest::Body)
    -> Result<reqwest::Response, Error>
async fn chat_once(&self, body: &[u8])
    -> Result<arbitrate_core::types::ChatResponse, Error>
async fn read_chat_response(response: reqwest::Response)
    -> Result<arbitrate_core::types::ChatResponse, Error>
pub(crate) fn ApiError::from_body(status: u16, body: &[u8]) -> ApiError
```

- [x] 按全局门禁运行 `cargo test -p arbitrate-deepseek --locked --lib --test chat --test wire --test http_fixture`。另运行 `cargo test -p arbitrate-llm --locked --test http_fixture` 回归共享截断 fixture。提交 `feat(deepseek): 实现非流式聊天请求和错误分类`。

### Task 3 · 实现可取消的单层退避与重试

- [x] 在 `tests/chat.rs` 从公开 `Client::chat` 验证完整响应序列与最终结果。保留 Task 2 对 `chat_once` 的单次错误分类单测，继续断言直接返回的 `Transport`、`Decode` 或 `Api`。使用 `Client::with_sleeper` 注入可推进 sleeper。普通用例可用 `|_| std::future::ready(())`，等待边界用 watch。以 sleeper 收到 `Duration::from_millis(500)` 为进入退避的同步点，分别记录 499ms、500ms 和当前虚拟时间 0ms 的取消。首次失败后只有任务真正进入 sleeper 才允许推进时间。运行 `cargo test -p arbitrate-deepseek --locked --test chat`，确认缺少入口或未走第二次请求。
- [x] 从下表为每个脚本写独立测试或表驱动项。使用 `MockHttp.requests` 的原始字节比对每次请求，断言最后错误类型、status 和完整 attempts 后缀。每个会重试的用例显式列出两次响应。成功用例必须消耗第二个 200，失败用例必须断言计划中给出的第二次错误。断言不依赖 mock 脚本耗尽后的默认 500。

| 响应脚本或本地事件 | 完成判据 |
| --- | --- |
| 503 或 429 → 200 | 2 次，成功。两次 JSON 原始字节逐字相同 |
| Hangup、200 后断流、400 后断流或 200 坏 JSON → 200 | 各恰好 2 次并成功。400 断流与完整 400 分别验证可重试传输错误和永久 API 错误。两次 JSON 逐字相同 |
| 503 → 502、429 → 429、坏 JSON → 坏 JSON | 各 2 次。最后错误附唯一的 ` (after 2 attempts)`，仍可取得最终 API 状态或解码来源 |
| 400、401、403、404、3xx | 各 1 次，保留 API 状态，无 attempts 注释。测试 3xx 时用专用回环响应带 Location，确认请求未被隐式转发 |
| 503 → 401 | 2 次，返回最终 401，无 attempts 注释 |
| 503 后等待 | 499ms 仍仅 1 次。推进至 500ms 才发送第二次 |
| 503 后退避中取消 | 时间保持 0ms 时返回 Cancelled，无 attempts 注释，继续推进虚拟时间仍只有 1 次 |
| 已取消、等待响应头时取消、读 body 时取消 | 预先取消为 0 次。等待响应头的集成用例为 1 次。读 body 单测在真实 Response 取得后开始，确认读取正在等待。两种等待状态均须立即返回 Cancelled，且无重发 |

等待响应头的集成用例从公开 `Client::chat` 发起请求。等待服务器确认收到完整请求，服务器保持连接并暂缓发送响应头。取消返回后才释放服务器，并断言一共收到 1 次请求。

读 body 单测放在 `src/client.rs` 的 `#[cfg(test)]` 模块。在 `client.rs` 文件顶层用 `#[cfg(test)]`、`#[allow(dead_code)]` 和 `#[path = "../../../tests/support/mock_http.rs"]` 声明 `mod mock_http;`，单测通过 `super::mock_http` 使用它。用 `send_json_once` 先取得 `Reply::PausedSse` 返回的 200 Response。将该 Response 放入 Option，在 `retry_call` 的操作闭包内以 take 交给生产用的 `read_chat_response`。取消值仍为 false 时，使用 `tokio::select! { biased; result = &mut pending => panic!("提前完成: {result:?}"), _ = std::future::ready(()) => {} }` 显式 poll 同一已 pin 的 future，确认读取为 Pending。保持服务端的暂停闩关闭，发送取消，再显式 poll 同一 future，要求当次返回 `Err(Error::Cancelled)`。最后释放暂停闩，断言闭包调用数及 HTTP 请求数均为 1。为取得 Response 和等待测试信号设置实时时钟上界。生产 `chat_once` 也必须调用该读取函数，并在 `retry_call` 的取消边界内完成整个操作。

- [x] 实现 `retry.rs` 的 crate 内通用 `retry_call<T, Op, Attempt, Sleep, Delay>(cancel, sleep, op) -> Result<T, Error>`。约束 `Op: FnMut() -> Attempt`、`Attempt: Future<Output = Result<T, Error>>`、`Sleep: FnOnce(Duration) -> Delay`、`Delay: Future<Output = ()>`。只允许两次操作。第一次可重试错误后调用 M1.0 `wait_retry(cancel, sleep)`；每次执行期间用 `until_cancelled` 交给同一个取消接收器。避免在已取消时启动 I/O，可将 `op()` 写在传给 `until_cancelled` 的 async 块内。仅 429、500–599、Transport、Decode 可重试。第二次仍为可重试错误时以 `Error::Exhausted { attempts: u8, source: Box<Error> }` 包装，Display 后缀固定。取消和第二次的永久错误直接返回。
- [x] 提供 `Client::with_sleeper<F, Fut>(self, sleeper: F) -> Self`，其中 `F: Fn(Duration) -> Fut + Send + Sync + 'static`、`Fut: Future<Output = ()> + Send + 'static`。客户端字段存储可复用的装箱 sleeper，生产默认 Tokio sleep。由 `Client::chat` 把一次编码所得的 bytes 借给每次尝试；重试助手接收操作 factory，批 C 可在每次尝试重建 multipart，批 B 可在每次开流调用使用独立预算。
- [x] 对带 Location 的 3xx 测试，优先在 Task 3 的 `tests/chat.rs` 启动一次性的回环 TCP 响应，写入 307、Location、Content-Length: 0，并将 Location 指向可记录请求的 MockHttp。注入预先启用跟随重定向的 builder，断言首个响应仍是 307 且 Location 目标收到 0 次请求，验证构造器覆盖了 builder 的转发策略。按全局门禁复跑 `cargo test -p arbitrate-deepseek --locked --test chat --test wire --test http_fixture`，同时运行 `cargo test -p arbitrate-deepseek --locked --lib`，确认读 body 的取消单测通过，且 500ms 用例无需真实等待。提交 `feat(deepseek): 接入可取消的单层重试策略`。

### Task 4 · 验收并保存下游契约

- [x] 从产品根目录执行下列工作区门禁。记录固定工具链、验收 SHA、本机测试计数和三平台 CI 链接。核对 Linux musl 与 cargo-deny、Windows 原生 build 的单独结果。由功能分支 CI 提供原生证据。

```sh
set -e
cargo fmt --check --all
cargo clippy --workspace --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
cargo deny --workspace --locked check
git diff --check
```

- [x] 在 `docs/m1-provider-contract.md` 登记批 A 的 `ClientBuilder` 注入签名、自定义默认头验收、库层重试和重定向策略。记录单次尝试错误分类单测及公开 Chat 重试矩阵的结果。批 B 需复用发送入口和 wire 消息映射，并验证 503→200 空流→503→200 正常流恰 4 次 HTTP。批 C 扩展 wire `content` 双形及附件解析。批 D 调用同一个单层重试助手，FIM 等各自验证请求判据。在线资格按 M1.4 的独立清单登记。
- [x] 将已验收的本计划存入产品仓库 `docs/archive/m1-deepseek-batch-a/2026-11-25-m1-plan2-deepseek-batch-a.md`。把 `../../../arbitrate-rs/docs/m1-provider-contract.md` 重写为 `../../../docs/m1-provider-contract.md`，以归档契约链接替代快照中的外层 M1 主文档链接。在验收记录反向链接这一快照。暂存后用 `git write-tree` 获取当前索引树，再用 `git archive <tree>` 导出临时目录，验证每个本地链接均指向导出树内存在的文件。提交 `docs(deepseek): 归档批 A 验收与下游交接`。
