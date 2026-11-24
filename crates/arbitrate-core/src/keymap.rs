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
