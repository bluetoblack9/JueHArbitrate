//! core::diff 契约测试 — LCS unified diff，期望串全部手算（规格导出，
//! 不录制实现输出）。tie-break 逐字上游，保证字节一致。

use arbitrate_core::diff::{unified, unified_context};

#[test]
fn identical_returns_empty() {
    assert_eq!(unified("a\nb\n", "a\nb\n", "f"), "");
    assert_eq!(unified("", "", "f"), "");
}

#[test]
fn single_replace_exact() {
    // old=[a,b,c] new=[a,B,c]：eq a, del b, ins B, eq c；ctx3 单 hunk
    let want = "--- a/f\n+++ b/f\n@@ -1,3 +1,3 @@\n a\n-b\n+B\n c\n";
    assert_eq!(unified("a\nb\nc\n", "a\nB\nc\n", "f"), want);
}

#[test]
fn replace_order_pin() {
    // 纯替换 x→y：回溯优先 insert，反转后 delete-first（tie-break 钉死）
    let want = "--- a/f\n+++ b/f\n@@ -1,1 +1,1 @@\n-x\n+y\n";
    assert_eq!(unified("x\n", "y\n", "f"), want);
}

#[test]
fn insert_at_start() {
    // old=[a] new=[b,a]：ins b, eq a
    let want = "--- a/f\n+++ b/f\n@@ -1,1 +1,2 @@\n+b\n a\n";
    assert_eq!(unified("a\n", "b\na\n", "f"), want);
}

#[test]
fn ctx_zero() {
    // ctx0：窗口仅变更 ops [del b, ins B]，oldStart/newStart=2
    let want = "--- a/f\n+++ b/f\n@@ -2,1 +2,1 @@\n-b\n+B\n";
    assert_eq!(unified_context("a\nb\nc\n", "a\nB\nc\n", "f", 0), want);

    let want = "--- a/f\n+++ b/f\n@@ -1,1 +1,1 @@\n-x\n+y\n";
    assert_eq!(unified_context("x\n", "y\n", "f", usize::MAX), want);
}

#[test]
fn empty_old() {
    // old=[] new=[a]：ins a；oldStart 恒 1（上游非标准 -1,0 照搬）
    let want = "--- a/f\n+++ b/f\n@@ -1,0 +1,1 @@\n+a\n";
    assert_eq!(unified("", "a\n", "f"), want);
}

#[test]
fn empty_new() {
    let want = "--- a/f\n+++ b/f\n@@ -1,1 +1,0 @@\n-a\n";
    assert_eq!(unified("a\n", "", "f"), want);
}

#[test]
fn trailing_newline_only_diff_is_empty() {
    // "a\nb\n" vs "a\nb"：splitLines 后行序列相同 → 无变更 → ""
    assert_eq!(unified("a\nb\n", "a\nb", "f"), "");
    // "a\n\n" → [a,""]；"a\n" → [a]：末空行是真实差异
    let want = "--- a/f\n+++ b/f\n@@ -1,2 +1,1 @@\n a\n-\n";
    assert_eq!(unified("a\n\n", "a\n", "f"), want);
}

#[test]
fn two_hunks_exact() {
    // 20 行，删除 line2 与 line15（间距 13 > 2*ctx+1=7 → 两 hunk）
    let old: String = (0..20).map(|i| format!("l{i}\n")).collect();
    let new: String = (0..20)
        .filter(|i| *i != 2 && *i != 15)
        .map(|i| format!("l{i}\n"))
        .collect();
    let want = "--- a/f\n+++ b/f\n\
@@ -1,6 +1,5 @@\n l0\n l1\n-l2\n l3\n l4\n l5\n\
@@ -13,7 +12,6 @@\n l12\n l13\n l14\n-l15\n l16\n l17\n l18\n";
    assert_eq!(unified(&old, &new, "f"), want);
}

#[test]
fn max_hunks_cap() {
    // 80 行，变更 idx 0,8,16,...,72（10 处，间距 8 → 10 hunk → 截断 8）
    let old: String = (0..80).map(|i| format!("l{i}\n")).collect();
    let new: String = (0..80)
        .map(|i| {
            if i % 8 == 0 {
                format!("X{i}\n")
            } else {
                format!("l{i}\n")
            }
        })
        .collect();
    let out = unified(&old, &new, "f");
    assert_eq!(out.matches("@@ ").count(), 8, "MaxHunks=8 截断");
    assert!(out.starts_with("--- a/f\n+++ b/f\n"));
    assert!(out.contains("-l56"), "第 8 hunk（idx56）保留");
    assert!(!out.contains("-l64"), "第 9 hunk（idx64）被截断");
}
