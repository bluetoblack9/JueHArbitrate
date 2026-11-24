//! core::imgrefs 契约测试 — 扩展名表 + stat 门 + cleanToken（规格：上游
//! internal/imgrefs，Rust 自有重写）。真实临时文件，不触真实用户数据。

use arbitrate_core::imgrefs::detect;

mod common;
#[path = "common/reference_fixture.rs"]
mod reference_fixture;
use reference_fixture::reference_test;

fn write_img(name: &str) -> String {
    std::fs::write(name, b"not-really-an-image").unwrap();
    name.to_owned()
}

reference_test!(stat_gate_no_false_positive, {
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
});

reference_test!(real_file, {
    let img = write_img("shot.png");
    assert_eq!(detect(&format!("look at {img} please")), vec![img]);
});

reference_test!(at_prefix_and_wrappers, {
    let img = write_img("shot.png");
    // 嵌套包裹 + 尾标点 + @ 前缀，循环剥至稳定
    assert_eq!(detect(&format!("see (@{img}),")), vec![img.clone()]);
    assert_eq!(detect(&format!("`{img}`")), vec![img.clone()]);
    assert_eq!(detect(&format!("[{img}]!")), vec![img.clone()]);
    let prefix = "@".repeat(64 * 1024);
    assert_eq!(detect(&format!("({prefix}{img}),")), vec![img]);
});

reference_test!(dedup_order, {
    let a = write_img("a.png");
    let b = write_img("b.jpg");
    let text = format!("{b} then {a} then {b}");
    assert_eq!(detect(&text), vec![b, a]); // 首次出现序，去重
});

reference_test!(all_exts_case_insensitive, {
    for name in [
        "a.png", "b.jpg", "c.jpeg", "d.webp", "e.tiff", "f.bmp", "g.heic", "h.gif", "I.PNG",
    ] {
        let img = write_img(name);
        assert_eq!(detect(&format!("read {img}")), vec![img], "ext {name}");
    }
});

reference_test!(directory_with_img_ext_not_ref, {
    std::fs::create_dir("fake.png").unwrap();
    assert!(detect("open fake.png").is_empty());
});

reference_test!(non_image_ext_not_ref, {
    let txt = write_img("note.txt");
    assert!(detect(&format!("read {txt}")).is_empty());
});

reference_test!(dotfile_ext, {
    // Go filepath.Ext(".png")=".png"；Rust Path::extension()=None → go_ext 钉死
    let img = write_img(".png");
    assert_eq!(detect(&format!("open {img}")), vec![img]);
});
