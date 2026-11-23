//! core::budget 契约测试 — 窗口表/阈值（规格：上游 internal/budget，
//! Rust 自有重写，不运行 Go）。
//!
//! 并行纪律（D3-6）：所有读 limit/known/set_limits 的测试取 SERIAL 守卫。

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

use arbitrate_core::budget::{self, DEFAULT, Severity};
use arbitrate_core::pricing::OPENCODE_GO_MODELS;

static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    let g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    budget::set_limits(BTreeMap::new());
    g
}

#[test]
fn limit_known_and_fallback() {
    let _g = serial();
    assert_eq!(budget::limit("deepseek-flash"), 1_000_000);
    assert_eq!(budget::limit("glm-5.3"), 1_000_000);
    assert_eq!(budget::limit("deepseek-v4-pro"), 1_000_000);
    // 退役/legacy id 与未知模型 → Default
    assert_eq!(budget::limit("deepseek-v4-flash"), DEFAULT);
    assert_eq!(budget::limit("deepseek-chat"), DEFAULT);
    assert_eq!(budget::limit("totally-unknown"), DEFAULT);
}

#[test]
fn opencode_catalogue_limits() {
    let _g = serial();
    // Go 目录每行都有显式窗口且 > Default——缺条目会在真实窗口 1/8 处
    // nag /compact（上游 2026-11-18 GET /v1/models limit.context 核验）。
    for m in OPENCODE_GO_MODELS {
        let (v, ok) = budget::known(m.id);
        assert!(ok, "catalogue id {} missing context limit", m.id);
        assert!(v > DEFAULT, "{} limit {} not > Default", m.id, v);
    }
}

#[test]
fn classify_boundaries() {
    let _g = serial();
    // 显式 1M 模型，钉死 >= 边界（上游测试用退役 id + 陈旧注释，不复制）
    let model = "deepseek-flash";
    assert_eq!(budget::classify(model, 0), Severity::Ok);
    assert_eq!(budget::classify(model, 599_999), Severity::Ok);
    assert_eq!(budget::classify(model, 600_000), Severity::Warn); // 恰 0.60 归 Warn
    assert_eq!(budget::classify(model, 700_000), Severity::Warn);
    assert_eq!(budget::classify(model, 749_999), Severity::Warn);
    assert_eq!(budget::classify(model, 750_000), Severity::Critical); // 恰 0.75 归 Critical
    assert_eq!(budget::classify(model, 1_000_001), Severity::Critical);
}

#[test]
fn fraction_raw_uncapped() {
    let _g = serial();
    assert_eq!(budget::fraction("deepseek-flash", 1_000_000), 1.0);
    assert_eq!(budget::fraction("totally-unknown", 0), 0.0);
    // 超出不是错误，是信号——不裁剪
    assert_eq!(budget::fraction("deepseek-flash", 2_000_000), 2.0);
}

#[test]
fn known_no_default_fallback() {
    let _g = serial();
    assert_eq!(budget::known("totally-unknown"), (0, false));
    // 真 128K 窗口模型可区分于回退（bool 而非与 Default 比较）
    assert_eq!(budget::known("gpt-4o"), (128_000, true));
}

#[test]
fn set_limits_lifecycle() {
    let _g = serial();
    let synth = "plan3-test-model";
    assert!(!budget::known(synth).1);
    let mut m = BTreeMap::new();
    m.insert(synth.to_string(), 500_000i64);
    m.insert("plan3-test-zero".to_string(), 0);
    m.insert("plan3-test-negative".to_string(), -5);
    budget::set_limits(m);
    assert_eq!(budget::known(synth), (500_000, true));
    assert!(
        !budget::known("plan3-test-zero").1,
        "zero window must be dropped"
    );
    assert!(
        !budget::known("plan3-test-negative").1,
        "negative window must be dropped"
    );
    // 同键覆盖优先级（守卫内做，结束清除）
    let mut m2 = BTreeMap::new();
    m2.insert("deepseek-flash".to_string(), 999i64);
    budget::set_limits(m2);
    assert_eq!(budget::limit("deepseek-flash"), 999);
    // 整体替换（非增量合并）：第二张表安装后，第一张表独有条目立即消失
    assert!(
        !budget::known(synth).1,
        "stale override survived wholesale replace"
    );
    // 空表清除 → 编译表恢复
    budget::set_limits(BTreeMap::new());
    assert_eq!(budget::limit("deepseek-flash"), 1_000_000);
    assert!(!budget::known(synth).1);
}
