//! DeepSeek 峰/谷计价窗口 + 无此窗口提供商（GLM、OpenCode Go 池）的平价卡。
//!
//! 内置费率是 2026-11-14 对照 api-docs.deepseek.com/quick_start/pricing 核验的
//! 峰时卡（谷时恰为峰时一半）。随版本发布更新，不做运行时抓取——价目 HTTP
//! 调用给纯防御性功能增加失败模式。
//!
//! 一条 lane 豁免此规则（与 budget 同理）：OpenCode Go 在文档化端点发布
//! 每模型 cost block，池子每周变动。M1 opencatalog 会话启动刷新并经
//! set_model_rates 安装；下方内置卡保持离线基线与合并目标。
//!
//! 所有时间用 Asia/Shanghai（UTC+8，无 DST）。硬编码偏移避免依赖系统
//! tzdata（最小 Linux 容器常缺）。时钟段见 Task 3（current_tier/next_transition）。

use std::collections::BTreeMap;
use std::sync::RwLock;

use crate::types::Usage;

/// DeepSeek 计价窗口判别。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    Standard,
    OffPeak,
}

/// 单张费率卡。所有费率单位 USD / 1M tokens。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ModelPricing {
    /// 缓存 miss 输入费率。
    pub input_miss_per_mtok: f64,
    /// 缓存 hit 输入费率（DeepSeek 约为 miss 的 5–10%）。
    pub input_hit_per_mtok: f64,
    /// 输出费率。
    pub output_per_mtok: f64,
    /// 标记无 DeepSeek 峰/谷窗口的提供商（GLM 全天同价）。置位后
    /// pricing_for 永不折半，无论时钟报告什么 tier——current_tier 是
    /// DeepSeek-only 概念，但调用方无条件传入。
    pub no_off_peak_discount: bool,
}

/// V4.1-Flash GA id（直连 API lane）。上游 pkg/deepseek.ModelV41Flash；
/// 按约束 5 分层归 core，M1 deepseek 别名再导出。
pub const MODEL_V41_FLASH: &str = "deepseek-flash";

/// DeepSeek 独立 pro 构建（DeepSeek-V4-Pro-0813 服务）。2026-09-10 V4.1
/// 发布时曾退役进 V4.1-Flash 路由，但价目页（2026-11-14 核验）重新列出
/// 它自己的——高得多的——费率卡，legacy-name 注释现在只点名两个 Flash
/// id。故有注册条目：unknown 回退会按 Flash 价少计它。
pub const MODEL_V4_PRO: &str = "deepseek-v4-pro";

/// 智谱 GLM-5.3，bigmodel.cn paas/v4 端点寻址——--provider=glm lane 默认模型。
pub const MODEL_GLM_53: &str = "glm-5.3";

/// OpenCode Go-plan 网关寻址的 V4.1 Flash——--provider=opencode lane 默认。
/// 与直连 API 的 "deepseek-flash" 是**不同 id 字符串**：同模型、两 lane、
/// 两费率条目，互不落对方回退。
pub const MODEL_OPENCODE_V41_FLASH: &str = "deepseek-v4.1-flash";

/// Go lane chat-completions 目录的一行。
pub struct OpenCodeGoModel {
    /// wire 模型 id——也是内置费率表的键。
    pub id: &'static str,
    /// 短选择器描述。TUI 渲染 32 列 id + 此 label，须短到 ~100 列终端
    /// 加 " (current)" 后缀仍放得下。
    pub label: &'static str,
}

