//! core::keymap 数据层契约测试（规格：上游 internal/keymap actions.go/keymap.go，
//! Rust 自有重写）。M0 无 bubbletea：Resolve 收 &str 规范键串（D4-12）。
//! 品牌适配：Interrupt 描述 "Quit arbitrate"（D4-4）。

use arbitrate_core::keymap::{Action, KeyMap, all_actions, is_known_action, is_reserved_key};

#[test]
fn new_default_binds_all() {
    let km = KeyMap::new_default();
    for info in all_actions() {
        assert_eq!(km.key_for(info.action), info.default_key);
        assert_eq!(km.source_for(info.action), "default");
        assert_eq!(km.resolve(info.default_key), info.action);
    }
}

#[test]
fn resolve_table() {
    let km = KeyMap::new_default();
    for (key, want) in [
        ("enter", Action::Submit),
        ("alt+enter", Action::Steer),
        ("ctrl+c", Action::Interrupt),
        ("esc", Action::Cancel),
        ("shift+tab", Action::CycleMode),
        ("ctrl+l", Action::ClearScreen),
        ("ctrl+r", Action::ToggleReasoning),
        ("?", Action::ToggleHelp),
        ("up", Action::HistoryPrev),
        ("down", Action::HistoryNext),
        ("ctrl+z", Action::None), // 未绑定
    ] {
        assert_eq!(km.resolve(key), want, "resolve({key:?})");
    }
}

#[test]
fn unbound_none() {
    let km = KeyMap::new_default();
    assert_eq!(km.resolve("ctrl+q"), Action::None);
    assert_eq!(km.resolve(""), Action::None);
}

#[test]
fn key_for_source_for_unknown() {
    let km = KeyMap::new_default();
    assert_eq!(km.key_for(Action::Submit), "enter");
    assert_eq!(km.source_for(Action::Submit), "default");
    assert_eq!(km.key_for(Action::None), ""); // 未知动作
    assert_eq!(km.source_for(Action::None), "");
}

#[test]
fn snapshot_order() {
    let km = KeyMap::new_default();
    let snap = km.snapshot();
    assert_eq!(snap.len(), 10);
    for (row, info) in snap.iter().zip(all_actions()) {
        assert_eq!(row.action, info.action);
        assert_eq!(row.key, info.default_key);
        assert_eq!(row.source, "default");
        assert_eq!(row.description, info.description);
    }
}

#[test]
fn snapshot_descriptions_brand() {
    let km = KeyMap::new_default();
    let snap = km.snapshot();
    let interrupt = snap.iter().find(|b| b.action == Action::Interrupt).unwrap();
    assert!(
        interrupt.description.contains("arbitrate"),
        "品牌适配 Quit arbitrate"
    );
    assert!(!interrupt.description.contains("seek"));
    let cycle = snap.iter().find(|b| b.action == Action::CycleMode).unwrap();
    assert!(cycle.description.contains("ask → plan → yolo"));
}

#[test]
fn reserved_keys() {
    for k in [
        "tab",
        "backspace",
        "space",
        "shift+enter",
        "pgup",
        "pgdn",
        "home",
        "end",
    ] {
        assert!(is_reserved_key(k), "{k} 应保留");
    }
    // enter/esc/up/down 可重绑（非保留）
    for k in ["enter", "esc", "up", "down"] {
        assert!(!is_reserved_key(k), "{k} 不应保留");
    }
}

#[test]
fn known_actions() {
    for info in all_actions() {
        assert!(is_known_action(info.action.as_str()));
    }
    assert!(!is_known_action("submmit")); // typo
    assert!(!is_known_action("")); // ActionNone 非已知
    assert!(!is_known_action("Submit")); // 大小写敏感
}
