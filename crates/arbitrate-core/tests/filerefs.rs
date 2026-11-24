//! core::filerefs 契约测试 — 文档扩展名表 + rich 前缀 + stat 门 + 首现去重
//! （规格：上游 internal/filerefs，Rust 自有重写）。literal-rich 用 stat 注入缝
//! 替代上游 Chdir（并行安全，D4-7）。

use arbitrate_core::filerefs::{Ref, detect, detect_refs, detect_refs_with};

mod common;
#[path = "common/reference_fixture.rs"]
mod reference_fixture;
use reference_fixture::reference_test;

fn write_doc(name: &str) -> String {
    std::fs::write(name, b"not-really-a-document").unwrap();
    name.to_owned()
}

reference_test!(stat_gate_no_false_positive, {
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
});

reference_test!(real_doc, {
    let doc = write_doc("report.pdf");
    assert_eq!(detect(&format!("summarize {doc} please")), vec![doc]);
});

reference_test!(at_prefix_and_punctuation, {
    let doc = write_doc("contract.docx");
    assert_eq!(detect(&format!("look at (@{doc}),")), vec![doc.clone()]);
    let prefix = "@".repeat(100_000);
    assert_eq!(detect(&format!("look at ({prefix}{doc}),")), vec![doc]);
});

reference_test!(dedup, {
    let doc = write_doc("a.pdf");
    assert_eq!(detect(&format!("{doc} and again {doc}")), vec![doc]);
});

reference_test!(all_exts_case_insensitive, {
    for name in ["a.pdf", "b.doc", "c.docx", "d.ppt", "e.pptx", "F.PDF"] {
        let doc = write_doc(name);
        assert_eq!(detect(&format!("read {doc}")), vec![doc], "ext {name}");
    }
});

reference_test!(directory_with_doc_ext_not_ref, {
    std::fs::create_dir("fake.pdf").unwrap();
    assert!(detect("open fake.pdf").is_empty());
});

reference_test!(image_ext_not_doc_ref, {
    let img = write_doc("shot.png");
    assert!(detect(&format!("look at {img}")).is_empty()); // 家族分离
});

reference_test!(rich_prefix, {
    let doc = write_doc("report.pdf");
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
    assert!(detect_refs("read @rich:missing.pdf").is_empty());
});

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

reference_test!(first_occurrence_wins, {
    let doc = write_doc("report.pdf");
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
});

reference_test!(detect_is_projection_of_detect_refs, {
    let a = write_doc("a.pdf");
    let b = write_doc("b.docx");
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
});
