//! core::pricing 契约测试 — 价目卡/目录/金额（规格：上游 internal/pricing，
//! Rust 自有重写，不运行 Go）。
//!
//! 并行纪律（D3-6）：Rust 测试默认并行，全局覆盖是进程级 static。
//! 所有读 pricing_for/lookup_rate/set_model_rates 的测试取 SERIAL 守卫；
//! 时钟测试（Task 3）纯函数不读覆盖，无需守卫。

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

use arbitrate_core::pricing::{
    self, MODEL_GLM_53, MODEL_OPENCODE_V41_FLASH, MODEL_V4_PRO, MODEL_V41_FLASH, ModelPricing,
    OPENCODE_GO_MODELS, Tier, format_cost, lookup_rate, pricing_for, set_model_rates,
};
use arbitrate_core::types::Usage;

static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    // 中毒恢复：覆盖生命周期测试若 panic 会毒化守卫，但覆盖表本身可能残留——
    // 每个取锁测试开头先清覆盖，保证从干净基线开始。
    let g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    set_model_rates(BTreeMap::new());
    g
}

fn usage(miss: i64, hit: i64, completion: i64) -> Usage {
    Usage {
        prompt_tokens: miss + hit,
        completion_tokens: completion,
        total_tokens: miss + hit + completion,
        prompt_cache_hit_tokens: hit,
        prompt_cache_miss_tokens: miss,
    }
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}

#[test]
fn flash_card_peak() {
    let _g = serial();
    assert_eq!(
        pricing_for(MODEL_V41_FLASH, Tier::Standard),
        ModelPricing {
            input_miss_per_mtok: 0.30,
            input_hit_per_mtok: 0.006,
            output_per_mtok: 1.20,
            no_off_peak_discount: false
        }
    );
}

#[test]
fn v4pro_own_card() {
    let _g = serial();
    let std = pricing_for(MODEL_V4_PRO, Tier::Standard);
    assert_eq!(
        std,
        ModelPricing {
            input_miss_per_mtok: 1.32,
            input_hit_per_mtok: 0.044,
            output_per_mtok: 3.96,
            no_off_peak_discount: false
        }
    );
    let off = pricing_for(MODEL_V4_PRO, Tier::OffPeak);
    close(off.input_miss_per_mtok, 0.66);
    close(off.output_per_mtok, 1.98);
    // 不是 Flash 卡（回退会严重少计）
    assert_ne!(std, pricing_for(MODEL_V41_FLASH, Tier::Standard));
}

#[test]
fn glm53_flat_card() {
    let _g = serial();
    let std = pricing_for(MODEL_GLM_53, Tier::Standard);
    close(std.input_miss_per_mtok, 8.0 / 7.0);
    close(std.input_hit_per_mtok, 2.0 / 7.0);
    close(std.output_per_mtok, 28.0 / 7.0);
    assert!(std.no_off_peak_discount);
    // 平价卡不套峰谷折扣（回归守卫：DeepSeek 折半逻辑不得波及 flat 卡）
    assert_eq!(pricing_for(MODEL_GLM_53, Tier::OffPeak), std);
    // DeepSeek 折半仍然生效
    let flash_off = pricing_for(MODEL_V41_FLASH, Tier::OffPeak);
    close(flash_off.input_miss_per_mtok, 0.15);
}

#[test]
fn opencode_lane_card() {
    let _g = serial();
    let card = pricing_for(MODEL_OPENCODE_V41_FLASH, Tier::Standard);
    assert_eq!(
        card,
        ModelPricing {
            input_miss_per_mtok: 0.30,
            input_hit_per_mtok: 0.006,
            output_per_mtok: 1.20,
            no_off_peak_discount: true
        }
    );
    assert_eq!(pricing_for(MODEL_OPENCODE_V41_FLASH, Tier::OffPeak), card);
    // lane default 有自己的条目，不是走 Flash 回退（bool 报告存在性）
    assert!(lookup_rate(MODEL_OPENCODE_V41_FLASH).1);
}

