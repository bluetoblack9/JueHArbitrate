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
    /// set_base/set_base_locked 已调用——武装 adopt_child 的双计数守卫。
    has_base: bool,
    /// 已被某父收养（归属标记，D3-10）——被收养的 Tracker 不得再收养，
    /// 也不得被第二个父收养。push 成功后才置位（panic 路径零状态变更）。
    has_parent: bool,
}

/// 全局收养锁（D3-10）：串行化所有 adopt_child 的"检查+建立"，维持
/// spawn 深度 1 的结构不变量。收养只在 spawn 时刻发生（冷路径），全局
/// 串行代价可忽略；record/cumulative 热路径不取此锁。
/// 中毒恢复：守卫 panic 是 bug 检测，panic 路径不改任何状态（归属标记
/// push 成功后才写），恢复锁不影响不变量。
static ADOPT_LOCK: Mutex<()> = Mutex::new(());

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
                has_base: false,
                has_parent: false,
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
        g.has_base = true;
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
        g.has_base = true;
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

    /// 注册子代理 Tracker，使其 record() 调用滚入**本** Tracker 的
    /// cumulative()/cumulative_cost()。child 继续独立接收自己的 record；
    /// 收养只改变父的聚合表面。
    ///
    /// 多 child（并行子代理）加性聚合。同一 Arc 重复收养是 no-op（指针
    /// 身份去重），spawn 重试路径可安全重调。自收养 no-op。
    ///
    /// 安全——不得收养 token 用量已折叠进本 Tracker base_usage 的 child
    /// （从盘恢复的 child，其先前轮已计入父保存的累计）。预期调用方是
    /// M4 subagent spawn 时刻：fresh child 构造上零用量。父 set_base/
    /// set_base_locked 后（resume 会话），收养有任何已记录轮或自己
    /// base_usage 的 child 会 **panic**——v0.6.0 G2 修复：bug 检测从
    /// "非正式编排器契约"升级为双计数现场运行时检查。守卫 gated on
    /// has_base：fresh（非 resume）父无重叠风险——收养已记录 child 良性
    /// 即使结构上不寻常。
    ///
    /// 安全——spawn 深度 = 1（总纲约束 6），所有权不变量（D3-10，
    /// arbitrate 自有硬化，上游同洞）：有 children 者不得被收养（上游
    /// nested 守卫）；被收养者不得再收养（has_parent）；child 不得被
    /// 第二父收养（归属唯一）。上游只查收养当下结构——先收养后嵌套、
    /// 并发互收养（成环 → cumulative 无限递归）两个洞由 ADOPT_LOCK
    /// 串行化"检查+建立" + has_parent 标记结构化关闭。agent 工具排除出
    /// subagent 注册表仍是第一道线。
    ///
    /// 并发：全局收养锁串行化所有 adopt（spawn 冷路径，代价可忽略；
    /// record/cumulative 热路径不取此锁）。三个顺序临界区 child 捕获 →
    /// 父校验+push → child 归属标记，**任何时刻不持两把 inner 锁**。
    /// "快照后释放"防死锁，不防守卫竞态——守卫一致性由 ADOPT_LOCK 提供。
    /// ADOPT_LOCK 中毒恢复：panic 路径零状态变更（标记 push 后才写），
    /// 恢复不破坏不变量。
    pub fn adopt_child(self: &Arc<Self>, child: &Arc<Self>) {
        if Arc::ptr_eq(self, child) {
            return;
        }
        let _adopt = ADOPT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // 临界区 1：child 锁内捕获结构/用量/归属，释放后才取父锁。
        let (nested, child_has_usage, child_owned) = {
            let g = child.inner.lock().unwrap();
            (
                !g.children.is_empty(),
                !g.turns.is_empty() || g.base_usage.total_tokens > 0,
                g.has_parent,
            )
        };
        if nested {
            panic!(
                "cache: AdoptChild on a Tracker that already has children — v5 forbids spawn depth > 1"
            );
        }
        // 临界区 2：父锁内校验 + push。
        let mut g = self.inner.lock().unwrap();
        if g.has_parent {
            panic!(
                "cache: AdoptChild on a Tracker that is itself an adopted child — v5 forbids spawn depth > 1"
            );
        }
        if g.children.iter().any(|c| Arc::ptr_eq(c, child)) {
            return; // 幂等重收养（spawn 重试）：child 归属已是本父
        }
        if child_owned {
            panic!(
                "cache: AdoptChild on a Tracker already adopted by another parent — v5 children belong to one parent"
            );
        }
        // G2 守卫：resume 父的 base_usage 已折叠每个先前会话 child 的
        // 累计用量。此时收养自带先前用量的 child 会经 cumulative 的
        // children 遍历双计数那些 token。指针去重先跑，record 后幂等
        // 重收养（spawn 重试路径）仍 no-op。
        if g.has_base && child_has_usage {
            panic!(
                "cache: AdoptChild on a non-fresh child after parent SetBase — double-count risk (parent's baseUsage already includes prior-session children)"
            );
        }
        g.children.push(Arc::clone(child));
        drop(g);
        // 临界区 3：push 成功后才写归属标记（panic 路径零状态变更）。
        child.inner.lock().unwrap().has_parent = true;
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
