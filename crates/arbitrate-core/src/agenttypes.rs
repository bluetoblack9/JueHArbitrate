//! 类型化子代理定义的只读注册表：Markdown + frontmatter 声明可 spawn 的代理类型。
//!
//! 本模块只有读 API——agents 目录的写路径属于 M2 agentsmgr（红线）。
//! 无热重载：会话启动时加载一次。

use crate::frontmatter;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// 定义来源标签。Config = 内置克隆携带 config agents 段路由覆盖。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Origin {
    #[default]
    Builtin,
    Project,
    User,
    Config,
}

impl Origin {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::Project => "project",
            Self::User => "user",
            Self::Config => "config",
        }
    }
}

/// 一个类型化代理定义。零值可选字段 = 继承父会话。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Definition {
    /// kebab-case 唯一标识（subagent_type wire 值）。
    pub name: String,
    /// 一行"何时 spawn 此类型"摘要。必填。
    pub description: String,
    /// 闭合 --- 之后的 markdown 角色提示。必填（内置 general-purpose 例外：空）。
    pub body: String,
    /// 显式工具白名单。空 = 继承父注册表减通用排除。
    pub tools: Vec<String>,
    /// 路由旋钮：原样携带，lane 校验在 spawn 时（M2）。
    pub provider: String,
    pub model: String,
    /// "" | low | medium | high
    pub effort: String,
    /// 0 = 继承。
    pub max_turns: i64,
    /// "" 或 "plan-analyze"（只紧不松）。
    pub workflow: String,
    pub origin: Origin,
    /// 文件路径，或 "builtin:<name>" / "config:agents:<name>"。
    pub source: String,
}

/// Parse 错误。消息文案是契约（Load 包装路径后上报，测试断言子串）。
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError(pub String);

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseError {}

/// kebab-case 契约：^[a-z][a-z0-9-]*$（ASCII）。写路径（M2 agentsmgr）同样依赖。
pub fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

const FORBIDDEN_TOOLS: [&str; 2] = ["agent", "ask_user"];
const LEGAL_EFFORTS: [&str; 3] = ["low", "medium", "high"];

/// 解析并静态校验一个代理定义文件。file_stem = 去掉 .md 的文件名（name 回退）。
/// 任何失败都是错误——半解析产物绝不进 Set。未知 frontmatter 键容忍丢弃。
pub fn parse(data: &[u8], file_stem: &str, source: &str) -> Result<Definition, ParseError> {
    let fm = frontmatter::split(data).map_err(|e| ParseError(e.to_string()))?;
    let get = |k: &str| fm.scalars.get(k).cloned().unwrap_or_default();
    let mut def = Definition {
        name: get("name"),
        description: get("description"),
        body: fm.body,
        tools: fm.lists.get("tools").cloned().unwrap_or_default(),
        provider: get("provider"),
        model: get("model"),
        effort: get("effort"),
        workflow: get("workflow"),
        source: source.to_string(),
        ..Definition::default()
    };
    if def.name.is_empty() {
        def.name = file_stem.to_string();
    }
    if def.name.is_empty() {
        return Err(ParseError(
            "no name: frontmatter `name` missing and filename stem empty".to_string(),
        ));
    }
    if !valid_name(&def.name) {
        return Err(ParseError(format!(
            "name {:?} must be kebab-case ([a-z][a-z0-9-]*)",
            def.name
        )));
    }
    if def.description.is_empty() {
        return Err(ParseError("frontmatter missing `description`".to_string()));
    }
    if def.body.trim().is_empty() {
        return Err(ParseError(
            "empty body — the role prompt is a required field".to_string(),
        ));
    }
    for tool in &def.tools {
        if FORBIDDEN_TOOLS.contains(&tool.as_str()) {
            return Err(ParseError(format!(
                "tools: {tool:?} is never allowed in an agent type (universal exclusion)"
            )));
        }
    }
    if !def.workflow.is_empty() && def.workflow != "plan-analyze" {
        return Err(ParseError(format!(
            "workflow: {:?} — only \"plan-analyze\" is allowed (tighten-only; loosening is a config error)",
            def.workflow
        )));
    }
    if !def.effort.is_empty() && !LEGAL_EFFORTS.contains(&def.effort.as_str()) {
        return Err(ParseError(format!(
            "effort: {:?} — valid: low, medium, high",
            def.effort
        )));
    }
    let raw = get("max_turns");
    if !raw.is_empty() {
        match raw.parse::<i64>() {
            Ok(n) if n > 0 => def.max_turns = n,
            _ => {
                return Err(ParseError(format!(
                    "max_turns: {raw:?} — must be a positive integer"
                )));
            }
        }
    }
    Ok(def)
}

