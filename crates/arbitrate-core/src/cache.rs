//! 跨代理会话追踪 DeepSeek 前缀缓存用量。主要优化表面——落在同一稳定
//! 前缀的每轮按缓存命中率（约 miss 费率的 5%）付输入 token，会话命中率
//! 直接映射省下的美元。
//!
//! Tracker 还在 record 时刻锁定**每轮成本**：cost 用该轮结算时生效的
//! (model, tier) 计算一次，渲染时绝不从累计 token 重推。因为：
//!
//! - 会话中切模型（如 flash 跑了一段后 /model deepseek-v4-pro）否则会
//!   把所有先前轮的 token 按新模型费率改价。Pro 约 4× Flash，显示累计
//!   会追溯跳 3× 尽管 DeepSeek 早已按低费率计了那些 token。
//! - 峰谷 tier 跨界（北京 09:00/12:00/14:00/18:00 边界）同理：显示累计
//!   在边界折半/翻倍，实际账单不动。
//!
//! 生产从 agent TurnEnd 事件接线（M2+）；传刚完成轮生效的模型名与计价 tier。

use std::sync::{Arc, Mutex};

use crate::pricing::{self, Tier};
use crate::types::{Usage, format_hit_ratio};

/// 每轮数据。cost 在 record 时预计算，之后绝不重推（见模块注释）。
/// 上游 turnRecord 还存 Model/Tier，但存而 never read——Rust dead_code
/// lint 禁止死字段，只存 usage+cost；M2 会话导出需要时再加（D3-5）。
struct TurnRecord {
    usage: Usage,
    cost: f64,
}

struct Inner {
    turns: Vec<TurnRecord>,
    /// 先前会话累计（set_base 载入）；只计入 cumulative。
    base_usage: Usage,
    /// base_usage 的美元成本。
    base_cost: f64,
    /// 平价每调用成本（Search API）——只计成本，从不是一轮。
    extra_cost: f64,
    /// 收养的子代理 Tracker（Task 6 adopt_child）。
    children: Vec<Arc<Tracker>>,
}

/// 累积每轮 Usage+cost 记录并计算会话级聚合。Record/快照调用并发安全
/// （Mutex 内部可变，&self 方法）。
///
/// base_usage/base_cost 持 resume 会话累计（set_base 载入）：计入
/// cumulative()/cumulative_cost() 但**排除**出 last()/last_cost()——
/// 否则把 resume 聚合当"一轮"记录会让 last 返回累计和、状态栏 ctx%
/// 超 100%（"累计 prompt token 无意义"）。
///
/// children 承载收养的子代理 Tracker：父在 cumulative()/cumulative_cost()
/// 遍历它们，会话显示总账含每个活跃/完成子代理的用量。children **不**
/// 影响 last()/last_cost()——状态栏 ctx% 压的是父自己的上下文窗口，
/// 不是 child 的独立窗口。
pub struct Tracker {
    inner: Mutex<Inner>,
}

impl Default for Tracker {
    fn default() -> Self {
        Self::new()
    }
}

