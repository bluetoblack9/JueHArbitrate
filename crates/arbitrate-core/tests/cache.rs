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

// ---- Task 6: 子代理收养与并发守卫 ----

use std::sync::Arc;

fn tracker_with(u: Usage) -> Arc<Tracker> {
    let tr = Arc::new(Tracker::new());
    tr.record(u, MODEL_V41_FLASH, Tier::Standard);
    tr
}

#[test]
fn adopt_aggregates_usage_and_cost() {
    let parent = Arc::new(Tracker::new());
    parent.record(usage(100, 20, 120, 80, 20), MODEL_V41_FLASH, Tier::Standard);
    let parent_own_cost = parent.cumulative_cost();
    let a = tracker_with(usage(200, 30, 230, 150, 50));
    let b = tracker_with(usage(50, 10, 60, 0, 50));
    let a_cost = a.cumulative_cost();
    let b_cost = b.cumulative_cost();
    parent.adopt_child(&a);
    parent.adopt_child(&b);
    let c = parent.cumulative();
    assert_eq!(c.prompt_tokens, 350);
    assert_eq!(c.completion_tokens, 60);
    assert_eq!(c.prompt_cache_hit_tokens, 230);
    assert_eq!(c.prompt_cache_miss_tokens, 120);
    assert!((parent.cumulative_cost() - (parent_own_cost + a_cost + b_cost)).abs() < 1e-9);
    // last 仍是父自己的最近轮，不含 children
    assert_eq!(parent.last().prompt_tokens, 100);
}

#[test]
fn adopt_idempotent() {
    let parent = Arc::new(Tracker::new());
    let child = tracker_with(usage(100, 0, 100, 0, 100));
    parent.adopt_child(&child);
    parent.adopt_child(&child);
    parent.adopt_child(&child);
    assert_eq!(parent.cumulative().prompt_tokens, 100);
}

#[test]
fn adopt_self_noop() {
    let parent = tracker_with(usage(10, 0, 10, 0, 10));
    parent.adopt_child(&parent);
    // 不无限递归，聚合正常
    assert_eq!(parent.cumulative().prompt_tokens, 10);
}

#[test]
#[should_panic(expected = "v5 forbids spawn depth > 1")]
fn adopt_nested_panics() {
    let parent = Arc::new(Tracker::new());
    let child = Arc::new(Tracker::new());
    let grand = tracker_with(usage(1, 0, 1, 0, 1));
    child.adopt_child(&grand); // child 已有 children
    parent.adopt_child(&child); // 嵌套收养 → panic
}

#[test]
#[should_panic(expected = "double-count risk")]
fn adopt_double_count_after_resume_panics() {
    let parent = Arc::new(Tracker::new());
    // resume base 已聚合先前会话 children 的累计
    parent.set_base(
        usage(900, 100, 1000, 800, 100),
        MODEL_V41_FLASH,
        Tier::Standard,
    );
    let child = tracker_with(usage(200, 0, 200, 0, 200)); // 有已记录轮
    parent.adopt_child(&child); // → panic（G2 守卫，turns 分支）
}

#[test]
#[should_panic(expected = "double-count risk")]
fn adopt_double_count_covers_child_base() {
    let parent = Arc::new(Tracker::new());
    parent.set_base(
        usage(900, 100, 1000, 800, 100),
        MODEL_V41_FLASH,
        Tier::Standard,
    );
    let child = Arc::new(Tracker::new());
    child.set_base(usage(50, 0, 50, 0, 50), MODEL_V41_FLASH, Tier::Standard); // 无 record，但有自己的 base
    parent.adopt_child(&child); // → panic（G2 守卫，child-base 分支）
}

#[test]
fn adopt_fresh_child_after_resume_ok() {
    let parent = Arc::new(Tracker::new());
    parent.set_base(
        usage(900, 100, 1000, 800, 100),
        MODEL_V41_FLASH,
        Tier::Standard,
    );
    let child = Arc::new(Tracker::new()); // fresh：零用量
    parent.adopt_child(&child); // 不 panic
    child.record(usage(50, 0, 50, 0, 50), MODEL_V41_FLASH, Tier::Standard);
    assert_eq!(parent.cumulative().total_tokens, 1050);
}