/// 三个编译期默认，稳定顺序。描述与 body 逐字进子代理系统提示（prompt-cache 契约）。
pub fn builtins() -> Vec<Definition> {
    let research_tools = || {
        ["read", "grep", "list_dir", "git", "webfetch", "think"]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    };
    vec![
        Definition {
            name: "general-purpose".to_string(),
            description: "Full tool set (minus agent/ask_user); inherits the parent session's model, effort, and permissions.".to_string(),
            origin: Origin::Builtin,
            source: "builtin:general-purpose".to_string(),
            ..Definition::default()
        },
        Definition {
            name: "explore".to_string(),
            description: "Read-only parallel research (read/grep/list_dir/git/webfetch/think) forced into plan-analyze; returns bulleted findings.".to_string(),
            tools: research_tools(),
            workflow: "plan-analyze".to_string(),
            body: "You are in research-only mode. You cannot write, edit, or run mutating commands. Return findings as bulleted summary.".to_string(),
            origin: Origin::Builtin,
            source: "builtin:explore".to_string(),
            ..Definition::default()
        },
        Definition {
            name: "plan".to_string(),
            description: "Read-only investigation that returns a numbered, structured action plan the parent (or a human reviewer) can execute step by step.".to_string(),
            tools: research_tools(),
            workflow: "plan-analyze".to_string(),
            body: "You are in plan-analyze mode. Investigate the task and return a numbered, structured plan in your final summary — explicit steps the parent (or a human reviewer) can execute. You cannot run mutating tools yourself.".to_string(),
            origin: Origin::Builtin,
            source: "builtin:plan".to_string(),
            ..Definition::default()
        },
    ]
}

/// 名字索引集合：first-writer-wins，插入序 List，败者 provenance 留档。
#[derive(Debug, Default)]
pub struct Set {
    by_name: BTreeMap<String, Definition>,
    order: Vec<String>,
    shadowed: BTreeMap<String, Vec<String>>,
}

impl Set {
    /// 名字已存在 → 拒收并记录败者 source，返回 false。空名拒收。
    pub fn add(&mut self, def: Definition) -> bool {
        if def.name.is_empty() {
            return false;
        }
        if self.by_name.contains_key(&def.name) {
            self.shadowed
                .entry(def.name.clone())
                .or_default()
                .push(def.source);
            return false;
        }
        self.order.push(def.name.clone());
        self.by_name.insert(def.name.clone(), def);
        true
    }

    pub fn get(&self, name: &str) -> Option<&Definition> {
        self.by_name.get(name)
    }

    /// 同名败者的 source 列表；无冲突为空。
    pub fn shadowed(&self, name: &str) -> &[String] {
        self.shadowed.get(name).map(Vec::as_slice).unwrap_or(&[])
    }

    /// 插入序（文件层在前，内置最后）。
    pub fn list(&self) -> Vec<&Definition> {
        self.order.iter().map(|n| &self.by_name[n]).collect()
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// 非内置定义个数（文件 + config 克隆）。
    pub fn custom_count(&self) -> usize {
        self.list()
            .iter()
            .filter(|d| d.origin != Origin::Builtin)
            .count()
    }
}

// ---- Load：作用域级联 ----

/// config agents 段的一个条目：仅路由旋钮。tools/body/workflow 刻意缺席——
/// config 只能重路由内置，不能重授权或重提示。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RouteOverride {
    pub provider: String,
    pub model: String,
    /// "" | low | medium | high，Load 内校验。
    pub effort: String,
    /// >0 生效；<0 警告丢弃；0 忽略。
    pub max_turns: i64,
}

/// Load 选项。测试一律注入两个目录，不走真实 home 默认分支。
#[derive(Debug, Clone, Default)]
pub struct LoadOptions {
    /// None = "."（cwd）。
    pub project_dir: Option<PathBuf>,
    /// None = Paths::resolve().user_agents()（best-effort，home 不可得跳过用户槽）。
    pub user_agents_dir: Option<PathBuf>,
    pub config_agents: BTreeMap<String, RouteOverride>,
}

/// 启动诊断：每来源计数 + 非致命错误。坏文件绝不阻塞启动。
#[derive(Debug, Default)]
pub struct LoadStats {
    pub by_source: BTreeMap<String, usize>,
    pub errors: Vec<String>,
}

