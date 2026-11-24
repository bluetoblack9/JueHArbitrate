//! core::diff — 两文本间 unified diff（edit 工具预览/回报用）。
//! 规格：上游 internal/diff（只读参考），经典 O(n²) LCS DP + hunk 窗口合并。
//! tie-break 逐字照搬（D4-5），保证与上游字节一致输出。std-only。

pub const DEFAULT_CONTEXT: usize = 3;
pub const MAX_HUNKS: usize = 8;

/// unified diff，默认 ctx=3。相同 → ""。
pub fn unified(old: &str, new: &str, filename: &str) -> String {
    unified_context(old, new, filename, DEFAULT_CONTEXT)
}

/// 可配 ctx 深度。
pub fn unified_context(old: &str, new: &str, filename: &str, ctx: usize) -> String {
    if old == new {
        return String::new();
    }
    let old_lines = split_lines(old);
    let new_lines = split_lines(new);
    let ops = lcs_edits(&old_lines, &new_lines);
    let mut hunks = make_hunks(&ops, ctx);
    if hunks.is_empty() {
        return String::new();
    }
    if hunks.len() > MAX_HUNKS {
        hunks.truncate(MAX_HUNKS);
    }
    let mut out = format!("--- a/{filename}\n+++ b/{filename}\n");
    for h in &hunks {
        out.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            h.old_start, h.old_count, h.new_start, h.new_count
        ));
        for o in &h.ops {
            out.push(o.kind);
            out.push_str(&o.text);
            out.push('\n');
        }
    }
    out
}

#[derive(Clone, Debug)]
struct Op {
    kind: char, // ' ' equal, '-' delete, '+' insert
    text: String,
}

struct Hunk {
    old_start: usize,
    old_count: usize,
    new_start: usize,
    new_count: usize,
    ops: Vec<Op>,
}

/// 分行：split('\n') 后末元素 "" 则 pop（尾换行不是额外空行）。"" → 空。
fn split_lines(s: &str) -> Vec<String> {
    if s.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<String> = s.split('\n').map(|l| l.to_string()).collect();
    if lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    lines
}

/// LCS DP + 回溯（tie-break 逐字上游 D4-5）。
fn lcs_edits(old: &[String], new: &[String]) -> Vec<Op> {
    let m = old.len();
    let n = new.len();
    // dp[i][j] = LCS(old[:i], new[:j])
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in 1..=m {
        for j in 1..=n {
            if old[i - 1] == new[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else if dp[i - 1][j] >= dp[i][j - 1] {
                dp[i][j] = dp[i - 1][j];
            } else {
                dp[i][j] = dp[i][j - 1];
            }
        }
    }
    let mut ops: Vec<Op> = Vec::with_capacity(m + n);
    let mut i = m;
    let mut j = n;
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && old[i - 1] == new[j - 1] {
            ops.push(Op {
                kind: ' ',
                text: old[i - 1].clone(),
            });
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || dp[i][j - 1] >= dp[i - 1][j]) {
            ops.push(Op {
                kind: '+',
                text: new[j - 1].clone(),
            });
            j -= 1;
        } else {
            ops.push(Op {
                kind: '-',
                text: old[i - 1].clone(),
            });
            i -= 1;
        }
    }
    ops.reverse();
    ops
}

/// hunk 窗口合并（忽略上游长度参数）。
fn make_hunks(ops: &[Op], ctx: usize) -> Vec<Hunk> {
    let changed: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, o)| o.kind != ' ')
        .map(|(i, _)| i)
        .collect();
    if changed.is_empty() {
        return Vec::new();
    }
    // 窗口 [lo, hi)，重叠合并
    let mut wins: Vec<(usize, usize)> = Vec::new();
    for idx in changed {
        let lo = idx.saturating_sub(ctx);
        let hi = (idx + ctx + 1).min(ops.len());
        if let Some(last) = wins.last_mut()
            && lo <= last.1
        {
            last.1 = last.1.max(hi);
            continue;
        }
        wins.push((lo, hi));
    }
    let mut hunks = Vec::with_capacity(wins.len());
    for (lo, hi) in wins {
        let mut old_start = 1;
        let mut new_start = 1;
        for o in &ops[..lo] {
            if o.kind == ' ' || o.kind == '-' {
                old_start += 1;
            }
            if o.kind == ' ' || o.kind == '+' {
                new_start += 1;
            }
        }
        let slice = &ops[lo..hi];
        let mut old_count = 0;
        let mut new_count = 0;
        for o in slice {
            if o.kind == ' ' || o.kind == '-' {
                old_count += 1;
            }
            if o.kind == ' ' || o.kind == '+' {
                new_count += 1;
            }
        }
        hunks.push(Hunk {
            old_start,
            old_count,
            new_start,
            new_count,
            ops: slice.to_vec(),
        });
    }
    hunks
}
