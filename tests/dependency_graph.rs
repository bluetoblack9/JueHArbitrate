use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::process::Command;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Violation {
    from: String,
    to: String,
    kind: &'static str,
}

fn allow_table() -> BTreeMap<String, Vec<String>> {
    let rows: &[(&str, &[&str])] = &[
        ("arbitrate-core", &[]),
        ("arbitrate-deepseek", &["arbitrate-core"]),
        ("arbitrate-llm", &["arbitrate-core"]),
        ("arbitrate-state", &["arbitrate-core"]),
        (
            "arbitrate-tools",
            &["arbitrate-core", "arbitrate-state", "arbitrate-deepseek"],
        ),
        (
            "arbitrate-agent",
            &[
                "arbitrate-core",
                "arbitrate-state",
                "arbitrate-tools",
                "arbitrate-deepseek",
                "arbitrate-llm",
            ],
        ),
        (
            "arbitrate-tui",
            &[
                "arbitrate-core",
                "arbitrate-state",
                "arbitrate-tools",
                "arbitrate-deepseek",
                "arbitrate-llm",
                "arbitrate-agent",
            ],
        ),
        (
            "arbitrate-surfaces",
            &[
                "arbitrate-core",
                "arbitrate-state",
                "arbitrate-tools",
                "arbitrate-deepseek",
                "arbitrate-llm",
                "arbitrate-agent",
            ],
        ),
        (
            "arbitrate",
            &[
                "arbitrate-core",
                "arbitrate-state",
                "arbitrate-deepseek",
                "arbitrate-llm",
                "arbitrate-tools",
                "arbitrate-agent",
                "arbitrate-tui",
                "arbitrate-surfaces",
            ],
        ),
    ];
    rows.iter()
        .map(|(k, v)| (k.to_string(), v.iter().map(|s| s.to_string()).collect()))
        .collect()
}

/// 直接边违规 + BFS 传递闭包违规（去重：direct 优先）。
fn check_edges(
    edges: &[(String, String)],
    allow: &BTreeMap<String, Vec<String>>,
) -> Vec<Violation> {
    let mut out: BTreeSet<Violation> = BTreeSet::new();
    let mut adj: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (f, t) in edges {
        adj.entry(f.clone()).or_default().push(t.clone());
        let ok = allow.get(f).map(|a| a.contains(t)).unwrap_or(false);
        if !ok {
            out.insert(Violation {
                from: f.clone(),
                to: t.clone(),
                kind: "direct",
            });
        }
    }
    for src in allow.keys() {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut q: VecDeque<String> = VecDeque::new();
        q.push_back(src.clone());
        while let Some(n) = q.pop_front() {
            for t in adj.get(&n).cloned().unwrap_or_default() {
                if seen.insert(t.clone()) {
                    q.push_back(t.clone());
                }
            }
        }
        for t in seen {
            if &t == src {
                continue;
            }
            let ok = allow.get(src).map(|a| a.contains(&t)).unwrap_or(false);
            if !ok
                && !out
                    .iter()
                    .any(|v| v.from == *src && v.to == t && v.kind == "direct")
            {
                out.insert(Violation {
                    from: src.clone(),
                    to: t,
                    kind: "transitive",
                });
            }
        }
    }
    out.into_iter().collect()
}

