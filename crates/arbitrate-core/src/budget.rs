//! 每模型上下文窗口上限 + UI "快撞上限" 告警阈值。
//!
//! 表硬编码——厂商文档会变，但陈旧表失败安全（少报容量）远好于运行时
//! 探测（可能泄漏 OSC-11/TTY 探测类问题）。
//!
//! 唯一豁免 set_limits：OpenCode Go lane 在文档化 HTTPS 端点
//! （GET /v1/models）发布全目录——含 limit.context——M1 opencatalog
//! 会话启动刷新并安装到此。编译条目保持离线基线，刷新失败原样保留。
//! 需要 TTY 探测的东西仍然与此无关。

use std::collections::BTreeMap;
use std::sync::RwLock;

/// arbitrate 可对话模型的输入+上下文大小（tokens）。数值反映各提供商
/// 2026-05 公开声明；厂商发布新上限时显式更新（无自动发现——见模块注释）。
///
/// DeepSeek V4（2026-01 发布）flash/pro 均 1M 上下文。legacy
/// deepseek-chat / deepseek-reasoner 别名 2026-07-24 服务端移除；携带
/// 这些名字的旧会话文件现在落 Default 回退。
const CONTEXT_LIMITS: &[(&str, i64)] = &[
    // DeepSeek — V4.1 Flash 是唯一存活 id（V4 发布起 1M 上下文）。退役
    // V4 id 在此未知、落 Default；legacy deepseek-chat/deepseek-reasoner
    // 别名（2026-07-24 移除）同样落 Default。
    ("deepseek-flash", 1_000_000),
    // M6 增补（提前声明，二线提供商点亮时表面不漂移）。
    ("claude-3-5-sonnet-20241022", 200_000),
    ("claude-sonnet-4-20250514", 200_000),
    ("gpt-4o", 128_000),
    ("gpt-4o-mini", 128_000),
    ("gemini-2.0-flash", 1_000_000),
    ("gemini-1.5-pro", 2_000_000),
    // GLM lane（per-agent 路由）：open.bigmodel.cn glm-5.3 单一
    // 1M-context tier——与价目表 GLM 行同源。
    ("glm-5.3", 1_000_000),
    // OpenCode Go lane (--provider=opencode)：与上方条目同一 DeepSeek
    // V4.1 Flash，按网关自己的 id 字符串寻址（1M 上下文）。Go 池其余
    // 模型也说 Chat-Completions、出现在 lane 模型菜单
    // （pricing::OPENCODE_GO_MODELS）——每个在此有自己的上限，取自同一
    // GET /v1/models 的 limit.context 字段（2026-11-18 核验）。缺条目
    // 会落 Default(128K)、在真实窗口 1/8 处 nag /compact。
    ("deepseek-v4.1-flash", 1_000_000),
    ("deepseek-v4-pro", 1_000_000),
    ("glm-5.2", 1_000_000),
    ("glm-5.3-flash", 1_000_000),
    ("kimi-k3", 1_048_576),
    ("kimi-k3.1-pro", 1_048_576),
    ("kimi-k2.7-code", 262_144),
    ("minimax-m3.1-flash", 1_000_000),
    ("mimo-v2.6-flash", 1_048_576),
    ("mimo-v2.6-pro", 1_048_576),
    ("longcat-2.0", 1_000_000),
    ("hy4", 1_048_576),
    ("hy4-preview", 1_024_000),
    ("hy3", 256_000),
    ("step-5-preview", 1_000_000),
    // Free（限时）池行——同源同字段。
    ("beam-free", 1_048_576),
    ("hy4-flash-free", 1_048_576),
    ("longcat-2.5-free", 1_000_000),
    ("iceman-contributor-free", 262_144),
];

/// 未知模型查询的回退。多数现代编码模型已在 100K+ 区间，保守 128K
/// 默认避免未编目模型误报，同时真小模型溢出前仍触发预算告警。
pub const DEFAULT: i64 = 128_000;

/// 运行时窗口覆盖，非空时赢过编译表。M1 opencatalog 每会话安装一次，
/// 新列模型得到真实窗口而非 Default。RwLock 守卫：limit 跑状态栏渲染
/// 路径，后台刷新可能正在安装新表。
static OVERRIDES: RwLock<Option<BTreeMap<String, i64>>> = RwLock::new(None);

/// 安装运行时窗口覆盖；空表清除。唯一合法调用方：M1 opencatalog。
/// ≤0 值丢弃——零窗口会让 classify 除零。
pub fn set_limits(m: BTreeMap<String, i64>) {
    let clean: BTreeMap<String, i64> = m.into_iter().filter(|(_, v)| *v > 0).collect();
    let mut g = OVERRIDES.write().unwrap();
    *g = if clean.is_empty() { None } else { Some(clean) };
}

/// 模型的显式窗口（覆盖优先，其次编译表），**无** Default 回退。
/// 必须区分"已编目/猜测"的调用方（主要是目录合并）用此，不与 Default
/// 比较——真 128K 窗口的模型会被误读。
pub fn known(model: &str) -> (i64, bool) {
    if let Some(v) = OVERRIDES
        .read()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(model).copied())
    {
        return (v, true);
    }
    match CONTEXT_LIMITS.iter().find(|(id, _)| *id == model) {
        Some((_, v)) => (*v, true),
        None => (0, false),
    }
}

/// 模型上下文窗口大小。未知模型回退 Default。
pub fn limit(model: &str) -> i64 {
    let (v, ok) = known(model);
    if ok { v } else { DEFAULT }
}

/// 给定 token 用量相对模型上下文窗口，UI 该多担心。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Ok,
    Warn,
    Critical,
}

/// 阈值（占 limit 的比例）：
/// - Ok 0.00–0.60：无告警
/// - Warn 0.60–0.75：状态栏着色，用户注意到
/// - Critical ≥0.75：显式 "consider /compact" 提示
///
/// 从 0.80/0.95 降到 0.60/0.75，让 /compact 提示早到可行动：1M 上下文
/// 模型的 95% 是 950K tokens——届时摘要调用本身花真钱，模型质量已被
/// 巨上下文拖垮。
pub const WARN_FRACTION: f64 = 0.60;
pub const CRITICAL_FRACTION: f64 = 0.75;

/// used_tokens 相对模型上限的严重度。
pub fn classify(model: &str, used_tokens: i64) -> Severity {
    let l = limit(model);
    if l <= 0 {
        return Severity::Ok;
    }
    let frac = used_tokens as f64 / l as f64;
    if frac >= CRITICAL_FRACTION {
        Severity::Critical
    } else if frac >= WARN_FRACTION {
        Severity::Warn
    } else {
        Severity::Ok
    }
}

/// 原始 used_tokens/limit 比值，**不裁剪**——超出是信号，调用方不应
/// 假设它被封顶。
pub fn fraction(model: &str, used_tokens: i64) -> f64 {
    let l = limit(model);
    if l <= 0 {
        return 0.0;
    }
    used_tokens as f64 / l as f64
}
