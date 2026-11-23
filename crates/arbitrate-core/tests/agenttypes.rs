use arbitrate_core::agenttypes::{Definition, Origin, Set, builtins, parse, valid_name};

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
