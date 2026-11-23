use arbitrate_core::agenttypes::{Definition, Origin, Set, builtins, parse, valid_name};

mod common;
use arbitrate_core::agenttypes::{LoadOptions, RouteOverride, load};
use common::TempRoot;
use std::collections::BTreeMap;
use std::path::Path;

const GOOD: &str = "---\nname: verifier\ndescription: Runs the test suite and reports failures. Read-only except tests.\ntools: [read, grep, list_dir, git, bash]\nprovider: glm\nmodel: glm-5.3\neffort: low\nworkflow: plan-analyze\nmax_turns: 60\n---\nYou are a test verifier subagent.";

#[test]
fn parse_good() {
    let d = parse(GOOD.as_bytes(), "verifier", "/x/verifier.md").unwrap();
    assert_eq!(d.name, "verifier");
    assert!(!d.description.is_empty());
    assert_eq!(d.provider, "glm");
    assert_eq!(d.model, "glm-5.3");
    assert_eq!(d.effort, "low");
    assert_eq!(d.workflow, "plan-analyze");
    assert_eq!(d.max_turns, 60);
    assert_eq!(d.tools, ["read", "grep", "list_dir", "git", "bash"]);
    assert_eq!(d.body, "You are a test verifier subagent.");
    assert_eq!(d.source, "/x/verifier.md");
}

#[test]
fn parse_name_fallback_to_filename() {
    let d = parse(
        b"---\ndescription: x\n---\nRole.",
        "verifier",
        "/x/verifier.md",
    )
    .unwrap();
    assert_eq!(d.name, "verifier");
}

#[test]
fn valid_name_table() {
    for n in ["a", "verifier", "plan-v2", "x0-y", "a-b-c", "x-"] {
        assert!(valid_name(n), "want true: {n:?}");
    }
    for n in [
        "", "A", "Verifier", "-x", "a_b", "a/b", "..\\x", "../x", "/abs", "..", ".", "x y",
    ] {
        assert!(!valid_name(n), "want false: {n:?}");
    }
}

#[test]
fn parse_minimal() {
    let d = parse(b"---\ndescription: x\n---\nRole.\n", "min", "src").unwrap();
    assert!(d.tools.is_empty());
    assert_eq!(d.provider, "");
    assert_eq!(d.workflow, "");
    assert_eq!(d.max_turns, 0);
}

#[test]
fn parse_bad_shapes() {
    let cases: &[(&str, &str, &str)] = &[
        ("t", "# heading\nBody.", "missing frontmatter"),
        ("t", "---\nname: t\n", "unterminated"),
        ("t", "---\nname: t\n---\nBody.", "missing `description`"),
        ("t", "---\nname: t\ndescription: d\n---\n", "empty body"),
        (
            "t",
            "---\nname: Bad_Name\ndescription: d\n---\nB.",
            "kebab-case",
        ),
        ("", "---\ndescription: d\n---\nB.", "no name"),
        (
            "t",
            "---\ndescription: d\ntools: [read, agent]\n---\nB.",
            "never allowed",
        ),
        (
            "t",
            "---\ndescription: d\ntools: [ask_user]\n---\nB.",
            "never allowed",
        ),
        (
            "t",
            "---\ndescription: d\nworkflow: plan-execute\n---\nB.",
            "plan-analyze",
        ),
        (
            "t",
            "---\ndescription: d\nworkflow: yolo\n---\nB.",
            "plan-analyze",
        ),
        (
            "t",
            "---\ndescription: d\neffort: turbo\n---\nB.",
            "low, medium, high",
        ),
        (
            "t",
            "---\ndescription: d\nmax_turns: 0\n---\nB.",
            "positive integer",
        ),
        (
            "t",
            "---\ndescription: d\nmax_turns: -3\n---\nB.",
            "positive integer",
        ),
        (
            "t",
            "---\ndescription: d\nmax_turns: many\n---\nB.",
            "positive integer",
        ),
    ];
    for (stem, input, want) in cases {
        let err = parse(input.as_bytes(), stem, "src").unwrap_err();
        assert!(
            err.to_string().contains(want),
            "input {input:?}: err {err}, want substring {want:?}"
        );
    }
}

#[test]
fn parse_block_list_tools() {
    let d = parse(
        b"---\ndescription: d\ntools:\n  - read\n  - git\n---\nB.",
        "t",
        "src",
    )
    .unwrap();
    assert_eq!(d.tools, ["read", "git"]);
}

#[test]
fn parse_empty_tools_list_inherits() {
    let d = parse(b"---\ndescription: d\ntools: []\n---\nB.", "t", "src").unwrap();
    assert!(d.tools.is_empty());
    assert_eq!(d.name, "t");
    assert_eq!(d.body, "B.");
}

