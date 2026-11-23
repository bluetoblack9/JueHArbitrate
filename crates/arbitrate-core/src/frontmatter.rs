//! Markdown-with-frontmatter 配置面（SKILL.md、agents/<name>.md）的共享解析器。
//!
//! 刻意不是 YAML 库（D2-5）：只支持实际使用的 4 种值形状，std-only 手写，
//! 与上游 internal/frontmatter 算法逐一对应。所有配置面共用这一种方言。

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrontmatterError {
    NoFrontmatter,
    Unterminated,
    EmptyAfterOpener,
}

impl std::fmt::Display for FrontmatterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoFrontmatter => write!(f, "missing frontmatter (file must start with `---`)"),
            Self::Unterminated => write!(f, "unterminated frontmatter (no closing `---`)"),
            Self::EmptyAfterOpener => write!(f, "empty after frontmatter opener"),
        }
    }
}

impl std::error::Error for FrontmatterError {}

/// split 的结果：scalar 表、list 表、markdown body。
#[derive(Debug, Clone, PartialEq)]
pub struct Split {
    pub scalars: BTreeMap<String, String>,
    pub lists: BTreeMap<String, Vec<String>>,
    pub body: String,
}

/// 解析原始文件字节。容忍 BOM 与 opener 前导空行；`---` 必须是首个非空内容。
pub fn split(data: &[u8]) -> Result<Split, FrontmatterError> {
    let text = String::from_utf8_lossy(data);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let trimmed = text.trim_start_matches(['\n', '\r', ' ']);
    if !trimmed.starts_with("---") {
        return Err(FrontmatterError::NoFrontmatter);
    }
    let lines: Vec<&str> = trimmed.split('\n').collect();
    if lines.len() < 2 {
        return Err(FrontmatterError::EmptyAfterOpener);
    }
    let close_idx = lines
        .iter()
        .skip(1)
        .position(|l| l.trim_end_matches('\r') == "---")
        .map(|p| p + 1);
    let Some(close_idx) = close_idx else {
        return Err(FrontmatterError::Unterminated);
    };
    let (scalars, lists) = parse_frontmatter(&lines[1..close_idx]);
    let body = lines[close_idx + 1..]
        .join("\n")
        .trim_start_matches('\n')
        .to_string();
    Ok(Split {
        scalars,
        lists,
        body,
    })
}

fn leading_spaces(s: &str) -> usize {
    s.bytes().take_while(|b| *b == b' ').count()
}

fn unquote(v: &str) -> &str {
    let b = v.as_bytes();
    if b.len() >= 2
        && ((b[0] == b'"' && b[b.len() - 1] == b'"') || (b[0] == b'\'' && b[b.len() - 1] == b'\''))
    {
        &v[1..v.len() - 1]
    } else {
        v
    }
}

fn split_inline_list(s: &str) -> Vec<String> {
    s.split(',')
        .map(|p| unquote(p.trim()))
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect()
}

/// block scalar：剥首个非空续行的缩进，join `\n`，剥尾部空行。
/// 返回 (值, 首个未消费行下标)。
fn read_block_scalar(lines: &[&str], from: usize) -> (String, usize) {
    let mut indent = None;
    for line in lines.iter().skip(from) {
        if line.trim().is_empty() {
            continue;
        }
        let n = leading_spaces(line);
        if n == 0 {
            break;
        }
        indent = Some(n);
        break;
    }
    let Some(indent) = indent else {
        return (String::new(), from);
    };
    let mut out: Vec<String> = Vec::new();
    let mut i = from;
    while i < lines.len() {
        let raw = lines[i].trim_end_matches('\r');
        if raw.trim().is_empty() {
            out.push(String::new());
            i += 1;
            continue;
        }
        if leading_spaces(raw) < indent {
            break;
        }
        out.push(raw[indent..].to_string());
        i += 1;
    }
    while out.last().is_some_and(|s| s.is_empty()) {
        out.pop();
    }
    (out.join("\n"), i)
}

/// block list：消费缩进 `- item` 行。返回 (项, 首个未消费行下标, 是否见到项)。
fn read_block_list(lines: &[&str], from: usize) -> (Vec<String>, usize, bool) {
    let mut items = Vec::new();
    let mut saw = false;
    let mut i = from;
    while i < lines.len() {
        let raw = lines[i].trim_end_matches('\r');
        if raw.trim().is_empty() {
            i += 1;
            continue;
        }
        if leading_spaces(raw) == 0 {
            break;
        }
        let t = raw.trim();
        if !t.starts_with("- ") && t != "-" {
            if !saw {
                return (Vec::new(), from, false);
            }
            break;
        }
        saw = true;
        let item = t.strip_prefix('-').unwrap().trim();
        items.push(unquote(item).to_string());
        i += 1;
    }
    (items, i, saw)
}

fn parse_frontmatter(lines: &[&str]) -> (BTreeMap<String, String>, BTreeMap<String, Vec<String>>) {
    let mut scalars = BTreeMap::new();
    let mut lists = BTreeMap::new();
    let mut i = 0;
    while i < lines.len() {
        let raw = lines[i].trim_end_matches('\r');
        let trim = raw.trim();
        if trim.is_empty() || trim.starts_with('#') || leading_spaces(raw) > 0 {
            i += 1;
            continue;
        }
        let Some(idx) = raw.find(':') else {
            i += 1;
            continue;
        };
        if idx == 0 {
            i += 1;
            continue;
        }
        let key = raw[..idx].trim().to_string();
        let val = raw[idx + 1..].trim();
        if val == "|" || val == "|-" {
            let (block, next) = read_block_scalar(lines, i + 1);
            scalars.insert(key, block);
            i = next;
            continue;
        }
        if val.is_empty() {
            let (items, next, ok) = read_block_list(lines, i + 1);
            if ok {
                lists.insert(key, items);
                i = next;
                continue;
            }
            scalars.insert(key, String::new());
            i += 1;
            continue;
        }
        if val.starts_with('[') && val.ends_with(']') {
            lists.insert(key, split_inline_list(&val[1..val.len() - 1]));
            i += 1;
            continue;
        }
        scalars.insert(key, unquote(val).to_string());
        i += 1;
    }
    (scalars, lists)
}
