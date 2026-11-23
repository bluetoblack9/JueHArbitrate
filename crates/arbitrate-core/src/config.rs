//! 配置类型与解析：per-provider API key + 默认 provider。
//!
//! key/base-url 解析优先级：provider 官方 env（非空）> config 文件 > 无。
//! 持久化（load/save）在本模块后半（Task 3）。schema_version 独立编号（D2-3）。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 配置文件名。
pub const FILENAME: &str = "config.json";
/// 当前配置 schema 版本（独立于包版本）。
pub const SCHEMA_VERSION: u32 = 1;

fn current_schema_version() -> u32 {
    SCHEMA_VERSION
}

fn is_empty_str(s: &str) -> bool {
    s.is_empty()
}

fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}

fn is_false(v: &bool) -> bool {
    !*v
}

/// 磁盘配置形状。未知 JSON 键忽略（serde 默认）；缺省字段走各自默认。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// arbitrate 自有 schema 版本；缺省视为当前版本。
    #[serde(default = "current_schema_version")]
    pub schema_version: u32,
    #[serde(skip_serializing_if = "is_empty_str")]
    pub default_provider: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub providers: BTreeMap<String, ProviderConfig>,
    #[serde(skip_serializing_if = "is_false")]
    pub path_prompt_done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggest_reply: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub push_webhooks: Vec<PushWebhook>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_notify_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read: Option<ReadConfig>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bash_env_passthrough: Vec<String>,
    #[serde(skip_serializing_if = "is_empty_str")]
    pub language: String,
    #[serde(skip_serializing_if = "is_empty_str")]
    pub safety_check: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serve: Option<ServeConfig>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub agents: BTreeMap<String, AgentConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            default_provider: String::new(),
            providers: BTreeMap::new(),
            path_prompt_done: false,
            suggest_reply: None,
            push_webhooks: Vec::new(),
            session_notify_seconds: None,
            read: None,
            bash_env_passthrough: Vec::new(),
            language: String::new(),
            safety_check: String::new(),
            serve: None,
            agents: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderConfig {
    #[serde(skip_serializing_if = "is_empty_str")]
    pub api_key: String,
    /// 完整 POST 目标（不是 base 前缀）；空 = 用 lane 编译默认。
    #[serde(skip_serializing_if = "is_empty_str")]
    pub base_url: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReadConfig {
    #[serde(skip_serializing_if = "is_zero_i64")]
    pub max_limit: i64,
    #[serde(skip_serializing_if = "is_zero_i64")]
    pub whole_read_bytes: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ServeConfig {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub projects: Vec<String>,
    /// <=0 = 执行器自身默认（默认值单点在 M5 controlplane）。
    #[serde(skip_serializing_if = "is_zero_i64")]
    pub max_concurrency: i64,
    #[serde(skip_serializing_if = "is_false")]
    pub yolo: bool,
    /// 空 = 出厂 loopback 默认（值在 M5 controlplane）。
    #[serde(skip_serializing_if = "is_empty_str")]
    pub web_addr: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentConfig {
    #[serde(skip_serializing_if = "is_empty_str")]
    pub provider: String,
    #[serde(skip_serializing_if = "is_empty_str")]
    pub model: String,
    /// low | medium | high；形状校验在 agenttypes。
    #[serde(skip_serializing_if = "is_empty_str")]
    pub effort: String,
    /// >0 时覆盖子代理 turn 上限。
    #[serde(skip_serializing_if = "is_zero_i64")]
    pub max_turns: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PushWebhook {
    /// POST 目标（format=feishu 时忽略）。必填字段，不省略序列化。
    pub url: String,
    /// ntfy | slack | discord | feishu | template | raw；空 = raw。
    #[serde(skip_serializing_if = "is_empty_str")]
    pub format: String,
    #[serde(skip_serializing_if = "is_empty_str")]
    pub template: String,
    /// 空 = 全部事件。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<String>,
    #[serde(skip_serializing_if = "is_empty_str")]
    pub app_id: String,
    #[serde(skip_serializing_if = "is_empty_str")]
    pub app_secret: String,
    #[serde(skip_serializing_if = "is_empty_str")]
    pub receive_id: String,
    /// chat_id | open_id | user_id | union_id | email；空 = chat_id。
    #[serde(skip_serializing_if = "is_empty_str")]
    pub receive_id_type: String,
}

impl Config {
    pub fn serve_yolo(&self) -> bool {
        self.serve.as_ref().is_some_and(|s| s.yolo)
    }

    pub fn read_max_limit(&self) -> i64 {
        match &self.read {
            Some(r) if r.max_limit > 0 => r.max_limit,
            _ => 200,
        }
    }

    pub fn read_whole_read_bytes(&self) -> i64 {
        match &self.read {
            Some(r) if r.whole_read_bytes > 0 => r.whole_read_bytes,
            _ => 32 * 1024,
        }
    }

    /// 仅 "high"（trim + 忽略大小写）生效；其余一律 off。
    pub fn safety_check_level(&self) -> &'static str {
        if self.safety_check.trim().eq_ignore_ascii_case("high") {
            "high"
        } else {
            ""
        }
    }

    pub fn session_notify_seconds_or_default(&self) -> i64 {
        self.session_notify_seconds.unwrap_or(60)
    }

    pub fn suggest_reply_enabled(&self) -> bool {
        self.suggest_reply.unwrap_or(true)
    }
}

/// provider → 官方 key 环境变量；未知 provider → None。
pub fn env_var_for(provider: &str) -> Option<&'static str> {
    match provider {
        "deepseek" => Some("DEEPSEEK_API_KEY"),
        "anthropic" => Some("ANTHROPIC_API_KEY"),
        "openai" => Some("OPENAI_API_KEY"),
        "gemini" => Some("GEMINI_API_KEY"),
        "glm" => Some("GLM_API_KEY"),
        "opencode" => Some("OPENCODE_API_KEY"),
        _ => None,
    }
}

/// provider → endpoint 覆盖环境变量；仅 glm/opencode。
pub fn base_url_env_for(provider: &str) -> Option<&'static str> {
    match provider {
        "glm" => Some("GLM_BASE_URL"),
        "opencode" => Some("OPENCODE_BASE_URL"),
        _ => None,
    }
}

/// 注入式环境查询（测试用假环境；禁止改真实环境变量）。
pub type EnvLookup<'a> = &'a dyn Fn(&str) -> Option<String>;

fn resolve_with(
    cfg: &Config,
    provider: &str,
    env: EnvLookup,
    env_var: Option<&str>,
    field: fn(&ProviderConfig) -> &str,
) -> String {
    if let Some(var) = env_var
        && let Some(v) = env(var)
        && !v.is_empty()
    {
        return v;
    }
    cfg.providers
        .get(provider)
        .map(field)
        .filter(|v| !v.is_empty())
        .unwrap_or_default()
        .to_string()
}

/// env > config 优先级解析 API key（注入环境）。
pub fn key_for_with(cfg: &Config, provider: &str, env: EnvLookup) -> String {
    resolve_with(cfg, provider, env, env_var_for(provider), |p| &p.api_key)
}

/// env > config 优先级解析 endpoint 覆盖（注入环境）。
pub fn base_url_for_with(cfg: &Config, provider: &str, env: EnvLookup) -> String {
    resolve_with(cfg, provider, env, base_url_env_for(provider), |p| {
        &p.base_url
    })
}

/// 读真实环境的便捷包装。
pub fn key_for(cfg: &Config, provider: &str) -> String {
    key_for_with(cfg, provider, &|name| std::env::var(name).ok())
}

/// 读真实环境的便捷包装。
pub fn base_url_for(cfg: &Config, provider: &str) -> String {
    base_url_for_with(cfg, provider, &|name| std::env::var(name).ok())
}

/// 纯内存变更：写入 provider key，保留 base_url；不触盘。
pub fn set_key(cfg: &mut Config, provider: &str, api_key: &str) {
    cfg.providers
        .entry(provider.to_string())
        .or_default()
        .api_key = api_key.to_string();
}
