//! core::keymap — 用户可定制键绑定（封闭动作集 + 默认表 + 双向查询）。
//! 规格：上游 internal/keymap（只读参考），Rust std-only 重写。
//! M0 数据层：Resolve 收规范键串 &str（bubbletea KeyMsg.String() 同形），
//! 不引入 bubbletea（core 零 TUI 依赖）；KeyPressMsg→&str 转换 M2/M3 接线（D4-12）。
//! 品牌适配：Interrupt 描述 "Quit arbitrate"，警告 `arbitrate keys actions`（D4-4）。
//! 绑定文件路径 ~/.arbitrate/keybindings.toml（paths::user_keybindings，无回退）。

use std::collections::{BTreeMap, HashMap};

/// 封闭可重绑动作集。None = Resolve 无匹配（调用方 fall through 到默认处理）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Action {
    None,
    Submit,
    Steer,
    Interrupt,
    Cancel,
    CycleMode,
    ClearScreen,
    ToggleReasoning,
    ToggleHelp,
    HistoryPrev,
    HistoryNext,
}

impl Action {
    /// 用户面标识符（toml 键 / `arbitrate keys actions` 列）。None → ""。
    pub fn as_str(self) -> &'static str {
        match self {
            Action::None => "",
            Action::Submit => "submit",
            Action::Steer => "steer",
            Action::Interrupt => "interrupt",
            Action::Cancel => "cancel",
            Action::CycleMode => "cycle-mode",
            Action::ClearScreen => "clear-screen",
            Action::ToggleReasoning => "toggle-reasoning",
            Action::ToggleHelp => "toggle-help",
            Action::HistoryPrev => "history-prev",
            Action::HistoryNext => "history-next",
        }
    }
    /// 名称 → 动作（大小写敏感）。未知/空 → None（Option）。
    pub fn parse(name: &str) -> Option<Action> {
        match name {
            "submit" => Some(Action::Submit),
            "steer" => Some(Action::Steer),
            "interrupt" => Some(Action::Interrupt),
            "cancel" => Some(Action::Cancel),
            "cycle-mode" => Some(Action::CycleMode),
            "clear-screen" => Some(Action::ClearScreen),
            "toggle-reasoning" => Some(Action::ToggleReasoning),
            "toggle-help" => Some(Action::ToggleHelp),
            "history-prev" => Some(Action::HistoryPrev),
            "history-next" => Some(Action::HistoryNext),
            _ => None,
        }
    }
}

/// 一个动作的标识符 + 默认键 + 一行描述。/help 与 `arbitrate keys actions` 直接渲染。
pub struct ActionInfo {
    pub action: Action,
    pub default_key: &'static str,
    pub description: &'static str,
}

/// 稳定显示序（新增动作须同步 M2/M3 dispatch）。Interrupt 描述品牌适配。
const ALL_ACTIONS: &[ActionInfo] = &[
    ActionInfo {
        action: Action::Submit,
        default_key: "enter",
        description: "Submit the current input (or queue while streaming)",
    },
    ActionInfo {
        action: Action::Steer,
        default_key: "alt+enter",
        description: "Interrupt the current stream and inject the input as a steer",
    },
    ActionInfo {
        action: Action::Interrupt,
        default_key: "ctrl+c",
        description: "Quit arbitrate (saves the session first)",
    },
    ActionInfo {
        action: Action::Cancel,
        default_key: "esc",
        description: "Cancel: stream / review-entry / setup-wizard / open picker",
    },
    ActionInfo {
        action: Action::CycleMode,
        default_key: "shift+tab",
        description: "Cycle permission mode: ask → plan → yolo",
    },
    ActionInfo {
        action: Action::ClearScreen,
        default_key: "ctrl+l",
        description: "Clear the visible terminal (scrollback preserved)",
    },
    ActionInfo {
        action: Action::ToggleReasoning,
        default_key: "ctrl+r",
        description: "Toggle whether reasoning chunks render in the transcript",
    },
    ActionInfo {
        action: Action::ToggleHelp,
        default_key: "?",
        description: "Open the help overlay (only when input is empty)",
    },
    ActionInfo {
        action: Action::HistoryPrev,
        default_key: "up",
        description: "Previous prompt from history (only when input is empty)",
    },
    ActionInfo {
        action: Action::HistoryNext,
        default_key: "down",
        description: "Next prompt from history",
    },
];

