//! core::projectmd 契约测试 — 向上 5 级发现 + 64KB UTF-8 安全截断 + Section
//! （规格：上游 internal/projectmd，Rust 自有重写）。边界 KAT 手算。

use arbitrate_core::projectmd::{FILENAME, Result, load};

mod common;
use common::TempRoot;

fn write_agents(dir: &std::path::Path, content: &str) -> String {
    let p = dir.join(FILENAME);
    std::fs::write(&p, content).unwrap();
    p.to_string_lossy().into_owned()
}

#[test]
fn parent_walk() {
    let root = TempRoot::new();
    let agents = write_agents(&root.0, "instructions");
    let sub = root.0.join("sub");
    std::fs::create_dir(&sub).unwrap();
    let r = load(&sub.to_string_lossy()).unwrap();
    assert_eq!(r.path, agents);
    assert_eq!(r.content, "instructions");
    assert!(!r.truncate);
}

#[test]
fn nearest_wins() {
    let root = TempRoot::new();
    write_agents(&root.0, "root");
    let a = root.0.join("a");
    std::fs::create_dir(&a).unwrap();
    let near = write_agents(&a, "a");
    let b = a.join("b");
    std::fs::create_dir(&b).unwrap();
    let r = load(&b.to_string_lossy()).unwrap();
    assert_eq!(r.path, near);
    assert_eq!(r.content, "a");
}

#[test]
fn none_found() {
    let root = TempRoot::new();
    // 7 级深起点：5 次候选全在 TempRoot 内（depth 7..3），绝不逃逸宿主目录（隔离）
    let deep = root.0.join("a/b/c/d/e/f/g");
    std::fs::create_dir_all(&deep).unwrap();
    let r = load(&deep.to_string_lossy()).unwrap();
    assert_eq!(r.path, "");
    assert_eq!(r.content, "");
    assert_eq!(r.bytes, 0);
}

#[test]
fn ascend_bounded() {
    let root = TempRoot::new();
    write_agents(&root.0, "root");
    // 7 级深：5 次迭代只到 depth3，root（depth0）不可达
    let deep = root.0.join("a/b/c/d/e/f/g");
    std::fs::create_dir_all(&deep).unwrap();
    let r = load(&deep.to_string_lossy()).unwrap();
    assert_eq!(r.path, "", "maxAscend=5 不得扫到 root");
}

#[test]
fn read_error_surfaced() {
    let root = TempRoot::new();
    // AGENTS.md 是目录 → 读错误（非 NotFound）→ surface
    std::fs::create_dir(root.0.join(FILENAME)).unwrap();
    let err = load(&root.0.to_string_lossy()).unwrap_err();
    assert!(err.to_string().contains("projectmd:"), "got {err}");
}

#[test]
fn notdir_treated_as_missing() {
    let root = TempRoot::new();
    let agents = write_agents(&root.0, "root");
    // blocker 是文件：blocker/sub/AGENTS.md 读 → ENOTDIR → 视为 not-exist 向上
    std::fs::write(root.0.join("blocker"), b"x").unwrap();
    let start = root.0.join("blocker/sub");
    let r = load(&start.to_string_lossy()).unwrap();
    assert_eq!(r.path, agents);
}

#[test]
fn truncate_oversized() {
    let root = TempRoot::new();
    let data = "x".repeat(70000);
    write_agents(&root.0, &data);
    let r = load(&root.0.to_string_lossy()).unwrap();
    assert!(r.truncate);
    assert_eq!(r.bytes, 70000);
    let want = format!(
        "{}\n\n…[truncated 4464 bytes; AGENTS.md is too large — keep it under 64 KB]\n",
        "x".repeat(65536)
    );
    assert_eq!(r.content, want);
}

#[test]
fn emoji_boundary() {
    let root = TempRoot::new();
    // 65533 a + 4-byte 🚀 = 65537；cut 65536 劈 emoji → drop 3 → 65533 a
    let data = format!("{}🚀", "a".repeat(65533));
    write_agents(&root.0, &data);
    let r = load(&root.0.to_string_lossy()).unwrap();
    assert!(r.truncate);
    assert_eq!(r.bytes, 65537);
    let want = format!(
        "{}\n\n…[truncated 4 bytes; AGENTS.md is too large — keep it under 64 KB]\n",
        "a".repeat(65533)
    );
    assert_eq!(r.content, want);
}

#[test]
fn cjk_boundary() {
    let root = TempRoot::new();
    // 65534 a + 3-byte 界 = 65537；cut 劈 → drop 2 → 65534 a
    let data = format!("{}界", "a".repeat(65534));
    write_agents(&root.0, &data);
    let r = load(&root.0.to_string_lossy()).unwrap();
    assert!(r.truncate);
    let want = format!(
        "{}\n\n…[truncated 3 bytes; AGENTS.md is too large — keep it under 64 KB]\n",
        "a".repeat(65534)
    );
    assert_eq!(r.content, want);
}

#[test]
fn exact_boundary_no_truncate() {
    let root = TempRoot::new();
    let data = "x".repeat(65536);
    write_agents(&root.0, &data);
    let r = load(&root.0.to_string_lossy()).unwrap();
    assert!(!r.truncate);
    assert_eq!(r.bytes, 65536);
    assert_eq!(r.content, data);
}

#[test]
fn section_empty_path() {
    let r = Result::default();
    assert_eq!(r.section(), "");
}

#[test]
fn section_label() {
    let r = Result {
        path: "/p/AGENTS.md".into(),
        content: "hello".into(),
        ..Default::default()
    };
    assert_eq!(
        r.section(),
        "# Project instructions (from /p/AGENTS.md)\n\nhello\n"
    );
}
