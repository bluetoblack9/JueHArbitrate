use sha2::{Digest, Sha256};

const FROZEN: &[(&str, &str)] = &[
    (
        "assets/webfrontend.html",
        "b432319014f380d18a0634d3ee95811d2dc878263da7d14e7bed1cf56295e61f",
    ),
    (
        "assets/LICENSE",
        "a8744160149f0d152a9bbc5e811fa8504be6cfe0a251e3a96079cad633073e7c",
    ),
    (
        "assets/NOTICE",
        "71eb67461263e784336170495f80cf99b2fc00ea6f682b69107b85da264c28e4",
    ),
];

fn sha256_file(path: &str) -> String {
    format!("{:x}", Sha256::digest(std::fs::read(path).unwrap()))
}

#[test]
fn frozen_assets_byte_identical() {
    for (path, want) in FROZEN {
        assert_eq!(&sha256_file(path), want, "冻结资产被改动: {path}");
    }
}

#[test]
fn manifest_matches_files() {
    let m: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string("assets/manifest.json").unwrap()).unwrap();
    assert_eq!(
        m["upstream_commit"],
        "cb27b90d2979f5f840526100b28ade586d034df9"
    );
    for e in m["entries"].as_array().unwrap() {
        let active = e["active_path"].as_str().unwrap();
        assert_eq!(
            &sha256_file(active),
            e["active_sha256"].as_str().unwrap(),
            "manifest 摘要不符: {active}"
        );
        let adaptation = e["adaptation"].as_str().unwrap();
        assert!(adaptation == "none" || adaptation == "minimal-brand");
        if adaptation == "none" {
            assert_eq!(
                e["original_sha256"], e["active_sha256"],
                "none 适配必须字节一致: {active}"
            );
        } else {
            assert!(
                !e["reason"].as_str().unwrap().is_empty(),
                "适配必须给原因: {active}"
            );
        }
    }
}
