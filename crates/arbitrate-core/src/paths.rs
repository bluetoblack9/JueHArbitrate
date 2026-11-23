//! 用户数据目录布局唯一事实源。
//!
//! 根解析：$SEEK_HOME 非空 → 原样；否则 OS home + `.arbitrate`。
//! 无 `~/.seek` 回退；不读 XDG。所有方法只计算路径，不创建目录。

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// 根覆盖环境变量名（沿用 SEEK_* 命名约定）。
pub const ENV_HOME: &str = "SEEK_HOME";
/// 默认根目录名 / 项目级目录名。
pub const ROOT_DIR_NAME: &str = ".arbitrate";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathsError {
    /// OS home 不可得且无 SEEK_HOME 覆盖。
    HomeUnavailable,
    /// 会话 id 为空。
    EmptySessionId,
    /// 子代理 id 为空。
    EmptySubagentId,
}

impl std::fmt::Display for PathsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HomeUnavailable => write!(f, "paths: home dir unavailable"),
            Self::EmptySessionId => write!(f, "paths: empty session id"),
            Self::EmptySubagentId => write!(f, "paths: empty subagent id"),
        }
    }
}

impl std::error::Error for PathsError {}

/// 纯根解析。`seek_home` 为 $SEEK_HOME 原始值（None/空 = 未设置）；
/// `os_home` 为调用方已解析的平台 home。测试注入两者，不改真实环境。
pub fn home_from(seek_home: Option<&str>, os_home: Option<&Path>) -> Result<PathBuf, PathsError> {
    if let Some(v) = seek_home
        && !v.is_empty()
    {
        return Ok(PathBuf::from(v));
    }
    match os_home {
        Some(h) => Ok(h.join(ROOT_DIR_NAME)),
        None => Err(PathsError::HomeUnavailable),
    }
}

/// 读真实环境解析根。
pub fn home() -> Result<PathBuf, PathsError> {
    let override_val = std::env::var(ENV_HOME).ok();
    home_from(override_val.as_deref(), std::env::home_dir().as_deref())
}

/// 项目标识：sha256(路径字节) 前 8 字节的 16 位小写 hex。
/// 逐字节哈希——无 symlink/大小写/Unicode 规范化；别名拼写 = 不同标识。
/// 调用方负责传入规范化物理路径（在首次取绝对路径处解析一次）。
pub fn project_id(abs: &Path) -> String {
    let mut h = Sha256::new();
    h.update(abs.as_os_str().as_encoded_bytes());
    h.finalize()
        .iter()
        .take(8)
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// 项目级 hooks 文件：<project>/.arbitrate/hooks.toml（在项目内，可提交共享）。
pub fn project_hooks_toml(abs_project: &Path) -> PathBuf {
    abs_project.join(ROOT_DIR_NAME).join("hooks.toml")
}

/// 已解析根下的全部路径组合。
pub struct Paths {
    root: PathBuf,
}

impl Paths {
    /// 读真实环境。
    pub fn resolve() -> Result<Self, PathsError> {
        Ok(Self { root: home()? })
    }

    /// 注入根（测试/已持有根的调用方）。
    pub fn from_root(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn sessions(&self) -> PathBuf {
        self.root.join("sessions")
    }

    pub fn mcp_config(&self) -> PathBuf {
        self.root.join("mcp.json")
    }

    pub fn cache(&self) -> PathBuf {
        self.root.join("cache")
    }

    pub fn user_skills(&self) -> PathBuf {
        self.root.join("skills")
    }

    pub fn user_skill_stats(&self) -> PathBuf {
        self.user_skills().join(".stats.jsonl")
    }

    pub fn user_agents(&self) -> PathBuf {
        self.root.join("agents")
    }

    pub fn projects(&self) -> PathBuf {
        self.root.join("projects")
    }

    pub fn project_dir(&self, abs: &Path) -> PathBuf {
        self.projects().join(project_id(abs))
    }

    pub fn project_plans(&self, abs: &Path) -> PathBuf {
        self.project_dir(abs).join("plans")
    }

    pub fn project_assets(&self, abs: &Path) -> PathBuf {
        self.project_dir(abs).join("assets")
    }

    pub fn soul(&self) -> PathBuf {
        self.root.join("soul.md")
    }

    pub fn project_session_dir(&self, abs: &Path, sid: &str) -> Result<PathBuf, PathsError> {
        if sid.is_empty() {
            return Err(PathsError::EmptySessionId);
        }
        Ok(self.project_dir(abs).join("sessions").join(sid))
    }

    pub fn session_checkpoint_dir(&self, abs: &Path, sid: &str) -> Result<PathBuf, PathsError> {
        self.project_session_dir(abs, sid)
    }

    pub fn subagents_index(&self, abs: &Path) -> PathBuf {
        self.project_dir(abs).join("subagents.jsonl")
    }

    pub fn subagent_session_dir(
        &self,
        abs: &Path,
        sid: &str,
        sub_sid: &str,
    ) -> Result<PathBuf, PathsError> {
        if sid.is_empty() {
            return Err(PathsError::EmptySessionId);
        }
        if sub_sid.is_empty() {
            return Err(PathsError::EmptySubagentId);
        }
        Ok(self
            .project_session_dir(abs, sid)?
            .join("subagents")
            .join(sub_sid))
    }

    pub fn cron_dir(&self) -> PathBuf {
        self.root.join("cron")
    }

    pub fn cron_jobs(&self) -> PathBuf {
        self.cron_dir().join("jobs.jsonl")
    }

    pub fn cron_tick_lock(&self) -> PathBuf {
        self.cron_dir().join("tick.lock")
    }

    pub fn cron_runs(&self) -> PathBuf {
        self.cron_dir().join("runs")
    }

    pub fn cron_env(&self) -> PathBuf {
        self.cron_dir().join("env")
    }

    pub fn cron_triggers(&self) -> PathBuf {
        self.cron_dir().join("triggers")
    }

    pub fn serve_dir(&self) -> PathBuf {
        self.root.join("serve")
    }

    pub fn user_keybindings(&self) -> PathBuf {
        self.root.join("keybindings.toml")
    }

    pub fn user_hooks_toml(&self) -> PathBuf {
        self.root.join("hooks.toml")
    }

    pub fn trusted_projects_json(&self) -> PathBuf {
        self.root.join("trusted-projects.json")
    }

    pub fn hooks_audit_log(&self) -> PathBuf {
        self.root.join("hooks-audit.jsonl")
    }
}
