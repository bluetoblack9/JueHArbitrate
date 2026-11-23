use arbitrate_core::config::{
    AgentConfig, Config, ConfigError, ProviderConfig, PushWebhook, ReadConfig, SCHEMA_VERSION,
    ServeConfig, base_url_env_for, base_url_for_with, env_var_for, key_for_with, load_from,
    save_to, set_key,
};
use std::collections::BTreeMap;

mod common;
use common::TempRoot;

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

#[test]
fn missing_file_loads_default() {
    let root = TempRoot::new();
    let cfg = load_from(&root.0.join("config.json")).unwrap();
    assert_eq!(cfg, Config::default());
}

#[test]
fn save_load_round_trip() {
    let root = TempRoot::new();
    let p = root.0.join("config.json");
    let mut cfg = Config::default();
    cfg.default_provider = "deepseek".to_string();
    set_key(&mut cfg, "deepseek", "sk-deepseek-example");
    cfg.providers.insert(
        "glm".to_string(),
        ProviderConfig {
            api_key: "sk-glm-example".to_string(),
            base_url: "https://glm.example/v4/chat/completions".to_string(),
        },
    );
    cfg.suggest_reply = Some(false);
    cfg.session_notify_seconds = Some(0);
    cfg.read = Some(ReadConfig {
        max_limit: 500,
        whole_read_bytes: 4096,
    });
    cfg.language = "zh".to_string();
    cfg.safety_check = "high".to_string();
    cfg.serve = Some(ServeConfig {
        projects: vec!["/tmp/p1".to_string(), "/tmp/p2".to_string()],
        max_concurrency: 8,
        yolo: true,
        web_addr: "100.80.25.74:8790".to_string(),
    });
    cfg.agents.insert(
        "explore".to_string(),
        AgentConfig {
            provider: "glm".to_string(),
            model: "glm-5.3".to_string(),
            effort: "low".to_string(),
            max_turns: 60,
        },
    );
    save_to(&p, &cfg).unwrap();
    assert_eq!(load_from(&p).unwrap(), cfg);
    // 覆盖写：第二次 save 后无残留 tmp
    save_to(&p, &cfg).unwrap();
    let tmp_left = std::fs::read_dir(&root.0)
        .unwrap()
        .filter_map(|e| e.ok())
        .any(|e| e.file_name().to_string_lossy().ends_with(".tmp"));
    assert!(!tmp_left);
}

#[test]
fn concurrent_saves_never_expose_partial_config() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let root = TempRoot::new();
    let p = root.0.join("config.json");
    let cfgs: Vec<Config> = (0..4)
        .map(|i| Config {
            default_provider: format!("provider-{i}"),
            ..Config::default()
        })
        .collect();
    // 初始有效配置：读线程先于任何写线程启动时也必读到完整配置
    save_to(&p, &cfgs[0]).unwrap();
    let stop = AtomicBool::new(false);
    let p_ref = &p;
    let stop_ref = &stop;
    let cfgs_ref = &cfgs;
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<()>();
    std::thread::scope(|s| {
        let reader = s.spawn(move || {
            // 首读完成并信号后主线程才 spawn 写线程 → reads >= 1 确定，无调度偶发
            let got = load_from(p_ref).unwrap();
            assert!(
                cfgs_ref.contains(&got),
                "partial config exposed during writes: {got:?}"
            );
            let mut n = 1usize;
            let _ = ready_tx.send(());
            // stop 为正常出口；百万次上限仅兜底，不是唯一退出依据
            while !stop_ref.load(Ordering::Acquire) && n < 1_000_000 {
                let got = load_from(p_ref).unwrap();
                assert!(
                    cfgs_ref.contains(&got),
                    "partial config exposed during writes: {got:?}"
                );
                n += 1;
            }
            n
        });
        // 读线程首读 panic → channel 断开，recv 报错，不会挂住
        ready_rx.recv().expect("reader died before first read");
        let mut writers = Vec::new();
        for cfg in &cfgs {
            // move 复制 &Config 与 &Path 引用；不借循环绑定，不移走 p。
            // 返回 Result 而非 unwrap：失败由主线程收集，不 panic 卡死读线程。
            writers.push(s.spawn(move || -> Result<(), ConfigError> {
                for _ in 0..20 {
                    save_to(p_ref, cfg)?;
                }
                Ok(())
            }));
        }
        // 主线程收集写结果；成功或失败都通知读线程停止，再回收
        let mut failure: Option<String> = None;
        for h in writers {
            match h.join() {
                Ok(Ok(())) => {}
                Ok(Err(e)) => failure = Some(format!("save failed: {e}")),
                Err(_) => failure = Some("writer thread panicked".to_string()),
            }
        }
        stop_ref.store(true, Ordering::Release);
        let reads = match reader.join() {
            Ok(n) => n,
            // 读线程断言失败（暴露半截配置）是真实测试失败，原样传播
            Err(payload) => std::panic::resume_unwind(payload),
        };
        assert!(failure.is_none(), "{}", failure.unwrap_or_default());
        assert!(reads > 0, "reader never ran");
    });
    // 终态也必须是某个完整配置
    let final_cfg = load_from(&p).unwrap();
    assert!(
        cfgs.contains(&final_cfg),
        "partial config exposed: {final_cfg:?}"
    );
    // 唯一 tmp 命名：并发后无残留
    let leftovers: Vec<String> = std::fs::read_dir(&root.0)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "tmp leftovers: {leftovers:?}");
}

