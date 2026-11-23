mod support;

const EXPECTED_FLAGS: [&str; 32] = [
    "p",
    "model",
    "max-turns",
    "max-tokens",
    "auto-continue",
    "yolo",
    "plan",
    "json",
    "resume",
    "continue",
    "no-save",
    "no-suggest",
    "no-cron-tick",
    "list",
    "no-project-md",
    "provider",
    "base-url",
    "provider-name",
    "theme",
    "rpc",
    "benchmark",
    "benchmark-out",
    "version",
    "upgrade",
    "upgrade-force",
    "upgrade-dry-run",
    "upgrade-check",
    "install",
    "dream",
    "dream-write",
    "keep-checkpoints",
    "tools",
];

#[test]
fn contract_complete_and_consistent() {
    let v: serde_json::Value =
        serde_json::from_str(&support::load_fixture("cli-contract.json")).unwrap();
    let flags = v["flags"].as_array().unwrap();
    assert_eq!(flags.len(), 32);
    let names: Vec<&str> = flags.iter().map(|f| f["name"].as_str().unwrap()).collect();
    for e in EXPECTED_FLAGS {
        assert!(names.contains(&e), "缺 flag {e}");
    }
    for f in flags {
        for k in ["name", "type", "default", "owner"] {
            assert!(f.get(k).is_some(), "flag {} 缺 {k}", f["name"]);
        }
        assert!(["M2", "M3", "M4", "M5"].contains(&f["owner"].as_str().unwrap()));
    }
    let d = &v["dispatch"];
    assert_eq!(d["pre_parse_subcommands"].as_array().unwrap().len(), 12);
    assert_eq!(
        d["positional_lanes_after_parse"].as_array().unwrap().len(),
        3
    );
    let order: Vec<&str> = d["post_parse_short_circuit_order"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap())
        .collect();
    assert_eq!(
        order,
        [
            "version",
            "upgrade|upgrade-dry-run|upgrade-force",
            "upgrade-check",
            "install"
        ]
    );
    assert_eq!(v["exit_codes"]["unknown_tools_name"], 2);
    assert_eq!(v["exit_codes"]["keys_usage_error"], 2);
    for field in [
        "help_verb_first_position_only",
        "help_flags_any_position_before_dashdash",
        "help_scan_stops_at_dashdash",
    ] {
        assert_eq!(d[field], true, "帮助扫描契约缺少或改变 {field}");
    }
    assert_eq!(
        d["help_scan_cases"],
        serde_json::json!([
            {"argv":["help"], "requested":true},
            {"argv":["-p", "help"], "requested":false},
            {"argv":["-p", "-h"], "requested":true},
            {"argv":["-p", "--help"], "requested":true},
            {"argv":["--", "help"], "requested":false},
            {"argv":["--", "-h"], "requested":false},
            {"argv":["--", "--help"], "requested":false},
            {"argv":["-h", "--"], "requested":true}
        ])
    );
}

#[test]
fn subcommand_inventory_covers_all_canonical_entries() {
    use std::collections::BTreeSet;
    const COMMANDS: &[&str] = &[
        "skill install",
        "skill uninstall",
        "skill update",
        "skill list",
        "skill status",
        "skill stats",
        "skill create",
        "skill drafts",
        "skill drafts show",
        "skill drafts approve",
        "skill drafts reject",
        "memory list",
        "memory show",
        "memory search",
        "memory archive",
        "memory add",
        "agents list",
        "agents check",
        "agents create",
        "agents remove",
        "session export",
        "checkpoint list",
        "checkpoint restore",
        "checkpoint prune",
        "undo",
        "redo",
        "hooks list",
        "hooks check",
        "hooks trust",
        "hooks audit",
        "worktree list",
        "worktree gc",
        "cron create",
        "cron list",
        "cron delete",
        "cron run",
        "cron tick",
        "cron config check",
        "keys list",
        "keys check",
        "keys actions",
        "serve",
        "acp",
        "goal",
        "autopilot",
    ];
    const GROUPS: &[&str] = &[
        "skill",
        "skill drafts",
        "memory",
        "agents",
        "session",
        "checkpoint",
        "hooks",
        "worktree",
        "cron",
        "cron config",
        "keys",
    ];
    let v: serde_json::Value =
        serde_json::from_str(&support::load_fixture("cli-subcommands.json")).unwrap();
    assert_eq!(
        v["source_commit"],
        "cb27b90d2979f5f840526100b28ade586d034df9"
    );
    let commands = v["commands"].as_array().unwrap();
    let names: BTreeSet<&str> = commands
        .iter()
        .map(|c| c["command"].as_str().unwrap())
        .collect();
    assert_eq!(commands.len(), names.len(), "规范入口不能重复");
    assert_eq!(names, COMMANDS.iter().copied().collect::<BTreeSet<_>>());
    for c in commands {
        assert!(["M2", "M3", "M4", "M5"].contains(&c["owner"].as_str().unwrap()));
        assert!(c["aliases"].is_array());
        assert!(!c["sources"].as_array().unwrap().is_empty());
        for key in ["positionals", "parsing", "help", "errors"] {
            assert!(c[key].is_object(), "{} 缺少结构化 {key}", c["command"]);
        }
        for flag in c["flags"].as_array().unwrap() {
            for key in [
                "name",
                "aliases",
                "type",
                "declared_default",
                "repeatable",
                "help",
            ] {
                assert!(flag.get(key).is_some(), "{} 的 flag 缺 {key}", c["command"]);
            }
        }
        assert!(!c["cases"].as_array().unwrap().is_empty());
    }
    let groups = v["groups"].as_array().unwrap();
    let group_names: BTreeSet<&str> = groups
        .iter()
        .map(|g| g["command"].as_str().unwrap())
        .collect();
    assert_eq!(groups.len(), group_names.len());
    assert_eq!(group_names, GROUPS.iter().copied().collect::<BTreeSet<_>>());
    for g in groups {
        for key in ["no_args", "help", "unknown_subcommand"] {
            assert!(g[key].is_object(), "{} 缺 {key}", g["command"]);
        }
    }
    let serve = commands.iter().find(|c| c["command"] == "serve").unwrap();
    for name in ["frontend", "listen"] {
        assert!(
            serve["flags"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["name"] == name)
        );
    }
    for field in [
        "help_verb_first_position_only",
        "help_flags_any_position_before_dashdash",
        "help_scan_stops_at_dashdash",
    ] {
        assert_eq!(serve["help"][field], true, "serve 帮助扫描缺 {field}");
    }
    let help_cases = serve["help"]["help_scan_cases"].as_array().unwrap();
    for expected in [
        serde_json::json!({"argv":["--frontend=web", "-h"], "requested":true}),
        serde_json::json!({"argv":["--", "-h"], "requested":false}),
    ] {
        assert!(
            help_cases.contains(&expected),
            "serve 缺帮助边界样例: {expected}"
        );
    }
    assert!(
        !std::fs::read_to_string("docs/cli-source-audit.md")
            .unwrap()
            .trim()
            .is_empty()
    );
}
