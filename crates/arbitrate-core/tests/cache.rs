//! core::cache 契约测试 — Tracker 成本锁定/base/seal/聚合（规格：上游
//! internal/cache，Rust 自有重写，不运行 Go）。
//! 约束 3 回归：切模型/峰谷跨界不追溯改价；累计 = 各轮锁定价之和。

use arbitrate_core::cache::Tracker;
use arbitrate_core::pricing::{MODEL_V4_PRO, MODEL_V41_FLASH, Tier};
use arbitrate_core::types::Usage;

fn usage(prompt: i64, completion: i64, total: i64, hit: i64, miss: i64) -> Usage {
    Usage {
        prompt_tokens: prompt,
        completion_tokens: completion,
        total_tokens: total,
        prompt_cache_hit_tokens: hit,
        prompt_cache_miss_tokens: miss,
    }
}

fn record_flash(tr: &Tracker, u: Usage) {
    tr.record(u, MODEL_V41_FLASH, Tier::Standard);
}

#[test]
fn empty_tracker() {
    let tr = Tracker::new();
    assert_eq!(tr.cumulative(), Usage::default());
    assert_eq!(tr.hit_ratio(), 0.0);
    assert_eq!(tr.cumulative_cost(), 0.0);
    assert_eq!(tr.last(), Usage::default());
    assert_eq!(tr.last_cost(), 0.0);
    assert!(tr.turns().is_empty());
    assert_eq!(tr.summary(), "cache: no turns recorded");
}

#[test]
fn cumulative_across_turns() {
    let tr = Tracker::new();
    record_flash(&tr, usage(100, 20, 120, 0, 100));
    record_flash(&tr, usage(200, 30, 230, 150, 50));
    record_flash(&tr, usage(200, 25, 225, 180, 20));
    let c = tr.cumulative();
    assert_eq!(c.prompt_tokens, 500);
    assert_eq!(c.completion_tokens, 75);
    assert_eq!(c.prompt_cache_hit_tokens, 330);
    assert_eq!(c.prompt_cache_miss_tokens, 170);
    assert!((tr.hit_ratio() - 0.66).abs() < 1e-12);
    assert_eq!(tr.saved_tokens(), 330);
}

#[test]
fn last_is_most_recent() {
    let tr = Tracker::new();
    assert_eq!(tr.last().prompt_tokens, 0);
    record_flash(&tr, usage(100, 10, 110, 0, 100));
    record_flash(&tr, usage(200, 20, 220, 0, 200));
    record_flash(&tr, usage(50, 5, 55, 0, 50));
    assert_eq!(tr.last().prompt_tokens, 50);
    assert_eq!(tr.cumulative().prompt_tokens, 350);
}

#[test]
fn set_base_last_ignores_base() {
    let tr = Tracker::new();
    // resume 聚合：500k prompt base，不是"一轮"
    tr.set_base(
        usage(500_000, 20_000, 520_000, 0, 500_000),
        "deepseek-v4-flash",
        Tier::Standard,
    );
    assert_eq!(tr.last(), Usage::default());
    assert_eq!(tr.last_cost(), 0.0);
    assert_eq!(tr.cumulative().prompt_tokens, 500_000);
    record_flash(&tr, usage(10_000, 1_000, 11_000, 0, 10_000));
    assert_eq!(tr.last().prompt_tokens, 10_000);
    assert_eq!(tr.cumulative().prompt_tokens, 510_000);
}

#[test]
fn set_base_empty_noop() {
    let tr = Tracker::new();
    tr.set_base(Usage::default(), MODEL_V41_FLASH, Tier::Standard);
    assert_eq!(tr.cumulative(), Usage::default());
    assert_eq!(tr.last(), Usage::default());
    assert_eq!(tr.cumulative_cost(), 0.0);
}

#[test]
fn seal_folds_turns_into_base() {
    let tr = Tracker::new();
    record_flash(&tr, usage(100, 10, 110, 60, 40));
    tr.record(
        usage(500_000, 800, 500_800, 499_000, 1_000),
        MODEL_V41_FLASH,
        Tier::OffPeak,
    );
    let before_usage = tr.cumulative();
    let before_cost = tr.cumulative_cost();
    tr.seal();
    // 累计不变（token 与美元保持计数）；加法序一致，精确等值（D3-7）
    assert_eq!(tr.cumulative(), before_usage);
    assert_eq!(tr.cumulative_cost(), before_cost);
    // last/turns 清空
    assert_eq!(tr.last(), Usage::default());
    assert_eq!(tr.last_cost(), 0.0);
    assert!(tr.turns().is_empty());
    // 压缩后新轮重新起步 ctx%
    record_flash(&tr, usage(20_000, 100, 20_100, 0, 20_000));
    assert_eq!(tr.last().prompt_tokens, 20_000);
    assert_eq!(
        tr.cumulative().prompt_tokens,
        before_usage.prompt_tokens + 20_000
    );
    assert!(tr.cumulative_cost() > before_cost);
}

#[test]
fn seal_idempotent_empty_safe() {
    let tr = Tracker::new();
    tr.seal(); // 空 seal 不 panic
    record_flash(&tr, usage(5, 1, 6, 0, 5));
    tr.seal();
    tr.seal();
    assert_eq!(tr.cumulative().prompt_tokens, 5);
    assert_eq!(tr.last(), Usage::default());
}

