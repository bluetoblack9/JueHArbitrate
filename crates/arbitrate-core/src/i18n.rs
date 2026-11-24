//! core::i18n — 视图层消息目录（en/zh 双表，英文兜底）+ Go fmt 迷你格式化器。
//! 规格：上游 internal/i18n（只读参考），Rust std-only 重写。
//! 范围：仅视图层人类可读文案本地化；LLM prompt 路径与状态栏不本地化（字节稳定）。
//! 品牌适配（约束 10）：lang.persist_failed 路径 ~/.seek → ~/.arbitrate；SEEK_LANG 保留。

use std::collections::BTreeMap;

pub const LANG_EN: &str = "en";
pub const LANG_ZH: &str = "zh";

/// en 源语言目录（14 key）。每个 key 必须存在；parity 测试钉死 zh 不漂移。
pub const MESSAGES_EN: &[(&str, &str)] = &[
    ("view.starting", "starting…"),
    (
        "view.provider_banner",
        "⚠ Provider: %s — FIM / cache stats / Reasoner disabled",
    ),
    ("view.thinking", "thinking…"),
    ("view.thinking_s", "thinking… %ds"),
    ("view.thinking_ms", "thinking… %dm%ds"),
    ("view.queue.steering", "↪ steering: "),
    ("view.queue.queued", "↰ queued: "),
    (
        "menu.hint.simple",
        "  ↑/↓ navigate · Enter accept · Esc cancel",
    ),
    (
        "menu.hint.toggle",
        "  ↑/↓ navigate · Space toggle · Enter confirm · Esc cancel",
    ),
    ("cmd.unknown", "unknown command %s — try /help"),
    (
        "lang.status",
        "Language: %s (view text). Switch with /lang <en|zh>; SEEK_LANG overrides per-launch.",
    ),
    (
        "lang.switched",
        "Language switched to %s. Already-rendered lines keep their original language.",
    ),
    (
        "lang.unsupported",
        "Unsupported language %q — supported: en, zh.",
    ),
    (
        "lang.persist_failed",
        "warning: could not persist language to ~/.arbitrate/config.json: %v",
    ),
];

/// zh 简体中文目录（14 key，key 集合与 en 一致）。占位符数量/顺序/类型与 en 同步。
pub const MESSAGES_ZH: &[(&str, &str)] = &[
    ("view.starting", "启动中…"),
    (
        "view.provider_banner",
        "⚠ 提供方：%s — FIM / 缓存统计 / Reasoner 已禁用",
    ),
    ("view.thinking", "思考中…"),
    ("view.thinking_s", "思考中… %d 秒"),
    ("view.thinking_ms", "思考中… %d 分 %d 秒"),
    ("view.queue.steering", "↪ 转向中："),
    ("view.queue.queued", "↰ 已排队："),
    ("menu.hint.simple", "  ↑/↓ 移动 · Enter 采纳 · Esc 取消"),
    (
        "menu.hint.toggle",
        "  ↑/↓ 移动 · Space 勾选 · Enter 确认 · Esc 取消",
    ),
    ("cmd.unknown", "未知命令 %s — 试试 /help"),
    (
        "lang.status",
        "语言：%s（界面文案）。用 /lang <en|zh> 切换；SEEK_LANG 可按次覆盖。",
    ),
    ("lang.switched", "已切换到 %s。已渲染的旧行保持原语言。"),
    ("lang.unsupported", "不支持的语言 %q —— 可选：en、zh。"),
    (
        "lang.persist_failed",
        "警告：语言设置未能写入 ~/.arbitrate/config.json：%v",
    ),
];

/// 折叠 locale tag 到支持的语言码，不支持 → ""。
/// 区域/脚本/编码后缀先剥离（IndexAny "_-." 位置 >0 才截断）。
pub fn normalize(tag: &str) -> String {
    let tag = tag.trim().to_lowercase();
    if tag.is_empty() {
        return String::new();
    }
    let cut = tag.find(['_', '-', '.']).unwrap_or(tag.len());
    let base = if cut > 0 { &tag[..cut] } else { &tag[..] };
    match base {
        LANG_EN => LANG_EN.to_string(),
        LANG_ZH => LANG_ZH.to_string(),
        _ => String::new(),
    }
}

/// 格式化实参（位置消费）。Str 供 %s/%q/%v，Int 供 %d。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Arg {
    Str(String),
    Int(i64),
}

