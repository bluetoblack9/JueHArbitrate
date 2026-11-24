//! core::childenv — arbitrate 派生子进程的环境构造（凭据泄漏防护）。
//! 规格：上游 internal/childenv（只读参考），Rust std-only 重写。
//! 名称匹配（非值匹配）：值嗅探既误报断构建又漏报泄漏，且需把每个秘密读进扫描器。
//! 名称匹配粗但可预测——可预测性让逃生舱（keep）可用。安全语义不简化（约束 6）。
//! SEEK_* 命名空间保留（wire 级契约）；默认数据路径已适配 arbitrate（paths）。

use std::collections::HashSet;

/// 大写名称 substring 匹配。故意不含 AUTH（SSH_AUTH_SOCK 是 socket 路径非凭据，
/// scrub 它会断 ssh git push）；不含 USER/ACCOUNT（身份非授权）。
const SENSITIVE_SUBSTRINGS: &[&str] =
    &["KEY", "SECRET", "TOKEN", "PASSWORD", "PASSWD", "CREDENTIAL"];
/// 大写名称 prefix 匹配。SEEK_ 是产品配置命名空间，非秘密但不应静默 steer 嵌套进程。
const SENSITIVE_PREFIXES: &[&str] = &["SEEK_"];

/// 名称是否应从子进程环境隐式 withhold。大小写不敏感；空名非敏感。
pub fn is_sensitive(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let up = name.to_uppercase();
    SENSITIVE_PREFIXES.iter().any(|p| up.starts_with(p))
        || SENSITIVE_SUBSTRINGS.iter().any(|s| up.contains(s))
}

fn keep_names(keep: &[&str]) -> HashSet<String> {
    keep.iter()
        .map(|k| k.trim().to_uppercase())
        .filter(|k| !k.is_empty())
        .collect()
}

fn keep_name(name: &str, keep: &HashSet<String>) -> bool {
    keep.contains(&name.to_uppercase()) || !is_sensitive(name)
}

/// 移除敏感条目，保持原序、其余字节原样。keep 精确（大小写不敏感，无 substring/glob）
/// 保留——模糊逃生舱会静默重泄漏邻居。无 '=' 或空名（"=value"）条目 drop。
pub fn scrub(env: &[String], keep: &[&str]) -> Vec<String> {
    let keep_set = keep_names(keep);
    let mut out = Vec::with_capacity(env.len());
    for e in env {
        let eq = match e.find('=') {
            Some(i) if i > 0 => i,
            _ => continue, // 无 = 或空名
        };
        let name = &e[..eq];
        if keep_name(name, &keep_set) {
            out.push(e.clone());
        }
    }
    out
}

/// Build a scrubbed environment for a child process, preserving environment order.
/// Names are filtered before their values are converted. Entries with non-Unicode
/// names or values are omitted because this API returns strings.
///
/// Call [std::process::Command::env_clear] before applying the returned name/value
/// pairs, then apply explicit overrides.
pub fn sanitized(keep: &[&str]) -> Vec<String> {
    let keep_set = keep_names(keep);
    std::env::vars_os()
        .filter_map(|(name, value)| {
            let name = name.to_str()?;
            if name.is_empty() || name.contains('=') || !keep_name(name, &keep_set) {
                return None;
            }
            let value = value.to_str()?;
            Some(format!("{name}={value}"))
        })
        .collect()
}

/// Scrub 会丢弃的名称（绝不值），去重，**环境序**（上游注释 "sorted" 是漂移，D4-9）。
/// 诊断用（doctor/debug 日志），结果可安全打印。
pub fn withheld(env: &[String], keep: &[&str]) -> Vec<String> {
    let kept: HashSet<String> = scrub(env, keep)
        .iter()
        .filter_map(|e| e.find('=').filter(|i| *i > 0).map(|i| e[..i].to_string()))
        .collect();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for e in env {
        let eq = match e.find('=') {
            Some(i) if i > 0 => i,
            _ => continue,
        };
        let name = &e[..eq];
        if kept.contains(name) || seen.contains(name) {
            continue;
        }
        seen.insert(name.to_string());
        out.push(name.to_string());
    }
    out
}