/// Go lane 完整菜单：ACP session/new 下拉与 TUI /model 选择器同渲染此表，
/// 模型不可能存在于一个表面而缺于另一个。2026-11-18 对照网关公开（无 key）
/// GET https://opencode.ai/inference/go/v1/models 核验：这些是说
/// Chat-Completions（兼容适配器 wire）的 Go-allowance 模型。刻意缺席：
/// - grok-4.7、gpt-5.6-luna、gpt-6-luna、muse-spark-1.{2,3}-contributor：仅 Responses API；
/// - claude-haiku-5-5、minimax-m3、qwen3.7-plus、qwen3.8-max、qwen3.8-flash、qwen4-max：Anthropic messages wire；
/// - gemini-4-argon：generate-content wire。
///
/// 四个 *-free id **属于**本目录（"Free" = opencode/opencode-go 模型无输入
/// 价：同网关同 key，只是不计 Go allowance；cost block 全零）。仅当
/// workspace 设 OPENCODE_DISABLE_FREE_MODELS 时不可用——网关带内回答
/// "Model unavailable"，是诚实错误而非静默回退。
pub const OPENCODE_GO_MODELS: &[OpenCodeGoModel] = &[
    OpenCodeGoModel {
        id: MODEL_OPENCODE_V41_FLASH,
        label: "1M ctx · vision · lane default",
    },
    OpenCodeGoModel {
        id: MODEL_V4_PRO,
        label: "1M ctx · pro",
    },
    OpenCodeGoModel {
        id: MODEL_GLM_53,
        label: "1M ctx · top GLM",
    },
    OpenCodeGoModel {
        id: "glm-5.2",
        label: "1M ctx · previous GLM",
    },
    OpenCodeGoModel {
        id: "glm-5.3-flash",
        label: "1M ctx · fast GLM",
    },
    OpenCodeGoModel {
        id: "kimi-k3",
        label: "1M ctx · long output",
    },
    OpenCodeGoModel {
        id: "kimi-k3.1-pro",
        label: "1M ctx · top Kimi",
    },
    OpenCodeGoModel {
        id: "kimi-k2.7-code",
        label: "256K ctx · code-tuned",
    },
    OpenCodeGoModel {
        id: "minimax-m3.1-flash",
        label: "1M ctx · fast",
    },
    OpenCodeGoModel {
        id: "mimo-v2.6-flash",
        label: "1M ctx · fast",
    },
    OpenCodeGoModel {
        id: "mimo-v2.6-pro",
        label: "1M ctx",
    },
    OpenCodeGoModel {
        id: "longcat-2.0",
        label: "1M ctx",
    },
    OpenCodeGoModel {
        id: "hy4",
        label: "1M ctx",
    },
    OpenCodeGoModel {
        id: "hy4-preview",
        label: "1M ctx · preview",
    },
    OpenCodeGoModel {
        id: "hy3",
        label: "256K ctx",
    },
    OpenCodeGoModel {
        id: "step-5-preview",
        label: "1M ctx · preview",
    },
    // Free（限时）行——$0 卡，无月度 allowance。
    OpenCodeGoModel {
        id: "beam-free",
        label: "free · 1M ctx",
    },
    OpenCodeGoModel {
        id: "hy4-flash-free",
        label: "free · 1M ctx · vision",
    },
    OpenCodeGoModel {
        id: "longcat-2.5-free",
        label: "free · 1M ctx · 100/day",
    },
    OpenCodeGoModel {
        id: "iceman-contributor-free",
        label: "free · 256K ctx",
    },
];

