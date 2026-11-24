//! core::projectmd — 发现并加载项目 AGENTS.md（启动时追加进 system prompt）。
//! 规格：上游 internal/projectmd（只读参考），Rust std-only 重写。
//! 约定文件名 AGENTS.md（中立，不另载 CLAUDE.md/.cursorrules，避免漂移）。
//! 向上 maxAscend=5 级，首现胜出；64KB 硬上限 + UTF-8 安全截断（约束 2 字节稳定）。
//! 路径天花板（D4-8）：std::path::absolute 不词典解析 .. / .（Go Abs 会 Clean）；
//! 生产调用方（M2）传已解析 cwd，测试用干净绝对路径。

pub const FILENAME: &str = "AGENTS.md";
const MAX_ASCEND: usize = 5;
const MAX_BYTES: usize = 64 * 1024;

/// Load 结果。path 空 = 未找到（非错误，多数项目没有）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Result {
    pub content: String,
    pub path: String,
    pub bytes: usize,
    pub truncate: bool,
}

impl Result {
    /// system-prompt-ready 块，标注来源路径。path 空 → ""（调用方可直接拼接）。
    pub fn section(&self) -> String {
        if self.path.is_empty() {
            return String::new();
        }
        format!(
            "# Project instructions (from {})\n\n{}\n",
            self.path, self.content
        )
    }
}

/// 从 start_dir 向上找 AGENTS.md。"" → cwd。缺失文件 = 常见情形（path 空，Ok）。
pub fn load(start_dir: &str) -> std::io::Result<Result> {
    let start = if start_dir.is_empty() {
        std::env::current_dir()?
    } else {
        std::path::absolute(start_dir)?
    };
    let mut dir = start;
    for _ in 0..MAX_ASCEND {
        let candidate = dir.join(FILENAME);
        match std::fs::read(&candidate) {
            Ok(data) => return Ok(shape(&candidate.to_string_lossy(), &data)),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
                // 缺失（含 ENOTDIR，镜像 Go errors.Is(fs.ErrNotExist)）→ 向上
                match dir.parent() {
                    Some(parent) if parent != dir => dir = parent.to_path_buf(),
                    _ => break, // 到文件系统根
                }
            }
            Err(e) => {
                return Err(std::io::Error::new(
                    e.kind(),
                    format!("projectmd: {}: {}", candidate.display(), e),
                ));
            }
        }
    }
    Ok(Result::default())
}

fn shape(path: &str, data: &[u8]) -> Result {
    let mut r = Result {
        path: path.to_string(),
        bytes: data.len(),
        ..Default::default()
    };
    if data.len() > MAX_BYTES {
        let mut b = &data[..MAX_BYTES];
        // drop 末字节避免劈多字节 UTF-8；循环上限 3（4-byte rune 最多 3 续字节）
        for _ in 0..3 {
            if b.is_empty() || std::str::from_utf8(b).is_ok() {
                break;
            }
            b = &b[..b.len() - 1];
        }
        let dropped = data.len() - b.len();
        r.content = format!(
            "{}\n\n…[truncated {} bytes; AGENTS.md is too large — keep it under {} KB]\n",
            String::from_utf8_lossy(b),
            dropped,
            MAX_BYTES / 1024
        );
        r.truncate = true;
    } else {
        r.content = String::from_utf8_lossy(data).into_owned();
    }
    r
}
