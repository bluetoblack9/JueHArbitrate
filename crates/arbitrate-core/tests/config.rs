use arbitrate_core::config::{
    AgentConfig, Config, ProviderConfig, ReadConfig, SCHEMA_VERSION, ServeConfig, base_url_env_for,
    base_url_for_with, env_var_for, key_for_with, set_key,
};
use std::collections::BTreeMap;

fn no_env(_: &str) -> Option<String> {
    None
}

fn fake_env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
    move |name| {
        pairs
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.to_string())
    }
}

#[test]
fn schema_version_current_is_one() {
    assert_eq!(SCHEMA_VERSION, 1);
    assert_eq!(Config::default().schema_version, 1);
}

#[test]
fn read_limits_defaults_overrides_zero_fallback() {
    let mut c = Config::default();
    assert_eq!(c.read_max_limit(), 200);
    assert_eq!(c.read_whole_read_bytes(), 32 * 1024);
    c.read = Some(ReadConfig {
        max_limit: 500,
        whole_read_bytes: 4096,
    });
    assert_eq!(c.read_max_limit(), 500);
    assert_eq!(c.read_whole_read_bytes(), 4096);
    c.read = Some(ReadConfig::default());
    assert_eq!(c.read_max_limit(), 200);
    assert_eq!(c.read_whole_read_bytes(), 32 * 1024);
    c.read = Some(ReadConfig {
        max_limit: -5,
        whole_read_bytes: 0,
    });
    assert_eq!(c.read_max_limit(), 200);
}

#[test]
fn suggest_reply_default_on_explicit_off() {
    assert!(Config::default().suggest_reply_enabled());
    let mut c = Config::default();
    c.suggest_reply = Some(false);
    assert!(!c.suggest_reply_enabled());
    c.suggest_reply = Some(true);
    assert!(c.suggest_reply_enabled());
}

#[test]
fn session_notify_default_60_zero_disables() {
    assert_eq!(Config::default().session_notify_seconds_or_default(), 60);
    let mut c = Config::default();
    c.session_notify_seconds = Some(0);
    assert_eq!(c.session_notify_seconds_or_default(), 0);
    c.session_notify_seconds = Some(90);
    assert_eq!(c.session_notify_seconds_or_default(), 90);
}

#[test]
fn safety_check_only_high_opts_in() {
    for s in ["high", "HIGH", " High ", "\thigh\n"] {
        let c = Config {
            safety_check: s.to_string(),
            ..Config::default()
        };
        assert_eq!(c.safety_check_level(), "high", "input {s:?}");
    }
    for s in [
        "",
        "low",
        "medium",
        "max",
        "off",
        "true",
        "1",
        "high,low",
        "high risk",
    ] {
        let c = Config {
            safety_check: s.to_string(),
            ..Config::default()
        };
        assert_eq!(c.safety_check_level(), "", "input {s:?}");
    }
}

#[test]
fn serve_yolo_defaults_off() {
    assert!(!Config::default().serve_yolo());
    let c = Config {
        serve: Some(ServeConfig {
            max_concurrency: 4,
            ..ServeConfig::default()
        }),
        ..Config::default()
    };
    assert!(!c.serve_yolo());
    let c = Config {
        serve: Some(ServeConfig {
            yolo: true,
            ..ServeConfig::default()
        }),
        ..Config::default()
    };
    assert!(c.serve_yolo());
}

#[test]
fn env_var_mapping() {
    for (p, k) in [
        ("deepseek", "DEEPSEEK_API_KEY"),
        ("anthropic", "ANTHROPIC_API_KEY"),
        ("openai", "OPENAI_API_KEY"),
        ("gemini", "GEMINI_API_KEY"),
        ("glm", "GLM_API_KEY"),
        ("opencode", "OPENCODE_API_KEY"),
    ] {
        assert_eq!(env_var_for(p), Some(k), "provider {p}");
    }
    assert_eq!(env_var_for("my-custom"), None);
    assert_eq!(base_url_env_for("glm"), Some("GLM_BASE_URL"));
    assert_eq!(base_url_env_for("opencode"), Some("OPENCODE_BASE_URL"));
    assert_eq!(base_url_env_for("deepseek"), None);
    assert_eq!(base_url_env_for("my-custom"), None);
}

