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
    let env: Vec<String> = ["B=2", "SECRET_A=x", "A=1", "TOKEN_B=y", "SECRET_A=z"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(
        withheld(&env, &[]),
        vec!["SECRET_A".to_string(), "TOKEN_B".to_string()]
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
fn sanitized_invariants() {
    // 薄包装读真实 env：只断言普适不变量，不录/不依赖具体凭据（D4-9）
    let out = sanitized(&[]);
    let input: Vec<String> = std::env::vars().map(|(k, v)| format!("{k}={v}")).collect();
    for e in &out {
        let eq = e.find('=').expect("sanitized 条目必含 =");
        assert!(eq > 0, "空名不得保留");
        assert!(!is_sensitive(&e[..eq]), "敏感名不得泄漏: {}", &e[..eq]);
    }
    // 顺序保持：out 是 input 的子序列
    let mut it = input.iter();
    for e in &out {
        assert!(it.any(|i| i == e), "sanitized 必须保持 env 序");
    }
}