#[test]
fn preexisting_tmp_files_are_never_touched() {
    let root = TempRoot::new();
    let p = root.0.join("config.json");
    // 预置当前 pid、计数器 0..256 的同名 tmp 候选（本测试二进制保存总数远小于 256，
    // 保证 save_to 的首选名落在占用区内）。名字冲突必须换下一编号重试，
    // 绝不删除或修改非本次创建的文件。
    let mut sentinels = Vec::new();
    for n in 0..256 {
        let sp = root
            .0
            .join(format!("config.json.{}.{}.tmp", std::process::id(), n));
        std::fs::write(&sp, format!("sentinel-{n}")).unwrap();
        sentinels.push(sp);
    }
    save_to(&p, &Config::default()).unwrap();
    for (n, sp) in sentinels.iter().enumerate() {
        assert_eq!(
            std::fs::read_to_string(sp).unwrap(),
            format!("sentinel-{n}")
        );
    }
    assert_eq!(load_from(&p).unwrap(), Config::default());
}

#[test]
fn unknown_keys_ignored() {
    let root = TempRoot::new();
    let p = root.0.join("config.json");
    std::fs::write(
        &p,
        r#"{"default_provider":"deepseek","ocr":{"enabled":true},"future_key":42}"#,
    )
    .unwrap();
    let cfg = load_from(&p).unwrap();
    assert_eq!(cfg.default_provider, "deepseek");
    assert_eq!(cfg.schema_version, 1);
}

