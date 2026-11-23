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

const ORIGINAL_SKILLS: &[(&str, &str)] = &[
    (
        "code-review",
        "0865f3bb258cb32fe901b7d93cedb7f34b879763b86ed2fa38fc625b8c664c61",
    ),
    (
        "dual-model",
        "5a1df1b8efbfd0631b32f878fe2177f0893c68f5e57b61eee94adc49f448349e",
    ),
    (
        "go-test-runner",
        "046031aceea0f961d4330b0a9c155043775b7dafac701f8d8d43722359bcdea3",
    ),
    (
        "plan-mode",
        "6a0a7f5f79c027405b6cb9c7a784bd320833e7e57db37316f6f84807ec961c80",
    ),
];

#[test]
fn original_skills_and_manifest_remain_pinned() {
    let m: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string("assets/manifest.json").unwrap()).unwrap();
    assert_eq!(
        m["upstream_commit"],
        "cb27b90d2979f5f840526100b28ade586d034df9"
    );
    let entries = m["entries"].as_array().unwrap();
    let mut expected: std::collections::BTreeSet<String> = [
        "internal/controlplane/webfrontend.html",
        "LICENSE",
        "NOTICE",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    expected.extend(
        ORIGINAL_SKILLS
            .iter()
            .map(|(name, _)| format!("internal/skill/builtin/{name}.md")),
    );
    let actual: std::collections::BTreeSet<String> = entries
        .iter()
        .map(|e| e["upstream_path"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(entries.len(), expected.len(), "禁止缺项或重复条目");
    assert_eq!(actual, expected);
    for (name, digest) in ORIGINAL_SKILLS {
        let original = format!("reference/upstream/builtin-skills-original/{name}.md");
        assert_eq!(sha256_file(&original), *digest, "归档原件漂移: {name}");
        let source = format!("internal/skill/builtin/{name}.md");
        let entry = entries
            .iter()
            .find(|e| e["upstream_path"] == source)
            .unwrap();
        assert_eq!(entry["original_sha256"].as_str().unwrap(), *digest);
    }
}

#[test]
fn original_digest_detects_modified_bytes() {
    let (name, digest) = ORIGINAL_SKILLS[0];
    let mut bytes = std::fs::read(format!(
        "reference/upstream/builtin-skills-original/{name}.md"
    ))
    .unwrap();
    assert_eq!(format!("{:x}", Sha256::digest(&bytes)), digest);
    bytes.push(b'\n');
    assert_ne!(format!("{:x}", Sha256::digest(&bytes)), digest);
}
