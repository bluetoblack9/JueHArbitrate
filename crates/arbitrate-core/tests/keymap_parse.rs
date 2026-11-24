//! core::keymap parse 层契约测试（规格：上游 internal/keymap parse.go，Rust 自有重写）。
//! 手写 TOML 子集（禁 BurntSushi，D4-13）；警告形状逐字 + 品牌适配（D4-4/D4-14）。
//! swap 硬化为 arbitrate 有意偏差（D4-14）。虚构键值，不触真实 ~/.arbitrate。

use arbitrate_core::keymap::{Action, KeyMap, check, load, load_bytes};

mod common;
use common::TempRoot;

fn warn_string(data: &[u8], path: &str) -> (KeyMap, bool, String) {
    let mut buf: Vec<u8> = Vec::new();
    let (km, ok) = load_bytes(data, path, &mut buf);
    (km, ok, String::from_utf8(buf).unwrap())
}

#[test]
fn missing_file_defaults() {
    let root = TempRoot::new();
    let p = root.0.join("nope.toml");
    let mut buf: Vec<u8> = Vec::new();
    let (km, ok) = load(&p, &mut buf);
    assert!(ok); // 无文件 → 静默默认
    assert!(buf.is_empty());
    assert_eq!(km.resolve("enter"), Action::Submit);
}

#[test]
fn syntax_error_rejects() {
    let (km, ok, w) = warn_string(b"[bindings\nsubmit = \"x\"", "f.toml");
    assert!(!ok);
    assert!(w.contains("toml syntax error"), "{w}");
    assert!(w.contains("(using defaults)"));
    assert_eq!(km.resolve("enter"), Action::Submit); // 全回退默认

    // D4-13 excludes dotted-key syntax, even when the value is a valid string.
    let (km, ok, w) = warn_string(b"[bindings]\nsubmit.foo = \"ctrl+x\"\n", "f.toml");
    assert!(!ok);
    assert!(w.contains("toml syntax error"), "{w}");
    assert_eq!(km.resolve("enter"), Action::Submit);
}

#[test]
fn unknown_action_soft() {
    let (km, ok, w) = warn_string(b"[bindings]\nsubmmit = \"ctrl+x\"\n", "f.toml");
    assert!(ok); // 软错误：丢该行，其余仍应用
    assert!(w.contains("unknown action \"submmit\""), "{w}");
    assert!(w.contains("available: submit, steer"));
    assert_eq!(km.resolve("ctrl+x"), Action::None); // 未应用
}

#[test]
fn empty_value_soft() {
    let (km, ok, w) = warn_string(b"[bindings]\nsubmit = \"\"\n", "f.toml");
    assert!(ok);
    assert!(w.contains("action \"submit\" has empty value"), "{w}");
    assert_eq!(km.key_for(Action::Submit), "enter"); // 仍默认
}

#[test]
fn reserved_key_soft() {
    let (km, ok, w) = warn_string(b"[bindings]\nclear-screen = \"tab\"\n", "f.toml");
    assert!(ok);
    assert!(w.contains("reserved key \"tab\" cannot be rebound"), "{w}");
    assert!(w.contains("clear-screen = \"tab\""));
    assert_eq!(km.key_for(Action::ClearScreen), "ctrl+l");
}

#[test]
fn unrecognised_key_soft() {
    let (km, ok, w) = warn_string(
        b"[bindings]\nsubmit = \"weird key with spaces\"\n",
        "f.toml",
    );
    assert!(ok);
    assert!(w.contains("unrecognised key"), "{w}");
    assert!(w.contains("arbitrate keys actions"), "品牌适配命令名");
    assert_eq!(km.key_for(Action::Submit), "enter");
}

#[test]
fn valid_rebind_reverse() {
    let (km, ok, w) = warn_string(b"[bindings]\nsubmit = \"ctrl+x\"\n", "f.toml");
    assert!(ok);
    assert!(w.is_empty());
    assert_eq!(km.resolve("ctrl+x"), Action::Submit);
    assert_eq!(km.resolve("enter"), Action::None); // 旧键反查移除
    assert_eq!(km.key_for(Action::Submit), "ctrl+x");
    assert_eq!(km.source_for(Action::Submit), "user");
}

#[test]
fn case_insensitive_key() {
    let (km, ok, _) = warn_string(b"[bindings]\nsubmit = \"Ctrl+X\"\n", "f.toml");
    assert!(ok);
    assert_eq!(km.resolve("ctrl+x"), Action::Submit); // normalizeKey 小写
}

#[test]
fn conflict_rejects_whole_file() {
    let (km, ok, w) = warn_string(
        b"[bindings]\nsubmit = \"ctrl+x\"\nsteer = \"ctrl+x\"\n",
        "f.toml",
    );
    assert!(!ok);
    assert!(w.contains("bound to multiple actions"), "{w}");
    assert!(w.contains("rejecting whole file"));
    assert_eq!(km.resolve("ctrl+x"), Action::None); // 全回退
    assert_eq!(km.key_for(Action::Submit), "enter");
}

