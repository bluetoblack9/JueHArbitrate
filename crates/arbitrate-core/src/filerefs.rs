//! core::filerefs — 文档引用检测（PDF/Word/PPT，DeepSeek 文件输入管线）。
//! 规格：上游 internal/filerefs（只读参考），Rust std-only 重写。
//! 与 imgrefs 同型（同 stat 门）；cleanToken/go_ext 故意重复（上游 D-F2：
//! 两包检测语义已分歧，保持独立，不重构成共享助手）。
//! rich: 前缀 = 每文件用途升级（D-F1），stat 门控。

/// Go filepath.Ext 镜像（D4-6）：末元素最后点起含点后缀（含位置 0 dotfile）。
/// 两种斜杠均视为分隔符。Go 在 Linux 上只将 `/` 视为分隔符。
fn go_ext(p: &str) -> &str {
    let start = p.rfind(['/', '\\']).map(|i| i + 1).unwrap_or(0);
    let elem = &p[start..];
    match elem.rfind('.') {
        Some(i) => &elem[i..],
        None => "",
    }
}

fn is_doc_ext(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        ".pdf" | ".doc" | ".docx" | ".ppt" | ".pptx"
    )
}

const RICH_PREFIX: &str = "rich:";

/// 一个检测到的文档引用，携带用户请求的用途（fileroute 路由输入）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ref {
    pub path: String,
    pub rich: bool,
}

/// cleanToken（与 imgrefs 独立重复，上游 D-F2 裁决）。
fn clean_token(mut tok: &str) -> String {
    loop {
        let prev = tok;
        tok = tok.trim_matches(['"', '\'', '`', '(', ')', '[', ']', '<', '>']);
        tok = tok.trim_end_matches(['.', ',', ';', ':', '!', '?']);
        tok = tok.strip_prefix('@').unwrap_or(tok);
        if tok == prev {
            return tok.to_string();
        }
    }
}

/// is_doc = 扩展名表 + stat 谓词（注入）。
fn is_doc_with(p: &str, is_regular: &impl Fn(&str) -> bool) -> bool {
    is_doc_ext(go_ext(p)) && is_regular(p)
}

/// 有序去重（按 path）返回文档引用。首现 Rich 标志胜出。
/// stat 谓词注入（D4-7）：生产用真实 fs，测试可注入替身（替代上游 Chdir）。
pub fn detect_refs_with(text: &str, is_regular: impl Fn(&str) -> bool) -> Vec<Ref> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for tok in text.split_whitespace() {
        let mut p = clean_token(tok);
        if p.is_empty() {
            continue;
        }
        let mut rich = false;
        // "rich:" 仅当余下是真实文档时绑定；字面名 "rich:x.pdf" 经 verbatim 回退保留
        if let Some(rest) = p.strip_prefix(RICH_PREFIX)
            && is_doc_with(rest, &is_regular)
        {
            p = rest.to_string();
            rich = true;
        }
        if seen.contains(&p) || !is_doc_with(&p, &is_regular) {
            continue;
        }
        seen.insert(p.clone());
        out.push(Ref { path: p, rich });
    }
    out
}

/// 真实 fs stat 门。
pub fn detect_refs(text: &str) -> Vec<Ref> {
    detect_refs_with(text, |p| {
        std::fs::metadata(p).map(|m| m.is_file()).unwrap_or(false)
    })
}

/// DetectRefs 的 path 投影（F2 表面，不关心用途的调用方）。
pub fn detect(text: &str) -> Vec<String> {
    detect_refs(text).into_iter().map(|r| r.path).collect()
}
