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
