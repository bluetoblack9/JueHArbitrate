use arbitrate_core::paths::{
    Paths, PathsError, ROOT_DIR_NAME, home_from, project_hooks_toml, project_id,
};
use std::path::{Path, PathBuf};

#[test]
fn home_resolution_precedence() {
    // 非空 SEEK_HOME 原样胜出（不规范化）
    assert_eq!(
        home_from(Some("/custom/root"), Some(Path::new("/home/u"))).unwrap(),
        PathBuf::from("/custom/root")
    );
    // 空字符串视为未设置
    assert_eq!(
        home_from(Some(""), Some(Path::new("/home/u"))).unwrap(),
        PathBuf::from("/home/u").join(ROOT_DIR_NAME)
    );
    // 未设置 → OS home + .arbitrate
    assert_eq!(
        home_from(None, Some(Path::new("/home/u"))).unwrap(),
        PathBuf::from("/home/u/.arbitrate")
    );
    // OS home 不可得 → 错误
    assert_eq!(home_from(None, None), Err(PathsError::HomeUnavailable));
    assert_eq!(home_from(Some(""), None), Err(PathsError::HomeUnavailable));
}

#[test]
fn root_dir_name_is_arbitrate_not_seek() {
    assert_eq!(ROOT_DIR_NAME, ".arbitrate");
    let p = home_from(None, Some(Path::new("/h"))).unwrap();
    assert!(!p.to_string_lossy().contains(".seek"));
}

#[test]
fn subdirs_compose_under_root() {
    let root = Path::new("/tmp/fake-root");
    let p = Paths::from_root(root);
    let cases: &[(&str, PathBuf)] = &[
        ("sessions", p.sessions()),
        ("mcp.json", p.mcp_config()),
        ("cache", p.cache()),
        ("skills", p.user_skills()),
        ("skills/.stats.jsonl", p.user_skill_stats()),
        ("agents", p.user_agents()),
        ("projects", p.projects()),
        ("soul.md", p.soul()),
        ("cron", p.cron_dir()),
        ("cron/jobs.jsonl", p.cron_jobs()),
        ("cron/tick.lock", p.cron_tick_lock()),
        ("cron/runs", p.cron_runs()),
        ("cron/env", p.cron_env()),
        ("cron/triggers", p.cron_triggers()),
        ("serve", p.serve_dir()),
        ("keybindings.toml", p.user_keybindings()),
        ("hooks.toml", p.user_hooks_toml()),
        ("trusted-projects.json", p.trusted_projects_json()),
        ("hooks-audit.jsonl", p.hooks_audit_log()),
    ];
    for (rel, got) in cases {
        assert_eq!(got, &root.join(rel), "case {rel}");
    }
}

#[test]
fn project_id_known_answer() {
    // KAT：sha256 标准算法的显式期望值（shasum -a 256 独立计算，非实现自证）
    assert_eq!(
        project_id(Path::new("/abs/path/to/project")),
        "aa8f97f559fb5e1e"
    );
    assert_eq!(
        project_id(Path::new("/Users/whyiyhw/code/github/seek")),
        "c4a152f964a6e6ac"
    );
}

#[test]
fn project_id_shape_and_determinism() {
    let a = project_id(Path::new("/Users/x/projectA"));
    let b = project_id(Path::new("/Users/x/projectA"));
    assert_eq!(a, b);
    assert_eq!(a.len(), 16);
    assert!(
        a.chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    );
    assert_ne!(a, project_id(Path::new("/Users/x/projectB")));
}

#[test]
fn project_id_alias_spellings_differ() {
    // 契约：逐字节哈希路径字符串，别名拼写 = 不同标识（无文件系统规范化）
    assert_eq!(project_id(Path::new("/tmp/seek-smoke")), "6f0fefdbf0b47414");
    assert_eq!(
        project_id(Path::new("/private/tmp/seek-smoke")),
        "dc20026e7e8b257d"
    );
    assert_ne!(
        project_id(Path::new("/tmp/seek-smoke")),
        project_id(Path::new("/private/tmp/seek-smoke"))
    );
}

#[test]
fn project_dirs_compose() {
    let root = Path::new("/tmp/fake-root");
    let p = Paths::from_root(root);
    let abs = Path::new("/abs/path/to/project");
    let id = project_id(abs);
    let dir = root.join("projects").join(&id);
    assert_eq!(p.project_dir(abs), dir);
    assert_eq!(p.project_plans(abs), dir.join("plans"));
    assert_eq!(p.project_assets(abs), dir.join("assets"));
    assert_eq!(p.subagents_index(abs), dir.join("subagents.jsonl"));
    assert_eq!(
        p.project_session_dir(abs, "sid123").unwrap(),
        dir.join("sessions").join("sid123")
    );
    assert_eq!(
        p.session_checkpoint_dir(abs, "sid123").unwrap(),
        p.project_session_dir(abs, "sid123").unwrap()
    );
    assert_eq!(
        p.subagent_session_dir(abs, "sid123", "sub456").unwrap(),
        dir.join("sessions")
            .join("sid123")
            .join("subagents")
            .join("sub456")
    );
}

#[test]
fn session_dirs_reject_empty_ids() {
    let p = Paths::from_root("/tmp/fake-root");
    let abs = Path::new("/abs");
    assert_eq!(
        p.project_session_dir(abs, ""),
        Err(PathsError::EmptySessionId)
    );
    assert_eq!(
        p.session_checkpoint_dir(abs, ""),
        Err(PathsError::EmptySessionId)
    );
    assert_eq!(
        p.subagent_session_dir(abs, "", "sub"),
        Err(PathsError::EmptySessionId)
    );
    assert_eq!(
        p.subagent_session_dir(abs, "sid", ""),
        Err(PathsError::EmptySubagentId)
    );
}

#[test]
fn project_hooks_toml_lives_inside_project() {
    assert_eq!(
        project_hooks_toml(Path::new("/abs/path/to/project")),
        PathBuf::from("/abs/path/to/project/.arbitrate/hooks.toml")
    );
}