#[test]
fn schema_version_rules() {
    let root = TempRoot::new();
    let p = root.0.join("config.json");
    // 缺省 → 1
    std::fs::write(&p, "{}").unwrap();
    assert_eq!(load_from(&p).unwrap().schema_version, 1);
    // 显式 1 → OK
    std::fs::write(&p, r#"{"schema_version":1}"#).unwrap();
    assert_eq!(load_from(&p).unwrap().schema_version, 1);
    // 未来版本 / 0 → 错误
    for bad in [r#"{"schema_version":2}"#, r#"{"schema_version":0}"#] {
        std::fs::write(&p, bad).unwrap();
        assert!(
            matches!(
                load_from(&p),
                Err(ConfigError::UnsupportedSchemaVersion { .. })
            ),
            "input {bad}"
        );
    }
}

#[test]
fn corrupt_json_is_error() {
    let root = TempRoot::new();
    let p = root.0.join("config.json");
    std::fs::write(&p, "{not json").unwrap();
    assert!(matches!(load_from(&p), Err(ConfigError::Parse { .. })));
}

#[test]
fn save_creates_parent_dirs() {
    let root = TempRoot::new();
    let p = root.0.join("nested").join("deeper").join("config.json");
    save_to(&p, &Config::default()).unwrap();
    assert_eq!(load_from(&p).unwrap(), Config::default());
}

#[cfg(unix)]
#[test]
fn unix_file_0600_parent_0700() {
    use std::os::unix::fs::PermissionsExt;
    let root = TempRoot::new();
    let dir = root.0.join("cfgdir");
    let p = dir.join("config.json");
    save_to(&p, &Config::default()).unwrap();
    let file_mode = std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
    assert_eq!(file_mode, 0o600);
    let dir_mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
    assert_eq!(dir_mode, 0o700);
}

/// D12 类型化配置样例：全字段 Config 序列化字节精确（字段顺序 = 声明顺序，
/// map 按键排序，2 空格缩进，末尾换行）。此样例即 arbitrate config.json 的格式契约。
#[test]
fn typed_config_sample_byte_exact() {
    let root = TempRoot::new();
    let p = root.0.join("config.json");
    let mut cfg = Config::default();
    cfg.default_provider = "deepseek".to_string();
    cfg.providers.insert(
        "deepseek".to_string(),
        ProviderConfig {
            api_key: "sk-deepseek-example".to_string(),
            base_url: String::new(),
        },
    );
    cfg.providers.insert(
        "glm".to_string(),
        ProviderConfig {
            api_key: "sk-glm-example".to_string(),
            base_url: "https://glm.example/v4/chat/completions".to_string(),
        },
    );
    cfg.suggest_reply = Some(false);
    cfg.push_webhooks = vec![PushWebhook {
        url: "https://ntfy.example/topic".to_string(),
        format: "ntfy".to_string(),
        events: vec!["session_done".to_string()],
        ..PushWebhook::default()
    }];
    cfg.session_notify_seconds = Some(0);
    cfg.read = Some(ReadConfig {
        max_limit: 500,
        whole_read_bytes: 4096,
    });
    cfg.language = "zh".to_string();
    cfg.safety_check = "high".to_string();
    cfg.serve = Some(ServeConfig {
        projects: vec!["/tmp/p1".to_string(), "/tmp/p2".to_string()],
        max_concurrency: 8,
        yolo: true,
        web_addr: "100.80.25.74:8790".to_string(),
    });
    cfg.agents.insert(
        "explore".to_string(),
        AgentConfig {
            provider: "glm".to_string(),
            model: "glm-5.3".to_string(),
            effort: "low".to_string(),
            max_turns: 60,
        },
    );
    save_to(&p, &cfg).unwrap();
    let expected = r#"{
  "schema_version": 1,
  "default_provider": "deepseek",
  "providers": {
    "deepseek": {
      "api_key": "sk-deepseek-example"
    },
    "glm": {
      "api_key": "sk-glm-example",
      "base_url": "https://glm.example/v4/chat/completions"
    }
  },
  "suggest_reply": false,
  "push_webhooks": [
    {
      "url": "https://ntfy.example/topic",
      "format": "ntfy",
      "events": [
        "session_done"
      ]
    }
  ],
  "session_notify_seconds": 0,
  "read": {
    "max_limit": 500,
    "whole_read_bytes": 4096
  },
  "language": "zh",
  "safety_check": "high",
  "serve": {
    "projects": [
      "/tmp/p1",
      "/tmp/p2"
    ],
    "max_concurrency": 8,
    "yolo": true,
    "web_addr": "100.80.25.74:8790"
  },
  "agents": {
    "explore": {
      "provider": "glm",
      "model": "glm-5.3",
      "effort": "low",
      "max_turns": 60
    }
  }
}
"#;
    assert_eq!(std::fs::read_to_string(&p).unwrap(), expected);
    // 样例回读等价
    assert_eq!(load_from(&p).unwrap(), cfg);
}