#[test]
fn swap_hardening() {
    // D4-14：上游 apply 循环 delete-oldKey 在交换绑定时丢反查条目（无上游测试）。
    // arbitrate 硬化：apply 后从 by_action 重建 by_key。
    let (km, ok, w) = warn_string(
        b"[bindings]\nsubmit = \"alt+enter\"\nsteer = \"enter\"\n",
        "f.toml",
    );
    assert!(ok);
    assert!(w.is_empty());
    assert_eq!(km.resolve("enter"), Action::Steer);
    assert_eq!(km.resolve("alt+enter"), Action::Submit);
    assert_eq!(km.key_for(Action::Submit), "alt+enter");
    assert_eq!(km.key_for(Action::Steer), "enter");
}

#[test]
fn check_good_empty() {
    let root = TempRoot::new();
    let p = root.0.join("kb.toml");
    std::fs::write(&p, b"[bindings]\nsubmit = \"ctrl+x\"\n").unwrap();
    assert!(check(&p).is_empty());
}

#[test]
fn check_bad_lines() {
    let root = TempRoot::new();
    let p = root.0.join("kb.toml");
    std::fs::write(&p, b"[bindings]\nsubmit = \"ctrl+x\"\nsteer = \"ctrl+x\"\n").unwrap();
    let lines = check(&p);
    assert!(!lines.is_empty());
    assert!(lines.iter().any(|l| l.contains("rejecting whole file")));
}

#[test]
fn inline_display_path() {
    let (_, _, w) = warn_string(b"[bindings]\nsubmmit = \"ctrl+x\"\n", "");
    assert!(w.contains("<inline>"), "{w}");
}

#[test]
fn other_sections_ignored() {
    let (km, ok, w) = warn_string(
        b"[other]\nfoo = \"bar\"\n[bindings]\nsubmit = \"ctrl+x\"\n",
        "f.toml",
    );
    assert!(ok);
    assert!(w.is_empty());
    assert_eq!(km.resolve("ctrl+x"), Action::Submit);
}

#[test]
fn duplicate_key_rejects() {
    let (_, ok, w) = warn_string(b"[bindings]\nsubmit = \"a\"\nsubmit = \"b\"\n", "f.toml");
    assert!(!ok);
    assert!(w.contains("toml syntax error"), "{w}");
}

#[test]
fn quoted_keys_trim() {
    let (km, ok, _) = warn_string(b"[bindings]\n\" submit \" = \"ctrl+x\"\n", "f.toml");
    assert!(ok);
    assert_eq!(km.resolve("ctrl+x"), Action::Submit); // 键 trim 后匹配
}

#[test]
fn warning_order_sorted_actions() {
    // 软警告按动作名排序（BTreeMap）：cancel < steer < submit
    let (km, ok, w) = warn_string(
        b"[bindings]\nsubmit = \"tab\"\ncancel = \"\"\nsteer = \"bad key\"\n",
        "f.toml",
    );
    assert!(ok);
    let lines: Vec<&str> = w.lines().collect();
    assert_eq!(lines.len(), 3, "{w}");
    assert!(lines[0].contains("\"cancel\""), "{}", lines[0]);
    assert!(lines[1].contains("\"steer\""), "{}", lines[1]);
    assert!(lines[2].contains("submit = \"tab\""), "{}", lines[2]);
    assert_eq!(km.key_for(Action::Submit), "enter"); // 全软丢，默认保留
}

#[test]
fn rebind_onto_default_conflict() {
    // D4-14 硬化（评审 P2-4）：单项重绑撞另一动作的未改默认键 = 真冲突。
    // submit→alt+enter 与 steer 默认 alt+enter 同键；查终表（默认+覆盖）拒全文件。
    let (km, ok, w) = warn_string(b"[bindings]\nsubmit = \"alt+enter\"\n", "f.toml");
    assert!(!ok);
    assert!(w.contains("bound to multiple actions"), "{w}");
    assert!(w.contains("rejecting whole file"));
    assert_eq!(km.key_for(Action::Submit), "enter"); // 全回退默认
    assert_eq!(km.resolve("alt+enter"), Action::Steer); // 默认反查未被覆盖
}

#[test]
fn illegal_multibyte_escape() {
    // 评审 P2-5：\u 后非 ASCII hex（界界）→ 字节级先验拒绝，不 panic（UTF-8 中间切片）。
    let (km, ok, w) = warn_string("[bindings]\nsubmit = \"\\u界界\"\n".as_bytes(), "f.toml");
    assert!(!ok);
    assert!(w.contains("toml syntax error"), "{w}");
    assert_eq!(km.resolve("enter"), Action::Submit); // 全回退
}

#[test]
fn quoted_section() {
    // D4-13 承诺 quoted 段名（评审 P2-5）：["bindings"] 与 [bindings] 等价收集。
    let (km, ok, w) = warn_string(b"[\"bindings\"]\nsubmit = \"ctrl+x\"\n", "f.toml");
    assert!(ok);
    assert!(w.is_empty());
    assert_eq!(km.resolve("ctrl+x"), Action::Submit);
}