/// workspace_members 的 package ID 定义内部成员；packages 还含第三方包，不能全部当成本地包。
/// 按成员声明的 name + path 定位目标成员，兼容 dependency rename，排除同名外部包。
/// 遍历全部依赖声明，不按 kind/target/optional 过滤，保留三平台和未启用 feature 的边。
fn local_edges(meta: &serde_json::Value) -> Vec<(String, String)> {
    let member_ids: BTreeSet<&str> = meta["workspace_members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap())
        .collect();
    let members: Vec<&serde_json::Value> = meta["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| member_ids.contains(p["id"].as_str().unwrap()))
        .collect();
    let mut edges = Vec::new();
    for p in &members {
        let from = p["name"].as_str().unwrap().to_string();
        for d in p["dependencies"].as_array().unwrap() {
            let Some(path) = d["path"].as_str() else {
                continue;
            };
            if let Some(target) = members.iter().find(|target| {
                target["name"] == d["name"]
                    && std::path::Path::new(target["manifest_path"].as_str().unwrap()).parent()
                        == Some(std::path::Path::new(path))
            }) {
                edges.push((from.clone(), target["name"].as_str().unwrap().to_string()));
            }
        }
    }
    edges.sort();
    edges.dedup();
    edges
}

fn s(x: &str) -> String {
    x.to_string()
}

#[test]
fn real_workspace_obey_allow_list() {
    let out = Command::new(env!("CARGO"))
        .args(["metadata", "--locked", "--format-version", "1"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let v = check_edges(&local_edges(&meta), &allow_table());
    assert!(v.is_empty(), "依赖违规: {v:?}");
}

#[test]
fn synthetic_legal_chain_passes() {
    // 合法链的允许表必须传递闭合：a 允许 b 也允许 c，否则 a→c 传递可达即违规
    let allow: BTreeMap<String, Vec<String>> = [
        (s("a"), vec![s("b"), s("c")]),
        (s("b"), vec![s("c")]),
        (s("c"), vec![]),
    ]
    .into_iter()
    .collect();
    let edges = vec![(s("a"), s("b")), (s("b"), s("c"))];
    assert!(check_edges(&edges, &allow).is_empty());
}

#[test]
fn synthetic_direct_violations() {
    // 镜像真实禁令：core 反向、deepseek→llm、state→provider、tools→agent
    let allow = allow_table();
    for (f, t) in [
        ("arbitrate-core", "arbitrate-state"),
        ("arbitrate-deepseek", "arbitrate-llm"),
        ("arbitrate-state", "arbitrate-deepseek"),
        ("arbitrate-state", "arbitrate-llm"),
        ("arbitrate-tools", "arbitrate-agent"),
    ] {
        let v = check_edges(&[(s(f), s(t))], &allow);
        assert!(
            v.iter()
                .any(|v| v.from == f && v.to == t && v.kind == "direct"),
            "{f}->{t} 应判 direct"
        );
    }
}

#[test]
fn synthetic_indirect_only_violation() {
    // 构造图：a→b、b→c 直接边各自合法，但 a 的允许表不含 c（闭包被打破）→ 仅 transitive
    let allow: BTreeMap<String, Vec<String>> = [
        (s("a"), vec![s("b")]),
        (s("b"), vec![s("c")]),
        (s("c"), vec![]),
    ]
    .into_iter()
    .collect();
    let edges = vec![(s("a"), s("b")), (s("b"), s("c"))];
    let v = check_edges(&edges, &allow);
    assert_eq!(
        v,
        vec![Violation {
            from: s("a"),
            to: s("c"),
            kind: "transitive"
        }]
    );
}

#[test]
fn metadata_filters_external_packages_and_same_name_dependencies() {
    let meta = serde_json::json!({
        "workspace_members": ["root-id", "core-id"],
        "packages": [
            {"id":"root-id", "name":"arbitrate", "manifest_path":"/w/Cargo.toml",
             "dependencies":[
                 {"name":"serde_json", "path":null},
                 {"name":"arbitrate-core", "path":"/w/core", "rename":"core_alias"},
                 {"name":"arbitrate-core", "path":"/external/core"}
             ]},
            {"id":"core-id", "name":"arbitrate-core", "manifest_path":"/w/core/Cargo.toml", "dependencies":[]},
            {"id":"json-id", "name":"serde_json", "manifest_path":"/external/json/Cargo.toml",
             "dependencies":[{"name":"arbitrate-core", "path":"/w/core"}]},
            {"id":"external-core-id", "name":"arbitrate-core", "manifest_path":"/external/core/Cargo.toml", "dependencies":[]}
        ]
    });
    let edges = local_edges(&meta);
    assert_eq!(edges, vec![(s("arbitrate"), s("arbitrate-core"))]);
    assert!(check_edges(&edges, &allow_table()).is_empty());
}

#[test]
fn metadata_keeps_dev_build_optional_and_other_target_declarations() {
    let meta = serde_json::json!({
        "workspace_members": ["core-id", "state-id", "deepseek-id", "llm-id"],
        "packages": [
            {"id":"core-id", "name":"arbitrate-core", "manifest_path":"/w/core/Cargo.toml",
             "dependencies":[
                 {"name":"arbitrate-state", "path":"/w/state", "kind":null, "target":null},
                 {"name":"arbitrate-deepseek", "path":"/w/deepseek", "kind":"dev", "target":"cfg(windows)"},
                 {"name":"arbitrate-llm", "path":"/w/llm", "kind":"build", "optional":true, "target":"cfg(target_os = \"linux\")"}
             ]},
            {"id":"state-id", "name":"arbitrate-state", "manifest_path":"/w/state/Cargo.toml", "dependencies":[]},
            {"id":"deepseek-id", "name":"arbitrate-deepseek", "manifest_path":"/w/deepseek/Cargo.toml", "dependencies":[]},
            {"id":"llm-id", "name":"arbitrate-llm", "manifest_path":"/w/llm/Cargo.toml", "dependencies":[]}
        ]
    });
    let edges = local_edges(&meta);
    assert_eq!(edges.len(), 3);
    let violations = check_edges(&edges, &allow_table());
    assert_eq!(violations.len(), 3);
    assert!(violations.iter().all(|v| v.kind == "direct"));
}