impl From<&str> for Arg {
    fn from(s: &str) -> Self {
        Arg::Str(s.to_string())
    }
}
impl From<String> for Arg {
    fn from(s: String) -> Self {
        Arg::Str(s)
    }
}
impl From<i64> for Arg {
    fn from(n: i64) -> Self {
        Arg::Int(n)
    }
}

/// Go strconv.Quote 近似（D4-1）：ASCII 可打印原样 + 常见转义；
/// 控制字符 \xHH / \uHHHH / \UHHHHHHHH；可打印非 ASCII 原样。
/// 天花板：Go IsPrint 比 char::is_control 宽（U+00AD 等），目录 %q 实参为 ASCII。
fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{7}' => out.push_str("\\a"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{b}' => out.push_str("\\v"),
            c if (c as u32) < 0x20 || (c as u32) == 0x7f => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c if c.is_control() => {
                let u = c as u32;
                if u > 0xffff {
                    out.push_str(&format!("\\U{:08x}", u));
                } else {
                    out.push_str(&format!("\\u{:04x}", u));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Go fmt 迷你格式化器（D4-1）。仅 %s/%d/%q/%v/%%（目录闭集）。
/// 类型不匹配 → Go 兼容错误标记；arg 耗尽 → MISSING；多余 arg 忽略（文档化偏差）。
pub fn format_message(s: &str, args: &[Arg]) -> String {
    let b = s.as_bytes();
    let mut out = String::new();
    let mut argi = 0;
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 1 < b.len() {
            let verb = b[i + 1] as char;
            if verb == '%' {
                out.push('%');
                i += 2;
                continue;
            }
            if !matches!(verb, 's' | 'd' | 'q' | 'v') {
                // 未知 verb 字面保留；仅消费 '%'，后续按完整 UTF-8 char 复制。
                out.push('%');
                i += 1;
                continue;
            }
            let arg = args.get(argi);
            argi += 1;
            match (verb, arg) {
                (_, None) => out.push_str(&format!("%!{verb}(MISSING)")),
                ('d', Some(Arg::Int(n))) => out.push_str(&n.to_string()),
                ('d', Some(Arg::Str(s))) => out.push_str(&format!("%!d(string={s})")),
                ('s', Some(Arg::Str(s))) => out.push_str(s),
                ('s', Some(Arg::Int(n))) => out.push_str(&format!("%!s(int={n})")),
                ('v', Some(Arg::Str(s))) => out.push_str(s),
                ('v', Some(Arg::Int(n))) => out.push_str(&n.to_string()),
                ('q', Some(Arg::Str(s))) => out.push_str(&quote(s)),
                ('q', Some(Arg::Int(n))) => out.push_str(&format!("%!q(int={n})")),
                _ => unreachable!(),
            }
            i += 2;
        } else {
            // 按 char 边界推进，保留多字节 UTF-8
            let ch = s[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// 单语言不可变消息束。构造后经 new/from_catalogue；不 mutate。
#[derive(Clone, Debug)]
pub struct Bundle {
    lang: String,
    msgs: BTreeMap<String, String>,
}

impl Bundle {
    /// lang 的束。不支持 → en（config 拼写错误代价是翻译，不是 CLI 罢工）。
    pub fn new(lang: &str) -> Bundle {
        let (code, table) = if normalize(lang) == LANG_ZH {
            (LANG_ZH, MESSAGES_ZH)
        } else {
            (LANG_EN, MESSAGES_EN)
        };
        Bundle {
            lang: code.to_string(),
            msgs: table
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    /// 测试缝（D4-3）：直接注入目录，镜像上游同包构造稀疏 Bundle。
    /// 生产不调用；仅供集成测试验证兜底洞。
    pub fn from_catalogue(lang: &str, msgs: BTreeMap<String, String>) -> Bundle {
        Bundle {
            lang: lang.to_string(),
            msgs,
        }
    }

    pub fn lang(&self) -> &str {
        &self.lang
    }

    /// 兜底链（D4-3 逐字照搬）：msgs 命中即用（命中但空 → key，不回退 en）；
    /// 未命中 → en；仍无/空 → key。有 args 则格式化。
    pub fn t(&self, key: &str, args: &[Arg]) -> String {
        let s = match self.msgs.get(key) {
            Some(v) => v.clone(),
            None => MESSAGES_EN
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
                .unwrap_or_default(),
        };
        if s.is_empty() {
            return key.to_string();
        }
        if args.is_empty() {
            s
        } else {
            format_message(&s, args)
        }
    }
}