pub fn all_actions() -> &'static [ActionInfo] {
    ALL_ACTIONS
}

/// 绝不可重绑到非默认动作的键（破坏 textarea/picker）。parse 时拒绝。
/// enter/esc/up/down 不在此列——它们可作 Submit/Cancel/History 源重绑。
const RESERVED_KEYS: &[&str] = &[
    "tab",
    "backspace",
    "space",
    "shift+enter",
    "pgup",
    "pgdn",
    "home",
    "end",
];

pub fn is_reserved_key(key: &str) -> bool {
    RESERVED_KEYS.contains(&key)
}
pub fn is_known_action(name: &str) -> bool {
    Action::parse(name).is_some()
}

/// Snapshot 一行——/help 与 `arbitrate keys list --json` 渲染所需元数据。
pub struct Binding {
    pub action: Action,
    pub key: String,
    pub source: &'static str, // "default" | "user"
    pub description: &'static str,
}

/// 已解析的 (action→key) 与 (key→action) 表。构造后不可变（Resolve 是热路径单查）。
/// 字段 pub(crate) 供 Task 10 parse 层 apply。
pub struct KeyMap {
    pub(crate) by_action: BTreeMap<Action, String>,
    pub(crate) by_key: HashMap<String, Action>,
    pub(crate) source: HashMap<Action, &'static str>,
}

impl KeyMap {
    /// 全动作绑默认键。无文件 / 文件被拒时用。
    pub fn new_default() -> KeyMap {
        let mut by_action = BTreeMap::new();
        let mut by_key = HashMap::new();
        let mut source = HashMap::new();
        for info in all_actions() {
            by_action.insert(info.action, info.default_key.to_string());
            by_key.insert(info.default_key.to_string(), info.action);
            source.insert(info.action, "default");
        }
        KeyMap {
            by_action,
            by_key,
            source,
        }
    }
    /// 规范键串 → 动作，无绑定 → None。
    pub fn resolve(&self, key: &str) -> Action {
        self.by_key.get(key).copied().unwrap_or(Action::None)
    }
    /// 动作当前绑定键，未知 → ""。
    pub fn key_for(&self, action: Action) -> &str {
        self.by_action
            .get(&action)
            .map(|s| s.as_str())
            .unwrap_or("")
    }
    /// "default" | "user"，未知 → ""。
    pub fn source_for(&self, action: Action) -> &'static str {
        self.source.get(&action).copied().unwrap_or("")
    }
    /// 当前绑定，AllActions 稳定显示序。
    pub fn snapshot(&self) -> Vec<Binding> {
        all_actions()
            .iter()
            .map(|info| Binding {
                action: info.action,
                key: self.key_for(info.action).to_string(),
                source: self.source_for(info.action),
                description: info.description,
            })
            .collect()
    }
}

// ---- parse 层（Task 10）----

use std::collections::HashSet;
use std::io::Write;
use std::path::Path;

