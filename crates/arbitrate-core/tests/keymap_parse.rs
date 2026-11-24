//! core::keymap parse 层契约测试（规格：上游 internal/keymap parse.go，Rust 自有重写）。
//! 手写 TOML 子集（禁 BurntSushi，D4-13）；警告形状逐字 + 品牌适配（D4-4/D4-14）。
//! swap 硬化为 arbitrate 有意偏差（D4-14）。虚构键值，不触真实 ~/.arbitrate。

use arbitrate_core::keymap::{Action, KeyMap, check, load, load_bytes, parse_toml_bindings};

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
    assert_eq!(
        w,
        "keybindings: toml syntax error in f.toml: line 1: unterminated section header (using defaults)\n"
    );
    assert_eq!(km.resolve("enter"), Action::Submit); // 全回退默认

    // D4-13 excludes dotted-key syntax, even when the value is a valid string.
    let (km, ok, w) = warn_string(b"[bindings]\nsubmit.foo = \"ctrl+x\"\n", "f.toml");
    assert!(!ok);
    assert_eq!(
        w,
        "keybindings: toml syntax error in f.toml: line 2: expected '=' after key (using defaults)\n"
    );
    assert_eq!(km.resolve("enter"), Action::Submit);

    for quote in ['"', '\''] {
        for control in ['\0', '\u{007f}'] {
            let input = format!("[bindings]\nsubmit = {quote}ctrl+{control}x{quote}\n");
            let (km, ok, w) = warn_string(input.as_bytes(), "f.toml");
            assert!(!ok, "accepted forbidden raw control: {input:?}");
            let style = if quote == '"' { "basic" } else { "literal" };
            assert_eq!(
                w,
                format!(
                    "keybindings: toml syntax error in f.toml: line 2: forbidden raw control in {style} string (using defaults)\n"
                )
            );
            assert_eq!(km.key_for(Action::Submit), "enter");
            assert_eq!(km.resolve("enter"), Action::Submit);
            assert_eq!(km.resolve(&format!("ctrl+{control}x")), Action::None);
        }
    }

    // Raw tabs and Unicode remain valid; controls decoded from escapes are allowed.
    let parsed = parse_toml_bindings(
        "[bindings]\nsubmit = \"a\t界\\b\\f\\n\\r\\t\\\"\\\\\\u0000\\U0000007F\\u754C\\U0001F600\"\nsteer = 'a\t界'\n",
    )
    .unwrap();
    assert_eq!(
        parsed["submit"],
        "a\t界\u{0008}\u{000c}\n\r\t\"\\\0\u{007f}界😀"
    );
    assert_eq!(parsed["steer"], "a\t界");
}

#[test]
fn unknown_action_soft() {
    let (km, ok, w) = warn_string(b"[bindings]\nsubmmit = \"ctrl+x\"\n", "f.toml");
    assert!(ok); // 软错误：丢该行，其余仍应用
    assert_eq!(
        w,
        "keybindings: unknown action \"submmit\" in f.toml (available: submit, steer, interrupt, cancel, cycle-mode, clear-screen, toggle-reasoning, toggle-help, history-prev, history-next)\n"
    );
    assert_eq!(km.resolve("ctrl+x"), Action::None); // 未应用
}

#[test]
fn empty_value_soft() {
    let (km, ok, w) = warn_string(b"[bindings]\nsubmit = \"\"\n", "f.toml");
    assert!(ok);
    assert_eq!(
        w,
        "keybindings: action \"submit\" has empty value in f.toml\n"
    );
    assert_eq!(km.key_for(Action::Submit), "enter"); // 仍默认
}

#[test]
fn reserved_key_soft() {
    let (km, ok, w) = warn_string(b"[bindings]\nclear-screen = \"tab\"\n", "f.toml");
    assert!(ok);
    assert_eq!(
        w,
        "keybindings: reserved key \"tab\" cannot be rebound (clear-screen = \"tab\" in f.toml)\n"
    );
    assert_eq!(km.key_for(Action::ClearScreen), "ctrl+l");
}

#[test]
fn unrecognised_key_soft() {
    let (km, ok, w) = warn_string(
        b"[bindings]\nsubmit = \"weird key with spaces\"\n",
        "f.toml",
    );
    assert!(ok);
    assert_eq!(
        w,
        "keybindings: unrecognised key \"weird key with spaces\" for action \"submit\" in f.toml (see `arbitrate keys actions` for examples)\n"
    );
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
    assert_eq!(
        w,
        "keybindings: key \"ctrl+x\" bound to multiple actions (submit, steer) in f.toml — rejecting whole file\n"
    );
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
    assert_eq!(
        lines,
        vec![format!(
            "keybindings: key \"ctrl+x\" bound to multiple actions (submit, steer) in {} — rejecting whole file",
            p.display()
        )]
    );
}

#[test]
fn inline_display_path() {
    let (_, _, w) = warn_string(b"[bindings]\nsubmmit = \"ctrl+x\"\n", "");
    assert_eq!(
        w,
        "keybindings: unknown action \"submmit\" in <inline> (available: submit, steer, interrupt, cancel, cycle-mode, clear-screen, toggle-reasoning, toggle-help, history-prev, history-next)\n"
    );
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
    assert_eq!(
        w,
        "keybindings: toml syntax error in f.toml: line 3: duplicate key submit (using defaults)\n"
    );
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
    assert_eq!(
        w,
        concat!(
            "keybindings: action \"cancel\" has empty value in f.toml\n",
            "keybindings: unrecognised key \"bad key\" for action \"steer\" in f.toml (see `arbitrate keys actions` for examples)\n",
            "keybindings: reserved key \"tab\" cannot be rebound (submit = \"tab\" in f.toml)\n",
        )
    );
    assert_eq!(km.key_for(Action::Submit), "enter"); // 全软丢，默认保留
}

#[test]
fn rebind_onto_default_conflict() {
    // D4-14 硬化（评审 P2-4）：单项重绑撞另一动作的未改默认键 = 真冲突。
    // submit→alt+enter 与 steer 默认 alt+enter 同键；查终表（默认+覆盖）拒全文件。
    let (km, ok, w) = warn_string(b"[bindings]\nsubmit = \"alt+enter\"\n", "f.toml");
    assert!(!ok);
    assert_eq!(
        w,
        "keybindings: key \"alt+enter\" bound to multiple actions (submit, steer) in f.toml — rejecting whole file\n"
    );
    assert_eq!(km.key_for(Action::Submit), "enter"); // 全回退默认
    assert_eq!(km.resolve("alt+enter"), Action::Steer); // 默认反查未被覆盖
}

#[test]
fn illegal_multibyte_escape() {
    // 评审 P2-5：\u 后非 ASCII hex（界界）→ 字节级先验拒绝，不 panic（UTF-8 中间切片）。
    let (km, ok, w) = warn_string("[bindings]\nsubmit = \"\\u界界\"\n".as_bytes(), "f.toml");
    assert!(!ok);
    assert_eq!(
        w,
        "keybindings: toml syntax error in f.toml: line 2: bad unicode escape (using defaults)\n"
    );
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