/// DeepSeek 峰时窗口（"standard" tier）的每模型全价卡。谷时经
/// OFF_PEAK_DISCOUNT 派生。
///
/// 数字跟随官方价目页（api-docs.deepseek.com/quick_start/pricing），
/// 2026-11-15 复核。英文页是锚：下方 USD 卡、峰时 tier、谷时恰半。中文页
/// 已分叉为不同区域卡——平价 CNY、无窗口（¥1/¥0.02/¥2 flash ·
/// ¥3/¥0.025/¥6 pro）——跨页比较会读出幻影费率变化；计费跟随账户区域。
/// 更新前先比同币种。
///
/// V4.1-Flash（deepseek-flash，DeepSeek-V4.1-Flash-1017 服务）：
/// $0.30 miss · $0.006 hit · $1.20 output——2026-09-10 12:00 CST 生效，
/// 2026-11-14 核验未变。V4-Pro（deepseek-v4-pro，DeepSeek-V4-Pro-0813）：
/// $1.32 miss · $0.044 hit · $3.96 output。两卡谷时恰半。
///
/// deepseek-flash 是唯一存活 Flash id；退役 deepseek-v4-flash /
/// -flash-vision-exp 服务端路由到 V4.1 Flash 并按 Flash 价计费——正是
/// 下方 unknown 回退给它们的价格，无需逐 id 条目。deepseek-v4-pro 已离开
/// legacy 列表（见 MODEL_V4_PRO）：按自己的卡计费，miss/output 约 4×
/// Flash——回退会严重少计。legacy deepseek-chat/deepseek-reasoner 别名
/// 2026-07-24 服务端移除。
///
/// GLM 行：open.bigmodel.cn（paas/v4）GLM-5.3 平价——无峰/谷窗口，故
/// no_off_peak_discount。来源 docs.bigmodel.cn/cn/guide/start/pricing，
/// 2026-11-12 核验：¥8 输入 · ¥2 缓存命中 · ¥28 输出 / 1M tokens（单一
/// 1M-context tier；核验时缓存存储限时免费）。按 7.0 CNY/USD 换算 USD——
/// 表单位 USD 因 cost 喂 $ 状态显示；更新费率时复核换算。GLM 其他 id
/// （glm-5.3-flash / -flashx）无条目、回退 DeepSeek-Flash 卡——错误提供商，
/// 但本 lane 今天不寻址这些模型。
const STANDARD_RATES: &[(&str, ModelPricing)] = &[
    (
        MODEL_V41_FLASH,
        ModelPricing {
            input_miss_per_mtok: 0.30,
            input_hit_per_mtok: 0.006,
            output_per_mtok: 1.20,
            no_off_peak_discount: false,
        },
    ),
    (
        MODEL_V4_PRO,
        ModelPricing {
            input_miss_per_mtok: 1.32,
            input_hit_per_mtok: 0.044,
            output_per_mtok: 3.96,
            no_off_peak_discount: false,
        },
    ),
    (
        MODEL_GLM_53,
        ModelPricing {
            input_miss_per_mtok: 8.0 / 7.0,
            input_hit_per_mtok: 2.0 / 7.0,
            output_per_mtok: 28.0 / 7.0,
            no_off_peak_discount: true,
        },
    ), // ¥8 / ¥2 / ¥28
    // --- OpenCode Go lane (--provider=opencode) ---
    // 平价 $10/月 REQUEST-WINDOW 订阅（5h/周/月请求配额），下方每 token
    // 数值从来不是用户实付——存在只为防御性成本显示给出合理数字。每行
    // no_off_peak_discount：订阅无 DeepSeek 峰时钟。
    //
    // 来源：网关自己的 GET /v1/models（2026-11-18 核验）——每模型带
    // USD/1M tokens 的 cost.text block，与文档价目表一致，除了文档对两个
    // DeepSeek id 印的是峰时数字：API 报 deepseek-v4.1-flash
    // $0.15/$0.003/$0.60 带 2× 周一–五 01:00–04:00 & 06:00–10:00 UTC
    // 乘数，v4-pro $0.66/$0.022/$1.98 同 2×。下方两卡保留峰时数字
    // （err high，与 search/ASR/TTS 同姿态）。其他池 id 无乘数，listed == peak。
    //
    // glm-5.3 与 deepseek-v4-pro 已有直连 lanes 的条目（MODEL_GLM_53 /
    // MODEL_V4_PRO），此处不重复——注册表按 wire id 单键。反正仅显示层：
    // GLM-5.3 的 CNY 派生卡（~$1.14/$0.29/$4.00）接近 Go 池
    // $1.40/$0.26/$4.40，V4-Pro 直连卡已恰等于 Go 峰时价。共享键也意味着
    // 这两个继承 DeepSeek 峰时钟：Go-lane 数字谷时折半，尽管 Go 计划无
    // 谷时。已知仅显示层瑕疵——修复需 lane-aware 计价键，为订阅估算不值。
    (
        MODEL_OPENCODE_V41_FLASH,
        ModelPricing {
            input_miss_per_mtok: 0.30,
            input_hit_per_mtok: 0.006,
            output_per_mtok: 1.20,
            no_off_peak_discount: true,
        },
    ),
    (
        "glm-5.2",
        ModelPricing {
            input_miss_per_mtok: 1.40,
            input_hit_per_mtok: 0.26,
            output_per_mtok: 4.40,
            no_off_peak_discount: true,
        },
    ),
    (
        "glm-5.3-flash",
        ModelPricing {
            input_miss_per_mtok: 0.15,
            input_hit_per_mtok: 0.03,
            output_per_mtok: 0.50,
            no_off_peak_discount: true,
        },
    ),
    (
        "kimi-k3",
        ModelPricing {
            input_miss_per_mtok: 3.00,
            input_hit_per_mtok: 0.30,
            output_per_mtok: 15.00,
            no_off_peak_discount: true,
        },
    ),
    (
        "kimi-k3.1-pro",
        ModelPricing {
            input_miss_per_mtok: 2.00,
            input_hit_per_mtok: 0.20,
            output_per_mtok: 10.00,
            no_off_peak_discount: true,
        },
    ),
    (
        "kimi-k2.7-code",
        ModelPricing {
            input_miss_per_mtok: 0.95,
            input_hit_per_mtok: 0.19,
            output_per_mtok: 4.00,
            no_off_peak_discount: true,
        },
    ),
    (
        "minimax-m3.1-flash",
        ModelPricing {
            input_miss_per_mtok: 0.15,
            input_hit_per_mtok: 0.015,
            output_per_mtok: 0.60,
            no_off_peak_discount: true,
        },
    ),
    (
        "mimo-v2.6-flash",
        ModelPricing {
            input_miss_per_mtok: 0.14,
            input_hit_per_mtok: 0.0028,
            output_per_mtok: 0.28,
            no_off_peak_discount: true,
        },
    ),
    (
        "mimo-v2.6-pro",
        ModelPricing {
            input_miss_per_mtok: 0.435,
            input_hit_per_mtok: 0.003625,
            output_per_mtok: 0.87,
            no_off_peak_discount: true,
        },
    ),
    (
        "longcat-2.0",
        ModelPricing {
            input_miss_per_mtok: 0.30,
            input_hit_per_mtok: 0.006,
            output_per_mtok: 1.20,
            no_off_peak_discount: true,
        },
    ),
    (
        "hy4",
        ModelPricing {
            input_miss_per_mtok: 0.834,
            input_hit_per_mtok: 0.042,
            output_per_mtok: 2.501,
            no_off_peak_discount: true,
        },
    ),
    (
        "hy4-preview",
        ModelPricing {
            input_miss_per_mtok: 0.834,
            input_hit_per_mtok: 0.042,
            output_per_mtok: 2.501,
            no_off_peak_discount: true,
        },
    ),
    (
        "hy3",
        ModelPricing {
            input_miss_per_mtok: 0.14,
            input_hit_per_mtok: 0.035,
            output_per_mtok: 0.58,
            no_off_peak_discount: true,
        },
    ),
    (
        "step-5-preview",
        ModelPricing {
            input_miss_per_mtok: 1.00,
            input_hit_per_mtok: 0.10,
            output_per_mtok: 2.70,
            no_off_peak_discount: true,
        },
    ),
    // Free（限时）池行：API cost block 全零，卡也全零——Go-lane 回合显示
    // $0 即真相。显式条目（而非让 unknown 回退按 Flash 计价）防止免费
    // 模型显示非零估算。
    (
        "beam-free",
        ModelPricing {
            input_miss_per_mtok: 0.0,
            input_hit_per_mtok: 0.0,
            output_per_mtok: 0.0,
            no_off_peak_discount: true,
        },
    ),
    (
        "hy4-flash-free",
        ModelPricing {
            input_miss_per_mtok: 0.0,
            input_hit_per_mtok: 0.0,
            output_per_mtok: 0.0,
            no_off_peak_discount: true,
        },
    ),
    (
        "longcat-2.5-free",
        ModelPricing {
            input_miss_per_mtok: 0.0,
            input_hit_per_mtok: 0.0,
            output_per_mtok: 0.0,
            no_off_peak_discount: true,
        },
    ),
    (
        "iceman-contributor-free",
        ModelPricing {
            input_miss_per_mtok: 0.0,
            input_hit_per_mtok: 0.0,
            output_per_mtok: 0.0,
            no_off_peak_discount: true,
        },
    ),
];

