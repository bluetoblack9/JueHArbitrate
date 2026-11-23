use std::process::Command;

const EXPECTED: [&str; 9] = [
    "arbitrate",
    "arbitrate-core",
    "arbitrate-state",
    "arbitrate-deepseek",
    "arbitrate-llm",
    "arbitrate-tools",
    "arbitrate-agent",
    "arbitrate-tui",
    "arbitrate-surfaces",
];

#[test]
fn nine_local_packages_all_0_1_0() {
    let out = Command::new(env!("CARGO"))
        .args(["metadata", "--locked", "--format-version", "1", "--no-deps"])
        .output()
        .unwrap();
    assert!(out.status.success(), "cargo metadata failed");
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let pkgs = meta["packages"].as_array().unwrap();
    assert_eq!(pkgs.len(), 9, "恰有 9 个本地 package");
    for p in pkgs {
        assert_eq!(p["version"], "0.1.0", "package {} 版本", p["name"]);
    }
    let names: Vec<&str> = pkgs.iter().map(|p| p["name"].as_str().unwrap()).collect();
    for e in EXPECTED {
        assert!(names.contains(&e), "缺 package {e}");
    }
}

#[test]
fn default_run_selects_only_the_product_entry() {
    let out = Command::new(env!("CARGO"))
        .args(["metadata", "--locked", "--format-version", "1", "--no-deps"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let root = meta["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "arbitrate")
        .unwrap();
    assert_eq!(root["default_run"], "arbitrate");
}
