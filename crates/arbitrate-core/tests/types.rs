//! core::types 契约测试 — Usage 领域类型（规格：上游 pkg/deepseek Usage/HitRatio/FormatHitRatio，
//! Rust 自有重写，不运行 Go）。

use arbitrate_core::types::{Usage, format_hit_ratio};

#[test]
fn default_is_zero() {
    let u = Usage::default();
    assert_eq!(u.prompt_tokens, 0);
    assert_eq!(u.completion_tokens, 0);
    assert_eq!(u.total_tokens, 0);
    assert_eq!(u.prompt_cache_hit_tokens, 0);
    assert_eq!(u.prompt_cache_miss_tokens, 0);
}

#[test]
fn hit_ratio_zero_without_cache_tokens() {
    assert_eq!(Usage::default().hit_ratio(), 0.0);
    // 只有 prompt/completion、无缓存记账 → 分母 0 → 0（不是 NaN）
    let u = Usage {
        prompt_tokens: 100,
        completion_tokens: 20,
        total_tokens: 120,
        ..Default::default()
    };
    assert_eq!(u.hit_ratio(), 0.0);
}

#[test]
fn hit_ratio_mixed() {
    let u = Usage {
        prompt_cache_hit_tokens: 330,
        prompt_cache_miss_tokens: 170,
        ..Default::default()
    };
    assert!((u.hit_ratio() - 0.66).abs() < 1e-12);
}

#[test]
fn hit_ratio_all_hit() {
    let u = Usage {
        prompt_cache_hit_tokens: 100,
        ..Default::default()
    };
    assert_eq!(u.hit_ratio(), 1.0);
}

#[test]
fn format_hit_ratio_na() {
    assert_eq!(format_hit_ratio(&Usage::default()), "n/a");
}

#[test]
fn format_hit_ratio_percent() {
    let half = Usage {
        prompt_cache_hit_tokens: 50,
        prompt_cache_miss_tokens: 50,
        ..Default::default()
    };
    assert_eq!(format_hit_ratio(&half), "50.0%");
    let u = Usage {
        prompt_cache_hit_tokens: 732,
        prompt_cache_miss_tokens: 268,
        ..Default::default()
    };
    assert_eq!(format_hit_ratio(&u), "73.2%");
}

#[test]
fn serde_wire_field_names() {
    // wire JSON 名是协议标识符（约束 10 保留）；这是 API 契约钉死，不是 Go 磁盘兼容。
    let u = Usage {
        prompt_tokens: 1,
        completion_tokens: 2,
        total_tokens: 3,
        prompt_cache_hit_tokens: 4,
        prompt_cache_miss_tokens: 5,
    };
    let v = serde_json::to_value(u).unwrap();
    assert_eq!(
        v,
        serde_json::json!({
            "prompt_tokens": 1,
            "completion_tokens": 2,
            "total_tokens": 3,
            "prompt_cache_hit_tokens": 4,
            "prompt_cache_miss_tokens": 5
        })
    );
    let back: Usage = serde_json::from_value(v).unwrap();
    assert_eq!(back, u);
}

#[test]
fn add_assign_sums_all_fields() {
    let mut a = Usage {
        prompt_tokens: 100,
        completion_tokens: 10,
        total_tokens: 110,
        prompt_cache_hit_tokens: 60,
        prompt_cache_miss_tokens: 40,
    };
    let b = Usage {
        prompt_tokens: 200,
        completion_tokens: 30,
        total_tokens: 230,
        prompt_cache_hit_tokens: 150,
        prompt_cache_miss_tokens: 50,
    };
    a += b;
    assert_eq!(
        a,
        Usage {
            prompt_tokens: 300,
            completion_tokens: 40,
            total_tokens: 340,
            prompt_cache_hit_tokens: 210,
            prompt_cache_miss_tokens: 90
        }
    );
}