/// 内置表查找（21 行线性 find，可忽略；D3-4）。
fn standard_rate(model: &str) -> Option<ModelPricing> {
    STANDARD_RATES
        .iter()
        .find(|(id, _)| *id == model)
        .map(|(_, card)| *card)
}

/// 运行时费率卡，非空时对其点名的 id 赢过内置表。M1 opencatalog 每会话
/// 安装一次。RwLock 守卫：pricing_for 跑每轮成本路径，后台刷新可能正在
/// 安装新表。
static OVERRIDES: RwLock<Option<BTreeMap<String, ModelPricing>>> = RwLock::new(None);

/// 安装运行时费率卡，覆盖内置表中点名的 id；空表清除。唯一合法调用方：
/// M1 opencatalog。卡被复制，且每张安装卡强制平价（no_off_peak_discount）
/// ——订阅 lane 无 DeepSeek 峰时钟，远端 cost block 永不被它折半。
/// 任一费率为负的卡丢弃。
pub fn set_model_rates(m: BTreeMap<String, ModelPricing>) {
    let mut clean = BTreeMap::new();
    for (id, mut card) in m {
        if card.input_miss_per_mtok < 0.0
            || card.input_hit_per_mtok < 0.0
            || card.output_per_mtok < 0.0
        {
            continue;
        }
        card.no_off_peak_discount = true;
        clean.insert(id, card);
    }
    let mut g = OVERRIDES.write().unwrap();
    *g = if clean.is_empty() { None } else { Some(clean) };
}