#[test]
fn parse_unknown_keys_tolerated() {
    let d = parse(
        b"---\ndescription: d\nfuture_key: whatever\n---\nB.",
        "t",
        "src",
    )
    .unwrap();
    assert_eq!(d.name, "t");
}

#[test]
fn builtins_shape() {
    let bs = builtins();
    let names: Vec<&str> = bs.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names, ["general-purpose", "explore", "plan"]);
    for d in &bs {
        assert_eq!(d.origin, Origin::Builtin);
        assert_eq!(d.source, format!("builtin:{}", d.name));
        assert!(!d.description.is_empty());
        if d.name == "explore" || d.name == "plan" {
            assert_eq!(d.workflow, "plan-analyze");
            assert_eq!(
                d.tools,
                ["read", "grep", "list_dir", "git", "webfetch", "think"]
            );
            assert!(!d.body.is_empty());
        }
        if d.name == "general-purpose" {
            assert!(d.tools.is_empty());
            assert_eq!(d.workflow, "");
            assert_eq!(d.body, "");
        }
    }
}

#[test]
fn set_first_writer_wins() {
    let mut s = Set::default();
    let a = Definition {
        name: "dupe".to_string(),
        source: "a.md".to_string(),
        origin: Origin::User,
        ..Definition::default()
    };
    let b = Definition {
        name: "dupe".to_string(),
        source: "b.md".to_string(),
        origin: Origin::User,
        ..Definition::default()
    };
    assert!(s.add(a));
    assert!(!s.add(b));
    assert_eq!(s.get("dupe").unwrap().source, "a.md");
    assert_eq!(s.shadowed("dupe"), ["b.md"]);
    assert!(s.shadowed("absent").is_empty());
    assert!(s.get("absent").is_none());
    // 空名拒收
    assert!(!s.add(Definition::default()));
    // 插入序 + custom_count
    let built = builtins();
    for d in built {
        s.add(d);
    }
    let order: Vec<&str> = s.list().iter().map(|d| d.name.as_str()).collect();
    assert_eq!(order, ["dupe", "general-purpose", "explore", "plan"]);
    assert_eq!(s.len(), 4);
    assert_eq!(s.custom_count(), 1);
}

const VERIFIER_MD: &str =
    "---\nname: verifier\ndescription: runs tests\ntools: [read, bash]\n---\nVerify everything.";

