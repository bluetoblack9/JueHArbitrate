use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

#[test]
fn manifest_covers_all_fixtures_with_correct_digests() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let m: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("manifest.json")).unwrap())
            .unwrap();
    let entries = m["fixtures"].as_array().unwrap();
    let mut files = Vec::new();
    walk(&root, &mut files);
    let listed: Vec<&str> = entries
        .iter()
        .map(|e| e["path"].as_str().unwrap())
        .collect();
    for f in &files {
        let rel = f
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if rel == "manifest.json" {
            continue;
        }
        assert!(listed.contains(&rel.as_str()), "fixture 未登记: {rel}");
    }
    for e in entries {
        let rel = e["path"].as_str().unwrap();
        let bytes = std::fs::read(root.join(rel)).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            e["sha256"].as_str().unwrap(),
            "摘要不符: {rel}"
        );
        assert!(!e["spec_source"].as_str().unwrap().is_empty());
        assert!(!e["schema_version"].as_str().unwrap().is_empty());
    }
}

#[test]
#[ignore = "仅在规格变更后显式生成 fixture manifest；普通测试和 CI 不写文件"]
fn regenerate_fixture_manifest() {
    const META: &[(&str, &str, &str)] = &[
        (
            "mcp-script.json",
            "M0.2 mock MCP；上游协议只读参考",
            "mcp-mock/1",
        ),
        (
            "cli-contract.json",
            "main.go@cb27b90 顶层 flag 与分发静态整理",
            "cli-contract/1",
        ),
        (
            "cli-subcommands.json",
            "锁定 commit 各 CLI 解析器；docs/cli-source-audit.md",
            "cli-subcommands/1",
        ),
        (
            "events/plain-text.jsonl",
            "events.go + 架构 §1 纯文本场景",
            "event-fixture/1",
        ),
        (
            "events/tool-call.jsonl",
            "events.go + 架构 §4 工具续轮场景",
            "event-fixture/1",
        ),
        (
            "events/permission-denied.jsonl",
            "events.go + 总纲约束 6 拒绝即结果",
            "event-fixture/1",
        ),
        (
            "events/plan.jsonl",
            "events.go Plan* 事件 + feature-plan-mode PRD",
            "event-fixture/1",
        ),
    ];
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut files = Vec::new();
    walk(&root, &mut files);
    let actual: std::collections::BTreeSet<String> = files
        .iter()
        .map(|p| {
            p.strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .filter(|p| p != "manifest.json")
        .collect();
    let expected: std::collections::BTreeSet<String> =
        META.iter().map(|(path, _, _)| path.to_string()).collect();
    assert_eq!(
        actual, expected,
        "先静态核对并登记新增/移除 fixture 的规格与 schema，再生成摘要"
    );
    let mut entries: Vec<serde_json::Value> = META
        .iter()
        .map(|(path, source, schema)| {
            let bytes = std::fs::read(root.join(path)).unwrap();
            serde_json::json!({
                "path": path,
                "sha256": format!("{:x}", Sha256::digest(&bytes)),
                "spec_source": source,
                "schema_version": schema
            })
        })
        .collect();
    entries.sort_by_key(|e| e["path"].as_str().unwrap().to_string());
    let mut bytes = serde_json::to_vec_pretty(&serde_json::json!({"fixtures": entries})).unwrap();
    bytes.push(b'\n');
    std::fs::write(root.join("manifest.json"), bytes).unwrap();
}