/// 解析模型卡：运行时覆盖优先，其次内置表。bool 报告是否存在**任何**卡
/// ——必须区分"已定价/回退"的调用方（目录合并以此为基线种子）用 bool，
/// 不与 Flash 卡数值比较。
pub fn lookup_rate(model: &str) -> (ModelPricing, bool) {
    if let Some(p) = OVERRIDES
        .read()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(model).copied())
    {
        return (p, true);
    }
    match standard_rate(model) {
        Some(p) => (p, true),
        None => (ModelPricing::default(), false),
    }
}

const OFF_PEAK_DISCOUNT: f64 = 0.5;

/// 模型+tier 的每 token 费率卡。set_model_rates 安装的卡赢过内置表。
/// unknown 模型回退 V4.1-Flash 费率——也是服务端给退役 Flash id
/// （deepseek-v4-flash / -flash-vision-exp）计费的卡，legacy 会话无需
/// 自己的条目即正确计价。deepseek-v4-pro **不在**该集合：按上方自己的
/// 注册卡计费。
pub fn pricing_for(model: &str, tier: Tier) -> ModelPricing {
    let (mut p, ok) = lookup_rate(model);
    if !ok {
        p = standard_rate(MODEL_V41_FLASH).expect("flash card embedded");
    }
    if tier == Tier::OffPeak && !p.no_off_peak_discount {
        p.input_miss_per_mtok *= OFF_PEAK_DISCOUNT;
        p.input_hit_per_mtok *= OFF_PEAK_DISCOUNT;
        p.output_per_mtok *= OFF_PEAK_DISCOUNT;
    }
    p
}

/// 一个 Usage 块在给定 tier 的 USD 成本。缓存命中输入按（便宜得多的）
/// hit 费率记账——前缀缓存优化在此变现。加法左结合，与上游一致（D3-7）。
pub fn cost(model: &str, tier: Tier, u: &Usage) -> f64 {
    let p = pricing_for(model, tier);
    const MILLION: f64 = 1_000_000.0;
    u.prompt_cache_miss_tokens as f64 * p.input_miss_per_mtok / MILLION
        + u.prompt_cache_hit_tokens as f64 * p.input_hit_per_mtok / MILLION
        + u.completion_tokens as f64 * p.output_per_mtok / MILLION
}

/// Search API 每调用费率：$0.75 / 1000 成功请求（英文价目页原生 USD；
/// 中文页 ¥5/1K。早先按 GLM 行 7.0 CNY/USD 换算 ¥5 的推导已退役：USD
/// 原生列出，无需换算）。2026-10-17 随 tool-API 家族开放；与上方 token
/// 表不同，仅 HTTP-200 调用计费。
const SEARCH_PER_CALL_USD: f64 = 0.00075; // 0.75 / 1000（Go 常量折叠位等价，D3-7）

/// n 次成功 Search API 调用的 USD 成本。失败请求免费——只传返回 2xx 的计数。
pub fn search_cost(calls: i64) -> f64 {
    calls as f64 * SEARCH_PER_CALL_USD
}