/// 手写 TOML 子集解析（禁 BurntSushi crate，D4-13）。仅 [bindings] 段收集；
/// 其余段语法校验但忽略。值必须字符串（basic/literal）；非字符串值拒绝
/// （文档化天花板：keybindings 只需字符串）。重复键/段拒绝。BTreeMap 排序迭代。
pub fn parse_toml_bindings(input: &str) -> Result<BTreeMap<String, String>, String> {
    let mut bindings = BTreeMap::new();
    let mut seen_sections: HashSet<String> = HashSet::new();
    let mut seen_keys: HashSet<(String, String)> = HashSet::new();
    let mut current = String::new();
    for (idx, raw) in input.lines().enumerate() {
        let lineno = idx + 1;
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix('[') {
            let inner = rest
                .strip_suffix(']')
                .ok_or_else(|| format!("line {lineno}: unterminated section header"))?
                .trim();
            // bare 或 quoted 段名（D4-13 承诺 quoted；评审 P2-5）
            let name = if inner.starts_with('"') || inner.starts_with('\'') {
                let (n, remainder) = parse_key(inner, lineno)?;
                if !remainder.trim().is_empty() {
                    return Err(format!(
                        "line {lineno}: trailing content after section name"
                    ));
                }
                n
            } else {
                if !inner
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
                {
                    return Err(format!("line {lineno}: invalid section name"));
                }
                inner.to_string()
            };
            if name.is_empty() {
                return Err(format!("line {lineno}: empty section name"));
            }
            if !seen_sections.insert(name.clone()) {
                return Err(format!("line {lineno}: duplicate section [{name}]"));
            }
            current = name;
            continue;
        }
        let (key, value) = split_key_value(line, lineno)?;
        if !seen_keys.insert((current.clone(), key.clone())) {
            return Err(format!("line {lineno}: duplicate key {key}"));
        }
        if current == "bindings" {
            bindings.insert(key, value);
        }
    }
    Ok(bindings)
}

/// 去注释（# 仅在字符串外）。basic 串处理 \\ 转义跳过，literal 串无转义。
fn strip_comment(line: &str) -> &str {
    let b = line.as_bytes();
    let (mut in_basic, mut in_literal) = (false, false);
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if in_basic {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == b'"' {
                in_basic = false;
            }
        } else if in_literal {
            if c == b'\'' {
                in_literal = false;
            }
        } else {
            match c {
                b'"' => in_basic = true,
                b'\'' => in_literal = true,
                b'#' => return &line[..i],
                _ => {}
            }
        }
        i += 1;
    }
    line
}

fn split_key_value(line: &str, lineno: usize) -> Result<(String, String), String> {
    let (key, rest) = parse_key(line, lineno)?;
    let rest = rest
        .trim_start()
        .strip_prefix('=')
        .ok_or_else(|| format!("line {lineno}: expected '=' after key"))?
        .trim_start();
    let value = parse_string_value(rest, lineno)?;
    Ok((key, value))
}

fn parse_key(line: &str, lineno: usize) -> Result<(String, &str), String> {
    if let Some(rest) = line.strip_prefix('"') {
        scan_basic(rest, lineno)
    } else if let Some(rest) = line.strip_prefix('\'') {
        scan_literal(rest, lineno)
    } else {
        let end = line
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
            .unwrap_or(line.len());
        if end == 0 {
            return Err(format!("line {lineno}: empty key"));
        }
        Ok((line[..end].to_string(), &line[end..]))
    }
}

fn parse_string_value(s: &str, lineno: usize) -> Result<String, String> {
    let (v, rest) = if let Some(rest) = s.strip_prefix('"') {
        scan_basic(rest, lineno)?
    } else if let Some(rest) = s.strip_prefix('\'') {
        scan_literal(rest, lineno)?
    } else {
        return Err(format!("line {lineno}: only string values are supported"));
    };
    if !rest.trim().is_empty() {
        return Err(format!(
            "line {lineno}: trailing content after string value"
        ));
    }
    Ok(v)
}