fn write_file(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

fn opts(project: &Path, user: &Path) -> LoadOptions {
    LoadOptions {
        project_dir: Some(project.to_path_buf()),
        user_agents_dir: Some(user.to_path_buf()),
        config_agents: BTreeMap::new(),
    }
}

fn fs_snapshot(root: &Path) -> BTreeMap<String, (u64, u128)> {
    fn walk(dir: &Path, root: &Path, out: &mut BTreeMap<String, (u64, u128)>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            let rel = p.strip_prefix(root).unwrap().to_string_lossy().to_string();
            if p.is_dir() {
                out.insert(format!("{rel}/"), (0, 0));
                walk(&p, root, out);
            } else {
                let m = std::fs::metadata(&p).unwrap();
                let mt = m
                    .modified()
                    .unwrap()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos();
                out.insert(rel, (m.len(), mt));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

#[test]
fn load_no_dirs_only_builtins() {
    let project = TempRoot::new();
    let user = TempRoot::new();
    let (set, stats) = load(&opts(&project.0, &user.0.join("nope")));
    assert!(stats.errors.is_empty(), "{:?}", stats.errors);
    assert_eq!(set.len(), 3);
    for name in ["general-purpose", "explore", "plan"] {
        assert!(set.get(name).is_some(), "builtin {name} missing");
    }
    assert_eq!(set.custom_count(), 0);
}

#[test]
fn load_project_and_user() {
    let project = TempRoot::new();
    let user = TempRoot::new();
    write_file(
        &project.0.join(".arbitrate/agents/verifier.md"),
        VERIFIER_MD,
    );
    write_file(
        &user.0.join("librarian.md"),
        "---\nname: librarian\ndescription: finds docs\n---\nIndex things.",
    );
    let (set, stats) = load(&opts(&project.0, &user.0));
    assert!(stats.errors.is_empty(), "{:?}", stats.errors);
    assert_eq!(set.len(), 5);
    assert_eq!(set.get("verifier").unwrap().origin, Origin::Project);
    assert_eq!(set.get("librarian").unwrap().origin, Origin::User);
    assert_eq!(set.custom_count(), 2);
}

#[test]
fn load_scope_override() {
    let project = TempRoot::new();
    let user = TempRoot::new();
    write_file(
        &project.0.join(".arbitrate/agents/explore.md"),
        "---\ndescription: project explore\n---\nFrom project.",
    );
    write_file(
        &user.0.join("explore.md"),
        "---\ndescription: user explore\n---\nFrom user.",
    );
    let (set, stats) = load(&opts(&project.0, &user.0));
    assert!(
        stats.errors.is_empty(),
        "scope override is by design: {:?}",
        stats.errors
    );
    let got = set.get("explore").unwrap();
    assert_eq!(got.origin, Origin::Project);
    assert_eq!(got.description, "project explore");
    assert_eq!(set.shadowed("explore").len(), 2); // user file + builtin
}

#[test]
fn load_duplicate_same_dir() {
    let project = TempRoot::new();
    let user = TempRoot::new();
    write_file(
        &user.0.join("a.md"),
        "---\nname: dupe\ndescription: from a\n---\nA body.",
    );
    write_file(
        &user.0.join("b.md"),
        "---\nname: dupe\ndescription: from b\n---\nB body.",
    );
    let (set, stats) = load(&opts(&project.0, &user.0));
    assert_eq!(set.get("dupe").unwrap().description, "from a");
    assert_eq!(stats.errors.len(), 1);
    assert!(stats.errors[0].contains("b.md"), "{:?}", stats.errors);
    let sh = set.shadowed("dupe");
    assert_eq!(sh.len(), 1);
    assert!(sh[0].contains("b.md"));
}

#[test]
fn load_bad_file_skipped_not_fatal() {
    let project = TempRoot::new();
    let user = TempRoot::new();
    write_file(
        &user.0.join("broken.md"),
        "---\nname: broken\ndescription: d\n",
    ); // unterminated
    write_file(&user.0.join("good.md"), VERIFIER_MD);
    let (set, stats) = load(&opts(&project.0, &user.0));
    assert_eq!(stats.errors.len(), 1);
    assert!(stats.errors[0].contains("broken.md"));
    assert!(set.get("broken").is_none());
    assert!(set.get("verifier").is_some());
    assert_eq!(set.len(), 4);
}

#[test]
fn load_subdirectory_warns() {
    let project = TempRoot::new();
    let user = TempRoot::new();
    std::fs::create_dir_all(user.0.join("oops")).unwrap();
    let (_, stats) = load(&opts(&project.0, &user.0));
    assert_eq!(stats.errors.len(), 1);
    assert!(stats.errors[0].contains("oops"));
}

#[test]
fn load_config_override_clones_builtin() {
    let project = TempRoot::new();
    let user = TempRoot::new();
    let mut o = opts(&project.0, &user.0.join("nope"));
    o.config_agents = BTreeMap::from([(
        "explore".to_string(),
        RouteOverride {
            provider: "glm".to_string(),
            model: "glm-5.3".to_string(),
            effort: "low".to_string(),
            max_turns: 60,
        },
    )]);
    let (set, stats) = load(&o);
    assert!(stats.errors.is_empty(), "{:?}", stats.errors);
    let got = set.get("explore").unwrap();
    assert_eq!(got.origin, Origin::Config);
    assert_eq!(
        (
            got.provider.as_str(),
            got.model.as_str(),
            got.effort.as_str(),
            got.max_turns
        ),
        ("glm", "glm-5.3", "low", 60)
    );
    // 非路由字段逐字来自内置
    let base = builtins()
        .into_iter()
        .find(|b| b.name == "explore")
        .unwrap();
    assert_eq!(got.body, base.body);
    assert_eq!(got.tools, base.tools);
    assert_eq!(got.workflow, base.workflow);
    assert_eq!(set.get("plan").unwrap().origin, Origin::Builtin);
    assert_eq!(stats.by_source["config agents"], 1);
    assert_eq!(set.custom_count(), 1);
}

#[test]
fn load_scope_stack_config_layer() {
    let project = TempRoot::new();
    let user = TempRoot::new();
    write_file(
        &project.0.join(".arbitrate/agents/explore.md"),
        "---\ndescription: project explore\n---\nFrom project.",
    );
    write_file(
        &user.0.join("explore.md"),
        "---\ndescription: user explore\n---\nFrom user.",
    );
    let cfg = BTreeMap::from([(
        "explore".to_string(),
        RouteOverride {
            provider: "glm".to_string(),
            ..RouteOverride::default()
        },
    )]);

    let layers: Vec<(LoadOptions, Origin, &str)> = vec![
        (
            LoadOptions {
                config_agents: cfg.clone(),
                ..opts(&project.0, &user.0)
            },
            Origin::Project,
            "project explore",
        ),
        (
            LoadOptions {
                config_agents: cfg.clone(),
                ..opts(&TempRoot::new().0, &user.0)
            },
            Origin::User,
            "user explore",
        ),
        (
            LoadOptions {
                config_agents: cfg.clone(),
                ..opts(&TempRoot::new().0, &TempRoot::new().0.join("nope"))
            },
            Origin::Config,
            "Read-only parallel research",
        ),
        (
            opts(&TempRoot::new().0, &TempRoot::new().0.join("nope")),
            Origin::Builtin,
            "Read-only parallel research",
        ),
    ];
    for (o, want_origin, want_desc) in layers {
        let (set, stats) = load(&o);
        assert!(stats.errors.is_empty(), "{:?}", stats.errors);
        let got = set.get("explore").unwrap();
        assert_eq!(got.origin, want_origin);
        assert!(got.description.contains(want_desc), "{}", got.description);
    }

    // 全栈一次加载：每个败者都留档
    let (set, _) = load(&LoadOptions {
        config_agents: cfg.clone(),
        ..opts(&project.0, &user.0)
    });
    assert_eq!(set.shadowed("explore").len(), 3); // user file + config clone + builtin
    // config 赢时携带覆盖
    let (cfg_set, _) = load(&LoadOptions {
        config_agents: cfg,
        ..opts(&TempRoot::new().0, &TempRoot::new().0.join("nope"))
    });
    assert_eq!(cfg_set.get("explore").unwrap().provider, "glm");
}

#[test]
fn load_config_unknown_key_and_file_shadow() {
    let project = TempRoot::new();
    let user = TempRoot::new();
    write_file(&user.0.join("verifier.md"), VERIFIER_MD);
    let mut o = opts(&project.0, &user.0);
    o.config_agents = BTreeMap::from([
        (
            "no-such-type".to_string(),
            RouteOverride {
                provider: "glm".to_string(),
                ..RouteOverride::default()
            },
        ),
        (
            "verifier".to_string(),
            RouteOverride {
                provider: "glm".to_string(),
                ..RouteOverride::default()
            },
        ),
    ]);
    let (set, stats) = load(&o);
    assert_eq!(stats.errors.len(), 1);
    assert!(stats.errors[0].contains("no-such-type"));
    let v = set.get("verifier").unwrap();
    assert_eq!(v.origin, Origin::User);
    assert_eq!(v.provider, "");
    let sh = set.shadowed("verifier");
    assert_eq!(sh.len(), 1);
    assert!(sh[0].contains("config"));
}

#[test]
fn load_config_bad_field_shapes() {
    let project = TempRoot::new();
    let user = TempRoot::new();
    let mut o = opts(&project.0, &user.0.join("nope"));
    o.config_agents = BTreeMap::from([(
        "plan".to_string(),
        RouteOverride {
            provider: "glm".to_string(),
            effort: "turbo".to_string(),
            max_turns: -5,
            ..RouteOverride::default()
        },
    )]);
    let (set, stats) = load(&o);
    assert_eq!(stats.errors.len(), 2, "{:?}", stats.errors);
    let got = set.get("plan").unwrap();
    assert_eq!(got.origin, Origin::Config);
    assert_eq!(got.provider, "glm");
    assert_eq!(got.effort, "");
    assert_eq!(got.max_turns, 0);
}

#[test]
fn load_readonly_never_writes() {
    let project = TempRoot::new();
    let user = TempRoot::new();
    let proj_agents = project.0.join(".arbitrate/agents");
    write_file(&proj_agents.join("verifier.md"), VERIFIER_MD);
    write_file(
        &proj_agents.join("broken.md"),
        "---\nname: broken\ntools: [read, agent]\ndescription: bad\n---\nBody.",
    );
    write_file(
        &user.0.join("librarian.md"),
        "---\nname: librarian\ndescription: finds docs\n---\nIndex things.",
    );
    let before_p = fs_snapshot(&project.0);
    let before_u = fs_snapshot(&user.0);
    let (set, stats) = load(&opts(&project.0, &user.0));
    assert_eq!(set.len(), 5);
    assert_eq!(stats.errors.len(), 1);
    assert_eq!(
        fs_snapshot(&project.0),
        before_p,
        "project tree mutated by load"
    );
    assert_eq!(fs_snapshot(&user.0), before_u, "user tree mutated by load");
}

#[test]
fn load_readonly_does_not_create_missing_dirs() {
    let project = TempRoot::new();
    let user = TempRoot::new().0.join("absent-agents");
    let missing_proj = project.0.join(".arbitrate/agents");
    let _ = load(&opts(&project.0, &user));
    assert!(!user.exists());
    assert!(!missing_proj.exists());
}

#[test]
fn load_readonly_mtime_unchanged() {
    let project = TempRoot::new();
    let user = TempRoot::new();
    let path = user.0.join("verifier.md");
    write_file(&path, VERIFIER_MD);
    // 预设旧 mtime：Load 若误写必然移动。以设置后读回的实际值为基准，
    // 不依赖文件系统对请求时间的存储精度。
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(old)
        .unwrap();
    let before = std::fs::metadata(&path).unwrap().modified().unwrap();
    let _ = load(&opts(&project.0, &user.0));
    let after = std::fs::metadata(&path).unwrap().modified().unwrap();
    assert_eq!(after, before, "mtime moved — load must not write");
}