#[test]
fn adopt_nonfresh_without_resume_ok() {
    // fresh 父无 has_base：收养已记录 child 良性（守卫 gated）
    let parent = Arc::new(Tracker::new());
    let child = tracker_with(usage(200, 0, 200, 0, 200));
    parent.adopt_child(&child);
    assert_eq!(parent.cumulative().prompt_tokens, 200);
}

#[test]
fn seal_does_not_arm_guard() {
    // seal 只折叠自己的轮，不置 has_base——合法收养流不被假 panic
    let parent = Arc::new(Tracker::new());
    parent.record(usage(100, 0, 100, 0, 100), MODEL_V41_FLASH, Tier::Standard);
    parent.seal();
    let child = tracker_with(usage(50, 0, 50, 0, 50));
    parent.adopt_child(&child); // 不 panic
    assert_eq!(parent.cumulative().total_tokens, 150);
}

#[test]
#[should_panic(expected = "itself an adopted child")]
fn adopt_then_nest_panics() {
    // 反序洞（D3-10）：先收养后嵌套——上游两次检查都过形成深度 2，
    // has_parent 归属标记拒绝：被收养的 child 不得再收养
    let parent = Arc::new(Tracker::new());
    let child = Arc::new(Tracker::new());
    let grand = tracker_with(usage(1, 0, 1, 0, 1));
    parent.adopt_child(&child); // 上游此处检查通过（洞）
    child.adopt_child(&grand); // → panic（self 是已被收养的 child）
}

#[test]
#[should_panic(expected = "already adopted by another parent")]
fn adopt_already_owned_panics() {
    // 归属唯一（D3-10）：child 属于一个父，第二父收养拒绝
    let p1 = Arc::new(Tracker::new());
    let p2 = Arc::new(Tracker::new());
    let child = tracker_with(usage(100, 0, 100, 0, 100));
    p1.adopt_child(&child);
    p2.adopt_child(&child); // → panic
}

#[test]
#[should_panic(expected = "double-count risk")]
fn adopt_double_count_after_locked_restore_panics() {
    // set_base_locked 武装与 set_base 相同的 G2 守卫（D3-9）
    let parent = Arc::new(Tracker::new());
    parent.set_base_locked(usage(900, 100, 1000, 800, 100), 0.42);
    let child = tracker_with(usage(200, 0, 200, 0, 200));
    parent.adopt_child(&child); // → panic
}

#[test]
fn adopt_mutual_concurrent_rejected() {
    // 并发互收养（D3-10）：全局收养锁下恰好一方成功，另一方 panic；无环。
    // 上游同洞：双方捕获期都见对方无 children → 互 push 成环 → 无限递归。
    use std::sync::Barrier;
    let a = Arc::new(Tracker::new());
    let b = Arc::new(Tracker::new());
    let barrier = Arc::new(Barrier::new(2));
    std::thread::scope(|s| {
        let h1 = {
            let a = Arc::clone(&a);
            let b = Arc::clone(&b);
            let barrier = Arc::clone(&barrier);
            s.spawn(move || {
                barrier.wait();
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| a.adopt_child(&b)))
            })
        };
        let h2 = {
            let a = Arc::clone(&a);
            let b = Arc::clone(&b);
            let barrier = Arc::clone(&barrier);
            s.spawn(move || {
                barrier.wait();
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| b.adopt_child(&a)))
            })
        };
        let r1 = h1.join().unwrap();
        let r2 = h2.join().unwrap();
        assert!(
            r1.is_ok() != r2.is_ok(),
            "exactly one mutual adopt must succeed"
        );
    });
    // 无环：聚合遍历终止（败方在 nested 捕获期 panic，不持 inner 锁，胜方未中毒）
    let _ = a.cumulative();
    let _ = b.cumulative();
}