#[test]
fn catalogue_invariants() {
    let _g = serial();
    // 与上游 TestOpenCodeGoCatalogue 同构：目录每行都有内置卡、label 短、
    // free 行 $0、非 free 行正费率、除共享直连键外全部 flat。
    let shared_with_direct_lane = [MODEL_GLM_53, MODEL_V4_PRO];
    let mut seen = std::collections::BTreeSet::new();
    let mut has_lane_default = false;
    for m in OPENCODE_GO_MODELS {
        assert!(seen.insert(m.id), "duplicate catalogue id {}", m.id);
        assert!(!m.label.is_empty(), "empty label for {}", m.id);
        assert!(
            m.label.len() <= 55,
            "label too long for {}: {}",
            m.id,
            m.label
        );
        let (card, ok) = lookup_rate(m.id);
        assert!(ok, "catalogue id {} missing rate card", m.id);
        if m.id == MODEL_OPENCODE_V41_FLASH {
            has_lane_default = true;
        }
        if m.id.ends_with("-free") {
            assert_eq!(card.input_miss_per_mtok, 0.0);
            assert_eq!(card.input_hit_per_mtok, 0.0);
            assert_eq!(card.output_per_mtok, 0.0);
            assert_eq!(
                pricing::cost(m.id, Tier::Standard, &usage(1000, 1000, 1000)),
                0.0
            );
        } else {
            assert!(card.input_miss_per_mtok > 0.0, "{} miss not positive", m.id);
            assert!(card.input_hit_per_mtok >= 0.0, "{} hit negative", m.id);
            assert!(card.output_per_mtok > 0.0, "{} output not positive", m.id);
        }
        if !shared_with_direct_lane.contains(&m.id) {
            assert!(card.no_off_peak_discount, "{} should be flat", m.id);
            assert_eq!(
                pricing_for(m.id, Tier::OffPeak),
                card,
                "{} off-peak changed flat card",
                m.id
            );
        }
    }
    assert!(has_lane_default, "lane default missing from catalogue");
}

#[test]
fn unknown_model_falls_back_to_flash() {
    let _g = serial();
    assert_eq!(
        pricing_for("deepseek-unknown", Tier::Standard),
        pricing_for(MODEL_V41_FLASH, Tier::Standard)
    );
}

#[test]
fn retired_flash_ids_billed_at_flash() {
    let _g = serial();
    // 退役 id 服务端路由到 V4.1 Flash 并按 Flash 计费——回退恰好正确，无需条目。
    for id in ["deepseek-v4-flash", "deepseek-v4-flash-vision-exp"] {
        for tier in [Tier::Standard, Tier::OffPeak] {
            assert_eq!(pricing_for(id, tier), pricing_for(MODEL_V41_FLASH, tier));
        }
    }
}

#[test]
fn offpeak_discounts_flash() {
    let _g = serial();
    let std = pricing_for(MODEL_V41_FLASH, Tier::Standard);
    let off = pricing_for(MODEL_V41_FLASH, Tier::OffPeak);
    close(off.input_miss_per_mtok, std.input_miss_per_mtok * 0.5);
    close(off.input_hit_per_mtok, std.input_hit_per_mtok * 0.5);
    close(off.output_per_mtok, std.output_per_mtok * 0.5);
}

#[test]
fn cost_typical_chat_call() {
    let _g = serial();
    // 800 miss / 200 hit / 100 completion @ Flash 峰时
    let got = pricing::cost(MODEL_V41_FLASH, Tier::Standard, &usage(800, 200, 100));
    let want = 800.0 * 0.30 / 1e6 + 200.0 * 0.006 / 1e6 + 100.0 * 1.20 / 1e6;
    close(got, want); // = 0.0003612
}

#[test]
fn cost_offpeak_is_half() {
    let _g = serial();
    let u = usage(1_000_000, 0, 1_000_000);
    let std = pricing::cost(MODEL_V41_FLASH, Tier::Standard, &u);
    let off = pricing::cost(MODEL_V41_FLASH, Tier::OffPeak, &u);
    assert!((off - std * 0.5).abs() < 1e-6);
}

#[test]
fn cost_glm53() {
    let _g = serial();
    // 100k prompt（60k hit / 40k miss）+ 5k completion
    let u = usage(40_000, 60_000, 5_000);
    let got = pricing::cost(MODEL_GLM_53, Tier::Standard, &u);
    let want =
        40_000.0 * (8.0 / 7.0) / 1e6 + 60_000.0 * (2.0 / 7.0) / 1e6 + 5_000.0 * (28.0 / 7.0) / 1e6;
    close(got, want);
    assert!(got > 0.0);
    // flat：谷时同价
    assert_eq!(pricing::cost(MODEL_GLM_53, Tier::OffPeak, &u), got);
}

#[test]
fn tier_label_strings() {
    assert_eq!(pricing::tier_label(Tier::Standard), "peak");
    assert_eq!(pricing::tier_label(Tier::OffPeak), "off-peak -50%");
}

#[test]
fn format_cost_bands() {
    // 期望串来自上游 FormatCost 规格（舍入已对 Rust 验证）
    let cases: &[(f64, &str)] = &[
        (0.0, "$0"),
        (0.001234, "$0.0012"),
        (0.0099, "$0.0099"),
        (0.01, "$0.010"),
        (0.42, "$0.420"),
        (1.5, "$1.50"),
        (9.99, "$9.99"),
    ];
    for &(usd, want) in cases {
        assert_eq!(format_cost(usd), want, "format_cost({usd})");
    }
}