impl Tracker {
    /// 空 Tracker。共享用 `Arc::new(Tracker::new())`。
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                turns: Vec::new(),
                base_usage: Usage::default(),
                base_cost: 0.0,
                extra_cost: 0.0,
                children: Vec::new(),
            }),
        }
    }

    /// 载入先前会话累计用量（--resume/--continue）。用量及其估算成本
    /// 计入 cumulative()/cumulative_cost()，排除出 last()/last_cost()。
    /// 须在任何 record 前调用——实践中启动时 TUI 开始前。
    ///
    /// model/tier 是 resume 时生效值；会话跨多模型或 tier 变迁时成本是近似
    /// （仅 token 可用时的近似路径；锁定成本恢复走 set_base_locked，D3-9）。
    pub fn set_base(&self, u: Usage, model: &str, tier: Tier) {
        let cost = pricing::cost(model, tier, &u);
        let mut g = self.inner.lock().unwrap();
        g.base_usage = u;
        g.base_cost = cost;
    }

    /// 以**锁定的累计成本**恢复 base（arbitrate 自有契约扩展，D3-9）。
    /// locked_cost 是持久化的 cumulative_cost() 总额（含平价 API 成本）：
    /// 恢复后累计成本与保存前精确一致，不随恢复时 model/tier 改价，新轮
    /// 只追加——约束 3 闭环到恢复侧。M2 持久化应保存 cumulative() +
    /// cumulative_cost() 并经此入口恢复；平价成本折叠进总额恢复
    /// （extra_cost 无独立读方，无需单独入口）。仅 token 可用时才走
    /// set_base 近似路径。武装与 set_base 相同的收养守卫（has_base，
    /// Task 6 引入）。
    pub fn set_base_locked(&self, u: Usage, locked_cost: f64) {
        let mut g = self.inner.lock().unwrap();
        g.base_usage = u;
        g.base_cost = locked_cost;
    }

    /// 把所有已记录轮折叠进仅累计 base：cumulative()/cumulative_cost()
    /// 不变（token 与美元保持计数），last()/last_cost() 清空直到下次 record。
    ///
    /// /compact 用。压缩前轮——含刚重读全历史的摘要调用——属于会话总账，
    /// 但不喂状态栏 ctx%（应在压缩后的小上下文上重新起步）。没有 seal，
    /// ctx% 会停在摘要器的全历史读数直到下一轮落地。
    ///
    /// 刻意不置 resume 标记（has_base，Task 6 引入）：其 adopt_child 守卫
    /// 针对 resume 时 base（已聚合先前会话 children）；进程内 seal 只折叠
    /// 本 Tracker 自己的轮（收养 children 留在 children 继续被遍历），
    /// 守卫前提不成立，置位会把合法收养流变成假 panic。后续 resume 经
    /// set_base/set_base_locked 正常武装守卫。
    pub fn seal(&self) {
        let mut g = self.inner.lock().unwrap();
        let turns = std::mem::take(&mut g.turns);
        for r in turns {
            g.base_usage += r.usage;
            g.base_cost += r.cost;
        }
    }

    /// 追加一轮。cost 此刻用 (model, tier) 锁定；同 usage 日后在不同
    /// model/tier 重放**不**改价。
    ///
    /// model 是该轮运行时生效的模型 id；tier 是当时的计价 tier（通常
    /// pricing::current_tier(当前秒)）。unknown 模型名按 pricing_for
    /// 回退 Flash 费率。
    pub fn record(&self, u: Usage, model: &str, tier: Tier) {
        let cost = pricing::cost(model, tier, &u);
        let mut g = self.inner.lock().unwrap();
        g.turns.push(TurnRecord { usage: u, cost });
    }

    /// 所有已记录轮 + set_base 载入 base + 每个收养子代理 Tracker 的
    /// cumulative 之和（Task 6 起 children 非空；传递遍历，但 spawn
    /// 深度 >1 被禁）。cost 不可从此聚合派生——用 cumulative_cost。
    ///
    /// 并发：锁内快照 children 切片，递归进每个 child 的 cumulative
    /// **之前**释放锁。持锁递归会让父的并发 record 调用方阻塞整个 child
    /// 遍历；快照只在热路径付一次切片拷贝。
    pub fn cumulative(&self) -> Usage {
        let (mut sum, children) = {
            let g = self.inner.lock().unwrap();
            let mut s = g.base_usage;
            for r in &g.turns {
                s += r.usage;
            }
            (s, g.children.clone())
        };
        for c in children {
            sum += c.cumulative();
        }
        sum
    }

    /// 给会话总账加平价 USD 金额，不造 token 轮——Search API 按成功调用
    /// 计费（pricing::search_cost），无 Usage 可 record。cumulative()/
    /// last()/last_cost() 不动，ctx% 与每轮 footer 保持 token 纯净；
    /// 只有 cumulative_cost() 增长。
    ///
    /// 仅内存：token-only 近似恢复路径（set_base）仍丢失平价成本（上游
    /// 天花板）；锁定恢复路径（set_base_locked，D3-9）经持久化
    /// cumulative_cost() 总额保留——平价成本折叠进总额，M2 接线归 M2。
    pub fn record_flat_cost(&self, usd: f64) {
        if usd == 0.0 {
            return;
        }
        let mut g = self.inner.lock().unwrap();
        g.extra_cost += usd;
    }

    /// 每轮成本之和（USD）：含 set_base base 成本、record_flat_cost 平价
    /// 成本、每个收养子代理 Tracker 的 cumulative_cost。每轮按记录时生效
    /// 的 model+tier 计价——切模型或跨 tier 边界不追溯改价。children 在
    /// 自己的遍历内独立适用同规则。
    ///
    /// 并发：同 cumulative 的快照后释放模式。加法序 (base+extra) 起步
    /// 逐轮累加，与上游左结合一致（D3-7）。
    pub fn cumulative_cost(&self) -> f64 {
        let (mut sum, children) = {
            let g = self.inner.lock().unwrap();
            let mut s = g.base_cost + g.extra_cost;
            for r in &g.turns {
                s += r.cost;
            }
            (s, g.children.clone())
        };
        for c in children {
            sum += c.cumulative_cost();
        }
        sum
    }

    /// 最近记录轮的 Usage；无轮则零值。预算指示器用：显示当前上下文
    /// 窗口占用（最近轮 prompt 大小），不是跨轮累计和。
    pub fn last(&self) -> Usage {
        let g = self.inner.lock().unwrap();
        g.turns.last().map(|r| r.usage).unwrap_or_default()
    }

    /// 最近轮的锁定美元成本；无轮则 0。与 last() 配对：每轮 footer
    /// 渲染不必按当前 model/tier 经 pricing::cost 重算——record 与
    /// render 之间 /model 切换会错价。
    pub fn last_cost(&self) -> f64 {
        let g = self.inner.lock().unwrap();
        g.turns.last().map(|r| r.cost).unwrap_or(0.0)
    }

    /// 每轮 Usage 历史的拷贝。
    pub fn turns(&self) -> Vec<Usage> {
        let g = self.inner.lock().unwrap();
        g.turns.iter().map(|r| r.usage).collect()
    }

    /// 会话级前缀缓存命中率（0..1）。
    pub fn hit_ratio(&self) -> f64 {
        self.cumulative().hit_ratio()
    }

    /// 估算付了便宜费率而非贵费率的输入 token。等价于
    /// prompt_cache_hit_tokens——这些 token 否则按缓存 miss 费率计费。
    pub fn saved_tokens(&self) -> i64 {
        self.cumulative().prompt_cache_hit_tokens
    }

    /// CLI stats footer 的文本报告。布局刻意固定宽度，同文本可最少
    /// 调整送 TUI 状态栏。轮数只数本 Tracker 的 turns；聚合数字含
    /// base+children。
    pub fn summary(&self) -> String {
        let c = self.cumulative();
        let turns = self.inner.lock().unwrap().turns.len();
        if turns == 0 {
            return "cache: no turns recorded".to_string();
        }
        format!(
            "cache: {} turn(s); prompt {} (hit {} / miss {}, ratio {}); completion {}; total {}",
            turns,
            c.prompt_tokens,
            c.prompt_cache_hit_tokens,
            c.prompt_cache_miss_tokens,
            format_hit_ratio(&c),
            c.completion_tokens,
            c.total_tokens
        )
    }
}