#[test]
fn adopt_concurrent_spawn() {
    // 50 并行子代理：各自 record 后收养进父（spawn 风暴路径）
    let parent = Arc::new(Tracker::new());
    std::thread::scope(|s| {
        for _ in 0..50 {
            let parent = Arc::clone(&parent);
            s.spawn(move || {
                let child = Arc::new(Tracker::new());
                child.record(usage(10, 0, 10, 5, 5), MODEL_V41_FLASH, Tier::Standard);
                parent.adopt_child(&child);
            });
        }
    });
    assert_eq!(parent.cumulative().prompt_tokens, 500);
    assert_eq!(parent.cumulative().prompt_cache_hit_tokens, 250);
}

/// 死锁工作负载：读者持续 cumulative + 200 收养者 + 500 父轮记录者并发。
/// Barrier 启动同步 + done 完成计数 → 读者覆盖整个并发阶段；迭代上限
/// 只是读者次级兜底（兜不住锁内阻塞——真正的死锁保护是父测试的子进程期限）。
fn deadlock_workload() {
    use std::sync::Barrier;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let parent = Arc::new(Tracker::new());
    let workers = 201; // 200 收养者 + 1 记录者
    let barrier = Arc::new(Barrier::new(workers + 1)); // + 读者
    let done = Arc::new(AtomicUsize::new(0));
    std::thread::scope(|s| {
        let reader = {
            let parent = Arc::clone(&parent);
            let barrier = Arc::clone(&barrier);
            let done = Arc::clone(&done);
            s.spawn(move || {
                barrier.wait();
                let mut iterations = 0u64;
                while done.load(Ordering::Relaxed) < workers && iterations < 1_000_000 {
                    let _ = parent.cumulative();
                    let _ = parent.cumulative_cost();
                    iterations += 1;
                }
            })
        };
        for _ in 0..200 {
            let parent = Arc::clone(&parent);
            let barrier = Arc::clone(&barrier);
            let done = Arc::clone(&done);
            s.spawn(move || {
                barrier.wait();
                let child = Arc::new(Tracker::new());
                child.record(usage(1, 0, 1, 0, 1), MODEL_V41_FLASH, Tier::Standard);
                parent.adopt_child(&child);
                done.fetch_add(1, Ordering::Relaxed);
            });
        }
        {
            let parent = Arc::clone(&parent);
            let barrier = Arc::clone(&barrier);
            let done = Arc::clone(&done);
            s.spawn(move || {
                barrier.wait();
                for _ in 0..500 {
                    parent.record(usage(1, 0, 1, 0, 1), MODEL_V41_FLASH, Tier::Standard);
                }
                done.fetch_add(1, Ordering::Relaxed);
            });
        }
        if let Err(e) = reader.join() {
            std::panic::resume_unwind(e);
        }
    });
    // 200 child × 1 + 500 父轮 = 700 prompt（scope 结束全部 join）
    assert_eq!(parent.cumulative().prompt_tokens, 700);
}

#[test]
fn adopt_concurrent_cumulative_no_deadlock() {
    // 真正的死锁保护 = 子进程期限（D3-6）：线程内迭代上限只能兜成功返回
    // 的循环，一旦 cumulative 阻塞在 Mutex 上计数便不再增长，join/scope
    // 可永久等待。re-exec 自身测试二进制跑工作负载，父测试 try_wait 轮询
    // + 30s deadline kill+reap——锁内阻塞被外部强制终止。
    if std::env::var_os("ARBITRATE_CACHE_DEADLOCK_WORKLOAD").is_some() {
        deadlock_workload();
        return;
    }
    let exe = std::env::current_exe().unwrap();
    let mut child = std::process::Command::new(exe)
        .arg("--exact")
        .arg("adopt_concurrent_cumulative_no_deadlock")
        .env("ARBITRATE_CACHE_DEADLOCK_WORKLOAD", "1")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn test binary");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        match child.try_wait().expect("try_wait") {
            Some(status) => {
                assert!(status.success(), "deadlock workload failed: {status}");
                return;
            }
            None => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("deadlock suspected: workload exceeded 30s deadline, killed");
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }
}