/// basic 串：转义 \b \t \n \f \r \" \\ \uXXXX \UXXXXXXXX。返回 (值, 闭引号后余串)。
fn scan_basic(s: &str, lineno: usize) -> Result<(String, &str), String> {
    let b = s.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'"' => return Ok((out, &s[i + 1..])),
            b'\\' => {
                i += 1;
                if i >= b.len() {
                    return Err(format!("line {lineno}: dangling escape"));
                }
                match b[i] {
                    b'b' => {
                        out.push('\u{0008}');
                        i += 1;
                    }
                    b't' => {
                        out.push('\t');
                        i += 1;
                    }
                    b'n' => {
                        out.push('\n');
                        i += 1;
                    }
                    b'f' => {
                        out.push('\u{000C}');
                        i += 1;
                    }
                    b'r' => {
                        out.push('\r');
                        i += 1;
                    }
                    b'"' => {
                        out.push('"');
                        i += 1;
                    }
                    b'\\' => {
                        out.push('\\');
                        i += 1;
                    }
                    b'u' | b'U' => {
                        let n = if b[i] == b'u' { 4 } else { 8 };
                        let start = i + 1;
                        if start + n > b.len() {
                            return Err(format!("line {lineno}: short unicode escape"));
                        }
                        // 先验 ASCII hex 字节（b 是 &[u8]，字节切片不劈 UTF-8）；全 ASCII →
                        // start+n 是 char 边界，str 切片安全（评审 P2-5：\u界界 走 Err 不 panic）
                        let hex_bytes = &b[start..start + n];
                        if !hex_bytes.iter().all(|c| c.is_ascii_hexdigit()) {
                            return Err(format!("line {lineno}: bad unicode escape"));
                        }
                        let cp = u32::from_str_radix(std::str::from_utf8(hex_bytes).unwrap(), 16)
                            .map_err(|_| format!("line {lineno}: bad unicode escape"))?;
                        out.push(
                            char::from_u32(cp)
                                .ok_or_else(|| format!("line {lineno}: invalid unicode scalar"))?,
                        );
                        i = start + n;
                    }
                    _ => return Err(format!("line {lineno}: unknown escape")),
                }
            }
            _ => {
                let ch = s[i..].chars().next().unwrap();
                out.push(ch);
                i += ch.len_utf8();
            }
        }
    }
    Err(format!("line {lineno}: unterminated basic string"))
}

/// literal 串：无转义，到首个 ' 止。
fn scan_literal(s: &str, lineno: usize) -> Result<(String, &str), String> {
    match s.find('\'') {
        Some(i) => Ok((s[..i].to_string(), &s[i + 1..])),
        None => Err(format!("line {lineno}: unterminated literal string")),
    }
}

fn display_path(path: &str) -> &str {
    if path.is_empty() { "<inline>" } else { path }
}

fn normalize_key(key: &str) -> String {
    key.to_lowercase()
}