#[test]
fn turns_returns_copy() {
    let tr = Tracker::new();
    record_flash(&tr, usage(1, 1, 2, 0, 1));
    let mut copy = tr.turns();
    copy[0].prompt_tokens = 999;
    assert_eq!(tr.cumulative().prompt_tokens, 1);
}

#[test]
fn summary_format() {
    let tr = Tracker::new();
    record_flash(&tr, usage(100, 10, 110, 50, 50));
    let s = tr.summary();
    assert!(s.contains("1 turn"), "{s}");
    assert!(s.contains("hit 50"), "{s}");
    assert!(s.contains("miss 50"), "{s}");
    assert!(s.contains("50.0%"), "{s}");
}

#[test]
fn cost_locked_at_record_model_switch() {
    // 约束 3 回归：flash→pro→退役 id，累计 = 各轮锁定价之和，不追溯改价
    let tr = Tracker::new();
    let u = usage(0, 1_000_000, 1_000_000, 0, 0);
    record_flash(&tr, u); // 1M output @ Flash 峰时 = $1.20
    assert!((tr.cumulative_cost() - 1.20).abs() < 1e-9);
    tr.record(u, MODEL_V4_PRO, Tier::Standard); // + 1M @ Pro = $3.96 → 5.16
    assert!((tr.cumulative_cost() - 5.16).abs() < 1e-9);
    tr.record(u, "deepseek-v4-flash", Tier::Standard); // 退役 id 回退 Flash +1.20 → 6.36
    assert!((tr.cumulative_cost() - 6.36).abs() < 1e-9);
}

#[test]
fn cost_locked_at_record_tier_transition() {
    // 峰谷跨界不追溯：1M 峰时 + 1M 谷时 = 1.20 + 0.60 = 1.80
    let tr = Tracker::new();
    let u = usage(0, 1_000_000, 1_000_000, 0, 0);
    record_flash(&tr, u);
    tr.record(u, MODEL_V41_FLASH, Tier::OffPeak);
    assert!((tr.cumulative_cost() - 1.80).abs() < 1e-9);
}

#[test]
fn record_flat_cost() {
    let tr = Tracker::new();
    record_flash(&tr, usage(100, 20, 120, 0, 100));
    let before_cost = tr.cumulative_cost();
    let before_last = tr.last();
    let before_cum = tr.cumulative();
    let before_last_cost = tr.last_cost();
    tr.record_flat_cost(0.0007142857); // ≈1 次 ASR 秒级调用量级的平价行项目
    tr.record_flat_cost(0.0); // 零是 no-op
    assert_eq!(tr.cumulative_cost(), before_cost + 0.0007142857);
    assert_eq!(tr.last(), before_last);
    assert_eq!(tr.cumulative(), before_cum);
    assert_eq!(tr.last_cost(), before_last_cost);
    // seal 不重数平价成本
    tr.seal();
    assert_eq!(tr.cumulative_cost(), before_cost + 0.0007142857);
}

#[test]
fn set_base_locked_preserves_cost() {
    // D3-9：锁定成本恢复——恢复前后累计成本精确一致，新轮只追加
    let tr = Tracker::new();
    record_flash(&tr, usage(0, 1_000_000, 1_000_000, 0, 0)); // $1.20 锁定
    tr.record_flat_cost(0.00075); // 平价行项目折叠进总额
    let saved_usage = tr.cumulative();
    let saved_cost = tr.cumulative_cost(); // 1.20075
    let resumed = Tracker::new();
    resumed.set_base_locked(saved_usage, saved_cost);
    assert_eq!(resumed.cumulative(), saved_usage);
    assert_eq!(resumed.cumulative_cost(), saved_cost); // 精确等值，不改价
    assert_eq!(resumed.last(), Usage::default());
    assert_eq!(resumed.last_cost(), 0.0);
    resumed.record(
        usage(0, 1_000_000, 1_000_000, 0, 0),
        MODEL_V4_PRO,
        Tier::Standard,
    );
    assert!((resumed.cumulative_cost() - (saved_cost + 3.96)).abs() < 1e-9);
}

#[test]
fn set_base_approximation_ceiling() {
    // 仅 token 近似路径（上游天花板，保留）：恢复时 model/tier 重算历史成本。
    // 对照 set_base_locked（D3-9）：历史 Flash 峰时 $1.20 按 Pro 恢复变 $3.96。
    let tr = Tracker::new();
    record_flash(&tr, usage(0, 1_000_000, 1_000_000, 0, 0));
    let saved = tr.cumulative();
    let resumed = Tracker::new();
    resumed.set_base(saved, MODEL_V4_PRO, Tier::Standard);
    assert!((resumed.cumulative_cost() - 3.96).abs() < 1e-9);
}

#[test]
fn concurrent_record_safe() {
    let tr = std::sync::Arc::new(Tracker::new());
    std::thread::scope(|s| {
        for _ in 0..100 {
            let tr = std::sync::Arc::clone(&tr);
            s.spawn(move || {
                tr.record(usage(1, 0, 1, 1, 0), MODEL_V41_FLASH, Tier::Standard);
            });
        }
    });
    assert_eq!(tr.cumulative().prompt_tokens, 100);
    assert_eq!(tr.cumulative().prompt_cache_hit_tokens, 100);
}
