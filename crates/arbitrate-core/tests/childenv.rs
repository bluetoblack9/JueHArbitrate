//! core::childenv 契约测试 — 名称敏感谓词 + 顺序保持 scrub + 环境序 withheld
//! （规格：上游 internal/childenv，Rust 自有重写）。安全语义不简化（约束 6）。
//! 虚构凭据名/值，不触真实凭据。

use arbitrate_core::childenv::{is_sensitive, sanitized, scrub, withheld};

const CREDENTIAL: &[&str] = &[
    "DEEPSEEK_API_KEY",
    "OPENAI_API_KEY",
    "GITHUB_TOKEN",
    "GH_TOKEN",
    "AWS_SECRET_ACCESS_KEY",
    "AWS_ACCESS_KEY_ID",
    "PGPASSWORD",
    "DB_PASSWD",
    "MY_CREDENTIAL",
    "NPM_AUTH_TOKEN",
    "SECRET_KEY",
    "APIKEY",
    "password",
    "mytoken",
    "STRIPE_SECRET",
];
const SEEK_NS: &[&str] = &[
    "SEEK_HOME",
    "SEEK_LANG",
    "SEEK_SESSIONS_DIR",
    "SEEK_AUTO_DISTILL",
];
const BENIGN: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "TERM",
    "LANG",
    "LC_ALL",
    "EDITOR",
    "PWD",
    "TMPDIR",
    "SSH_AUTH_SOCK",
    "XDG_CONFIG_HOME",
    "DISPLAY",
    "COLORTERM",
    "HOSTNAME",
];

#[test]
fn sensitive_table() {
    for name in CREDENTIAL.iter().chain(SEEK_NS.iter()) {
        assert!(is_sensitive(name), "{name} 应敏感");
    }
    for name in BENIGN {
        assert!(
            !is_sensitive(name),
            "{name} 应良性（SSH_AUTH_SOCK 是 socket 路径，USER 是身份）"
        );
    }
}

#[test]
fn case_insensitive() {
    assert!(is_sensitive("deepseek_api_key"));
    assert!(is_sensitive("Github_Token"));
    assert!(is_sensitive("seek_home"));
    assert!(!is_sensitive("ssh_auth_sock"));
}

#[test]
fn empty_name_not_sensitive() {
    assert!(!is_sensitive(""));
}

#[test]
fn scrub_order_byte_identical() {
    let env: Vec<String> = [
        "PATH=/usr/bin",
        "DEEPSEEK_API_KEY=sk-fake",
        "HOME=/home/u",
        "GH_TOKEN=ghp_fake",
        "TERM=xterm",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let out = scrub(&env, &[]);
    assert_eq!(
        out,
        vec![
            "PATH=/usr/bin".to_string(),
            "HOME=/home/u".to_string(),
            "TERM=xterm".to_string()
        ]
    );
}

#[test]
fn never_leak_value() {
    let env: Vec<String> = [
        "DEEPSEEK_API_KEY=sk-secret-fake",
        "GITHUB_TOKEN=ghp_fake",
        "PATH=/bin",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let out = scrub(&env, &[]);
    let joined = out.join("\n");
    assert!(!joined.contains("sk-secret-fake"));
    assert!(!joined.contains("ghp_fake"));
    assert!(!joined.contains("DEEPSEEK_API_KEY"));
}

#[test]
fn keep_exact_not_substring() {
    let env: Vec<String> = ["GH_TOKEN=ghp_fake", "GH_TOKEN_EXTRA=x", "PATH=/bin"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let out = scrub(&env, &["gh_token"]); // 大小写不敏感精确匹配
    assert_eq!(
        out,
        vec!["GH_TOKEN=ghp_fake".to_string(), "PATH=/bin".to_string()]
    );
    // GH_TOKEN_EXTRA 不被 keep（精确，非 substring）→ 仍 drop
}

#[test]
fn blank_keep_ignored() {
    let env: Vec<String> = ["GH_TOKEN=x", "PATH=/bin"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let out = scrub(&env, &["", "   "]);
    assert_eq!(out, vec!["PATH=/bin".to_string()]); // 空 keep 无效果
}

#[test]
fn malformed_entries_dropped() {
    let env: Vec<String> = ["NOEQUALS", "=value", "PATH=/bin", "EMPTY="]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let out = scrub(&env, &[]);
    // NOEQUALS（无 =）、=value（空名）drop；EMPTY= 名非空保留
    assert_eq!(out, vec!["PATH=/bin".to_string(), "EMPTY=".to_string()]);
}

#[test]
fn equals_in_value_retained() {
    let env: Vec<String> = ["FOO=a=b=c"].iter().map(|s| s.to_string()).collect();
    assert_eq!(scrub(&env, &[]), vec!["FOO=a=b=c".to_string()]); // 首个 = 分割，值含 = 原样
}

#[test]
fn no_mutate_input() {
    let env: Vec<String> = ["PATH=/bin", "SECRET_KEY=x"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let snapshot = env.clone();
    let _ = scrub(&env, &[]);
    assert_eq!(env, snapshot); // 输入不被修改
}

#[test]
fn withheld_env_order_not_sorted() {
    // 上游注释 "sorted" 是陈旧漂移：实际环境序（D4-9 钉死）
    let env: Vec<String> = ["B=2", "TOKEN_B=x", "A=1", "SECRET_A=y", "TOKEN_B=z"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(
        withheld(&env, &[]),
        vec!["TOKEN_B".to_string(), "SECRET_A".to_string()]
    );
}

#[test]
fn withheld_dedup_names_never_values() {
    let env: Vec<String> = ["TOKEN_B=y", "TOKEN_B=z", "PATH=/bin"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let w = withheld(&env, &[]);
    assert_eq!(w, vec!["TOKEN_B".to_string()]); // 去重
    assert!(!w.iter().any(|n| n.contains('y') || n.contains('z'))); // 绝不值
}

#[test]
#[cfg(any(unix, windows))]
fn sanitized_handles_non_unicode_environment() {
    use std::ffi::OsString;
    use std::process::Command;

    const CHILD: &str = "ARBITRATE_CHILDENV_TEST";
    if std::env::var_os(CHILD).is_some() {
        let out = sanitized(&["kept_token", "KEPT_BAD_TOKEN"]);
        let mut entries: Vec<_> = out
            .iter()
            .filter(|entry| entry.starts_with("FIXTURE_") || entry.starts_with("KEPT_"))
            .map(String::as_str)
            .collect();
        entries.sort_unstable();
        assert_eq!(entries, ["FIXTURE_PLAIN=a=b", "KEPT_TOKEN=synthetic"]);
        return;
    }

    #[cfg(unix)]
    let non_unicode = {
        use std::os::unix::ffi::OsStringExt;
        OsString::from_vec(vec![0xff])
    };
    #[cfg(windows)]
    let non_unicode = {
        use std::os::windows::ffi::OsStringExt;
        OsString::from_wide(&[0xd800])
    };
    let mut invalid_name = OsString::from("FIXTURE_");
    invalid_name.push(&non_unicode);

    let result = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "sanitized_handles_non_unicode_environment"])
        .env_clear()
        .env(CHILD, "1")
        .env("FIXTURE_PLAIN", "a=b")
        .env("FIXTURE_TOKEN", &non_unicode)
        .env("FIXTURE_SECRET", "synthetic")
        .env("FIXTURE_OTHER", &non_unicode)
        .env("KEPT_TOKEN", "synthetic")
        .env("KEPT_BAD_TOKEN", &non_unicode)
        .env(invalid_name, "synthetic")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}
