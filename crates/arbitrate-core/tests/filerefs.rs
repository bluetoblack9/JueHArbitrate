//! core::filerefs 契约测试 — 文档扩展名表 + rich 前缀 + stat 门 + 首现去重
//! （规格：上游 internal/filerefs，Rust 自有重写）。literal-rich 用 stat 注入缝
//! 替代上游 Chdir（并行安全，D4-7）。

use arbitrate_core::filerefs::{Ref, detect, detect_refs, detect_refs_with};

mod common;
use common::TempRoot;

fn write_doc(root: &TempRoot, name: &str) -> String {
    let p = root.0.join(name);
    std::fs::write(&p, b"not-really-a-document").unwrap();
    p.to_string_lossy().into_owned()
}

#[test]
fn stat_gate_no_false_positive() {
    for text in [
        "see https://example.com/report.pdf for details",
        r#"if strings.HasSuffix(name, ".pdf") { parse(name) }"#,
        "the manual is at docs/manual.docx and slides at ~/deck.pptx",
    ] {
        assert!(
            detect(text).is_empty(),
            "non-existent must not detect: {text}"
        );
    }
}

#[test]
fn real_doc() {
    let root = TempRoot::new();
    let doc = write_doc(&root, "report.pdf");
    assert_eq!(detect(&format!("summarize {doc} please")), vec![doc]);
}

#[test]
fn at_prefix_and_punctuation() {
    let root = TempRoot::new();
    let doc = write_doc(&root, "contract.docx");
    assert_eq!(detect(&format!("look at (@{doc}),")), vec![doc.clone()]);
    let prefix = "@".repeat(100_000);
    assert_eq!(detect(&format!("look at ({prefix}{doc}),")), vec![doc]);
}

#[test]
fn dedup() {
    let root = TempRoot::new();
    let doc = write_doc(&root, "a.pdf");
    assert_eq!(detect(&format!("{doc} and again {doc}")), vec![doc]);
}

#[test]
fn all_exts_case_insensitive() {
    let root = TempRoot::new();
    for name in ["a.pdf", "b.doc", "c.docx", "d.ppt", "e.pptx", "F.PDF"] {
        let doc = write_doc(&root, name);
        assert_eq!(detect(&format!("read {doc}")), vec![doc], "ext {name}");
    }
}

#[test]
fn directory_with_doc_ext_not_ref() {
    let root = TempRoot::new();
    let dir = root.0.join("fake.pdf");
    std::fs::create_dir(&dir).unwrap();
    assert!(detect(&format!("open {}", dir.to_string_lossy())).is_empty());
}

#[test]
fn image_ext_not_doc_ref() {
    let root = TempRoot::new();
    let img = write_doc(&root, "shot.png");
    assert!(detect(&format!("look at {img}")).is_empty()); // 家族分离
}

#[test]
fn rich_prefix() {
    let root = TempRoot::new();
    let doc = write_doc(&root, "report.pdf");
    // plain → rich=false
    assert_eq!(
        detect_refs(&format!("read {doc}")),
        vec![Ref {
            path: doc.clone(),
            rich: false
        }]
    );
    // @rich: → 同 path，rich=true
    assert_eq!(
        detect_refs(&format!("read @rich:{doc}")),
        vec![Ref {
            path: doc.clone(),
            rich: true
        }]
    );
    // 包裹 + 标点 rich 仍解析
    assert_eq!(
        detect_refs(&format!("see (@rich:{doc}),")),
        vec![Ref {
            path: doc.clone(),
            rich: true
        }]
    );
    // @rich: 不存在路径 → stat 门拒绝
    assert!(detect_refs("read @rich:/nope/missing.pdf").is_empty());
}

#[test]
fn rich_literal_name_fallback() {
    // 文件字面名 "rich:x.pdf"：前缀仅当余下 stat 为文档时绑定；
    // 这里 "x.pdf" 不存在 → 整 token verbatim 再试 → 命中。
    // 用注入缝（仅 "rich:x.pdf" 存在），无 chdir、无真实文件（D4-7）。
    let refs = detect_refs_with("open rich:x.pdf", |p| p == "rich:x.pdf");
    assert_eq!(
        refs,
        vec![Ref {
            path: "rich:x.pdf".to_string(),
            rich: false
        }]
    );
    // 对照：若 "x.pdf" 也存在，前缀绑定 → path="x.pdf" rich=true
    let refs2 = detect_refs_with("open rich:x.pdf", |p| p == "rich:x.pdf" || p == "x.pdf");
    assert_eq!(
        refs2,
        vec![Ref {
            path: "x.pdf".to_string(),
            rich: true
        }]
    );
}

#[test]
fn first_occurrence_wins() {
    let root = TempRoot::new();
    let doc = write_doc(&root, "report.pdf");
    // plain-first → rich=false
    assert_eq!(
        detect_refs(&format!("{doc} and @rich:{doc}")),
        vec![Ref {
            path: doc.clone(),
            rich: false
        }]
    );
    // rich-first → rich=true
    assert_eq!(
        detect_refs(&format!("@rich:{doc} and {doc}")),
        vec![Ref {
            path: doc,
            rich: true
        }]
    );
}

#[test]
fn detect_is_projection_of_detect_refs() {
    let root = TempRoot::new();
    let a = write_doc(&root, "a.pdf");
    let b = write_doc(&root, "b.docx");
    let text = format!("first {a} then @rich:{b} then {a}");
    let refs = detect_refs(&text);
    let paths = detect(&text);
    assert_eq!(refs.len(), 2);
    assert_eq!(paths.len(), 2);
    for (r, p) in refs.iter().zip(paths.iter()) {
        assert_eq!(&r.path, p);
    }
    assert!(!refs[0].rich);
    assert!(refs[1].rich);
}