fn cfg_with_deepseek(key: &str) -> Config {
    let mut c = Config::default();
    c.providers.insert(
        "deepseek".to_string(),
        ProviderConfig {
            api_key: key.to_string(),
            base_url: String::new(),
        },
    );
    c
}

#[test]
fn key_resolution_precedence() {
    let cfg = cfg_with_deepseek("from-config");
    // env 非空胜出
    let env = fake_env(&[("DEEPSEEK_API_KEY", "from-env")]);
    assert_eq!(key_for_with(&cfg, "deepseek", &env), "from-env");
    // env 空字符串视为无 → config 回退
    let env = fake_env(&[("DEEPSEEK_API_KEY", "")]);
    assert_eq!(key_for_with(&cfg, "deepseek", &env), "from-config");
    // 无 env → config
    assert_eq!(key_for_with(&cfg, "deepseek", &no_env), "from-config");
    // 两者皆无 → ""
    assert_eq!(key_for_with(&Config::default(), "deepseek", &no_env), "");
    // config key 空 → ""
    assert_eq!(
        key_for_with(&cfg_with_deepseek(""), "deepseek", &no_env),
        ""
    );
    // 未知 provider 只查 config
    let mut c = Config::default();
    c.providers.insert(
        "my-custom".to_string(),
        ProviderConfig {
            api_key: "abc".to_string(),
            base_url: String::new(),
        },
    );
    assert_eq!(key_for_with(&c, "my-custom", &no_env), "abc");
}

#[test]
fn base_url_resolution_precedence() {
    let mut cfg = Config::default();
    cfg.providers.insert(
        "glm".to_string(),
        ProviderConfig {
            api_key: "k".to_string(),
            base_url: "https://from-config.example/v4/chat/completions".to_string(),
        },
    );
    let env = fake_env(&[(
        "GLM_BASE_URL",
        "https://from-env.example/v4/chat/completions",
    )]);
    assert_eq!(
        base_url_for_with(&cfg, "glm", &env),
        "https://from-env.example/v4/chat/completions"
    );
    assert_eq!(
        base_url_for_with(&cfg, "glm", &no_env),
        "https://from-config.example/v4/chat/completions"
    );
    // key-only block / 无 block / 未知 provider → ""
    let key_only = Config {
        providers: BTreeMap::from([(
            "glm".to_string(),
            ProviderConfig {
                api_key: "k".to_string(),
                base_url: String::new(),
            },
        )]),
        ..Config::default()
    };
    assert_eq!(base_url_for_with(&key_only, "glm", &no_env), "");
    assert_eq!(base_url_for_with(&Config::default(), "glm", &no_env), "");
    assert_eq!(
        base_url_for_with(&Config::default(), "my-custom", &no_env),
        ""
    );
}

#[test]
fn set_key_creates_map_and_preserves_base_url() {
    let mut c = Config::default();
    set_key(&mut c, "deepseek", "sk-xxx");
    assert_eq!(c.providers["deepseek"].api_key, "sk-xxx");
    c.providers.get_mut("deepseek").unwrap().base_url = "https://x.example".to_string();
    set_key(&mut c, "deepseek", "sk-yyy");
    assert_eq!(c.providers["deepseek"].api_key, "sk-yyy");
    assert_eq!(c.providers["deepseek"].base_url, "https://x.example");
}

#[test]
fn agents_section_holds_routing_knobs_only() {
    let mut c = Config::default();
    c.agents.insert(
        "explore".to_string(),
        AgentConfig {
            provider: "glm".to_string(),
            model: "glm-5.3".to_string(),
            effort: "low".to_string(),
            max_turns: 60,
        },
    );
    let e = &c.agents["explore"];
    assert_eq!(
        (
            e.provider.as_str(),
            e.model.as_str(),
            e.effort.as_str(),
            e.max_turns
        ),
        ("glm", "glm-5.3", "low", 60)
    );
}