#[test]
fn search_cost() {
    let _g = serial();
    close(pricing::search_cost(1), 0.00075);
    close(pricing::search_cost(1000), 0.75);
    assert_eq!(pricing::search_cost(0), 0.0);
    assert_eq!(format_cost(pricing::search_cost(1)), "$0.0008");
}

#[test]
fn asr_cost() {
    let _g = serial();
    // 计费秒向上取整：{raw seconds -> billable seconds}
    let cases: &[(f64, f64)] = &[
        (0.1, 1.0),
        (1.0, 1.0),
        (1.001, 2.0),
        (3.2, 4.0),
        (120.0, 120.0),
        (30.0, 30.0),
    ];
    for &(secs, billable) in cases {
        close(pricing::asr_cost(secs), billable * 0.116 / 3600.0);
    }
    assert_eq!(pricing::asr_cost(0.0), 0.0);
    assert_eq!(pricing::asr_cost(-3.0), 0.0);
    // 调用方传 duration_ms/1000：3200ms → 4 计费秒
    close(pricing::asr_cost(3200.0 / 1000.0), 4.0 * 0.116 / 3600.0);
}

#[test]
fn tts_cost() {
    let _g = serial();
    close(pricing::tts_cost(10_000), 0.45); // 上限 TTSMaxBillableChars=10_000（M1 deepseek 常量）
    close(pricing::tts_cost(200), 0.45 / 50.0);
    assert_eq!(pricing::tts_cost(0), 0.0);
    assert_eq!(pricing::tts_cost(-5), 0.0);
    assert_eq!(format_cost(pricing::tts_cost(200)), "$0.0090");
}

#[test]
fn parse_cost() {
    let _g = serial();
    close(pricing::parse_cost(1000), 0.225);
    close(pricing::parse_cost(1), 0.000225);
    close(pricing::parse_cost(34), 34.0 * 0.000225);
    assert_eq!(pricing::parse_cost(0), 0.0);
    assert_eq!(pricing::parse_cost(-2), 0.0);
    assert_eq!(format_cost(pricing::parse_cost(34)), "$0.0076");
}

#[test]
fn lookup_rate_absent() {
    let _g = serial();
    let (card, ok) = lookup_rate("no-such-model-plan3");
    assert!(!ok);
    assert_eq!(card, ModelPricing::default());
}

#[test]
fn set_model_rates_lifecycle() {
    let _g = serial();
    // 合成 id（不在内置表）：安装/强制 flat/负值丢弃/清除。
    let synth = "plan3-test-override";
    assert!(!lookup_rate(synth).1);
    let mut m = BTreeMap::new();
    m.insert(
        synth.to_string(),
        ModelPricing {
            input_miss_per_mtok: 1.0,
            input_hit_per_mtok: 0.1,
            output_per_mtok: 2.0,
            no_off_peak_discount: false,
        },
    );
    m.insert(
        "plan3-test-negative".to_string(),
        ModelPricing {
            input_miss_per_mtok: -1.0,
            input_hit_per_mtok: 0.0,
            output_per_mtok: 0.0,
            no_off_peak_discount: false,
        },
    );
    set_model_rates(m);
    let (card, ok) = lookup_rate(synth);
    assert!(ok);
    assert!(
        card.no_off_peak_discount,
        "installed card must be forced flat"
    );
    assert_eq!(
        pricing_for(synth, Tier::OffPeak),
        card,
        "subscription card must never halve"
    );
    assert!(
        !lookup_rate("plan3-test-negative").1,
        "negative-rate card must be dropped"
    );
    // 同键覆盖优先级（守卫内做，结束清除）：覆盖赢过内置表
    let mut m2 = BTreeMap::new();
    m2.insert(
        MODEL_V41_FLASH.to_string(),
        ModelPricing {
            input_miss_per_mtok: 9.0,
            input_hit_per_mtok: 9.0,
            output_per_mtok: 9.0,
            no_off_peak_discount: false,
        },
    );
    set_model_rates(m2);
    let (over, ok) = lookup_rate(MODEL_V41_FLASH);
    assert!(ok);
    assert_eq!(over.input_miss_per_mtok, 9.0);
    assert!(over.no_off_peak_discount);
    // 整体替换（非增量合并）：第二张表安装后，第一张表独有条目立即消失
    assert!(
        !lookup_rate(synth).1,
        "stale override survived wholesale replace"
    );
    // 空表清除 → 内置卡恢复
    set_model_rates(BTreeMap::new());
    assert!(!lookup_rate(synth).1);
    assert_eq!(
        pricing_for(MODEL_V41_FLASH, Tier::Standard).input_miss_per_mtok,
        0.30
    );
}