/// 四层级联：project .arbitrate/agents > 用户 agents > config 覆盖 > 内置。
/// 无失败路径（目录读错误计入 stats.errors）。只读：不创建任何目录。
pub fn load(opts: &LoadOptions) -> (Set, LoadStats) {
    let mut stats = LoadStats::default();
    let project_dir = opts
        .project_dir
        .clone()
        .unwrap_or_else(|| PathBuf::from("."));
    let user_agents_dir = match &opts.user_agents_dir {
        Some(p) => Some(p.clone()),
        None => crate::paths::Paths::resolve().ok().map(|p| p.user_agents()),
    };
    let mut set = Set::default();

    let project_agents = project_dir.join(".arbitrate").join("agents");
    let (n, errs) = load_from_dir(
        &mut set,
        &project_agents,
        "project .arbitrate",
        Origin::Project,
    );
    stats.by_source.insert("project .arbitrate".to_string(), n);
    stats.errors.extend(errs);

    if let Some(dir) = user_agents_dir {
        let (n, errs) = load_from_dir(&mut set, &dir, "user ~/.arbitrate", Origin::User);
        stats.by_source.insert("user ~/.arbitrate".to_string(), n);
        stats.errors.extend(errs);
    }

    let applied = apply_config_overrides(&mut set, &opts.config_agents, &mut stats);
    stats.by_source.insert("config agents".to_string(), applied);

    let mut builtin_added = 0;
    for def in builtins() {
        if set.add(def) {
            builtin_added += 1;
        }
    }
    if builtin_added > 0 {
        stats.by_source.insert("builtin".to_string(), builtin_added);
    }
    (set, stats)
}

/// 读一个作用域目录。缺失 = 常态（不报错、不创建）。文件名排序保证同目录冲突确定性。
fn load_from_dir(set: &mut Set, dir: &Path, label: &str, origin: Origin) -> (usize, Vec<String>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (0, Vec::new()),
        Err(e) => {
            return (
                0,
                vec![format!("agents: read {} ({label}): {e}", dir.display())],
            );
        }
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());
    let mut added = 0;
    let mut errs = Vec::new();
    for e in entries {
        let name = e.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        let path = dir.join(&*name);
        if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            errs.push(format!(
                "agents: {}: subdirectory ignored (agent types are single .md files)",
                path.display()
            ));
            continue;
        }
        let Some(stem) = name.strip_suffix(".md") else {
            continue;
        };
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(e) => {
                errs.push(format!("agents: {}: {e}", path.display()));
                continue;
            }
        };
        let mut def = match parse(&data, stem, &path.display().to_string()) {
            Ok(d) => d,
            Err(e) => {
                errs.push(format!("agents: {}: {e}", path.display()));
                continue;
            }
        };
        def.origin = origin;
        let def_name = def.name.clone();
        if set.add(def) {
            added += 1;
        } else if let Some(winner) = set.get(&def_name)
            && winner.origin == origin
        {
            errs.push(format!(
                "agents: {}: duplicate name {def_name:?} — shadows {} (sorted-filename order decides within a scope)",
                path.display(),
                winner.source
            ));
        }
    }
    (added, errs)
}

/// config 层：内置克隆 + 路由覆盖。键排序访问（BTreeMap），插入序确定性。
fn apply_config_overrides(
    set: &mut Set,
    overrides: &BTreeMap<String, RouteOverride>,
    stats: &mut LoadStats,
) -> usize {
    if overrides.is_empty() {
        return 0;
    }
    let bs = builtins();
    let mut applied = 0;
    for (name, ov) in overrides {
        let Some(base) = bs.iter().find(|b| &b.name == name) else {
            if set.get(name).is_none() {
                stats.errors.push(format!(
                    "agents: config key {name:?} matches no built-in or file type — ignored"
                ));
            } else {
                // 文件占用该名：config < files，败者留档。
                set.add(Definition {
                    name: name.clone(),
                    source: format!("config:agents:{name}"),
                    ..Definition::default()
                });
            }
            continue;
        };
        let mut clone = base.clone();
        if !ov.provider.is_empty() {
            clone.provider = ov.provider.clone();
        }
        if !ov.model.is_empty() {
            clone.model = ov.model.clone();
        }
        if !ov.effort.is_empty() {
            if LEGAL_EFFORTS.contains(&ov.effort.as_str()) {
                clone.effort = ov.effort.clone();
            } else {
                stats.errors.push(format!(
                    "agents: config {name:?} effort {:?} — valid: low, medium, high; field ignored",
                    ov.effort
                ));
            }
        }
        if ov.max_turns != 0 {
            if ov.max_turns < 0 {
                stats.errors.push(format!(
                    "agents: config {name:?} max_turns {} — must be positive; field ignored",
                    ov.max_turns
                ));
            } else {
                clone.max_turns = ov.max_turns;
            }
        }
        clone.origin = Origin::Config;
        clone.source = format!("config:agents:{name}");
        if set.add(clone) {
            applied += 1;
        }
    }
    applied
}