fn known_actions_list() -> String {
    all_actions()
        .iter()
        .map(|i| i.action.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// 宽松启发式：拒明显垃圾（空段/非修饰符前缀/末段含空白），接受 modifier+name 形。
fn looks_like_known_key(key: &str) -> bool {
    if key.is_empty() {
        return false;
    }
    let parts: Vec<&str> = key.split('+').collect();
    for (i, p) in parts.iter().enumerate() {
        if p.is_empty() {
            return false;
        }
        if i < parts.len() - 1 {
            match *p {
                "ctrl" | "alt" | "shift" | "cmd" | "meta" | "super" => continue,
                _ => return false,
            }
        }
    }
    let last = parts[parts.len() - 1];
    !last.contains(' ') && !last.contains('\t')
}

/// 解析文件 + 应用用户覆盖。警告逐行写 warn（不返回 error，总返回可用 KeyMap）。
/// bool = 文件被应用(true) / 全拒回退默认(false)。硬拒：toml 语法错 / 键冲突。
/// 软错（未知动作/保留键/不可识别键/空值）只丢该行 + 警告。
pub fn load(path: &Path, warn: &mut dyn Write) -> (KeyMap, bool) {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (KeyMap::new_default(), true),
        Err(e) => {
            let _ = writeln!(
                warn,
                "keybindings: read {}: {e} (using defaults)",
                path.display()
            );
            return (KeyMap::new_default(), false);
        }
    };
    load_bytes(&data, &path.to_string_lossy(), warn)
}

/// 从字节解析。path 仅用于警告前缀（"" → <inline>）。语义同 load。
pub fn load_bytes(data: &[u8], path: &str, warn: &mut dyn Write) -> (KeyMap, bool) {
    let text = match std::str::from_utf8(data) {
        Ok(t) => t,
        Err(e) => {
            let _ = writeln!(
                warn,
                "keybindings: toml syntax error in {}: {e} (using defaults)",
                display_path(path)
            );
            return (KeyMap::new_default(), false);
        }
    };
    let raw = match parse_toml_bindings(text) {
        Ok(m) => m,
        Err(e) => {
            let _ = writeln!(
                warn,
                "keybindings: toml syntax error in {}: {e} (using defaults)",
                display_path(path)
            );
            return (KeyMap::new_default(), false);
        }
    };
    let mut km = KeyMap::new_default();
    let mut accepted: Vec<(Action, String)> = Vec::new();
    // BTreeMap 排序迭代 → 警告序与冲突 first-wins 确定（D4-14）
    for (name_raw, value) in &raw {
        let key = value.trim();
        let action_name = name_raw.trim();
        let action = match Action::parse(action_name) {
            Some(a) => a,
            None => {
                let _ = writeln!(
                    warn,
                    "keybindings: unknown action {:?} in {} (available: {})",
                    action_name,
                    display_path(path),
                    known_actions_list()
                );
                continue;
            }
        };
        if key.is_empty() {
            let _ = writeln!(
                warn,
                "keybindings: action {:?} has empty value in {}",
                action_name,
                display_path(path)
            );
            continue;
        }
        let key = normalize_key(key);
        if is_reserved_key(&key) {
            let _ = writeln!(
                warn,
                "keybindings: reserved key {:?} cannot be rebound ({} = {:?} in {})",
                key,
                action_name,
                key,
                display_path(path)
            );
            continue;
        }
        if !looks_like_known_key(&key) {
            let _ = writeln!(
                warn,
                "keybindings: unrecognised key {:?} for action {:?} in {} (see `arbitrate keys actions` for examples)",
                key,
                action_name,
                display_path(path)
            );
            continue;
        }
        accepted.push((action, key));
    }
    // 冲突检测：终表（默认 + 用户覆盖）同键被两动作占用 → 全文件拒（PRD §4.4 rule 4）。
    // D4-14 硬化（评审 P2-4）：查终表而非仅 accepted——单项重绑撞另一动作的未改默认键
    // 也是真冲突（上游漏检致反表静默覆盖）。swap 终表键互异 → 不拒。
    let mut final_map: BTreeMap<Action, String> = km.by_action.clone();
    for (a, k) in &accepted {
        final_map.insert(*a, k.clone());
    }
    let mut multi: BTreeMap<String, Vec<Action>> = BTreeMap::new();
    for (a, k) in &final_map {
        multi.entry(k.clone()).or_default().push(*a);
    }
    let mut conflicts = false;
    for (key, actions) in &multi {
        if actions.len() > 1 {
            let names: Vec<&str> = actions.iter().map(|a| a.as_str()).collect();
            let _ = writeln!(
                warn,
                "keybindings: key {:?} bound to multiple actions ({}) in {} — rejecting whole file",
                key,
                names.join(", "),
                display_path(path)
            );
            conflicts = true;
        }
    }
    if conflicts {
        return (KeyMap::new_default(), false);
    }
    // 应用覆盖
    for (a, k) in &accepted {
        km.by_action.insert(*a, k.clone());
        km.source.insert(*a, "user");
    }
    // D4-14 硬化：从 by_action 重建 by_key（上游 delete-oldKey 在交换绑定时丢反查条目）。
    // 单重绑行为不变；交换正确。arbitrate 有意偏差，验收汇总登记。
    km.by_key.clear();
    for (a, k) in &km.by_action {
        km.by_key.insert(k.clone(), *a);
    }
    (km, true)
}

/// 干跑校验（`arbitrate keys check` / CI）。返回完整警告行（空 = 有效）。
pub fn check(path: &Path) -> Vec<String> {
    let mut buf: Vec<u8> = Vec::new();
    let (_, ok) = load(path, &mut buf);
    let text = String::from_utf8_lossy(&buf);
    let trimmed = text.trim();
    let out: Vec<String> = if trimmed.is_empty() {
        Vec::new()
    } else {
        trimmed.split('\n').map(|s| s.to_string()).collect()
    };
    if !ok && out.is_empty() {
        return vec![format!(
            "keybindings: {} rejected for unknown reason",
            display_path(&path.to_string_lossy())
        )];
    }
    out
}
