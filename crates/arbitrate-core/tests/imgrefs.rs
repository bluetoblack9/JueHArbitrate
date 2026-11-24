//! core::imgrefs 契约测试 — 扩展名表 + stat 门 + cleanToken（规格：上游
//! internal/imgrefs，Rust 自有重写）。真实临时文件，不触真实用户数据。

use arbitrate_core::imgrefs::detect;

mod common;
use common::TempRoot;

fn write_img(root: &TempRoot, name: &str) -> String {
    let p = root.0.join(name);
    std::fs::write(&p, b"not-really-an-image").unwrap();
    p.to_string_lossy().into_owned()
}

#[test]
fn stat_gate_no_false_positive() {
    // 不存在的 .png 子串（URL/代码/错误消息）不得触发
    for text in [
        "see https://example.com/shot.png for details",
        r#"if strings.HasSuffix(name, ".png") { parse(name) }"#,
        "the image is at docs/shot.png and ~/pic.jpeg",
    ] {
        assert!(
            detect(text).is_empty(),
            "non-existent must not detect: {text}"
        );
    }
}

#[test]
fn real_file() {
    let root = TempRoot::new();
    let img = write_img(&root, "shot.png");
    assert_eq!(detect(&format!("look at {img} please")), vec![img]);
}

#[test]
fn at_prefix_and_wrappers() {
    let root = TempRoot::new();
    let img = write_img(&root, "shot.png");
    // 嵌套包裹 + 尾标点 + @ 前缀，循环剥至稳定
    assert_eq!(detect(&format!("see (@{img}),")), vec![img.clone()]);
    assert_eq!(detect(&format!("`{img}`")), vec![img.clone()]);
    assert_eq!(detect(&format!("[{img}]!")), vec![img.clone()]);
    let prefix = "@".repeat(64 * 1024);
    assert_eq!(detect(&format!("({prefix}{img}),")), vec![img]);
}

#[test]
fn dedup_order() {
    let root = TempRoot::new();
    let a = write_img(&root, "a.png");
    let b = write_img(&root, "b.jpg");
    let text = format!("{b} then {a} then {b}");
    assert_eq!(detect(&text), vec![b, a]); // 首次出现序，去重
}

#[test]
fn all_exts_case_insensitive() {
    let root = TempRoot::new();
    for name in [
        "a.png", "b.jpg", "c.jpeg", "d.webp", "e.tiff", "f.bmp", "g.heic", "h.gif", "I.PNG",
    ] {
        let img = write_img(&root, name);
        assert_eq!(detect(&format!("read {img}")), vec![img], "ext {name}");
    }
}

#[test]
fn directory_with_img_ext_not_ref() {
    let root = TempRoot::new();
    let dir = root.0.join("fake.png");
    std::fs::create_dir(&dir).unwrap();
    assert!(detect(&format!("open {}", dir.to_string_lossy())).is_empty());
}

#[test]
fn non_image_ext_not_ref() {
    let root = TempRoot::new();
    let txt = write_img(&root, "note.txt");
    assert!(detect(&format!("read {txt}")).is_empty());
}

#[test]
fn dotfile_ext() {
    // Go filepath.Ext(".png")=".png"；Rust Path::extension()=None → go_ext 钉死
    let root = TempRoot::new();
    let img = write_img(&root, ".png");
    assert_eq!(detect(&format!("open {img}")), vec![img]);
}