/// ASR API 每音频小时费率：$0.116/小时，英文页原生（2026-11-16 核验；
/// 中文页 ¥0.78/h——已知区域卡分叉，见 STANDARD_RATES 注释）。限时 5 折
/// （$0.058 / ¥0.39）无公布截止日，刻意**不建模**——与 search 同姿态：
/// 常量保持列表价，防御性显示不少计。按音频秒计费，向上取整。
const ASR_PER_HOUR_USD: f64 = 0.116;

/// 转录 seconds 秒音频的 USD 成本。API 按秒计费、不足 1 秒按 1 秒——
/// ceil 在**内部**做，调用方传 ASRResponse 的原始 duration_ms/1000。
/// 零/负时长（失败调用）免费。
pub fn asr_cost(seconds: f64) -> f64 {
    if seconds <= 0.0 {
        return 0.0;
    }
    seconds.ceil() * ASR_PER_HOUR_USD / 3600.0
}

/// TTS API 每万计费字符费率：$0.45 / 10000 计费字符，英文页原生
/// （2026-11-16 核验；中文页 ¥3/万——已知区域卡分叉）。限时 5 折
/// （$0.225 / ¥1.5）无公布截止日，刻意不建模——与 search/ASR 同姿态。
/// 计费字符 = 1 rune、汉字 rune 计 2；计数助手 TTSBillableChars 属 M1 deepseek。
const TTS_PER_10K_CHARS_USD: f64 = 0.45;

/// 合成 billable_chars 个计费字符的 USD 成本。TTS 响应是裸音频无 JSON
/// 信封，无服务端报告计数——调用方本地计数：仅成功调用传
/// tts_cost(TTSBillableChars(text))。失败请求免费。
pub fn tts_cost(billable_chars: i64) -> f64 {
    if billable_chars <= 0 {
        return 0.0;
    }
    billable_chars as f64 * TTS_PER_10K_CHARS_USD / 10_000.0
}

/// Files API 文档解析费率：$0.225 / 1000 页，英文页原生（2026-11-18
/// 核验——tools 家族已回到英文页，旧 ¥→$ 推导注释退役；中文页 ¥1.5/千页
/// 同值）。上传时计一次：解析即上传（返回 file id = 文档已解析），
/// text_recognition 与 rich_layout_recognition 同价。与 search/ASR/TTS
/// 同姿态：仅列表价，不建模折扣——防御性显示宁高。
const PARSE_PER_PAGE_USD: f64 = 0.000225; // 0.225 / 1000（Go 常量折叠位等价，D3-7）

/// 经 Files API 解析 pages 页文档的 USD 成本。页数 ≤0（服务端省略/上传
/// 失败）免费——router 按上传**返回**的 FileObject 记录，绝不按"尝试过
/// 上传"。
pub fn parse_cost(pages: i64) -> f64 {
    if pages <= 0 {
        return 0.0;
    }
    pages as f64 * PARSE_PER_PAGE_USD
}

/// tier 的短显示串。
pub fn tier_label(t: Tier) -> &'static str {
    match t {
        Tier::OffPeak => "off-peak -50%",
        Tier::Standard => "peak",
    }
}

/// 渲染 USD 金额，精度随量级缩放：不足 1 分 4 位小数（DeepSeek 每调用
/// 成本常低于分）、不足 1 元 3 位、1 元起 2 位。零渲染 "$0"——新会话
/// 分文未花，"$0.0000" 是精度表演。
pub fn format_cost(usd: f64) -> String {
    if usd == 0.0 {
        "$0".to_string()
    } else if usd < 0.01 {
        format!("${usd:.4}")
    } else if usd < 1.0 {
        format!("${usd:.3}")
    } else {
        format!("${usd:.2}")
    }
}

// ---- UTC+8 峰谷时钟（Task 3）----
// 所有时间 Asia/Shanghai（UTC+8，无 DST）。硬编码偏移避免依赖系统 tzdata
// （最小 Linux 容器常缺）。输入输出均为 Unix epoch 秒（UTC）；生产传
// SystemTime::now() 换算秒，测试注入固定秒（D3-1）。

