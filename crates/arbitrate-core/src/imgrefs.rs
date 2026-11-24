//! core::imgrefs — 用户输入中图片文件引用检测。
//! 规格：上游 internal/imgrefs（只读参考），Rust std-only 重写。
//! stat 门是承重正确性守卫：代码/URL/错误消息里的 ".png" 子串不是文件，
//! 不得触发图片附件。cleanToken 与 filerefs 故意重复（上游 D-F2 裁决，不共享）。

/// Go filepath.Ext 镜像（D4-6）：末元素最后点起含点后缀（含位置 0 dotfile）。
/// '/' 与 '\\' 均视为分隔符（跨平台确定；Go-on-Linux 仅对含反斜杠 exotic token 有别）。
fn go_ext(p: &str) -> &str {
    let start = p.rfind(['/', '\\']).map(|i| i + 1).unwrap_or(0);
    let elem = &p[start..];
    match elem.rfind('.') {
        Some(i) => &elem[i..],
        None => "",
    }
}

fn is_image_ext(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        ".png" | ".jpg" | ".jpeg" | ".webp" | ".tiff" | ".bmp" | ".heic" | ".gif"
    )
}

/// 剥 @ 前缀、两端引号/反引号/括号、尾句标点——循环至稳定。
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

/// 有序去重返回 text 中引用现存 regular 图片文件的 token。@ 前缀剥离。
pub fn detect(text: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for tok in text.split_whitespace() {
        let p = clean_token(tok);
        if p.is_empty() || seen.contains(&p) {
            continue;
        }
        if !is_image_ext(go_ext(&p)) {
            continue;
        }
        // stat 门：不是真实 regular 文件 → 不是图片引用
        if !std::fs::metadata(&p).map(|m| m.is_file()).unwrap_or(false) {
            continue;
        }
        seen.insert(p.clone());
        out.push(p);
    }
    out
}
