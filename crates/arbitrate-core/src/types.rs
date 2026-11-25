//! Shared message and usage values. Providers and state own their encodings.

use serde::{Deserialize, Serialize};

/// token 记账 + DeepSeek 缓存元数据。
///
/// prompt_cache_hit_tokens / prompt_cache_miss_tokens 是 DeepSeek 特有字段，
/// 由前缀缓存特性暴露：高命中率意味着请求前缀匹配了已见前缀，按约 1/10
/// 的费率计费。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub prompt_cache_hit_tokens: i64,
    pub prompt_cache_miss_tokens: i64,
}

impl Usage {
    /// 本请求的前缀缓存命中率（0..1）；无缓存记账 token 时返回 0。
    pub fn hit_ratio(&self) -> f64 {
        let total = self.prompt_cache_hit_tokens + self.prompt_cache_miss_tokens;
        if total == 0 {
            return 0.0;
        }
        self.prompt_cache_hit_tokens as f64 / total as f64
    }
}

impl std::ops::AddAssign for Usage {
    /// 逐字段累加（cache Tracker 的 seal/cumulative 用）。
    fn add_assign(&mut self, other: Self) {
        self.prompt_tokens += other.prompt_tokens;
        self.completion_tokens += other.completion_tokens;
        self.total_tokens += other.total_tokens;
        self.prompt_cache_hit_tokens += other.prompt_cache_hit_tokens;
        self.prompt_cache_miss_tokens += other.prompt_cache_miss_tokens;
    }
}

/// 人类可读命中百分比（如 "73.2%"）；无缓存记账 token 时返回 "n/a"。
/// 上游 lives in pkg/deepseek（FormatHitRatio）；按约束 5 归 core 共享领域层，
/// M1 deepseek 再导出，外部调用方不必重造。
pub fn format_hit_ratio(u: &Usage) -> String {
    if u.prompt_cache_hit_tokens + u.prompt_cache_miss_tokens == 0 {
        return "n/a".to_string();
    }
    format!("{:.1}%", u.hit_ratio() * 100.0)
}

/// A message in conversation history.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Message {
    pub role: String,
    pub content: String,
    pub name: String,
    pub tool_call_id: String,
    pub tool_calls: Vec<ToolCall>,
    pub images: Vec<ImagePart>,
    pub files: Vec<FilePart>,
    pub reasoning_content: String,
    pub predicted_next: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolCall {
    /// Position supplied by a streaming delta, when present.
    pub index: Option<usize>,
    pub id: String,
    pub kind: String,
    pub name: String,
    /// Original arguments, carried as text until an adapter or tool parses them.
    pub arguments: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImagePart {
    pub asset: String,
    pub url: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FilePart {
    pub asset: String,
    pub file_id: String,
    /// Unix seconds. Zero means permanent or unknown.
    pub expires_at: i64,
    pub purpose: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Choice {
    pub index: usize,
    pub message: Message,
    pub finish_reason: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChatResponse {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Usage,
}