/// 北京偏移秒数。
const BEIJING_OFFSET_SECS: i64 = 8 * 3600;
const SECS_PER_DAY: i64 = 86_400;

/// 峰时窗口（北京时间）：周一–五 09:00–12:00 与 14:00–18:00，左闭右开，
/// 对应 DeepSeek 公布的工作日 01:00–04:00 / 06:00–10:00 UTC（2026-11-14
/// 价目页重申）。价目页还把中国法定节假日排除出峰时——刻意近似掉：
/// 节假日表是内置卡设计拒绝的数据依赖，且把节假日工作日按峰时计价只
/// 高估防御性成本显示（err high）。其余一切——晚间、清晨、周末——
/// 谷时（半价）。
const PEAK1_START_MINS: i64 = 9 * 60;
const PEAK1_END_MINS: i64 = 12 * 60;
const PEAK2_START_MINS: i64 = 14 * 60;
const PEAK2_END_MINS: i64 = 18 * 60;

/// 拆 Unix 秒为（1970-01-01 起的北京日序, 日内秒）。
fn beijing_parts(now_unix: i64) -> (i64, i64) {
    let local = now_unix + BEIJING_OFFSET_SECS;
    (
        local.div_euclid(SECS_PER_DAY),
        local.rem_euclid(SECS_PER_DAY),
    )
}

/// 星期：0=周一 .. 6=周日。日序 0（1970-01-01）是周四。
fn weekday(days: i64) -> i64 {
    (days + 3).rem_euclid(7)
}

/// 给定瞬间生效的计价 tier。生产传当前时间，测试传固定 Unix 秒。
pub fn current_tier(now_unix: i64) -> Tier {
    let (days, secs) = beijing_parts(now_unix);
    if weekday(days) >= 5 {
        return Tier::OffPeak;
    }
    let mins = secs / 60;
    if (PEAK1_START_MINS..PEAK1_END_MINS).contains(&mins)
        || (PEAK2_START_MINS..PEAK2_END_MINS).contains(&mins)
    {
        Tier::Standard
    } else {
        Tier::OffPeak
    }
}

/// 下一 tier 变化及其起始瞬间（Unix 秒）。驱动"等谷时?"提示。
///
/// 举例（全部北京时间）：
/// - 09:00 峰时 → (谷时, 当日 12:00)
/// - 13:00 谷时 → (峰时, 当日 14:00)
/// - 03:00 谷时 → (峰时, 当日 09:00)
/// - 23:59 谷时 → (峰时, 次日 09:00——次日是周末则跳到周一 09:00)
pub fn next_transition(now_unix: i64) -> (Tier, i64) {
    let (days, secs) = beijing_parts(now_unix);
    let at = |day: i64, hour: i64, min: i64| {
        day * SECS_PER_DAY + hour * 3600 + min * 60 - BEIJING_OFFSET_SECS
    };
    // 周末全天谷时：周末内任意位置的下一变化都是周一 09:00 峰时恢复——
    // 不是最近日历边界（周六 12:00 根本不是变化点）。
    if weekday(days) >= 5 {
        let mut next = days + 1;
        while weekday(next) != 0 {
            next += 1;
        }
        return (Tier::Standard, at(next, 9, 0));
    }
    let mins = secs / 60;
    if mins < PEAK1_START_MINS {
        (Tier::Standard, at(days, 9, 0)) // [00:00,09:00) 谷 → 峰 1
    } else if mins < PEAK1_END_MINS {
        (Tier::OffPeak, at(days, 12, 0)) // [09:00,12:00) 峰 1 → 谷
    } else if mins < PEAK2_START_MINS {
        (Tier::Standard, at(days, 14, 0)) // [12:00,14:00) 谷 → 峰 2
    } else if mins < PEAK2_END_MINS {
        (Tier::OffPeak, at(days, 18, 0)) // [14:00,18:00) 峰 2 → 谷
    } else {
        // [18:00,24:00) 谷 → 次日峰 1。次日可能是周六（周五晚）——
        // 跳过整个周末到周一。
        let mut next = days + 1;
        while weekday(next) >= 5 {
            next += 1;
        }
        (Tier::Standard, at(next, 9, 0))
    }
}
