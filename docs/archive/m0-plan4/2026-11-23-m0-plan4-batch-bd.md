# M0 计划 4 — 批 B+D：文本引用与环境键位（i18n · diff · imgrefs · filerefs · projectmd · childenv · pathop · keymap）Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 arbitrate-core 内以 Rust 重写批 B（i18n/diff/imgrefs/filerefs/projectmd）与批 D（childenv/pathop/keymap 数据层）共 8 个叶子包，落地总纲约束 10 的品牌/路径最小适配、约束 6 的子进程凭据防护、约束 2 的 system prompt 字节稳定性，全部用 Rust 自有测试验证。

**Architecture:** core 内 8 个新模块，全部 std-only（零新增外部依赖）。i18n = en/zh const 目录 + 英文兜底 Bundle + Go fmt 迷你格式化器（%s/%d/%q/%v）+ 进程级 `LazyLock<RwLock<Bundle>>` 默认束；diff = O(n²) LCS DP + hunk 窗口合并（tie-break 逐字照搬保证字节一致）；imgrefs/filerefs = 扩展名表 + stat 门 + cleanToken（按上游 D-F2 裁决**故意重复**，不共享）；projectmd = 向上 5 级发现 AGENTS.md + 64KB UTF-8 安全截断；childenv = 名称敏感谓词 + 顺序保持 scrub + 环境序 withheld（安全语义不简化）；pathop = 平台 cfg 分文件 + Windows 算法纯函数化（可注入注册表/通知替身，跨平台 stub 测试）+ std-only `unsafe extern` 真实注册表 FFI 后端（M0 建，edition 2024 unsafe 合规，cfg(windows) 仅 Windows CI build+clippy 首验）；keymap = 封闭 Action 枚举 + 双向表 + 手写 TOML 子集解析器（零依赖，BurntSushi 禁入）。

**Tech Stack:** Rust 1.90.0（edition 2024, resolver 3），std-only（`std::path::absolute`、`LazyLock`、`RwLock`、`BTreeMap`、`fs::metadata`）。**零新增外部依赖**——core 外部依赖保持 sha2/serde/serde_json（本计划不动任何 manifest），Cargo.lock 全计划零变化。

**上游参照（只读）:** `/Users/deep/Documents/workspaceB/openC/seek` @ `cb27b90d2979f5f840526100b28ade586d034df9`。批 B+D 源文件：`internal/i18n/{i18n,messages_en,messages_zh}.go`（158+34+32）、`internal/diff/diff.go`（230）、`internal/imgrefs/imgrefs.go`（60）、`internal/filerefs/filerefs.go`（118）、`internal/projectmd/projectmd.go`（128）、`internal/childenv/childenv.go`（203）、`internal/pathop/{pathop,pathop_common,pathop_windows}.go`（31+26+181）、`internal/keymap/{actions,keymap,parse}.go`（126+95+247）。行为规格已静态提取进本计划各 task；实现者不需要也不允许运行 Go。

**产品仓库:** `/Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs`，起点 HEAD `4a4d734`（计划 3 完成态，main 分支，无 remote，身份 repo-local bluedeep9）。

## Global Constraints

（与计划 1/2/3 相同，逐字沿用；每个 task 隐含包含）

- 初始版本 `0.1.0`：workspace.package.version 固定，9 个 package 均 `version.workspace = true` 继承；存储/配置 schema 版本独立编号。
- 工具链固定具体 `x.y.z`（本计划 1.90.0），禁浮动 channel；提交 Cargo.lock；CI/发布构建均 `--locked`。
- 数据根 `~/.arbitrate`、项目级 `.arbitrate`；**无回退**读取 `~/.seek`/`.seek`；不兼容历史 Go 数据。
- 环境变量沿用 `SEEK_*`（SEEK_HOME/SEEK_ACP_YOLO/SEEK_NO_UPGRADE_CHECK/SEEK_SMOKE）。
- 不编译/运行 Go，不执行 Go 测试，不采集 Go 输出，不做双二进制对照；上游源码与测试文本仅只读参考。
- 测试不依赖开发者真实数据/凭据：虚构 key、SEEK_HOME 临时根、不读真实 `~/.seek`/`.seek`/`~/.arbitrate`；pathop 类测试注入替身，不改真实 PATH。
- 依赖允许表（M0 §2）：core 无；deepseek/llm/state 仅 core；tools = core,state,deepseek；agent = core,state,tools,deepseek,llm；tui/surfaces = core,state,tools,deepseek,llm,agent；根 binary = 全部 8 个。覆盖 normal/build/dev 及 cfg 分支。（仅约束本地 package；外部库受 core 无网络/无 UI 职责约束。）
- 品牌：冻结 webfrontend.html/LICENSE/NOTICE 字节原样；builtin skills 仅身份/命令/路径最小适配 + 自指语言事实修正（总纲 v2.8）；禁全局替换 DeepSeek/溯源/SEEK_*/协议标识。
- 期望值来自显式规格，不得只录制待测实现输出自证；快照归一化仅覆盖列明的非确定字段。
- 阶段 stub：后续阶段能力显式 "not yet implemented" 非零退出；本计划 bin 仅无参数启动时输出脚手架版本，其余参数/子命令一律明确拒绝，真实 -version flag 与 CLI 分发留 M2。
- 跨平台字节一致性：冻结资产、原件归档及 fixture 禁止 Git 文本转换，Rust/配置等活动文本固定 LF。不可归一化后再哈希来绕过原字节摘要。
- 每个 task 的提交前绿灯门禁固定为 `cargo fmt --all` → `cargo fmt --check --all` → `cargo clippy --workspace --locked -- -D warnings` → 本 task 的 `cargo test --workspace --locked ...`，全部成功后再提交。红灯步骤只执行指定的失败测试，不要求 clippy/test 通过。CI 只检查，不自动修改或重新生成 fixture。
- 上游参照 commit：`cb27b90d2979f5f840526100b28ade586d034df9`（`git describe`: v0.12.0-5-gcb27b90）。
- 产品目标：default-run = arbitrate；mcp-mock 只供测试，发布打包仅取 arbitrate / arbitrate.exe。

## 计划 4 专属决策（规格决定，实现不得擅改）

- **D4-1 i18n 迷你格式化器**：上游 `fmt.Sprintf` 占位符仅 `%s/%d/%q/%v`（目录闭集，parity 测试钉死）。Rust 不引 fmt 机器，手写 `format_message(s, &[Arg])`。`pub enum Arg { Str(String), Int(i64) }`（`From<&str>/From<String>/From<i64>` 便利转换）。位置消费：`%s/%q/%v` 取 `Arg::Str`，`%d` 取 `Arg::Int`；`%%` 输出字面 `%`（不消费 arg）。**类型语义显式断言**：verb 与 Arg kind 不匹配时输出 Go 兼容错误标记 `%!s(int=7)` / `%!d(string=x)`（镜像 Go fmt 行为）；arg 耗尽输出 `%!s(MISSING)`；多余 arg **忽略**（Go 追加 `%!(EXTRA …)`，上游调用方从不超供，parity 测试钉死每 key verb 数 = 期望 arg 数；此为文档化偏差）。未知 verb 字面保留 `%z`（目录无此 verb，parity 测试保证不可达）。`%q` 镜像 `strconv.Quote`：ASCII 可打印原样（`"`→`\"`、`\`→`\\`），`\a\b\f\n\r\t\v` 转义，其余 <0x20/0x7f → `\xHH`，非 ASCII `char::is_control()` → `\uHHHH`/`\UHHHHHHHH`，可打印非 ASCII 原样保留。**近似天花板**：Go `IsPrint` 比 Rust `is_control` 宽（如 U+00AD 软连字符 Go 转义、Rust 保留）；目录 `%q` 实参为 ASCII 语言码，测试钉 ASCII 精确 + 文档化该近似。
- **D4-2 i18n resolve 与全局默认**：`resolve_with(config_lang, env: impl Fn(&str)->Option<String>) -> String` 纯核心（优先级 SEEK_LANG > configLang > LC_ALL > LANG > en，每级 `normalize`，不支持则穿透），`resolve(config_lang)` 薄包装读 `std::env::var`（沿用 config.rs `base_url_for_with` 注入缝模式，D2-4）。**测试绝不改真实 env**（edition 2024 `set_var` unsafe + 并行竞态），全部经 `resolve_with` 注入闭包。`normalize`：`to_lowercase(trim)`，空→""，`IndexAny("_-.")` 位置 **>0** 才截断（`"_weird"`→"" 因 index 0），en/zh 否则 ""。全局默认 `static DEFAULT: LazyLock<RwLock<Bundle>>`（初始 en）；`set_default(Option<Bundle>)`（None→en，镜像 Go nil 重置）；`default_bundle()->Bundle`（clone，避免把锁守卫生命周期泄漏给调用方）；包级 `t/lang` 读锁短暂。读写锁中毒恢复 `unwrap_or_else(|e| e.into_inner())`。**并行纪律**：tests/i18n.rs 内所有触碰全局默认的测试共享文件级 `static SERIAL: Mutex<()>`（每 fn 第一行取锁 + 中毒恢复）；`resolve_with`/Bundle/目录测试纯函数无守卫。并发测试 8 读者 × 200 交换（Barrier 同步），读者断言结果 ∈ {en 值, zh 值}。
- **D4-3 i18n Bundle 兜底链与测试缝**：`Bundle { lang: String, msgs: BTreeMap<String,String> }`。`Bundle::new(lang)`：`normalize(lang)==zh`→zh 目录否则 en（clone const 切片入 BTreeMap）。`Bundle::from_catalogue(lang, msgs)` **pub 测试缝**（镜像上游同包直接构造稀疏 Bundle；集成测试无法触私有字段，故开此入口；文档标注 test-only 注入，M2/M3 生产不调用）。`t(key,args)` 兜底链**逐字照搬**：`msgs.get(key)` 命中即用（**命中但值为空串 → 返回 key，不回退 en**——上游 `if s==""` 在 en 兜底之后）；未命中 → `MESSAGES_EN` 线性 find；仍无/空 → 返回 key。稀疏兜底洞测试钉死两条路径（缺 key→en 值；present-but-empty→key）。目录 = `const MESSAGES_EN/MESSAGES_ZH: &[(&str,&str)]`（14 key 各，O(n) find 可忽略，可迭代供 parity 测试）。
- **D4-4 品牌适配精确清单（约束 10）**：仅改产品身份/命令/默认数据路径，保留项不动。i18n `lang.persist_failed` en+zh 的 `~/.seek/config.json` → `~/.arbitrate/config.json`（**唯一** i18n 路径适配；`lang.status` 的 `SEEK_LANG` 保留）。keymap interrupt 描述 `"Quit seek (saves the session first)"` → `"Quit arbitrate (saves the session first)"`；unrecognised-key 警告 `` `seek keys actions` `` → `` `arbitrate keys actions` ``。**保留**：全部 14 key 名、`%s/%d/%q/%v` 占位符数量/顺序/类型、em-dash `—`、全角标点 `（）：、。——`、glyph `⚠ ↪ ↰ … ↑/↓ ·`、DeepSeek/SEEK_*/协议标识。keymap Action 标识符（submit/steer/…）= 用户配置 toml key，非品牌，逐字保留。Rust doc 注释中 `~/.seek/keybindings.toml` → `~/.arbitrate/keybindings.toml`（与 paths::user_keybindings 一致）。
- **D4-5 diff LCS 逐字移植**：tie-break **必须精确复制**保证字节一致输出——DP 填充 `else if dp[i-1][j] >= dp[i][j-1]` 取上；回溯 `case j>0 && (i==0 || dp[i][j-1] >= dp[i-1][j])` 优先 insert（反向产出，反转后纯替换渲染为 delete-first）。`splitLines`：`split('\n')` 后若末元素 "" 则 pop（`""`→空 vec；`"a\n"`→["a"]；`"a\n\n"`→["a",""]；`"\n"`→[""]）。`makeHunks` 忽略上游 `_, _ int` 长度参数（Rust 不收）。窗口 `[max(0,idx-ctx), min(len,idx+ctx+1))`，`lo<=last.hi` 合并；oldStart/newStart 从 `ops[:lo]` 计数（equal+delete / equal+insert），oldCount/newCount 从窗口切片计数。`MaxHunks=8` 截断。格式逐字：`--- a/{f}\n+++ b/{f}\n`、`@@ -{os},{oc} +{ns},{nc} @@\n`、`{kind}{text}\n`（kind ' '/'-'/'+'）。**边界钉死**：仅尾换行差异（`"a\nb\n"` vs `"a\nb"`）→ 行序列相同 → ""；空 old/new → oldStart/newStart 恒 1（上游非标准 `-1,0`，照搬）。期望串全部手算（规格导出），不录制实现输出。
- **D4-6 imgrefs/filerefs 扩展名与 cleanToken**：Go `filepath.Ext(".png")=".png"`（末元素最后点起，含位置 0），Rust `Path::extension()` 对 dotfile 返回 None → **自定义 `go_ext(p)`**：从末尾向前扫至 '.' 或分隔符（'/' 与 '\\' 均视为分隔符，跨平台确定；匹配 Go Windows，Go-on-Linux 仅对含反斜杠 exotic token 有别，文档化天花板），返回含点后缀或 ""。扩展名表 lowercase 比对。stat 门 = `fs::metadata(p).map(|m| m.is_file()).unwrap_or(false)`（跟随符号链接，镜像 os.Stat；is_file ≈ IsRegular）。`cleanToken` 循环至稳定：两端 Trim `"'`()[]<>` → TrimRight `.,;:!?` → TrimPrefix `@`。**按上游 PRD D-F2 裁决故意重复**：imgrefs 与 filerefs 各持一份 cleanToken，**不共享**（两包检测语义已分歧，保持独立；评审不得以 DRY 要求合并）。
- **D4-7 filerefs stat 注入缝（替代 Chdir）**：上游 literal-rich 测试用 `os.Chdir` 使相对 token `"rich:x.pdf"` 可 stat——Rust 测试并行 + edition 2024 chdir 进程级竞态，**禁 chdir**。开 `detect_refs_with(text, is_regular: impl Fn(&str)->bool) -> Vec<Ref>` 纯核心（stat 谓词注入），`detect_refs(text)` 薄包装用真实 fs。literal-rich 场景经 stub 谓词（仅 `"rich:x.pdf"` 存在）验证前缀回退，无真实文件/无 chdir。其余场景用真实临时文件（common::TempRoot）经 `detect_refs`。`rich:` 前缀**仅当余下部分 stat 为文档时绑定**（`CutPrefix`→`isDoc(rest)`→否则整 token verbatim 再试）；seen 按 path 去重，**首次出现的 Rich 标志胜出**（plain-first/rich-first 两序钉死）。`Detect` = `DetectRefs` 的 path 投影（空→nil/empty）。imgrefs 无相对 token 场景，**不开缝**，纯真实 fs 测试（YAGNI）。
- **D4-8 projectmd 路径与截断**：`std::path::absolute`（1.79+）替代 `filepath.Abs`。**天花板**：`absolute` 不词典解析 `..`/`.`（Go Abs 会 Clean）；生产调用方（M2）传已解析 cwd，测试用干净绝对路径，文档化该偏差（M2 接线若需词典 clean 再补）。m0 表 projectmd→paths 依赖 = **M2 接线层**（调用方经 paths/env 解析 cwd），M0 模块本身 std-only 不 import paths。发现：`for _ in 0..MAX_ASCEND(5)` 读 `dir/AGENTS.md`，Ok→shape；`NotFound | NotADirectory`→parent（`Path::parent()` None→break，镜像 Go Dir 根自等）；其他 err→`Err("projectmd: {path}: {err}")`。`NotADirectory` 一并视为 not-exist（镜像 Go `errors.Is(fs.ErrNotExist)` 覆盖 ENOTDIR）。截断：`Bytes=data.len()`；`>MAX_BYTES(65536)`→`b=data[..65536]`，`for i in 0..3 && !b.is_empty() && std::str::from_utf8(b).is_err()`  drop 末字节；`Content = str(b) + "\n\n…[truncated {dropped} bytes; AGENTS.md is too large — keep it under 64 KB]\n"`，`Truncate=true`。`Section()`：Path 空→""，否则 `"# Project instructions (from {path})\n\n{content}\n"`。边界 KAT 手算：emoji 4-byte 跨界 drop 3、CJK 3-byte drop 2、恰好 65536 不截断。
- **D4-9 childenv 安全语义不简化**：`is_sensitive(name)`：空→false；uppercase 后先.prefix `SEEK_` 再.substring `[KEY,SECRET,TOKEN,PASSWORD,PASSWD,CREDENTIAL]`。**NON-entries 钉死**：`AUTH` 不在表（`SSH_AUTH_SOCK` 是 socket 路径非凭据，scrub 它会断 ssh git push）；`USER`/`ACCOUNT` 不在表（身份非授权）。`scrub(env:&[String], keep:&[&str])`：keep 集 = trim+uppercase+非空+**精确**匹配（无 substring/glob，模糊逃生舱会静默重泄漏邻居）；`eq = find('=')`，`eq<=0` drop（无 '=' 或空名 "=value"）；keep 命中→原样保留；is_sensitive→drop；否则**字节原样、顺序保持**。`withheld` 返回 Scrub 丢弃的**名称**（绝不值），去重，**环境序**（上游注释 "sorted" 是陈旧漂移，测试钉环境序非排序）。`sanitized(keep)` = scrub(std::env::vars 格式化)；测试只断言不变量（无敏感泄漏、顺序为 vars 子序列），不读/不录真实凭据值。Go variadic keep → Rust `&[&str]`。
- **D4-10 pathop 平台隔离与可注入 Windows 算法**：Go build tags → Rust `cfg`。非 Windows 公开 API 逐字 no-op：`is_in_user_path/ensure_in_path/ensure_in_path_with_broadcast → Ok(false)`；`is_in_path` = `path_contains_dir(env PATH, dir, fold=false)`。Windows 算法**纯函数化 + 替身注入**（跨平台 stub 测试，不触真实注册表/PATH）：`trait WindowsBackend { abs; read_user_path()->Result<Option<(String,ValueKind)>>; write_user_path; process_path; set_process_path; env; broadcast }`，`ensure_in_path_windows(dir, with_broadcast, &mut dyn WindowsBackend)` 与 `is_in_user_path_windows` 纯核心。`ValueKind { ExpandSz, Sz }`；read None（ErrNotExist）→ ensure 用 ("", ExpandSz)。newPath：空→absDir；尾 ';'→current+absDir；否则 current+";"+absDir。写后 ensureProcessPATH（fold 命中→return；否则 PATH+";"+absDir，空→absDir）；with_broadcast→broadcast。**真实注册表 FFI + SendMessageTimeoutW 广播本计划（M0）建**（评审 P2-2 裁决：零依赖不强制延后，extern FFI 无需 crate）：std-only `unsafe extern "system"` 声明 advapi32 `RegOpenKeyExW/RegQueryValueExW/RegSetValueExW/RegCloseKey` + user32 `SendMessageTimeoutW`（**edition 2024 两层 unsafe，评审 P1**：extern 块须 `unsafe extern`；`unsafe fn` 内每个原生调用须显式 `unsafe {}` 块——`unsafe_op_in_unsafe_fn` edition 2024 默认 warn，经门禁 `-D warnings` 升级为错误；每调用附 SAFETY 注释述指针/缓冲长度/句柄有效性依据）；`RealBackend` 实现 `WindowsBackend`（abs=`std::path::absolute`+`clean_windows`；read/write `HKCU\Environment\Path`，ErrNotExist→None；process_path/set_process_path=`std::env` PATH，edition 2024 `set_var` unsafe 包；env=`std::env::var`；broadcast=`SendMessageTimeoutW` HWND_BROADCAST/WM_SETTINGCHANGE/SMTO_ABORTIFHUNG/2000ms）。Windows 公开 `is_in_user_path/ensure_in_path/ensure_in_path_with_broadcast` 委托 RealBackend（**去 NotYetImplemented**）。`cfg(windows)` 段 mac/linux 门禁不编译（纯算法替身测不出 FFI unsafe 错误），正确性靠 **Windows CI 首验：`cargo build --workspace --locked` + `cargo clippy --workspace --locked -- -D warnings` + `cargo test --workspace --locked`**（m0 CI 行 43 三平台矩阵）；测试仍用替身覆盖纯算法，**绝不触真实注册表/PATH**。`PathopError` 仅 `Backend(String)`。M3 仅剩 Windows 交互 PATH 提示（CLI 流程），非底层能力。`is_in_path` 全平台可用（纯核心 `is_in_path_with(env_path, dir)` + env 薄包装）。
- **D4-11 pathop 规范化与 expand_env**：`clean_posix`/`clean_windows` 词典 Clean 端口（pub，规范化助手）：空→"."，去 "."/空段，".." 弹栈（ rooted 根处丢弃，非 rooted 保留），折叠重复分隔符；Windows 先 '/'→'\\'，保留驱动器卷 "C:"（"C:"→"C:."），UNC 前导 "\\\\" 保留（**UNC 规范化天花板**：当前不保护 host/share 段免于 `..` 弹栈；该保护延后 M3）。`path_contains_dir(env_path, dir, fold)`：cfg 分隔符（win ';' / posix ':'）+ cfg clean；fold 用 `eq_ignore_ascii_case`（**天花板**：Go `strings.EqualFold` 是 Unicode 简单折叠，Rust ASCII；PATH 条目实际 ASCII，测试钉 ASCII，文档化）。`expand_env_with(s, get)` 镜像 **Go `os.Expand`/`os.ExpandEnv` $-风格**，`shell_name` 逐字端口 Go `getShellName`：`${name}` 花括号内**扫到 `}` 无 alnum 限制**（`${a-b}`→name `a-b`）；`${special}`（`*#$@!?-0-9`，len>2 && s[2]=='}'）→ w3；`$special`→w1；`$name` ASCII alnum+_；缺变量→get 返回 ""；lone `$` 保留；`${}`→eat w2、`${`→eat w1（均映射 ""）。**非 `%VAR%`**（上游 `userPathContains` 注释 "expanding %VAR%" 是陈旧漂移，代码实为 os.ExpandEnv；测试钉 `%VAR%` 字面不展开）。评审 P2-6 修正：`$$`/`${}`/`${a-b}` 经 faithful 端口均得 ""（HOME-only 映射），非旧稿 `$$`/`}`/`a-b}`。`user_path_contains_with`：split ';'→trim→skip 空→`clean_windows(expand_env_with(p))`→`eq_ignore_ascii_case(abs_dir)`。
- **D4-12 keymap 数据层（无 bubbletea）**：`Resolve(&str) -> Action` 数据层原语——core **不引 bubbletea**（TUI 在 M2/M3 把 `KeyPressMsg.String()` 转 key 字符串再调；延后登记）。`pub enum Action { None, Submit, Steer, Interrupt, Cancel, CycleMode, ClearScreen, ToggleReasoning, ToggleHelp, HistoryPrev, HistoryNext }`（`as_str()`：None→""，其余逐字上游标识；`parse(name)->Option<Action>`："" / 未知→None）。`all_actions() -> &'static [ActionInfo]` const 切片（**稳定显示序**，10 行，描述品牌适配见 D4-4）。`KeyMap { by_action: BTreeMap<Action,String>, by_key: HashMap<String,Action>, source: HashMap<Action,&'static str> }`；`new_default()`（source 全 "default"）；`resolve/key_for/source_for/snapshot`。Go nil 守卫随类型系统消失（D3-5 先例）。`is_reserved_key`（8 key 精确匹配，parser 先 lowercase 再查）；`is_known_action` = `Action::parse(name).is_some()`（None 排除）。`Binding { action, key, source, description }`，Snapshot 按 all_actions 序。
- **D4-13 keymap 手写 TOML 子集解析器（零依赖）**：BurntSushi 禁入（core 无外部依赖；D2-5 frontmatter 手写解析器先例）。`parse_toml_bindings(data:&str) -> Result<BTreeMap<String,String>, String>`：支持 `[section]` 头（bare/quoted 名）、`key = value`（bare/quoted key + basic/literal 字符串值）、`#` 注释（串外）、basic 转义 `\" \\ \b \f \n \r \t \uXXXX \UXXXXXXXX`。**只收集 [bindings]**，其余 section 校验语法但丢弃。**子集天花板**（文档化 + 延后登记）：非字符串值（int/bool/array/inline-table/datetime/multiline/dotted-key）→ 语法错误拒绝（BurntSushi 在忽略 section 会接受；现实 keybindings.toml 仅 [bindings]）；重复 key/重复 [bindings] → 错误（镜像 BurntSushi）；invalid UTF-8 → 错误。错误详情文本为 Rust 解析器自有（BurntSushi 文本不可复制），**警告形状逐字**：`keybindings: toml syntax error in {display}: {err} (using defaults)\n`。BTreeMap 收集 → 排序迭代 = 上游 `sort.Strings`（警告序确定）。评审 P2-5：quoted 段名经 `parse_key` 实现（`["bindings"]`≡`[bindings]`）；`\u`/`\U` 转义先验 ASCII hex 字节（`&b[start..start+n]` 是 `&[u8]` 字节切片，不劈 UTF-8），全 ASCII 才 str 切片 + from_str_radix，非法（含多字节）→ Err 不 panic。
- **D4-14 keymap Load/Check 与 swap 硬化**：`load(path:&Path, warn:&mut dyn io::Write) -> (KeyMap,bool)`：NotExist→(default,**true**)；其他读 err→警告 `keybindings: read {path}: {err} (using defaults)\n`→(default,false)；否则 load_bytes。`load_bytes(data:&[u8], path:&str, warn)`：from_utf8 err / parse err→syntax 警告→(default,false)；排序遍历行：trim value/actionName；unknown action→警告（available 列表）；空值→警告；`normalize_key`=to_lowercase；reserved→警告；`looks_like_known_key`（'+' 分段，非末段必 ctrl/alt/shift/cmd/meta/super，末段无空格/tab）失败→警告（品牌 `arbitrate keys actions`）；accepted 收集。冲突（**评审 P2-4 硬化**）：构终表 `final_map = by_action.clone()` + accepted 覆盖，按 key→Vec<Action> 聚合，len>1→警告 `… bound to multiple actions ({names}) in {display} — rejecting whole file\n`（em-dash，names ", " join；BTreeMap 排序 → 多冲突警告序确定，上游 Go map 迭代序非确定，此为改进）→(default,false)。**查终表而非仅 accepted**：单项重绑撞另一动作未改默认键也是真冲突（上游漏检致反表静默覆盖，arbitrate 有意偏差，验收汇总登记）。`display_path`：""→"<inline>"。`check(path)->Vec<String>`：load 入 Vec 缓冲，trim+split('\n')，防御分支 `keybindings: {display} rejected for unknown reason`。**swap 硬化（arbitrate 自有偏差，上游同洞）**：上游 apply 逐行 `delete(byKey, oldKey)` 在**交换绑定**（submit↔steer 互换 key）时误删后应用行的新 reverse 条目 → `Resolve("enter")` 失效（上游 keymap_test **无 swap 覆盖**，bug 未测）。Rust 改为 apply 循环后**从 by_action 整体重建 by_key**（reverse 表构造性一致）：单 rebind 行为不变（旧 key 消失），swap 两 key 均正确 resolve。测试钉 swap（Rust 自有规格）+ 单 rebind reverse（上游行为）。登记验收汇总「上游行为偏差（有意硬化）」。警告 `%q` 用 Rust `{:?}`（ASCII 输入与 Go strconv.Quote 字节一致；non-ASCII 转义形式有别，文档化近似， action/key 名实际 ASCII）。
- **D4-15 测试纪律与助手**：契约测试 `crates/arbitrate-core/tests/<module>.rs`（集成测试，只测 pub API，D2-6 沿用）。文件级临时根用 **`mod common; use common::TempRoot;`**（`TempRoot(pub PathBuf)`，`::new()` 无参，`.0` 取路径，Drop 清理；计划 1 已建，本计划 imgrefs/filerefs/projectmd/keymap 首批消费）。env 注入沿用 config.rs `impl Fn(&str)->Option<String>` 闭包缝。i18n 全局默认测试文件级 SERIAL（D4-2）。期望值全部规格导出/手算，不录制实现输出。跨平台：pathop 纯算法 + 可注入替身测试三平台均跑；`cfg(not(windows))` no-op 测试仅非 Windows 跑（Windows tally 不计这 2 个）；真实 Windows 后端（`cfg(windows)`）由 Windows 构建 + 严格 clippy（`-D warnings`）覆盖编译期正确性，无 Windows 专属公开 stub 测试（公开 API 已委托 RealBackend），测试绝不修改真实注册表/PATH。
- **D4-16 零新增依赖 / 门禁**：本计划**不动任何 Cargo.toml**（8 模块全 std-only）。core 外部依赖保持 sha2/serde/serde_json。门禁含 `git diff --exit-code Cargo.lock`（零变化）。所有门禁命令块 `set -e` 开头（D3-11 沿用），任一步失败整块非零退出，提交只在块完整成功后执行。

## File Structure

- `crates/arbitrate-core/src/lib.rs` — 逐 task 追加 `pub mod`（最终字母序：agenttypes, budget, cache, childenv, config, diff, filerefs, frontmatter, i18n, imgrefs, keymap, pathop, paths, pricing, projectmd, types）
- `crates/arbitrate-core/src/i18n.rs` — en/zh 目录 + normalize + Bundle + 迷你格式化器（Task 1）+ resolve_with/resolve + 全局默认（Task 2）
- `crates/arbitrate-core/src/diff.rs` — LCS DP + hunk 合并 + unified 格式（Task 3）
- `crates/arbitrate-core/src/imgrefs.rs` — 图片扩展名表 + go_ext + stat 门 + cleanToken（Task 4）
- `crates/arbitrate-core/src/filerefs.rs` — 文档扩展名表 + rich 前缀 + stat 注入缝 + cleanToken（故意重复）（Task 5）
- `crates/arbitrate-core/src/projectmd.rs` — AGENTS.md 发现 + UTF-8 安全截断 + Section（Task 6）
- `crates/arbitrate-core/src/childenv.rs` — 敏感谓词 + scrub + withheld + sanitized（Task 7）
- `crates/arbitrate-core/src/pathop.rs` — cfg 平台分文件 + clean/expand_env + Windows 可注入算法（Task 8）
- `crates/arbitrate-core/src/keymap.rs` — Action 枚举 + KeyMap 双向表 + Snapshot（Task 9）+ TOML 子集解析 + Load/Check（Task 10）
- `crates/arbitrate-core/tests/{i18n,diff,imgrefs,filerefs,projectmd,childenv,pathop,keymap,keymap_parse}.rs` — 契约测试（114 个：i18n 18 / diff 10 / imgrefs 8 / filerefs 11 / projectmd 12 / childenv 13 / pathop 14 / keymap 28）
- `crates/arbitrate-core/tests/common/mod.rs` — 已存在（TempRoot），本计划消费，不改

---
### Task 1: core::i18n — 目录 + Normalize + Bundle + 迷你格式化器

**Files:**
- Create: `crates/arbitrate-core/src/i18n.rs`
- Modify: `crates/arbitrate-core/src/lib.rs`（加 `pub mod i18n;`）
- Test: `crates/arbitrate-core/tests/i18n.rs`

**Interfaces:**
- Consumes: 无（std-only）
- Produces（Task 2 及 M2/M3 依赖）:
  - `pub const LANG_EN: &str = "en"; pub const LANG_ZH: &str = "zh";`
  - `pub fn normalize(tag: &str) -> String`
  - `pub enum Arg { Str(String), Int(i64) }` + `From<&str>/From<String>/From<i64>`
  - `pub struct Bundle`（私有字段 lang/msgs）
  - `Bundle::new(lang: &str) -> Bundle`、`Bundle::from_catalogue(lang: &str, msgs: BTreeMap<String,String>) -> Bundle`、`Bundle::lang(&self) -> &str`、`Bundle::t(&self, key: &str, args: &[Arg]) -> String`
  - `pub fn format_message(s: &str, args: &[Arg]) -> String`（pub 供格式化器直测）
  - `const MESSAGES_EN / MESSAGES_ZH: &[(&str,&str)]`（pub 供 parity 测试迭代）

- [x] **Step 1: 写失败测试** — `crates/arbitrate-core/tests/i18n.rs`

```rust
//! core::i18n 契约测试 — en/zh 目录、英文兜底、迷你格式化器、品牌适配
//! （规格：上游 internal/i18n，Rust 自有重写，不运行 Go）。

use arbitrate_core::i18n::{Arg, Bundle, LANG_EN, LANG_ZH, MESSAGES_EN, MESSAGES_ZH, format_message, normalize};
use std::collections::BTreeMap;

#[test]
fn normalize_table() {
    let cases = [
        ("en", "en"), ("EN", "en"), (" en ", "en"), ("zh", "zh"),
        ("zh_CN", "zh"), ("zh-Hans", "zh"), ("zh.UTF-8", "zh"),
        ("EN_us", "en"), ("en-US", "en"), ("", ""), ("fr", ""),
        ("_weird", ""), ("fr_FR", ""), ("  ZH-Hant  ", "zh"),
    ];
    for (input, want) in cases {
        assert_eq!(normalize(input), want, "normalize({input:?})");
    }
}

#[test]
fn bundle_new_lang() {
    assert_eq!(Bundle::new("zh").lang(), LANG_ZH);
    assert_eq!(Bundle::new("ZH-CN").lang(), LANG_ZH);
    assert_eq!(Bundle::new("en").lang(), LANG_EN);
    assert_eq!(Bundle::new("fr").lang(), LANG_EN); // 不支持 → en
    assert_eq!(Bundle::new("").lang(), LANG_EN);
}

#[test]
fn t_fallback_chain() {
    let zh = Bundle::new("zh");
    assert_eq!(zh.t("view.starting", &[]), "启动中…");
    // 未知 key → 返回 key 本身（en 也没有）
    assert_eq!(zh.t("nope.missing", &[]), "nope.missing");
    let en = Bundle::new("en");
    assert_eq!(en.t("view.starting", &[]), "starting…");
}

#[test]
fn t_sparse_fallback_hole() {
    // 缺 key → en 兜底
    let mut sparse = BTreeMap::new();
    sparse.insert("view.thinking".to_string(), "思考中…".to_string());
    let b = Bundle::from_catalogue("zh", sparse);
    assert_eq!(b.t("view.starting", &[]), "starting…"); // en 兜底
    assert_eq!(b.t("view.thinking", &[]), "思考中…");
    // present-but-empty → 返回 key（不回退 en！上游 if s=="" 在兜底之后）
    let mut empty = BTreeMap::new();
    empty.insert("view.starting".to_string(), String::new());
    let b2 = Bundle::from_catalogue("zh", empty);
    assert_eq!(b2.t("view.starting", &[]), "view.starting");
}

#[test]
fn t_args_zh_en() {
    let zh = Bundle::new("zh");
    let en = Bundle::new("en");
    assert_eq!(zh.t("view.thinking_s", &[Arg::Int(7)]), "思考中… 7 秒");
    assert_eq!(zh.t("view.thinking_ms", &[Arg::Int(2), Arg::Int(5)]), "思考中… 2 分 5 秒");
    assert_eq!(en.t("view.thinking_ms", &[Arg::Int(2), Arg::Int(5)]), "thinking… 2m5s");
    assert_eq!(en.t("view.provider_banner", &[Arg::Str("x".into())]),
        "⚠ Provider: x — FIM / cache stats / Reasoner disabled");
    assert_eq!(en.t("cmd.unknown", &[Arg::Str("/foo".into())]), "unknown command /foo — try /help");
    assert_eq!(zh.t("lang.unsupported", &[Arg::Str("fr".into())]), "不支持的语言 \"fr\" —— 可选：en、zh。");
    assert_eq!(en.t("lang.unsupported", &[Arg::Str("fr".into())]), "Unsupported language \"fr\" — supported: en, zh.");
}

#[test]
fn formatter_verbs() {
    assert_eq!(format_message("%s-%d", &[Arg::Str("a".into()), Arg::Int(3)]), "a-3");
    assert_eq!(format_message("%v", &[Arg::Str("err".into())]), "err");
    assert_eq!(format_message("%v", &[Arg::Int(9)]), "9");
    assert_eq!(format_message("100%%", &[]), "100%");
    assert_eq!(format_message("no verbs", &[Arg::Int(1)]), "no verbs"); // 多余 arg 忽略
}

#[test]
fn formatter_quote() {
    assert_eq!(format_message("%q", &[Arg::Str("fr".into())]), "\"fr\"");
    assert_eq!(format_message("%q", &[Arg::Str("a\nb".into())]), "\"a\\nb\"");
    assert_eq!(format_message("%q", &[Arg::Str("a\"b".into())]), "\"a\\\"b\"");
    assert_eq!(format_message("%q", &[Arg::Str("a\\b".into())]), "\"a\\\\b\"");
    assert_eq!(format_message("%q", &[Arg::Str("t\th".into())]), "\"t\\th\"");
    assert_eq!(format_message("%q", &[Arg::Str("\u{1}".into())]), "\"\\x01\"");
    assert_eq!(format_message("%q", &[Arg::Str("中".into())]), "\"中\""); // 可打印非 ASCII 原样
}

#[test]
fn formatter_type_mismatch() {
    // Go fmt 兼容错误标记
    assert_eq!(format_message("%s", &[Arg::Int(7)]), "%!s(int=7)");
    assert_eq!(format_message("%d", &[Arg::Str("x".into())]), "%!d(string=x)");
    assert_eq!(format_message("%q", &[Arg::Int(7)]), "%!q(int=7)");
}

#[test]
fn formatter_missing_extra() {
    assert_eq!(format_message("%s and %s", &[Arg::Str("a".into())]), "a and %!s(MISSING)");
    assert_eq!(format_message("%d", &[]), "%!d(MISSING)");
    // 多余 arg 忽略（文档化偏差，Go 追加 EXTRA）
    assert_eq!(format_message("%s", &[Arg::Str("a".into()), Arg::Int(2)]), "a");
}

#[test]
fn catalogue_parity() {
    let en: BTreeMap<_, _> = MESSAGES_EN.iter().copied().collect();
    let zh: BTreeMap<_, _> = MESSAGES_ZH.iter().copied().collect();
    assert_eq!(en.len(), 14);
    assert_eq!(zh.len(), 14);
    assert_eq!(en.keys().collect::<Vec<_>>(), zh.keys().collect::<Vec<_>>(), "en/zh key 集合必须一致");
}

#[test]
fn placeholder_signature_parity() {
    fn verbs(s: &str) -> Vec<char> {
        let b = s.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'%' && i + 1 < b.len() {
                let v = b[i + 1] as char;
                if v == '%' { i += 2; continue; }
                out.push(v);
                i += 2;
            } else { i += 1; }
        }
        out
    }
    let zh: BTreeMap<_, _> = MESSAGES_ZH.iter().copied().collect();
    for (key, en_val) in MESSAGES_EN {
        let zh_val = zh.get(key).unwrap();
        assert_eq!(verbs(en_val), verbs(zh_val), "key {key} 占位符签名 en/zh 必须一致");
    }
}

#[test]
fn brand_adaptation() {
    let en = Bundle::new("en");
    let zh = Bundle::new("zh");
    // 唯一 i18n 路径适配：~/.seek → ~/.arbitrate
    assert_eq!(en.t("lang.persist_failed", &[Arg::Str("disk full".into())]),
        "warning: could not persist language to ~/.arbitrate/config.json: disk full");
    assert_eq!(zh.t("lang.persist_failed", &[Arg::Str("disk full".into())]),
        "警告：语言设置未能写入 ~/.arbitrate/config.json：disk full");
    // SEEK_LANG 保留（约束 10 禁全局替换）
    assert!(en.t("lang.status", &[Arg::Str("en".into())]).contains("SEEK_LANG"));
    assert!(zh.t("lang.status", &[Arg::Str("zh".into())]).contains("SEEK_LANG"));
}

#[test]
fn translation_smoke() {
    // 所有目录值渲染后不得含 Go fmt 错误标记 "%!"
    for bundle in [Bundle::new("en"), Bundle::new("zh")] {
        for (key, _) in MESSAGES_EN {
            let rendered = bundle.t(key, &[]);
            assert!(!rendered.contains("%!"), "key {key} 渲染含错误标记: {rendered}");
        }
    }
}
```

- [x] **Step 2: 运行测试确认失败**

Run: `cd /Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs && cargo test --workspace --locked --test i18n`
Expected: FAIL — `unresolved import arbitrate_core::i18n`（模块未建）

- [x] **Step 3: 写实现** — `crates/arbitrate-core/src/i18n.rs`

```rust
//! core::i18n — 视图层消息目录（en/zh 双表，英文兜底）+ Go fmt 迷你格式化器。
//! 规格：上游 internal/i18n（只读参考），Rust std-only 重写。
//! 范围：仅视图层人类可读文案本地化；LLM prompt 路径与状态栏不本地化（字节稳定）。
//! 品牌适配（约束 10）：lang.persist_failed 路径 ~/.seek → ~/.arbitrate；SEEK_LANG 保留。

use std::collections::BTreeMap;

pub const LANG_EN: &str = "en";
pub const LANG_ZH: &str = "zh";

/// en 源语言目录（14 key）。每个 key 必须存在；parity 测试钉死 zh 不漂移。
pub const MESSAGES_EN: &[(&str, &str)] = &[
    ("view.starting", "starting…"),
    ("view.provider_banner", "⚠ Provider: %s — FIM / cache stats / Reasoner disabled"),
    ("view.thinking", "thinking…"),
    ("view.thinking_s", "thinking… %ds"),
    ("view.thinking_ms", "thinking… %dm%ds"),
    ("view.queue.steering", "↪ steering: "),
    ("view.queue.queued", "↰ queued: "),
    ("menu.hint.simple", "  ↑/↓ navigate · Enter accept · Esc cancel"),
    ("menu.hint.toggle", "  ↑/↓ navigate · Space toggle · Enter confirm · Esc cancel"),
    ("cmd.unknown", "unknown command %s — try /help"),
    ("lang.status", "Language: %s (view text). Switch with /lang <en|zh>; SEEK_LANG overrides per-launch."),
    ("lang.switched", "Language switched to %s. Already-rendered lines keep their original language."),
    ("lang.unsupported", "Unsupported language %q — supported: en, zh."),
    ("lang.persist_failed", "warning: could not persist language to ~/.arbitrate/config.json: %v"),
];

/// zh 简体中文目录（14 key，key 集合与 en 一致）。占位符数量/顺序/类型与 en 同步。
pub const MESSAGES_ZH: &[(&str, &str)] = &[
    ("view.starting", "启动中…"),
    ("view.provider_banner", "⚠ 提供方：%s — FIM / 缓存统计 / Reasoner 已禁用"),
    ("view.thinking", "思考中…"),
    ("view.thinking_s", "思考中… %d 秒"),
    ("view.thinking_ms", "思考中… %d 分 %d 秒"),
    ("view.queue.steering", "↪ 转向中："),
    ("view.queue.queued", "↰ 已排队："),
    ("menu.hint.simple", "  ↑/↓ 移动 · Enter 采纳 · Esc 取消"),
    ("menu.hint.toggle", "  ↑/↓ 移动 · Space 勾选 · Enter 确认 · Esc 取消"),
    ("cmd.unknown", "未知命令 %s — 试试 /help"),
    ("lang.status", "语言：%s（界面文案）。用 /lang <en|zh> 切换；SEEK_LANG 可按次覆盖。"),
    ("lang.switched", "已切换到 %s。已渲染的旧行保持原语言。"),
    ("lang.unsupported", "不支持的语言 %q —— 可选：en、zh。"),
    ("lang.persist_failed", "警告：语言设置未能写入 ~/.arbitrate/config.json：%v"),
];

/// 折叠 locale tag 到支持的语言码，不支持 → ""。
/// 区域/脚本/编码后缀先剥离（IndexAny "_-." 位置 >0 才截断）。
pub fn normalize(tag: &str) -> String {
    let tag = tag.trim().to_lowercase();
    if tag.is_empty() {
        return String::new();
    }
    let cut = tag.find(['_', '-', '.']).unwrap_or(tag.len());
    let base = if cut > 0 { &tag[..cut] } else { &tag[..] };
    match base {
        LANG_EN => LANG_EN.to_string(),
        LANG_ZH => LANG_ZH.to_string(),
        _ => String::new(),
    }
}

/// 格式化实参（位置消费）。Str 供 %s/%q/%v，Int 供 %d。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Arg {
    Str(String),
    Int(i64),
}

impl From<&str> for Arg {
    fn from(s: &str) -> Self { Arg::Str(s.to_string()) }
}
impl From<String> for Arg {
    fn from(s: String) -> Self { Arg::Str(s) }
}
impl From<i64> for Arg {
    fn from(n: i64) -> Self { Arg::Int(n) }
}

/// Go strconv.Quote 近似（D4-1）：ASCII 可打印原样 + 常见转义；
/// 控制字符 \xHH / \uHHHH / \UHHHHHHHH；可打印非 ASCII 原样。
/// 天花板：Go IsPrint 比 char::is_control 宽（U+00AD 等），目录 %q 实参为 ASCII。
fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{7}' => out.push_str("\\a"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{b}' => out.push_str("\\v"),
            c if (c as u32) < 0x20 || (c as u32) == 0x7f => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c if c.is_control() => {
                let u = c as u32;
                if u > 0xffff {
                    out.push_str(&format!("\\U{:08x}", u));
                } else {
                    out.push_str(&format!("\\u{:04x}", u));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Go fmt 迷你格式化器（D4-1）。仅 %s/%d/%q/%v/%%（目录闭集）。
/// 类型不匹配 → Go 兼容错误标记；arg 耗尽 → MISSING；多余 arg 忽略（文档化偏差）。
pub fn format_message(s: &str, args: &[Arg]) -> String {
    let b = s.as_bytes();
    let mut out = String::new();
    let mut argi = 0;
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 1 < b.len() {
            let verb = b[i + 1] as char;
            if verb == '%' {
                out.push('%');
                i += 2;
                continue;
            }
            if !matches!(verb, 's' | 'd' | 'q' | 'v') {
                // 未知 verb 字面保留（目录无此 verb，parity 保证不可达）
                out.push('%');
                out.push(verb);
                i += 2;
                continue;
            }
            let arg = args.get(argi);
            argi += 1;
            match (verb, arg) {
                (_, None) => out.push_str(&format!("%!{verb}(MISSING)")),
                ('d', Some(Arg::Int(n))) => out.push_str(&n.to_string()),
                ('d', Some(Arg::Str(s))) => out.push_str(&format!("%!d(string={s})")),
                ('s', Some(Arg::Str(s))) => out.push_str(s),
                ('s', Some(Arg::Int(n))) => out.push_str(&format!("%!s(int={n})")),
                ('v', Some(Arg::Str(s))) => out.push_str(s),
                ('v', Some(Arg::Int(n))) => out.push_str(&n.to_string()),
                ('q', Some(Arg::Str(s))) => out.push_str(&quote(s)),
                ('q', Some(Arg::Int(n))) => out.push_str(&format!("%!q(int={n})")),
                _ => unreachable!(),
            }
            i += 2;
        } else {
            // 按 char 边界推进，保留多字节 UTF-8
            let ch = s[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// 单语言不可变消息束。构造后经 new/from_catalogue；不 mutate。
#[derive(Clone, Debug)]
pub struct Bundle {
    lang: String,
    msgs: BTreeMap<String, String>,
}

impl Bundle {
    /// lang 的束。不支持 → en（config 拼写错误代价是翻译，不是 CLI 罢工）。
    pub fn new(lang: &str) -> Bundle {
        let (code, table) = if normalize(lang) == LANG_ZH {
            (LANG_ZH, MESSAGES_ZH)
        } else {
            (LANG_EN, MESSAGES_EN)
        };
        Bundle {
            lang: code.to_string(),
            msgs: table.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        }
    }

    /// 测试缝（D4-3）：直接注入目录，镜像上游同包构造稀疏 Bundle。
    /// 生产不调用；仅供集成测试验证兜底洞。
    pub fn from_catalogue(lang: &str, msgs: BTreeMap<String, String>) -> Bundle {
        Bundle { lang: lang.to_string(), msgs }
    }

    pub fn lang(&self) -> &str {
        &self.lang
    }

    /// 兜底链（D4-3 逐字照搬）：msgs 命中即用（命中但空 → key，不回退 en）；
    /// 未命中 → en；仍无/空 → key。有 args 则格式化。
    pub fn t(&self, key: &str, args: &[Arg]) -> String {
        let s = match self.msgs.get(key) {
            Some(v) => v.clone(),
            None => MESSAGES_EN.iter().find(|(k, _)| *k == key).map(|(_, v)| v.to_string()).unwrap_or_default(),
        };
        if s.is_empty() {
            return key.to_string();
        }
        if args.is_empty() { s } else { format_message(&s, args) }
    }
}
```

- [x] **Step 4: 加模块声明** — `crates/arbitrate-core/src/lib.rs`

Edit: old `pub mod frontmatter;\npub mod paths;` → new `pub mod frontmatter;\npub mod i18n;\npub mod paths;`

- [x] **Step 5: 运行测试确认通过**

Run: `cd /Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs && cargo test --workspace --locked --test i18n`
Expected: PASS — 13 passed

- [x] **Step 6: 门禁 + 提交**

```bash
set -e
cd /Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs
cargo fmt --all
cargo fmt --check --all
cargo clippy --workspace --locked -- -D warnings
cargo test --workspace --locked
git diff --exit-code Cargo.lock
git add crates/arbitrate-core/src/i18n.rs crates/arbitrate-core/src/lib.rs crates/arbitrate-core/tests/i18n.rs
git commit -m "feat(core): i18n catalogues + normalize + bundle + mini formatter (plan 4 task 1)"
```
Expected: 门禁全绿，177 passed + 1 ignored（164 基线 + 13），Cargo.lock 零变化，提交成功。

---
### Task 2: core::i18n — resolve 优先级 + 进程级默认束 + 并发

**Files:**
- Modify: `crates/arbitrate-core/src/i18n.rs`（追加 resolve/全局默认）
- Test: `crates/arbitrate-core/tests/i18n.rs`（追加 5 测试 + SERIAL 守卫）

**Interfaces:**
- Consumes: Task 1 `normalize`、`Bundle::new`、`LANG_EN/LANG_ZH`、`Arg`
- Produces（M2 cmd 启动接线 / M3 /lang 依赖）:
  - `pub fn resolve_with(config_lang: &str, env: impl Fn(&str) -> Option<String>) -> String`
  - `pub fn resolve(config_lang: &str) -> String`
  - `pub fn set_default(b: Option<Bundle>)`（None → en）
  - `pub fn default_bundle() -> Bundle`（clone）
  - `pub fn t(key: &str, args: &[Arg]) -> String`（包级，读全局）
  - `pub fn lang() -> String`（包级）

- [x] **Step 1: 写失败测试** — 追加到 `crates/arbitrate-core/tests/i18n.rs`

```rust
// ---- Task 2: resolve + 全局默认 ----
use arbitrate_core::i18n::{default_bundle, lang, resolve, resolve_with, set_default, t};
use std::sync::{Arc, Barrier, Mutex};

// 全局默认是进程级 static：所有触碰它的测试共享串行守卫（D4-2）。
static SERIAL: Mutex<()> = Mutex::new(());
fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

fn env_from<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |name| pairs.iter().find(|(k, _)| *k == name).map(|(_, v)| v.to_string())
}

#[test]
fn resolve_precedence() {
    // SEEK_LANG > configLang > LC_ALL > LANG > en；不支持穿透
    let cases: &[(&[(&str, &str)], &str, &str)] = &[
        (&[("SEEK_LANG", "zh"), ("LC_ALL", "en"), ("LANG", "en")], "en", "zh"),
        (&[("SEEK_LANG", "fr")], "zh", "zh"),           // SEEK_LANG 不支持 → config
        (&[("SEEK_LANG", ""), ("LC_ALL", "zh_CN"), ("LANG", "en")], "", "zh"),
        (&[("LC_ALL", ""), ("LANG", "zh")], "", "zh"),
        (&[], "", "en"),                                 // 全空 → en
        (&[("SEEK_LANG", "fr"), ("LC_ALL", "fr"), ("LANG", "zh")], "fr", "zh"),
        (&[("LANG", "EN_us")], "", "en"),
    ];
    for (pairs, config, want) in cases {
        assert_eq!(resolve_with(config, env_from(pairs)), *want, "config={config:?} pairs={pairs:?}");
    }
}

#[test]
fn resolve_normalize_config() {
    // configLang 不预归一化，resolve 内部 normalize
    assert_eq!(resolve_with(" EN_us ", env_from(&[])), "en");
    assert_eq!(resolve_with("zh-Hant", env_from(&[])), "zh");
    assert_eq!(resolve_with("klingon", env_from(&[])), "en"); // 不支持 → en 兜底
}

#[test]
fn default_lifecycle() {
    let _g = serial();
    set_default(None); // 重置 en（测试 hermetic）
    assert_eq!(lang(), "en");
    assert_eq!(t("view.starting", &[]), "starting…");
    set_default(Some(Bundle::new("zh")));
    assert_eq!(lang(), "zh");
    assert_eq!(default_bundle().lang(), "zh");
    assert_eq!(t("view.starting", &[]), "启动中…");
    set_default(None); // nil → en
    assert_eq!(lang(), "en");
    assert_eq!(t("view.starting", &[]), "starting…");
}

#[test]
fn concurrent_default_swap() {
    let _g = serial();
    set_default(None);
    const READERS: usize = 8;
    const SWAPS: usize = 200;
    let barrier = Arc::new(Barrier::new(READERS + 1));
    let mut handles = Vec::new();
    for _ in 0..READERS {
        let b = Arc::clone(&barrier);
        handles.push(std::thread::spawn(move || {
            b.wait();
            for _ in 0..(SWAPS * 4) {
                let s = t("view.starting", &[]);
                assert!(s == "starting…" || s == "启动中…", "竞态撕裂: {s}");
                let l = lang();
                assert!(l == "en" || l == "zh");
            }
        }));
    }
    barrier.wait();
    for i in 0..SWAPS {
        set_default(Some(Bundle::new(if i % 2 == 0 { "zh" } else { "en" })));
    }
    for h in handles { h.join().unwrap(); }
    set_default(None);
    assert_eq!(lang(), "en");
}

#[test]
fn resolve_wrapper_smoke() {
    // 薄包装读真实 env：结果必为支持码（不依赖开发者具体 locale）
    let code = resolve("");
    assert!(code == "en" || code == "zh", "resolve 必返回支持码，got {code}");
}
```

- [x] **Step 2: 运行测试确认失败**

Run: `cd /Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs && cargo test --workspace --locked --test i18n`
Expected: FAIL — `unresolved import ... resolve_with / set_default / t / lang`（未定义）

- [x] **Step 3: 写实现** — 追加到 `crates/arbitrate-core/src/i18n.rs`

```rust
// ---- resolve 优先级 + 进程级默认束（D4-2）----
use std::sync::{LazyLock, RwLock};

/// 会话语言优先级（高→低）：SEEK_LANG > configLang > LC_ALL > LANG > en。
/// 纯核心：env 经闭包注入（测试不改真实 env，并行安全）。
pub fn resolve_with(config_lang: &str, env: impl Fn(&str) -> Option<String>) -> String {
    for code in [
        normalize(&env("SEEK_LANG").unwrap_or_default()),
        normalize(config_lang),
        normalize(&env("LC_ALL").unwrap_or_default()),
        normalize(&env("LANG").unwrap_or_default()),
    ] {
        if !code.is_empty() {
            return code;
        }
    }
    LANG_EN.to_string()
}

/// 薄包装：读真实进程 env。生产启动调用一次（M2 cmd 接线）。
pub fn resolve(config_lang: &str) -> String {
    resolve_with(config_lang, |name| std::env::var(name).ok())
}

/// 进程级默认束。启动 + /lang 交换；RwLock 保证渲染中途交换竞态干净。
static DEFAULT: LazyLock<RwLock<Bundle>> = LazyLock::new(|| RwLock::new(Bundle::new(LANG_EN)));

/// 安装全局束。None → en（镜像 Go SetDefault(nil) 重置）。
pub fn set_default(b: Option<Bundle>) {
    let bundle = b.unwrap_or_else(|| Bundle::new(LANG_EN));
    *DEFAULT.write().unwrap_or_else(|e| e.into_inner()) = bundle;
}

/// 全局束快照（clone，避免锁守卫生命周期泄漏给调用方）。
pub fn default_bundle() -> Bundle {
    DEFAULT.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// 包级翻译：经全局束。见 Bundle::t。
pub fn t(key: &str, args: &[Arg]) -> String {
    DEFAULT.read().unwrap_or_else(|e| e.into_inner()).t(key, args)
}

/// 包级语言码。
pub fn lang() -> String {
    DEFAULT.read().unwrap_or_else(|e| e.into_inner()).lang().to_string()
}
```

- [x] **Step 4: 运行测试确认通过**

Run: `cd /Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs && cargo test --workspace --locked --test i18n`
Expected: PASS — 18 passed

- [x] **Step 5: 门禁 + 提交**

```bash
set -e
cd /Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs
cargo fmt --all
cargo fmt --check --all
cargo clippy --workspace --locked -- -D warnings
cargo test --workspace --locked
git diff --exit-code Cargo.lock
git add crates/arbitrate-core/src/i18n.rs crates/arbitrate-core/tests/i18n.rs
git commit -m "feat(core): i18n resolve precedence + process default bundle + concurrency (plan 4 task 2)"
```
Expected: 门禁全绿，182 passed + 1 ignored（177 + 5），Cargo.lock 零变化，提交成功。

---
### Task 3: core::diff — LCS unified diff

**Files:**
- Create: `crates/arbitrate-core/src/diff.rs`
- Modify: `crates/arbitrate-core/src/lib.rs`（加 `pub mod diff;`）
- Test: `crates/arbitrate-core/tests/diff.rs`

**Interfaces:**
- Consumes: 无（std-only）
- Produces（M2 edit 工具预览/回报、M3 TUI 共用）:
  - `pub const DEFAULT_CONTEXT: usize = 3; pub const MAX_HUNKS: usize = 8;`
  - `pub fn unified(old: &str, new: &str, filename: &str) -> String`
  - `pub fn unified_context(old: &str, new: &str, filename: &str, ctx: usize) -> String`

- [x] **Step 1: 写失败测试** — `crates/arbitrate-core/tests/diff.rs`

```rust
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
    let new: String = (0..20).filter(|i| *i != 2 && *i != 15).map(|i| format!("l{i}\n")).collect();
    let want = "--- a/f\n+++ b/f\n\
@@ -1,6 +1,5 @@\n l0\n l1\n-l2\n l3\n l4\n l5\n\
@@ -13,7 +12,6 @@\n l12\n l13\n l14\n-l15\n l16\n l17\n l18\n";
    assert_eq!(unified(&old, &new, "f"), want);
}

#[test]
fn max_hunks_cap() {
    // 80 行，变更 idx 0,8,16,...,72（10 处，间距 8 → 10 hunk → 截断 8）
    let old: String = (0..80).map(|i| format!("l{i}\n")).collect();
    let new: String = (0..80).map(|i| if i % 8 == 0 { format!("X{i}\n") } else { format!("l{i}\n") }).collect();
    let out = unified(&old, &new, "f");
    assert_eq!(out.matches("@@ ").count(), 8, "MaxHunks=8 截断");
    assert!(out.starts_with("--- a/f\n+++ b/f\n"));
    assert!(out.contains("-l56"), "第 8 hunk（idx56）保留");
    assert!(!out.contains("-l64"), "第 9 hunk（idx64）被截断");
}
```

- [x] **Step 2: 运行测试确认失败**

Run: `cd /Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs && cargo test --workspace --locked --test diff`
Expected: FAIL — `unresolved import arbitrate_core::diff`

- [x] **Step 3: 写实现** — `crates/arbitrate-core/src/diff.rs`

```rust
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
        out.push_str(&format!("@@ -{},{} +{},{} @@\n", h.old_start, h.old_count, h.new_start, h.new_count));
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
            ops.push(Op { kind: ' ', text: old[i - 1].clone() });
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || dp[i][j - 1] >= dp[i - 1][j]) {
            ops.push(Op { kind: '+', text: new[j - 1].clone() });
            j -= 1;
        } else {
            ops.push(Op { kind: '-', text: old[i - 1].clone() });
            i -= 1;
        }
    }
    ops.reverse();
    ops
}

/// hunk 窗口合并（忽略上游长度参数）。
fn make_hunks(ops: &[Op], ctx: usize) -> Vec<Hunk> {
    let changed: Vec<usize> = ops.iter().enumerate().filter(|(_, o)| o.kind != ' ').map(|(i, _)| i).collect();
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
            if o.kind == ' ' || o.kind == '-' { old_start += 1; }
            if o.kind == ' ' || o.kind == '+' { new_start += 1; }
        }
        let slice = &ops[lo..hi];
        let mut old_count = 0;
        let mut new_count = 0;
        for o in slice {
            if o.kind == ' ' || o.kind == '-' { old_count += 1; }
            if o.kind == ' ' || o.kind == '+' { new_count += 1; }
        }
        hunks.push(Hunk { old_start, old_count, new_start, new_count, ops: slice.to_vec() });
    }
    hunks
}
```

- [x] **Step 4: 加模块声明** — `crates/arbitrate-core/src/lib.rs`

Edit: old `pub mod config;\npub mod frontmatter;` → new `pub mod config;\npub mod diff;\npub mod frontmatter;`

- [x] **Step 5: 运行测试确认通过**

Run: `cd /Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs && cargo test --workspace --locked --test diff`
Expected: PASS — 10 passed

- [x] **Step 6: 门禁 + 提交**

```bash
set -e
cd /Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs
cargo fmt --all
cargo fmt --check --all
cargo clippy --workspace --locked -- -D warnings
cargo test --workspace --locked
git diff --exit-code Cargo.lock
git add crates/arbitrate-core/src/diff.rs crates/arbitrate-core/src/lib.rs crates/arbitrate-core/tests/diff.rs
git commit -m "feat(core): diff LCS unified diff with exact tie-break parity (plan 4 task 3)"
```
Expected: 门禁全绿，192 passed + 1 ignored（182 + 10），Cargo.lock 零变化，提交成功。

---
### Task 4: core::imgrefs — 图片引用检测

**Files:**
- Create: `crates/arbitrate-core/src/imgrefs.rs`
- Modify: `crates/arbitrate-core/src/lib.rs`（加 `pub mod imgrefs;`）
- Test: `crates/arbitrate-core/tests/imgrefs.rs`

**Interfaces:**
- Consumes: `common::TempRoot`（测试助手，已存在）
- Produces（M2 视觉链路消费）:
  - `pub fn detect(text: &str) -> Vec<String>`（有序去重，stat 门控的图片路径）

- [x] **Step 1: 写失败测试** — `crates/arbitrate-core/tests/imgrefs.rs`

```rust
//! core::imgrefs 契约测试 — 扩展名表 + stat 门 + cleanToken（规格：上游
//! internal/imgrefs，Rust 自有重写）。真实临时文件，不触真实用户数据。

use arbitrate_core::imgrefs::detect;

mod common;
use common::TempRoot;

fn write_img(root: &TempRoot, name: &str) -> String {
    let p = root.0.join(name);
    std::fs::write(&p, b"not-really-an-image").unwrap();
    p.to_string_lossy().into_owned()
}

#[test]
fn stat_gate_no_false_positive() {
    // 不存在的 .png 子串（URL/代码/错误消息）不得触发
    for text in [
        "see https://example.com/shot.png for details",
        r#"if strings.HasSuffix(name, ".png") { parse(name) }"#,
        "the image is at docs/shot.png and ~/pic.jpeg",
    ] {
        assert!(detect(text).is_empty(), "non-existent must not detect: {text}");
    }
}

#[test]
fn real_file() {
    let root = TempRoot::new();
    let img = write_img(&root, "shot.png");
    assert_eq!(detect(&format!("look at {img} please")), vec![img]);
}

#[test]
fn at_prefix_and_wrappers() {
    let root = TempRoot::new();
    let img = write_img(&root, "shot.png");
    // 嵌套包裹 + 尾标点 + @ 前缀，循环剥至稳定
    assert_eq!(detect(&format!("see (@{img}),")), vec![img.clone()]);
    assert_eq!(detect(&format!("`{img}`")), vec![img.clone()]);
    assert_eq!(detect(&format!("[{img}]!")), vec![img]);
}

#[test]
fn dedup_order() {
    let root = TempRoot::new();
    let a = write_img(&root, "a.png");
    let b = write_img(&root, "b.jpg");
    let text = format!("{b} then {a} then {b}");
    assert_eq!(detect(&text), vec![b, a]); // 首次出现序，去重
}

#[test]
fn all_exts_case_insensitive() {
    let root = TempRoot::new();
    for name in ["a.png", "b.jpg", "c.jpeg", "d.webp", "e.tiff", "f.bmp", "g.heic", "h.gif", "I.PNG"] {
        let img = write_img(&root, name);
        assert_eq!(detect(&format!("read {img}")), vec![img], "ext {name}");
    }
}

#[test]
fn directory_with_img_ext_not_ref() {
    let root = TempRoot::new();
    let dir = root.0.join("fake.png");
    std::fs::create_dir(&dir).unwrap();
    assert!(detect(&format!("open {}", dir.to_string_lossy())).is_empty());
}

#[test]
fn non_image_ext_not_ref() {
    let root = TempRoot::new();
    let txt = write_img(&root, "note.txt");
    assert!(detect(&format!("read {txt}")).is_empty());
}

#[test]
fn dotfile_ext() {
    // Go filepath.Ext(".png")=".png"；Rust Path::extension()=None → go_ext 钉死
    let root = TempRoot::new();
    let img = write_img(&root, ".png");
    assert_eq!(detect(&format!("open {img}")), vec![img]);
}
```

- [x] **Step 2: 运行测试确认失败**

Run: `cd /Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs && cargo test --workspace --locked --test imgrefs`
Expected: FAIL — `unresolved import arbitrate_core::imgrefs`

- [x] **Step 3: 写实现** — `crates/arbitrate-core/src/imgrefs.rs`

```rust
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
    matches!(ext.to_lowercase().as_str(),
        ".png" | ".jpg" | ".jpeg" | ".webp" | ".tiff" | ".bmp" | ".heic" | ".gif")
}

/// 剥 @ 前缀、两端引号/反引号/括号、尾句标点——循环至稳定。
fn clean_token(tok: &str) -> String {
    let mut tok = tok.to_string();
    loop {
        let prev = tok.clone();
        tok = tok.trim_matches(['"', '\'', '`', '(', ')', '[', ']', '<', '>']).to_string();
        tok = tok.trim_end_matches(['.', ',', ';', ':', '!', '?']).to_string();
        tok = tok.strip_prefix('@').unwrap_or(&tok).to_string();
        if tok == prev {
            return tok;
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
```

- [x] **Step 4: 加模块声明** — `crates/arbitrate-core/src/lib.rs`

Edit: old `pub mod i18n;\npub mod paths;` → new `pub mod i18n;\npub mod imgrefs;\npub mod paths;`

- [x] **Step 5: 运行测试确认通过**

Run: `cd /Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs && cargo test --workspace --locked --test imgrefs`
Expected: PASS — 8 passed

- [x] **Step 6: 门禁 + 提交**

```bash
set -e
cd /Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs
cargo fmt --all
cargo fmt --check --all
cargo clippy --workspace --locked -- -D warnings
cargo test --workspace --locked
git diff --exit-code Cargo.lock
git add crates/arbitrate-core/src/imgrefs.rs crates/arbitrate-core/src/lib.rs crates/arbitrate-core/tests/imgrefs.rs
git commit -m "feat(core): imgrefs stat-gated image reference detection (plan 4 task 4)"
```
Expected: 门禁全绿，200 passed + 1 ignored（192 + 8），Cargo.lock 零变化，提交成功。

---
### Task 5: core::filerefs — 文档引用检测 + rich 前缀

**Files:**
- Create: `crates/arbitrate-core/src/filerefs.rs`
- Modify: `crates/arbitrate-core/src/lib.rs`（加 `pub mod filerefs;`）
- Test: `crates/arbitrate-core/tests/filerefs.rs`

**Interfaces:**
- Consumes: `common::TempRoot`
- Produces（M2 fileroute 消费）:
  - `pub struct Ref { pub path: String, pub rich: bool }`（Clone/Debug/PartialEq/Eq）
  - `pub fn detect_refs(text: &str) -> Vec<Ref>`
  - `pub fn detect_refs_with(text: &str, is_regular: impl Fn(&str) -> bool) -> Vec<Ref>`（stat 注入缝，D4-7）
  - `pub fn detect(text: &str) -> Vec<String>`（path 投影）

- [x] **Step 1: 写失败测试** — `crates/arbitrate-core/tests/filerefs.rs`

```rust
//! core::filerefs 契约测试 — 文档扩展名表 + rich 前缀 + stat 门 + 首现去重
//! （规格：上游 internal/filerefs，Rust 自有重写）。literal-rich 用 stat 注入缝
//! 替代上游 Chdir（并行安全，D4-7）。

use arbitrate_core::filerefs::{Ref, detect, detect_refs, detect_refs_with};

mod common;
use common::TempRoot;

fn write_doc(root: &TempRoot, name: &str) -> String {
    let p = root.0.join(name);
    std::fs::write(&p, b"not-really-a-document").unwrap();
    p.to_string_lossy().into_owned()
}

#[test]
fn stat_gate_no_false_positive() {
    for text in [
        "see https://example.com/report.pdf for details",
        r#"if strings.HasSuffix(name, ".pdf") { parse(name) }"#,
        "the manual is at docs/manual.docx and slides at ~/deck.pptx",
    ] {
        assert!(detect(text).is_empty(), "non-existent must not detect: {text}");
    }
}

#[test]
fn real_doc() {
    let root = TempRoot::new();
    let doc = write_doc(&root, "report.pdf");
    assert_eq!(detect(&format!("summarize {doc} please")), vec![doc]);
}

#[test]
fn at_prefix_and_punctuation() {
    let root = TempRoot::new();
    let doc = write_doc(&root, "contract.docx");
    assert_eq!(detect(&format!("look at (@{doc}),")), vec![doc]);
}

#[test]
fn dedup() {
    let root = TempRoot::new();
    let doc = write_doc(&root, "a.pdf");
    assert_eq!(detect(&format!("{doc} and again {doc}")), vec![doc]);
}

#[test]
fn all_exts_case_insensitive() {
    let root = TempRoot::new();
    for name in ["a.pdf", "b.doc", "c.docx", "d.ppt", "e.pptx", "F.PDF"] {
        let doc = write_doc(&root, name);
        assert_eq!(detect(&format!("read {doc}")), vec![doc], "ext {name}");
    }
}

#[test]
fn directory_with_doc_ext_not_ref() {
    let root = TempRoot::new();
    let dir = root.0.join("fake.pdf");
    std::fs::create_dir(&dir).unwrap();
    assert!(detect(&format!("open {}", dir.to_string_lossy())).is_empty());
}

#[test]
fn image_ext_not_doc_ref() {
    let root = TempRoot::new();
    let img = write_doc(&root, "shot.png");
    assert!(detect(&format!("look at {img}")).is_empty()); // 家族分离
}

#[test]
fn rich_prefix() {
    let root = TempRoot::new();
    let doc = write_doc(&root, "report.pdf");
    // plain → rich=false
    assert_eq!(detect_refs(&format!("read {doc}")), vec![Ref { path: doc.clone(), rich: false }]);
    // @rich: → 同 path，rich=true
    assert_eq!(detect_refs(&format!("read @rich:{doc}")), vec![Ref { path: doc.clone(), rich: true }]);
    // 包裹 + 标点 rich 仍解析
    assert_eq!(detect_refs(&format!("see (@rich:{doc}),")), vec![Ref { path: doc.clone(), rich: true }]);
    // @rich: 不存在路径 → stat 门拒绝
    assert!(detect_refs("read @rich:/nope/missing.pdf").is_empty());
}

#[test]
fn rich_literal_name_fallback() {
    // 文件字面名 "rich:x.pdf"：前缀仅当余下 stat 为文档时绑定；
    // 这里 "x.pdf" 不存在 → 整 token verbatim 再试 → 命中。
    // 用注入缝（仅 "rich:x.pdf" 存在），无 chdir、无真实文件（D4-7）。
    let refs = detect_refs_with("open rich:x.pdf", |p| p == "rich:x.pdf");
    assert_eq!(refs, vec![Ref { path: "rich:x.pdf".to_string(), rich: false }]);
    // 对照：若 "x.pdf" 也存在，前缀绑定 → path="x.pdf" rich=true
    let refs2 = detect_refs_with("open rich:x.pdf", |p| p == "rich:x.pdf" || p == "x.pdf");
    assert_eq!(refs2, vec![Ref { path: "x.pdf".to_string(), rich: true }]);
}

#[test]
fn first_occurrence_wins() {
    let root = TempRoot::new();
    let doc = write_doc(&root, "report.pdf");
    // plain-first → rich=false
    assert_eq!(detect_refs(&format!("{doc} and @rich:{doc}")), vec![Ref { path: doc.clone(), rich: false }]);
    // rich-first → rich=true
    assert_eq!(detect_refs(&format!("@rich:{doc} and {doc}")), vec![Ref { path: doc, rich: true }]);
}

#[test]
fn detect_is_projection_of_detect_refs() {
    let root = TempRoot::new();
    let a = write_doc(&root, "a.pdf");
    let b = write_doc(&root, "b.docx");
    let text = format!("first {a} then @rich:{b} then {a}");
    let refs = detect_refs(&text);
    let paths = detect(&text);
    assert_eq!(refs.len(), 2);
    assert_eq!(paths.len(), 2);
    for (r, p) in refs.iter().zip(paths.iter()) {
        assert_eq!(&r.path, p);
    }
    assert!(!refs[0].rich);
    assert!(refs[1].rich);
}
```

- [x] **Step 2: 运行测试确认失败**

Run: `cd /Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs && cargo test --workspace --locked --test filerefs`
Expected: FAIL — `unresolved import arbitrate_core::filerefs`

- [x] **Step 3: 写实现** — `crates/arbitrate-core/src/filerefs.rs`

```rust
//! core::filerefs — 文档引用检测（PDF/Word/PPT，DeepSeek 文件输入管线）。
//! 规格：上游 internal/filerefs（只读参考），Rust std-only 重写。
//! 与 imgrefs 同型（同 stat 门）；cleanToken/go_ext 故意重复（上游 D-F2：
//! 两包检测语义已分歧，保持独立，不重构成共享助手）。
//! rich: 前缀 = 每文件用途升级（D-F1），stat 门控。

/// Go filepath.Ext 镜像（与 imgrefs 独立重复，D4-6）。
fn go_ext(p: &str) -> &str {
    let start = p.rfind(['/', '\\']).map(|i| i + 1).unwrap_or(0);
    let elem = &p[start..];
    match elem.rfind('.') {
        Some(i) => &elem[i..],
        None => "",
    }
}

fn is_doc_ext(ext: &str) -> bool {
    matches!(ext.to_lowercase().as_str(), ".pdf" | ".doc" | ".docx" | ".ppt" | ".pptx")
}

const RICH_PREFIX: &str = "rich:";

/// 一个检测到的文档引用，携带用户请求的用途（fileroute 路由输入）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ref {
    pub path: String,
    pub rich: bool,
}

/// cleanToken（与 imgrefs 独立重复，上游 D-F2 裁决）。
fn clean_token(tok: &str) -> String {
    let mut tok = tok.to_string();
    loop {
        let prev = tok.clone();
        tok = tok.trim_matches(['"', '\'', '`', '(', ')', '[', ']', '<', '>']).to_string();
        tok = tok.trim_end_matches(['.', ',', ';', ':', '!', '?']).to_string();
        tok = tok.strip_prefix('@').unwrap_or(&tok).to_string();
        if tok == prev {
            return tok;
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
    detect_refs_with(text, |p| std::fs::metadata(p).map(|m| m.is_file()).unwrap_or(false))
}

/// DetectRefs 的 path 投影（F2 表面，不关心用途的调用方）。
pub fn detect(text: &str) -> Vec<String> {
    detect_refs(text).into_iter().map(|r| r.path).collect()
}
```

- [x] **Step 4: 加模块声明** — `crates/arbitrate-core/src/lib.rs`

Edit: old `pub mod diff;\npub mod frontmatter;` → new `pub mod diff;\npub mod filerefs;\npub mod frontmatter;`

- [x] **Step 5: 运行测试确认通过**

Run: `cd /Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs && cargo test --workspace --locked --test filerefs`
Expected: PASS — 11 passed

- [x] **Step 6: 门禁 + 提交**

```bash
set -e
cd /Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs
cargo fmt --all
cargo fmt --check --all
cargo clippy --workspace --locked -- -D warnings
cargo test --workspace --locked
git diff --exit-code Cargo.lock
git add crates/arbitrate-core/src/filerefs.rs crates/arbitrate-core/src/lib.rs crates/arbitrate-core/tests/filerefs.rs
git commit -m "feat(core): filerefs doc detection + rich prefix + stat injection seam (plan 4 task 5)"
```
Expected: 门禁全绿，211 passed + 1 ignored（200 + 11），Cargo.lock 零变化，提交成功。

---
### Task 6: core::projectmd — AGENTS.md 发现与截断

**Files:**
- Create: `crates/arbitrate-core/src/projectmd.rs`
- Modify: `crates/arbitrate-core/src/lib.rs`（加 `pub mod projectmd;`）
- Test: `crates/arbitrate-core/tests/projectmd.rs`

**Interfaces:**
- Consumes: `common::TempRoot`
- Produces（M2 sysprompt 追加消费，约束 2 字节稳定）:
  - `pub const FILENAME: &str = "AGENTS.md";`
  - `pub struct Result { pub content: String, pub path: String, pub bytes: usize, pub truncate: bool }`（Clone/Debug/Default/PartialEq/Eq）
  - `pub fn load(start_dir: &str) -> std::io::Result<Result>`
  - `Result::section(&self) -> String`

- [x] **Step 1: 写失败测试** — `crates/arbitrate-core/tests/projectmd.rs`

```rust
//! core::projectmd 契约测试 — 向上 5 级发现 + 64KB UTF-8 安全截断 + Section
//! （规格：上游 internal/projectmd，Rust 自有重写）。边界 KAT 手算。

use arbitrate_core::projectmd::{FILENAME, Result, load};

mod common;
use common::TempRoot;

fn write_agents(dir: &std::path::Path, content: &str) -> String {
    let p = dir.join(FILENAME);
    std::fs::write(&p, content).unwrap();
    p.to_string_lossy().into_owned()
}

#[test]
fn parent_walk() {
    let root = TempRoot::new();
    let agents = write_agents(&root.0, "instructions");
    let sub = root.0.join("sub");
    std::fs::create_dir(&sub).unwrap();
    let r = load(&sub.to_string_lossy()).unwrap();
    assert_eq!(r.path, agents);
    assert_eq!(r.content, "instructions");
    assert!(!r.truncate);
}

#[test]
fn nearest_wins() {
    let root = TempRoot::new();
    write_agents(&root.0, "root");
    let a = root.0.join("a");
    std::fs::create_dir(&a).unwrap();
    let near = write_agents(&a, "a");
    let b = a.join("b");
    std::fs::create_dir(&b).unwrap();
    let r = load(&b.to_string_lossy()).unwrap();
    assert_eq!(r.path, near);
    assert_eq!(r.content, "a");
}

#[test]
fn none_found() {
    let root = TempRoot::new();
    // 7 级深起点：5 次候选全在 TempRoot 内（depth 7..3），绝不逃逸宿主目录（隔离）
    let deep = root.0.join("a/b/c/d/e/f/g");
    std::fs::create_dir_all(&deep).unwrap();
    let r = load(&deep.to_string_lossy()).unwrap();
    assert_eq!(r.path, "");
    assert_eq!(r.content, "");
    assert_eq!(r.bytes, 0);
}

#[test]
fn ascend_bounded() {
    let root = TempRoot::new();
    write_agents(&root.0, "root");
    // 7 级深：5 次迭代只到 depth3，root（depth0）不可达
    let deep = root.0.join("a/b/c/d/e/f/g");
    std::fs::create_dir_all(&deep).unwrap();
    let r = load(&deep.to_string_lossy()).unwrap();
    assert_eq!(r.path, "", "maxAscend=5 不得扫到 root");
}

#[test]
fn read_error_surfaced() {
    let root = TempRoot::new();
    // AGENTS.md 是目录 → 读错误（非 NotFound）→ surface
    std::fs::create_dir(root.0.join(FILENAME)).unwrap();
    let err = load(&root.0.to_string_lossy()).unwrap_err();
    assert!(err.to_string().contains("projectmd:"), "got {err}");
}

#[test]
fn notdir_treated_as_missing() {
    let root = TempRoot::new();
    let agents = write_agents(&root.0, "root");
    // blocker 是文件：blocker/sub/AGENTS.md 读 → ENOTDIR → 视为 not-exist 向上
    std::fs::write(root.0.join("blocker"), b"x").unwrap();
    let start = root.0.join("blocker/sub");
    let r = load(&start.to_string_lossy()).unwrap();
    assert_eq!(r.path, agents);
}

#[test]
fn truncate_oversized() {
    let root = TempRoot::new();
    let data = "x".repeat(70000);
    write_agents(&root.0, &data);
    let r = load(&root.0.to_string_lossy()).unwrap();
    assert!(r.truncate);
    assert_eq!(r.bytes, 70000);
    let want = format!("{}\n\n…[truncated 4464 bytes; AGENTS.md is too large — keep it under 64 KB]\n", "x".repeat(65536));
    assert_eq!(r.content, want);
}

#[test]
fn emoji_boundary() {
    let root = TempRoot::new();
    // 65533 a + 4-byte 🚀 = 65537；cut 65536 劈 emoji → drop 3 → 65533 a
    let data = format!("{}🚀", "a".repeat(65533));
    write_agents(&root.0, &data);
    let r = load(&root.0.to_string_lossy()).unwrap();
    assert!(r.truncate);
    assert_eq!(r.bytes, 65537);
    let want = format!("{}\n\n…[truncated 4 bytes; AGENTS.md is too large — keep it under 64 KB]\n", "a".repeat(65533));
    assert_eq!(r.content, want);
}

#[test]
fn cjk_boundary() {
    let root = TempRoot::new();
    // 65534 a + 3-byte 界 = 65537；cut 劈 → drop 2 → 65534 a
    let data = format!("{}界", "a".repeat(65534));
    write_agents(&root.0, &data);
    let r = load(&root.0.to_string_lossy()).unwrap();
    assert!(r.truncate);
    let want = format!("{}\n\n…[truncated 3 bytes; AGENTS.md is too large — keep it under 64 KB]\n", "a".repeat(65534));
    assert_eq!(r.content, want);
}

#[test]
fn exact_boundary_no_truncate() {
    let root = TempRoot::new();
    let data = "x".repeat(65536);
    write_agents(&root.0, &data);
    let r = load(&root.0.to_string_lossy()).unwrap();
    assert!(!r.truncate);
    assert_eq!(r.bytes, 65536);
    assert_eq!(r.content, data);
}

#[test]
fn section_empty_path() {
    let r = Result::default();
    assert_eq!(r.section(), "");
}

#[test]
fn section_label() {
    let r = Result { path: "/p/AGENTS.md".into(), content: "hello".into(), ..Default::default() };
    assert_eq!(r.section(), "# Project instructions (from /p/AGENTS.md)\n\nhello\n");
}
```

- [x] **Step 2: 运行测试确认失败**

Run: `cd /Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs && cargo test --workspace --locked --test projectmd`
Expected: FAIL — `unresolved import arbitrate_core::projectmd`

- [x] **Step 3: 写实现** — `crates/arbitrate-core/src/projectmd.rs`

```rust
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
        format!("# Project instructions (from {})\n\n{}\n", self.path, self.content)
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
            Err(e) if matches!(e.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory) => {
                // 缺失（含 ENOTDIR，镜像 Go errors.Is(fs.ErrNotExist)）→ 向上
                match dir.parent() {
                    Some(parent) if parent != dir => dir = parent.to_path_buf(),
                    _ => break, // 到文件系统根
                }
            }
            Err(e) => {
                return Err(std::io::Error::new(e.kind(), format!("projectmd: {}: {}", candidate.display(), e)));
            }
        }
    }
    Ok(Result::default())
}

fn shape(path: &str, data: &[u8]) -> Result {
    let mut r = Result { path: path.to_string(), bytes: data.len(), ..Default::default() };
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
            String::from_utf8_lossy(b), dropped, MAX_BYTES / 1024
        );
        r.truncate = true;
    } else {
        r.content = String::from_utf8_lossy(data).into_owned();
    }
    r
}
```

- [x] **Step 4: 加模块声明** — `crates/arbitrate-core/src/lib.rs`

Edit: old `pub mod pricing;\npub mod types;` → new `pub mod pricing;\npub mod projectmd;\npub mod types;`

- [x] **Step 5: 运行测试确认通过**

Run: `cd /Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs && cargo test --workspace --locked --test projectmd`
Expected: PASS — 12 passed

- [x] **Step 6: 门禁 + 提交**

```bash
set -e
cd /Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs
cargo fmt --all
cargo fmt --check --all
cargo clippy --workspace --locked -- -D warnings
cargo test --workspace --locked
git diff --exit-code Cargo.lock
git add crates/arbitrate-core/src/projectmd.rs crates/arbitrate-core/src/lib.rs crates/arbitrate-core/tests/projectmd.rs
git commit -m "feat(core): projectmd AGENTS.md discovery + UTF-8 safe truncation (plan 4 task 6)"
```
Expected: 门禁全绿，223 passed + 1 ignored（211 + 12），Cargo.lock 零变化，提交成功。

---
### Task 7: core::childenv — 子进程环境凭据防护

**Files:**
- Create: `crates/arbitrate-core/src/childenv.rs`
- Modify: `crates/arbitrate-core/src/lib.rs`（加 `pub mod childenv;`）
- Test: `crates/arbitrate-core/tests/childenv.rs`

**Interfaces:**
- Consumes: 无（std-only）
- Produces（M2 bash 工具 / M4 MCP·LSP·hooks spawn 消费；安全相关，语义不简化）:
  - `pub fn is_sensitive(name: &str) -> bool`
  - `pub fn scrub(env: &[String], keep: &[&str]) -> Vec<String>`
  - `pub fn sanitized(keep: &[&str]) -> Vec<String>`
  - `pub fn withheld(env: &[String], keep: &[&str]) -> Vec<String>`

- [x] **Step 1: 写失败测试** — `crates/arbitrate-core/tests/childenv.rs`

```rust
//! core::childenv 契约测试 — 名称敏感谓词 + 顺序保持 scrub + 环境序 withheld
//! （规格：上游 internal/childenv，Rust 自有重写）。安全语义不简化（约束 6）。
//! 虚构凭据名/值，不触真实凭据。

use arbitrate_core::childenv::{is_sensitive, sanitized, scrub, withheld};

const CREDENTIAL: &[&str] = &[
    "DEEPSEEK_API_KEY", "OPENAI_API_KEY", "GITHUB_TOKEN", "GH_TOKEN",
    "AWS_SECRET_ACCESS_KEY", "AWS_ACCESS_KEY_ID", "PGPASSWORD", "DB_PASSWD",
    "MY_CREDENTIAL", "NPM_AUTH_TOKEN", "SECRET_KEY", "APIKEY", "password",
    "mytoken", "STRIPE_SECRET",
];
const SEEK_NS: &[&str] = &["SEEK_HOME", "SEEK_LANG", "SEEK_SESSIONS_DIR", "SEEK_AUTO_DISTILL"];
const BENIGN: &[&str] = &[
    "PATH", "HOME", "USER", "LOGNAME", "SHELL", "TERM", "LANG", "LC_ALL",
    "EDITOR", "PWD", "TMPDIR", "SSH_AUTH_SOCK", "XDG_CONFIG_HOME", "DISPLAY",
    "COLORTERM", "HOSTNAME",
];

#[test]
fn sensitive_table() {
    for name in CREDENTIAL.iter().chain(SEEK_NS.iter()) {
        assert!(is_sensitive(name), "{name} 应敏感");
    }
    for name in BENIGN {
        assert!(!is_sensitive(name), "{name} 应良性（SSH_AUTH_SOCK 是 socket 路径，USER 是身份）");
    }
}

#[test]
fn case_insensitive() {
    assert!(is_sensitive("deepseek_api_key"));
    assert!(is_sensitive("Github_Token"));
    assert!(is_sensitive("seek_home"));
    assert!(!is_sensitive("ssh_auth_sock"));
}

#[test]
fn empty_name_not_sensitive() {
    assert!(!is_sensitive(""));
}

#[test]
fn scrub_order_byte_identical() {
    let env: Vec<String> = ["PATH=/usr/bin", "DEEPSEEK_API_KEY=sk-fake", "HOME=/home/u", "GH_TOKEN=ghp_fake", "TERM=xterm"]
        .iter().map(|s| s.to_string()).collect();
    let out = scrub(&env, &[]);
    assert_eq!(out, vec!["PATH=/usr/bin".to_string(), "HOME=/home/u".to_string(), "TERM=xterm".to_string()]);
}

#[test]
fn never_leak_value() {
    let env: Vec<String> = ["DEEPSEEK_API_KEY=sk-secret-fake", "GITHUB_TOKEN=ghp_fake", "PATH=/bin"]
        .iter().map(|s| s.to_string()).collect();
    let out = scrub(&env, &[]);
    let joined = out.join("\n");
    assert!(!joined.contains("sk-secret-fake"));
    assert!(!joined.contains("ghp_fake"));
    assert!(!joined.contains("DEEPSEEK_API_KEY"));
}

#[test]
fn keep_exact_not_substring() {
    let env: Vec<String> = ["GH_TOKEN=ghp_fake", "GH_TOKEN_EXTRA=x", "PATH=/bin"]
        .iter().map(|s| s.to_string()).collect();
    let out = scrub(&env, &["gh_token"]); // 大小写不敏感精确匹配
    assert_eq!(out, vec!["GH_TOKEN=ghp_fake".to_string(), "PATH=/bin".to_string()]);
    // GH_TOKEN_EXTRA 不被 keep（精确，非 substring）→ 仍 drop
}

#[test]
fn blank_keep_ignored() {
    let env: Vec<String> = ["GH_TOKEN=x", "PATH=/bin"].iter().map(|s| s.to_string()).collect();
    let out = scrub(&env, &["", "   "]);
    assert_eq!(out, vec!["PATH=/bin".to_string()]); // 空 keep 无效果
}

#[test]
fn malformed_entries_dropped() {
    let env: Vec<String> = ["NOEQUALS", "=value", "PATH=/bin", "EMPTY="].iter().map(|s| s.to_string()).collect();
    let out = scrub(&env, &[]);
    // NOEQUALS（无 =）、=value（空名）drop；EMPTY= 名非空保留
    assert_eq!(out, vec!["PATH=/bin".to_string(), "EMPTY=".to_string()]);
}

#[test]
fn equals_in_value_retained() {
    let env: Vec<String> = ["FOO=a=b=c"].iter().map(|s| s.to_string()).collect();
    assert_eq!(scrub(&env, &[]), vec!["FOO=a=b=c".to_string()]); // 首个 = 分割，值含 = 原样
}

#[test]
fn no_mutate_input() {
    let env: Vec<String> = ["PATH=/bin", "SECRET_KEY=x"].iter().map(|s| s.to_string()).collect();
    let snapshot = env.clone();
    let _ = scrub(&env, &[]);
    assert_eq!(env, snapshot); // 输入不被修改
}

#[test]
fn withheld_env_order_not_sorted() {
    // 上游注释 "sorted" 是陈旧漂移：实际环境序（D4-9 钉死）
    let env: Vec<String> = ["B=2", "SECRET_A=x", "A=1", "TOKEN_B=y", "SECRET_A=z"]
        .iter().map(|s| s.to_string()).collect();
    assert_eq!(withheld(&env, &[]), vec!["SECRET_A".to_string(), "TOKEN_B".to_string()]);
}

#[test]
fn withheld_dedup_names_never_values() {
    let env: Vec<String> = ["TOKEN_B=y", "TOKEN_B=z", "PATH=/bin"].iter().map(|s| s.to_string()).collect();
    let w = withheld(&env, &[]);
    assert_eq!(w, vec!["TOKEN_B".to_string()]); // 去重
    assert!(!w.iter().any(|n| n.contains('y') || n.contains('z'))); // 绝不值
}

#[test]
fn sanitized_invariants() {
    // 薄包装读真实 env：只断言普适不变量，不录/不依赖具体凭据（D4-9）
    let out = sanitized(&[]);
    let input: Vec<String> = std::env::vars().map(|(k, v)| format!("{k}={v}")).collect();
    for e in &out {
        let eq = e.find('=').expect("sanitized 条目必含 =");
        assert!(eq > 0, "空名不得保留");
        assert!(!is_sensitive(&e[..eq]), "敏感名不得泄漏: {}", &e[..eq]);
    }
    // 顺序保持：out 是 input 的子序列
    let mut it = input.iter();
    for e in &out {
        assert!(it.any(|i| i == e), "sanitized 必须保持 env 序");
    }
}
```

- [x] **Step 2: 运行测试确认失败**

Run: `cd /Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs && cargo test --workspace --locked --test childenv`
Expected: FAIL — `unresolved import arbitrate_core::childenv`

- [x] **Step 3: 写实现** — `crates/arbitrate-core/src/childenv.rs`

```rust
//! core::childenv — arbitrate 派生子进程的环境构造（凭据泄漏防护）。
//! 规格：上游 internal/childenv（只读参考），Rust std-only 重写。
//! 名称匹配（非值匹配）：值嗅探既误报断构建又漏报泄漏，且需把每个秘密读进扫描器。
//! 名称匹配粗但可预测——可预测性让逃生舱（keep）可用。安全语义不简化（约束 6）。
//! SEEK_* 命名空间保留（wire 级契约）；默认数据路径已适配 arbitrate（paths）。

use std::collections::HashSet;

/// 大写名称 substring 匹配。故意不含 AUTH（SSH_AUTH_SOCK 是 socket 路径非凭据，
/// scrub 它会断 ssh git push）；不含 USER/ACCOUNT（身份非授权）。
const SENSITIVE_SUBSTRINGS: &[&str] = &["KEY", "SECRET", "TOKEN", "PASSWORD", "PASSWD", "CREDENTIAL"];
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

/// 移除敏感条目，保持原序、其余字节原样。keep 精确（大小写不敏感，无 substring/glob）
/// 保留——模糊逃生舱会静默重泄漏邻居。无 '=' 或空名（"=value"）条目 drop。
pub fn scrub(env: &[String], keep: &[&str]) -> Vec<String> {
    let keep_set: HashSet<String> = keep.iter()
        .map(|k| k.trim().to_uppercase())
        .filter(|k| !k.is_empty())
        .collect();
    let mut out = Vec::with_capacity(env.len());
    for e in env {
        let eq = match e.find('=') {
            Some(i) if i > 0 => i,
            _ => continue, // 无 = 或空名
        };
        let name = &e[..eq];
        if keep_set.contains(&name.to_uppercase()) {
            out.push(e.clone());
            continue;
        }
        if is_sensitive(name) {
            continue;
        }
        out.push(e.clone());
    }
    out
}

/// scrub(进程 env, keep)——交给命令/代码非 arbitrate 控制的子进程的环境。
/// 调用方需 ADD 变量应 append 到结果（cmd.Env last-wins，显式值总胜父环境）。
pub fn sanitized(keep: &[&str]) -> Vec<String> {
    let env: Vec<String> = std::env::vars().map(|(k, v)| format!("{k}={v}")).collect();
    scrub(&env, keep)
}

/// Scrub 会丢弃的名称（绝不值），去重，**环境序**（上游注释 "sorted" 是漂移，D4-9）。
/// 诊断用（doctor/debug 日志），结果可安全打印。
pub fn withheld(env: &[String], keep: &[&str]) -> Vec<String> {
    let kept: HashSet<String> = scrub(env, keep).iter()
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
```

- [x] **Step 4: 加模块声明** — `crates/arbitrate-core/src/lib.rs`

Edit: old `pub mod cache;\npub mod config;` → new `pub mod cache;\npub mod childenv;\npub mod config;`

- [x] **Step 5: 运行测试确认通过**

Run: `cd /Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs && cargo test --workspace --locked --test childenv`
Expected: PASS — 13 passed

- [x] **Step 6: 门禁 + 提交**

```bash
set -e
cd /Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs
cargo fmt --all
cargo fmt --check --all
cargo clippy --workspace --locked -- -D warnings
cargo test --workspace --locked
git diff --exit-code Cargo.lock
git add crates/arbitrate-core/src/childenv.rs crates/arbitrate-core/src/lib.rs crates/arbitrate-core/tests/childenv.rs
git commit -m "feat(core): childenv credential scrub + env-order withheld (plan 4 task 7)"
```
Expected: 门禁全绿，236 passed + 1 ignored（223 + 13），Cargo.lock 零变化，提交成功。

---
### Task 8: core::pathop — 平台 PATH 辅助 + 可注入 Windows 算法

**Files:**
- Create: `crates/arbitrate-core/src/pathop.rs`
- Modify: `crates/arbitrate-core/src/lib.rs`（加 `pub mod pathop;`）
- Test: `crates/arbitrate-core/tests/pathop.rs`

**Interfaces:**
- Consumes: 无（std-only）
- Produces（M2 CLI / M3 Windows 交互 PATH 提示接线）:
  - `pub enum PathopError { Backend(String) }`（Debug/Clone/PartialEq/Eq + Display + Error）
  - `pub enum ValueKind { ExpandSz, Sz }`（Clone/Copy/Debug/PartialEq/Eq）
  - `pub trait WindowsBackend { abs; read_user_path; write_user_path; process_path; set_process_path; env; broadcast }`
  - `pub fn clean_posix(path: &str) -> String`、`pub fn clean_windows(path: &str) -> String`
  - `pub fn expand_env_with(s: &str, get: impl Fn(&str) -> String) -> String`
  - `pub fn path_contains_dir(env_path: &str, dir: &str, fold: bool) -> bool`（cfg 分隔符/clean）
  - `pub fn is_in_path(dir: &str) -> bool`、`pub fn is_in_path_with(env_path: &str, dir: &str) -> bool`
  - `pub fn user_path_contains_with(current_path: &str, abs_dir: &str, get: impl Fn(&str) -> String) -> bool`
  - `pub fn ensure_in_path_windows(dir: &str, with_broadcast: bool, b: &mut dyn WindowsBackend) -> Result<bool, PathopError>`
  - `pub fn is_in_user_path_windows(dir: &str, b: &dyn WindowsBackend) -> Result<bool, PathopError>`
  - `pub fn is_in_user_path(dir: &str) -> Result<bool, PathopError>`、`pub fn ensure_in_path(dir: &str) -> Result<bool, PathopError>`、`pub fn ensure_in_path_with_broadcast(dir: &str) -> Result<bool, PathopError>`（cfg：非 Windows Ok(false)；Windows 委托 `win::RealBackend` std-only FFI，M0 建，Windows CI 首验）

- [x] **Step 1: 写失败测试** — `crates/arbitrate-core/tests/pathop.rs`

```rust
//! core::pathop 契约测试 — 规范化/expand_env/Windows 可注入算法（规格：上游
//! internal/pathop，Rust 自有重写）。Windows 算法经 stub 跨平台验证，绝不触
//! 真实注册表/PATH（约束：测试注入替身）。%VAR% 不展开（上游注释漂移钉死）。

use arbitrate_core::pathop::{
    PathopError, ValueKind, WindowsBackend, clean_posix, clean_windows, ensure_in_path_windows,
    expand_env_with, is_in_user_path_windows, user_path_contains_with,
};

#[test]
fn clean_posix_table() {
    for (input, want) in [
        ("", "."), ("/a/b/../c", "/a/c"), ("a/../..", ".."), ("/..", "/"),
        ("a//b", "a/b"), ("./a", "a"), ("../a", "../a"), ("a/b/..", "a"), ("/", "/"),
    ] {
        assert_eq!(clean_posix(input), want, "clean_posix({input:?})");
    }
}

#[test]
fn clean_windows_table() {
    for (input, want) in [
        ("", "."), ("C:\\Tools\\", "C:\\Tools"), ("C:\\Tools\\..\\Bin", "C:\\Bin"),
        ("C:\\a\\\\b", "C:\\a\\b"), ("C:", "C:."), ("C:\\", "C:\\"),
        ("Tools\\", "Tools"), ("C:/Tools/", "C:\\Tools"), ("\\\\host\\share", "\\\\host\\share"),
    ] {
        assert_eq!(clean_windows(input), want, "clean_windows({input:?})");
    }
}

#[test]
fn expand_env_table() {
    let home = |n: &str| if n == "HOME" { "/home/u".to_string() } else { String::new() };
    assert_eq!(expand_env_with("$HOME/bin", home), "/home/u/bin");
    assert_eq!(expand_env_with("${HOME}/bin", home), "/home/u/bin");
    assert_eq!(expand_env_with("$MISSING/x", home), "/x"); // 缺变量 → ""
    assert_eq!(expand_env_with("plain", home), "plain");
    assert_eq!(expand_env_with("$", home), "$");           // lone $ 保留
    assert_eq!(expand_env_with("$ ", home), "$ ");
    assert_eq!(expand_env_with("$$", home), "");            // $ 是 special var → mapping("$")=""
    assert_eq!(expand_env_with("${}", home), "");           // bad syntax eat "${}" → ""
    assert_eq!(expand_env_with("${a-b}", home), "");        // 花括号扫到 '}'，name="a-b" → mapping=""
    // %VAR% 字面不展开（上游 os.ExpandEnv 是 $-风格，注释 "expanding %VAR%" 是漂移）
    assert_eq!(expand_env_with("%USERPROFILE%\\bin", home), "%USERPROFILE%\\bin");
}

#[test]
#[cfg(not(windows))]
fn path_contains_dir_posix() {
    use arbitrate_core::pathop::path_contains_dir;
    assert!(path_contains_dir("/usr/bin:/usr/local/bin", "/usr/bin", false));
    assert!(path_contains_dir("/usr/bin:", "/usr/bin/", false)); // clean 双方
    assert!(!path_contains_dir("/usr/bin", "/USR/BIN", false));  // posix 大小写敏感
    assert!(path_contains_dir("/usr/bin", "/USR/BIN", true));    // fold
    assert!(path_contains_dir(" : /usr/bin : ", "/usr/bin", false)); // 空段跳过 + trim
    assert!(!path_contains_dir("", "/usr/bin", false));
}

#[test]
fn user_path_contains_expand_fold() {
    let get = |n: &str| if n == "HOME" { "C:\\Users\\x".to_string() } else { String::new() };
    // $HOME 展开 + 尾分隔符 clean + 大小写折叠
    assert!(user_path_contains_with("$HOME/bin;C:\\Tools\\", "c:\\tools", get));
    assert!(user_path_contains_with("$HOME/bin", "C:\\Users\\x\\bin", get));
    // %VAR% 不展开 → 不匹配真实路径
    assert!(!user_path_contains_with("%USERPROFILE%\\bin", "C:\\Users\\x\\bin", get));
    assert!(!user_path_contains_with("C:\\Other", "C:\\Tools", get));
}

// ---- Windows 算法 stub 替身（绝不触真实注册表/PATH）----
struct Stub {
    user_path: Option<(String, ValueKind)>,
    read_err: bool,
    write_err: bool,
    written: Vec<(String, ValueKind)>,
    proc_path: String,
    proc_set: Vec<String>,
    env: Vec<(String, String)>,
    broadcasts: usize,
}
impl Stub {
    fn new() -> Self {
        Stub { user_path: None, read_err: false, write_err: false, written: Vec::new(),
               proc_path: String::new(), proc_set: Vec::new(), env: Vec::new(), broadcasts: 0 }
    }
}
impl WindowsBackend for Stub {
    fn abs(&self, dir: &str) -> Result<String, PathopError> {
        Ok(format!("C:\\{dir}")) // 替身：前缀 C:\，已 clean
    }
    fn read_user_path(&self) -> Result<Option<(String, ValueKind)>, PathopError> {
        if self.read_err { Err(PathopError::Backend("read fail".into())) } else { Ok(self.user_path.clone()) }
    }
    fn write_user_path(&mut self, value: &str, kind: ValueKind) -> Result<(), PathopError> {
        if self.write_err { return Err(PathopError::Backend("write fail".into())); }
        self.written.push((value.to_string(), kind));
        Ok(())
    }
    fn process_path(&self) -> String { self.proc_path.clone() }
    fn set_process_path(&mut self, value: &str) { self.proc_set.push(value.to_string()); self.proc_path = value.to_string(); }
    fn env(&self, name: &str) -> Option<String> { self.env.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone()) }
    fn broadcast(&mut self) { self.broadcasts += 1; }
}

#[test]
fn ensure_missing_value_uses_expand_sz() {
    let mut b = Stub::new(); // user_path None → ("", ExpandSz)
    let added = ensure_in_path_windows("Tools", false, &mut b).unwrap();
    assert!(added);
    assert_eq!(b.written, vec![("C:\\Tools".to_string(), ValueKind::ExpandSz)]);
    assert_eq!(b.proc_set, vec!["C:\\Tools".to_string()]); // 进程 PATH 空 → absDir
}

#[test]
fn ensure_trailing_semicolon() {
    let mut b = Stub::new();
    b.user_path = Some(("C:\\A;".to_string(), ValueKind::Sz));
    let added = ensure_in_path_windows("Tools", false, &mut b).unwrap();
    assert!(added);
    assert_eq!(b.written, vec![("C:\\A;C:\\Tools".to_string(), ValueKind::Sz)]); // 尾 ';' 不重复加
}

#[test]
fn ensure_already_present_no_write() {
    let mut b = Stub::new();
    b.user_path = Some(("C:\\Tools".to_string(), ValueKind::Sz));
    let added = ensure_in_path_windows("Tools", false, &mut b).unwrap();
    assert!(!added);
    assert!(b.written.is_empty()); // 已存在 → 不写注册表
    assert_eq!(b.proc_set, vec!["C:\\Tools".to_string()]); // 仍确保进程 PATH
}

#[test]
fn ensure_broadcast_flag() {
    let mut b = Stub::new();
    ensure_in_path_windows("Tools", true, &mut b).unwrap();
    assert_eq!(b.broadcasts, 1);
    let mut b2 = Stub::new();
    ensure_in_path_windows("Tools", false, &mut b2).unwrap();
    assert_eq!(b2.broadcasts, 0);
}

#[test]
fn ensure_process_path_variants() {
    // 进程 PATH 含其他 → append
    let mut b = Stub::new();
    b.user_path = Some(("C:\\Tools".to_string(), ValueKind::Sz)); // already-present 分支
    b.proc_path = "C:\\Other".to_string();
    ensure_in_path_windows("Tools", false, &mut b).unwrap();
    assert_eq!(b.proc_set, vec!["C:\\Other;C:\\Tools".to_string()]);
    // 进程 PATH 已含 → 不 set
    let mut b2 = Stub::new();
    b2.user_path = Some(("C:\\Tools".to_string(), ValueKind::Sz));
    b2.proc_path = "C:\\Tools".to_string();
    ensure_in_path_windows("Tools", false, &mut b2).unwrap();
    assert!(b2.proc_set.is_empty());
}

#[test]
fn ensure_read_error_propagates() {
    let mut b = Stub::new();
    b.read_err = true;
    let err = ensure_in_path_windows("Tools", false, &mut b).unwrap_err();
    assert_eq!(err, PathopError::Backend("read fail".into()));
    assert!(b.written.is_empty());
}

#[test]
fn ensure_write_error_propagates() {
    let mut b = Stub::new();
    b.write_err = true;
    let err = ensure_in_path_windows("Tools", false, &mut b).unwrap_err();
    assert_eq!(err, PathopError::Backend("write fail".into()));
}

#[test]
fn is_in_user_path_stub() {
    let mut b = Stub::new();
    b.user_path = Some(("C:\\Tools".to_string(), ValueKind::Sz));
    assert!(is_in_user_path_windows("Tools", &b).unwrap());
    b.user_path = Some(("C:\\Other".to_string(), ValueKind::Sz));
    assert!(!is_in_user_path_windows("Tools", &b).unwrap());
    b.user_path = None;
    assert!(!is_in_user_path_windows("Tools", &b).unwrap());
}

#[test]
#[cfg(not(windows))]
fn nonwindows_noops() {
    use arbitrate_core::pathop::{ensure_in_path, ensure_in_path_with_broadcast, is_in_user_path};
    assert_eq!(is_in_user_path("/whatever").unwrap(), false);
    assert_eq!(ensure_in_path("/whatever").unwrap(), false);
    assert_eq!(ensure_in_path_with_broadcast("/whatever").unwrap(), false);
}
```

- [x] **Step 2: 运行测试确认失败**

Run: `cd /Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs && cargo test --workspace --locked --test pathop`
Expected: FAIL — `unresolved import arbitrate_core::pathop`

- [x] **Step 3: 写实现** — `crates/arbitrate-core/src/pathop.rs`

```rust
//! core::pathop — 路径规范化 + Windows 用户 PATH 底层辅助（平台 cfg 隔离）。
//! 规格：上游 internal/pathop（只读参考），Rust std-only 重写。
//! Go build tags → Rust cfg。Windows 算法纯函数化 + 可注入替身（跨平台 stub 测试，
//! 不触真实注册表/PATH）；真实注册表 FFI + WM_SETTINGCHANGE 广播本计划 M0 建（std-only
//! extern，零依赖禁 winreg/windows-sys crate；cfg(windows) 段仅 Windows CI 编译/首验）。
//! 非 Windows 公开 API 逐字 no-op（shell rc 编辑超范围）。M3 仅剩 Windows 交互 PATH 提示。

use std::fmt;

/// pathop 错误。Backend = 系统调用/FFI 失败详情。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathopError {
    Backend(String),
}
impl fmt::Display for PathopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PathopError::Backend(s) => write!(f, "pathop: {s}"),
        }
    }
}
impl std::error::Error for PathopError {}

/// 注册表值类型（EXPAND_SZ 含 %VAR% 引用，Sz 纯字符串）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind { ExpandSz, Sz }

/// Windows 后端注入缝（D4-10）。生产 M0 `RealBackend` 接注册表 FFI + std::env + 广播；
/// 测试用内存替身。abs = cleanAbsDir（Abs+Clean）。
pub trait WindowsBackend {
    fn abs(&self, dir: &str) -> Result<String, PathopError>;
    /// None = 值不存在（registry.ErrNotExist）。
    fn read_user_path(&self) -> Result<Option<(String, ValueKind)>, PathopError>;
    fn write_user_path(&mut self, value: &str, kind: ValueKind) -> Result<(), PathopError>;
    fn process_path(&self) -> String;
    fn set_process_path(&mut self, value: &str);
    fn env(&self, name: &str) -> Option<String>;
    fn broadcast(&mut self);
}

// ---- 规范化（Go filepath.Clean 词典端口，D4-11）----

/// POSIX Clean：空→"."，去 "."/空段，".." 弹栈（rooted 根处丢弃），折叠分隔符。
pub fn clean_posix(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }
    let rooted = path.starts_with('/');
    let mut out: Vec<&str> = Vec::new();
    for comp in path.split('/') {
        match comp {
            "" | "." => {}
            ".." => {
                if !out.is_empty() && *out.last().unwrap() != ".." {
                    out.pop();
                } else if !rooted {
                    out.push("..");
                }
            }
            c => out.push(c),
        }
    }
    let joined = out.join("/");
    if rooted {
        format!("/{joined}")
    } else if joined.is_empty() {
        ".".to_string()
    } else {
        joined
    }
}

/// Windows Clean：'/'→'\\'，保留驱动器卷 "C:"（"C:"→"C:."），UNC 前导 "\\\\" 保留。
/// 天花板：UNC share 卷段不可弹（exotic UNC 延后 M3）；测试用驱动器路径。
pub fn clean_windows(path: &str) -> String {
    let p = path.replace('/', "\\");
    if p.is_empty() {
        return ".".to_string();
    }
    let bytes = p.as_bytes();
    let vol_end = if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() { 2 } else { 0 };
    let vol = &p[..vol_end];
    let rest = &p[vol_end..];
    let rooted = rest.starts_with('\\');
    let unc = vol_end == 0 && p.starts_with("\\\\");
    let mut out: Vec<&str> = Vec::new();
    for comp in rest.split('\\') {
        match comp {
            "" | "." => {}
            ".." => {
                if !out.is_empty() && *out.last().unwrap() != ".." {
                    out.pop();
                } else if !rooted && !unc {
                    out.push("..");
                }
            }
            c => out.push(c),
        }
    }
    let joined = out.join("\\");
    if unc {
        format!("\\\\{joined}")
    } else if rooted {
        format!("{vol}\\{joined}")
    } else if joined.is_empty() {
        if vol.is_empty() { ".".to_string() } else { format!("{vol}.") }
    } else {
        format!("{vol}{joined}")
    }
}

// ---- expand_env（Go os.ExpandEnv $-风格，非 %VAR%，D4-11）----

fn is_alnum(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
}

/// Go isShellSpecialVar：* # $ @ ! ? - 0-9。
fn is_shell_special_var(c: u8) -> bool {
    matches!(c, b'*' | b'#' | b'$' | b'@' | b'!' | b'?' | b'-' | b'0'..=b'9')
}

/// Go getShellName 逐字端口：${name}（花括号内扫到 '}'，**无 alnum 限制**）/
/// ${special}（len>2 && special && s[2]=='}' → w3）/ $special（w1）/ $name（alnum+_）。
fn shell_name(s: &str) -> (&str, usize) {
    let b = s.as_bytes();
    if b.is_empty() { return ("", 0); }
    if b[0] == b'{' {
        if b.len() > 2 && is_shell_special_var(b[1]) && b[2] == b'}' {
            return (&s[1..2], 3);
        }
        for i in 1..b.len() {
            if b[i] == b'}' {
                if i == 1 { return ("", 2); } // bad syntax：eat "${}"
                return (&s[1..i], i + 1);
            }
        }
        return ("", 1); // bad syntax：eat "${"
    }
    if is_shell_special_var(b[0]) {
        return (&s[0..1], 1);
    }
    let mut i = 0;
    while i < b.len() && is_alnum(b[i]) { i += 1; }
    (&s[..i], i)
}

/// Go os.Expand 端口。缺变量 → get 返回 ""（调用方传 unwrap_or_default）。
/// lone '$' 保留；invalid syntax 吃字符；%VAR% 字面不动（$-风格）。
pub fn expand_env_with(s: &str, get: impl Fn(&str) -> String) -> String {
    let b = s.as_bytes();
    let mut out = String::new();
    let mut i = 0; // pending 字面起点
    let mut j = 0;
    while j < b.len() {
        if b[j] == b'$' && j + 1 < b.len() {
            out.push_str(&s[i..j]);
            let (name, w) = shell_name(&s[j + 1..]);
            if name.is_empty() && w > 0 {
                // invalid syntax：吃字符（不输出）
            } else if name.is_empty() {
                out.push('$'); // 合法语法但 $ 后无名 → 保留 $
            } else {
                out.push_str(&get(name));
            }
            j += w;
            i = j + 1;
        }
        j += 1;
    }
    out.push_str(&s[i..]);
    out
}

// ---- path_contains_dir（cfg 平台分隔符/clean）----

#[cfg(windows)]
const SEP: char = ';';
#[cfg(not(windows))]
const SEP: char = ':';
#[cfg(windows)]
fn clean(p: &str) -> String { clean_windows(p) }
#[cfg(not(windows))]
fn clean(p: &str) -> String { clean_posix(p) }

/// env_path 是否含 dir（clean 双方，trim，空段跳过）。fold = 大小写不敏感。
/// 天花板：fold 用 eq_ignore_ascii_case（Go EqualFold 是 Unicode 简单折叠；PATH 实际 ASCII）。
pub fn path_contains_dir(env_path: &str, dir: &str, fold: bool) -> bool {
    let dir = clean(dir);
    for p in env_path.split(SEP) {
        let p = p.trim();
        if p.is_empty() { continue; }
        let cleaned = clean(p);
        if fold {
            if cleaned.eq_ignore_ascii_case(&dir) { return true; }
        } else if cleaned == dir {
            return true;
        }
    }
    false
}

/// Windows 语义版本（始终 ';' + clean_windows + fold），供 Windows 纯算法跨平台测试。
fn path_contains_dir_windows(env_path: &str, dir: &str) -> bool {
    let dir = clean_windows(dir);
    for p in env_path.split(';') {
        let p = p.trim();
        if p.is_empty() { continue; }
        if clean_windows(p).eq_ignore_ascii_case(&dir) { return true; }
    }
    false
}

/// is_in_path 纯核心（env_path 注入）。Windows：cleanAbsDir + fold；非 Windows：fold=false。
pub fn is_in_path_with(env_path: &str, dir: &str) -> bool {
    #[cfg(windows)]
    {
        match std::path::absolute(dir) {
            Ok(abs) => path_contains_dir(env_path, &clean_windows(&abs.to_string_lossy()), true),
            Err(_) => false,
        }
    }
    #[cfg(not(windows))]
    {
        path_contains_dir(env_path, dir, false)
    }
}

/// 薄包装：读真实进程 PATH。
pub fn is_in_path(dir: &str) -> bool {
    is_in_path_with(&std::env::var("PATH").unwrap_or_default(), dir)
}

/// 用户永久 PATH 是否含 abs_dir（展开 $VAR + clean_windows + fold）。
pub fn user_path_contains_with(current_path: &str, abs_dir: &str, get: impl Fn(&str) -> String) -> bool {
    for p in current_path.split(';') {
        let p = p.trim();
        if p.is_empty() { continue; }
        let expanded = clean_windows(&expand_env_with(p, &get));
        if expanded.eq_ignore_ascii_case(abs_dir) { return true; }
    }
    false
}

// ---- Windows 纯算法（可注入替身，跨平台测试）----

fn ensure_process_path(abs_dir: &str, b: &mut dyn WindowsBackend) {
    let proc_path = b.process_path();
    if path_contains_dir_windows(&proc_path, abs_dir) {
        return;
    }
    let new = if proc_path.is_empty() { abs_dir.to_string() } else { format!("{proc_path};{abs_dir}") };
    b.set_process_path(&new);
}

/// ensureInPATH 纯核心。返回 true = 新注册表条目已 append。
pub fn ensure_in_path_windows(dir: &str, with_broadcast: bool, b: &mut dyn WindowsBackend) -> Result<bool, PathopError> {
    let abs_dir = b.abs(dir)?;
    let (current_path, val_type) = match b.read_user_path()? {
        Some(v) => v,
        None => (String::new(), ValueKind::ExpandSz), // ErrNotExist → "" + EXPAND_SZ
    };
    if user_path_contains_with(&current_path, &abs_dir, |n| b.env(n).unwrap_or_default()) {
        ensure_process_path(&abs_dir, b);
        return Ok(false);
    }
    let new_path = if current_path.is_empty() {
        abs_dir.clone()
    } else if current_path.ends_with(';') {
        format!("{current_path}{abs_dir}")
    } else {
        format!("{current_path};{abs_dir}")
    };
    b.write_user_path(&new_path, val_type)?;
    ensure_process_path(&abs_dir, b);
    if with_broadcast {
        b.broadcast();
    }
    Ok(true)
}

/// IsInUserPATH 纯核心。
pub fn is_in_user_path_windows(dir: &str, b: &dyn WindowsBackend) -> Result<bool, PathopError> {
    let abs_dir = b.abs(dir)?;
    let current = match b.read_user_path()? {
        Some((v, _)) => v,
        None => String::new(),
    };
    Ok(user_path_contains_with(&current, &abs_dir, |n| b.env(n).unwrap_or_default()))
}

// ---- 真实 Windows 后端（cfg(windows)，std-only FFI，D4-10）----
// mac/linux 门禁不编译此段；正确性靠 Windows CI 首验。测试用替身，绝不触真实注册表/PATH。

#[cfg(windows)]
mod win {
    use super::{PathopError, ValueKind, WindowsBackend, clean_windows};

    const HKEY_CURRENT_USER: isize = 0x80000001u32 as i32 as isize;
    const KEY_QUERY_VALUE: u32 = 0x0001;
    const KEY_SET_VALUE: u32 = 0x0002;
    const REG_SZ: u32 = 1;
    const REG_EXPAND_SZ: u32 = 2;
    const ERROR_FILE_NOT_FOUND: i32 = 2;
    const HWND_BROADCAST: isize = 0xFFFF;
    const WM_SETTINGCHANGE: u32 = 0x001A;
    const SMTO_ABORTIFHUNG: u32 = 0x0002;

    // edition 2024：extern 块须 `unsafe extern`（unsafe-extern）；unsafe fn 内每个原生
    // 调用须显式 `unsafe {}`（unsafe_op_in_unsafe_fn edition 2024 默认 warn，经 -D warnings 升级为错误）。
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegOpenKeyExW(hkey: isize, subkey: *const u16, options: u32, sam: u32, result: *mut isize) -> i32;
        fn RegQueryValueExW(hkey: isize, name: *const u16, reserved: *mut u32, typ: *mut u32, data: *mut u8, cb: *mut u32) -> i32;
        fn RegSetValueExW(hkey: isize, name: *const u16, reserved: u32, typ: u32, data: *const u8, cb: u32) -> i32;
        fn RegCloseKey(hkey: isize) -> i32;
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SendMessageTimeoutW(hwnd: isize, msg: u32, wparam: usize, lparam: *const u16, flags: u32, timeout: u32, result: *mut usize) -> isize;
    }

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe fn read_path() -> Result<Option<(String, ValueKind)>, PathopError> {
        let subkey = to_wide("Environment"); // NUL 结尾 wide，调用期间存活
        let mut hkey: isize = 0;
        // SAFETY: subkey.as_ptr() 指 NUL 结尾 wide 串；&mut hkey 是合法 out 指针；HKCU 伪句柄常量。
        let rc = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_QUERY_VALUE, &mut hkey) };
        if rc != 0 { return Err(PathopError::Backend(format!("RegOpenKeyEx: {rc}"))); }
        let name = to_wide("Path");
        let mut typ: u32 = 0;
        let mut cb: u32 = 0;
        // SAFETY: hkey 已由 RegOpenKeyExW 成功打开；name 合法；data=null 仅查询尺寸，cb/typ 为合法 out。
        let rc = unsafe { RegQueryValueExW(hkey, name.as_ptr(), std::ptr::null_mut(), &mut typ, std::ptr::null_mut(), &mut cb) };
        if rc == ERROR_FILE_NOT_FOUND { unsafe { RegCloseKey(hkey) }; return Ok(None); }
        if rc != 0 { unsafe { RegCloseKey(hkey) }; return Err(PathopError::Backend(format!("RegQueryValueEx size: {rc}"))); }
        if typ != REG_SZ && typ != REG_EXPAND_SZ {
            unsafe { RegCloseKey(hkey) };
            return Err(PathopError::Backend(format!("Path type {typ} not string")));
        }
        let mut buf = vec![0u8; cb as usize]; // 尺寸由首次查询返回，buf 容量 = cb
        // SAFETY: buf.as_mut_ptr() 可写 cb 字节（vec 容量 cb）；hkey 仍有效；cb 为合法 in/out。
        let rc = unsafe { RegQueryValueExW(hkey, name.as_ptr(), std::ptr::null_mut(), &mut typ, buf.as_mut_ptr(), &mut cb) };
        unsafe { RegCloseKey(hkey) }; // SAFETY: hkey 有效，关闭释放
        if rc != 0 { return Err(PathopError::Backend(format!("RegQueryValueEx read: {rc}"))); }
        let units: Vec<u16> = buf[..cb as usize].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        let s = String::from_utf16_lossy(&units).trim_end_matches('\0').to_string();
        let kind = if typ == REG_EXPAND_SZ { ValueKind::ExpandSz } else { ValueKind::Sz };
        Ok(Some((s, kind)))
    }

    unsafe fn write_path(value: &str, kind: ValueKind) -> Result<(), PathopError> {
        let subkey = to_wide("Environment");
        let mut hkey: isize = 0;
        // SAFETY: subkey NUL 结尾 wide；&mut hkey 合法 out；sam 含 QUERY|SET 以读改写。
        let rc = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_QUERY_VALUE | KEY_SET_VALUE, &mut hkey) };
        if rc != 0 { return Err(PathopError::Backend(format!("RegOpenKeyEx: {rc}"))); }
        let name = to_wide("Path");
        let wide = to_wide(value); // 含尾 NUL，字节数 = wide.len()*2
        let typ = if kind == ValueKind::ExpandSz { REG_EXPAND_SZ } else { REG_SZ };
        // SAFETY: wide.as_ptr() 指 wide.len()*2 字节合法缓冲（含 NUL）；cb 与之精确匹配；hkey 有效。
        let rc = unsafe { RegSetValueExW(hkey, name.as_ptr(), 0, typ, wide.as_ptr() as *const u8, (wide.len() * 2) as u32) };
        unsafe { RegCloseKey(hkey) }; // SAFETY: hkey 有效
        if rc != 0 { return Err(PathopError::Backend(format!("RegSetValueEx: {rc}"))); }
        Ok(())
    }

    unsafe fn broadcast() {
        let env = to_wide("Environment"); // lparam 指 "Environment" wide 串，调用期间存活
        let mut result: usize = 0;
        // SAFETY: HWND_BROADCAST/WM_SETTINGCHANGE 常量；env.as_ptr() NUL 结尾 wide；&mut result 合法 out；超时 2000ms。
        unsafe { SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, 0, env.as_ptr(), SMTO_ABORTIFHUNG, 2000, &mut result) };
    }

    pub struct RealBackend;
    impl WindowsBackend for RealBackend {
        fn abs(&self, dir: &str) -> Result<String, PathopError> {
            let a = std::path::absolute(dir).map_err(|e| PathopError::Backend(format!("abs: {e}")))?;
            Ok(clean_windows(&a.to_string_lossy()))
        }
        fn read_user_path(&self) -> Result<Option<(String, ValueKind)>, PathopError> { unsafe { read_path() } }
        fn write_user_path(&mut self, value: &str, kind: ValueKind) -> Result<(), PathopError> { unsafe { write_path(value, kind) } }
        fn process_path(&self) -> String { std::env::var("PATH").unwrap_or_default() }
        fn set_process_path(&mut self, value: &str) { unsafe { std::env::set_var("PATH", value); } }
        fn env(&self, name: &str) -> Option<String> { std::env::var(name).ok() }
        fn broadcast(&mut self) { unsafe { broadcast(); } }
    }
}

// ---- 公开平台 API（cfg）----

#[cfg(not(windows))]
pub fn is_in_user_path(_dir: &str) -> Result<bool, PathopError> { Ok(false) }
#[cfg(not(windows))]
pub fn ensure_in_path(_dir: &str) -> Result<bool, PathopError> { Ok(false) }
#[cfg(not(windows))]
pub fn ensure_in_path_with_broadcast(_dir: &str) -> Result<bool, PathopError> { Ok(false) }

#[cfg(windows)]
pub fn is_in_user_path(dir: &str) -> Result<bool, PathopError> { is_in_user_path_windows(dir, &win::RealBackend) }
#[cfg(windows)]
pub fn ensure_in_path(dir: &str) -> Result<bool, PathopError> { ensure_in_path_windows(dir, false, &mut win::RealBackend) }
#[cfg(windows)]
pub fn ensure_in_path_with_broadcast(dir: &str) -> Result<bool, PathopError> { ensure_in_path_windows(dir, true, &mut win::RealBackend) }
```

- [x] **Step 4: 加模块声明** — `crates/arbitrate-core/src/lib.rs`

Edit: old `pub mod imgrefs;\npub mod paths;` → new `pub mod imgrefs;\npub mod pathop;\npub mod paths;`

- [x] **Step 5: 运行测试确认通过**

Run: `cd /Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs && cargo test --workspace --locked --test pathop`
Expected: PASS — 14 passed（mac/linux；真实 Windows 后端 `cfg(windows)` mac 门禁不编译，覆盖纯算法 + 替身）。**Windows CI 首验须含 `cargo build` + `cargo clippy -- -D warnings` + `cargo test`**（edition 2024 `unsafe extern` + `unsafe_op_in_unsafe_fn` 仅 Windows 编译期暴露，评审 P1）

- [x] **Step 6: 门禁 + 提交**

```bash
set -e
cd /Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs
cargo fmt --all
cargo fmt --check --all
cargo clippy --workspace --locked -- -D warnings
cargo test --workspace --locked
git diff --exit-code Cargo.lock
git add crates/arbitrate-core/src/pathop.rs crates/arbitrate-core/src/lib.rs crates/arbitrate-core/tests/pathop.rs
git commit -m "feat(core): pathop platform PATH helpers + injectable Windows algorithm (plan 4 task 8)"
```
Expected: 门禁全绿，250 passed + 1 ignored（236 + 14），Cargo.lock 零变化，提交成功。

---
### Task 9: core::keymap — 动作枚举 + 默认表 + 数据层查询

**Files:**
- Create: `crates/arbitrate-core/src/keymap.rs`
- Modify: `crates/arbitrate-core/src/lib.rs`（加 `pub mod keymap;`）
- Test: `crates/arbitrate-core/tests/keymap.rs`

**Interfaces:**
- Consumes: 无（std-only；M0 数据层，无 bubbletea）
- Produces（Task 10 parse 层 + M2/M3 TUI 接线消费）:
  - `pub enum Action { None, Submit, Steer, Interrupt, Cancel, CycleMode, ClearScreen, ToggleReasoning, ToggleHelp, HistoryPrev, HistoryNext }`（Clone/Copy/Debug/PartialEq/Eq/PartialOrd/Ord/Hash）
  - `impl Action { pub fn as_str(self) -> &'static str; pub fn parse(name: &str) -> Option<Action> }`
  - `pub struct ActionInfo { pub action: Action, pub default_key: &'static str, pub description: &'static str }`
  - `pub fn all_actions() -> &'static [ActionInfo]`（10 行稳定序）
  - `pub fn is_reserved_key(key: &str) -> bool`、`pub fn is_known_action(name: &str) -> bool`
  - `pub struct Binding { pub action: Action, pub key: String, pub source: &'static str, pub description: &'static str }`
  - `pub struct KeyMap { by_action: BTreeMap<Action,String>, by_key: HashMap<String,Action>, source: HashMap<Action,&'static str> }`（字段 pub(crate) 供 Task 10 apply）
  - `impl KeyMap { pub fn new_default() -> KeyMap; pub fn resolve(&self, key: &str) -> Action; pub fn key_for(&self, action: Action) -> &str; pub fn source_for(&self, action: Action) -> &str; pub fn snapshot(&self) -> Vec<Binding> }`

- [x] **Step 1: 写失败测试** — `crates/arbitrate-core/tests/keymap.rs`

```rust
//! core::keymap 数据层契约测试（规格：上游 internal/keymap actions.go/keymap.go，
//! Rust 自有重写）。M0 无 bubbletea：Resolve 收 &str 规范键串（D4-12）。
//! 品牌适配：Interrupt 描述 "Quit arbitrate"（D4-4）。

use arbitrate_core::keymap::{Action, KeyMap, all_actions, is_known_action, is_reserved_key};

#[test]
fn new_default_binds_all() {
    let km = KeyMap::new_default();
    for info in all_actions() {
        assert_eq!(km.key_for(info.action), info.default_key);
        assert_eq!(km.source_for(info.action), "default");
        assert_eq!(km.resolve(info.default_key), info.action);
    }
}

#[test]
fn resolve_table() {
    let km = KeyMap::new_default();
    for (key, want) in [
        ("enter", Action::Submit), ("alt+enter", Action::Steer), ("ctrl+c", Action::Interrupt),
        ("esc", Action::Cancel), ("shift+tab", Action::CycleMode), ("ctrl+l", Action::ClearScreen),
        ("ctrl+r", Action::ToggleReasoning), ("?", Action::ToggleHelp),
        ("up", Action::HistoryPrev), ("down", Action::HistoryNext),
        ("ctrl+z", Action::None), // 未绑定
    ] {
        assert_eq!(km.resolve(key), want, "resolve({key:?})");
    }
}

#[test]
fn unbound_none() {
    let km = KeyMap::new_default();
    assert_eq!(km.resolve("ctrl+q"), Action::None);
    assert_eq!(km.resolve(""), Action::None);
}

#[test]
fn key_for_source_for_unknown() {
    let km = KeyMap::new_default();
    assert_eq!(km.key_for(Action::Submit), "enter");
    assert_eq!(km.source_for(Action::Submit), "default");
    assert_eq!(km.key_for(Action::None), "");   // 未知动作
    assert_eq!(km.source_for(Action::None), "");
}

#[test]
fn snapshot_order() {
    let km = KeyMap::new_default();
    let snap = km.snapshot();
    assert_eq!(snap.len(), 10);
    for (row, info) in snap.iter().zip(all_actions()) {
        assert_eq!(row.action, info.action);
        assert_eq!(row.key, info.default_key);
        assert_eq!(row.source, "default");
        assert_eq!(row.description, info.description);
    }
}

#[test]
fn snapshot_descriptions_brand() {
    let km = KeyMap::new_default();
    let snap = km.snapshot();
    let interrupt = snap.iter().find(|b| b.action == Action::Interrupt).unwrap();
    assert!(interrupt.description.contains("arbitrate"), "品牌适配 Quit arbitrate");
    assert!(!interrupt.description.contains("seek"));
    let cycle = snap.iter().find(|b| b.action == Action::CycleMode).unwrap();
    assert!(cycle.description.contains("ask → plan → yolo"));
}

#[test]
fn reserved_keys() {
    for k in ["tab", "backspace", "space", "shift+enter", "pgup", "pgdn", "home", "end"] {
        assert!(is_reserved_key(k), "{k} 应保留");
    }
    // enter/esc/up/down 可重绑（非保留）
    for k in ["enter", "esc", "up", "down"] {
        assert!(!is_reserved_key(k), "{k} 不应保留");
    }
}

#[test]
fn known_actions() {
    for info in all_actions() {
        assert!(is_known_action(info.action.as_str()));
    }
    assert!(!is_known_action("submmit")); // typo
    assert!(!is_known_action(""));        // ActionNone 非已知
    assert!(!is_known_action("Submit"));  // 大小写敏感
}
```

- [x] **Step 2: 运行测试确认失败**

Run: `cd /Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs && cargo test --workspace --locked --test keymap`
Expected: FAIL — `unresolved import arbitrate_core::keymap`

- [x] **Step 3: 写实现** — `crates/arbitrate-core/src/keymap.rs`

```rust
//! core::keymap — 用户可定制键绑定（封闭动作集 + 默认表 + 双向查询）。
//! 规格：上游 internal/keymap（只读参考），Rust std-only 重写。
//! M0 数据层：Resolve 收规范键串 &str（bubbletea KeyMsg.String() 同形），
//! 不引入 bubbletea（core 零 TUI 依赖）；KeyPressMsg→&str 转换 M2/M3 接线（D4-12）。
//! 品牌适配：Interrupt 描述 "Quit arbitrate"，警告 `arbitrate keys actions`（D4-4）。
//! 绑定文件路径 ~/.arbitrate/keybindings.toml（paths::user_keybindings，无回退）。

use std::collections::{BTreeMap, HashMap};

/// 封闭可重绑动作集。None = Resolve 无匹配（调用方 fall through 到默认处理）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Action {
    None, Submit, Steer, Interrupt, Cancel, CycleMode,
    ClearScreen, ToggleReasoning, ToggleHelp, HistoryPrev, HistoryNext,
}

impl Action {
    /// 用户面标识符（toml 键 / `arbitrate keys actions` 列）。None → ""。
    pub fn as_str(self) -> &'static str {
        match self {
            Action::None => "",
            Action::Submit => "submit",
            Action::Steer => "steer",
            Action::Interrupt => "interrupt",
            Action::Cancel => "cancel",
            Action::CycleMode => "cycle-mode",
            Action::ClearScreen => "clear-screen",
            Action::ToggleReasoning => "toggle-reasoning",
            Action::ToggleHelp => "toggle-help",
            Action::HistoryPrev => "history-prev",
            Action::HistoryNext => "history-next",
        }
    }
    /// 名称 → 动作（大小写敏感）。未知/空 → None（Option）。
    pub fn parse(name: &str) -> Option<Action> {
        match name {
            "submit" => Some(Action::Submit),
            "steer" => Some(Action::Steer),
            "interrupt" => Some(Action::Interrupt),
            "cancel" => Some(Action::Cancel),
            "cycle-mode" => Some(Action::CycleMode),
            "clear-screen" => Some(Action::ClearScreen),
            "toggle-reasoning" => Some(Action::ToggleReasoning),
            "toggle-help" => Some(Action::ToggleHelp),
            "history-prev" => Some(Action::HistoryPrev),
            "history-next" => Some(Action::HistoryNext),
            _ => None,
        }
    }
}

/// 一个动作的标识符 + 默认键 + 一行描述。/help 与 `arbitrate keys actions` 直接渲染。
pub struct ActionInfo {
    pub action: Action,
    pub default_key: &'static str,
    pub description: &'static str,
}

/// 稳定显示序（新增动作须同步 M2/M3 dispatch）。Interrupt 描述品牌适配。
const ALL_ACTIONS: &[ActionInfo] = &[
    ActionInfo { action: Action::Submit, default_key: "enter", description: "Submit the current input (or queue while streaming)" },
    ActionInfo { action: Action::Steer, default_key: "alt+enter", description: "Interrupt the current stream and inject the input as a steer" },
    ActionInfo { action: Action::Interrupt, default_key: "ctrl+c", description: "Quit arbitrate (saves the session first)" },
    ActionInfo { action: Action::Cancel, default_key: "esc", description: "Cancel: stream / review-entry / setup-wizard / open picker" },
    ActionInfo { action: Action::CycleMode, default_key: "shift+tab", description: "Cycle permission mode: ask → plan → yolo" },
    ActionInfo { action: Action::ClearScreen, default_key: "ctrl+l", description: "Clear the visible terminal (scrollback preserved)" },
    ActionInfo { action: Action::ToggleReasoning, default_key: "ctrl+r", description: "Toggle whether reasoning chunks render in the transcript" },
    ActionInfo { action: Action::ToggleHelp, default_key: "?", description: "Open the help overlay (only when input is empty)" },
    ActionInfo { action: Action::HistoryPrev, default_key: "up", description: "Previous prompt from history (only when input is empty)" },
    ActionInfo { action: Action::HistoryNext, default_key: "down", description: "Next prompt from history" },
];

pub fn all_actions() -> &'static [ActionInfo] { ALL_ACTIONS }

/// 绝不可重绑到非默认动作的键（破坏 textarea/picker）。parse 时拒绝。
/// enter/esc/up/down 不在此列——它们可作 Submit/Cancel/History 源重绑。
const RESERVED_KEYS: &[&str] = &["tab", "backspace", "space", "shift+enter", "pgup", "pgdn", "home", "end"];

pub fn is_reserved_key(key: &str) -> bool { RESERVED_KEYS.contains(&key) }
pub fn is_known_action(name: &str) -> bool { Action::parse(name).is_some() }

/// Snapshot 一行——/help 与 `arbitrate keys list --json` 渲染所需元数据。
pub struct Binding {
    pub action: Action,
    pub key: String,
    pub source: &'static str, // "default" | "user"
    pub description: &'static str,
}

/// 已解析的 (action→key) 与 (key→action) 表。构造后不可变（Resolve 是热路径单查）。
/// 字段 pub(crate) 供 Task 10 parse 层 apply。
pub struct KeyMap {
    pub(crate) by_action: BTreeMap<Action, String>,
    pub(crate) by_key: HashMap<String, Action>,
    pub(crate) source: HashMap<Action, &'static str>,
}

impl KeyMap {
    /// 全动作绑默认键。无文件 / 文件被拒时用。
    pub fn new_default() -> KeyMap {
        let mut by_action = BTreeMap::new();
        let mut by_key = HashMap::new();
        let mut source = HashMap::new();
        for info in all_actions() {
            by_action.insert(info.action, info.default_key.to_string());
            by_key.insert(info.default_key.to_string(), info.action);
            source.insert(info.action, "default");
        }
        KeyMap { by_action, by_key, source }
    }
    /// 规范键串 → 动作，无绑定 → None。
    pub fn resolve(&self, key: &str) -> Action {
        self.by_key.get(key).copied().unwrap_or(Action::None)
    }
    /// 动作当前绑定键，未知 → ""。
    pub fn key_for(&self, action: Action) -> &str {
        self.by_action.get(&action).map(|s| s.as_str()).unwrap_or("")
    }
    /// "default" | "user"，未知 → ""。
    pub fn source_for(&self, action: Action) -> &str {
        self.source.get(&action).copied().unwrap_or("")
    }
    /// 当前绑定，AllActions 稳定显示序。
    pub fn snapshot(&self) -> Vec<Binding> {
        all_actions().iter().map(|info| Binding {
            action: info.action,
            key: self.key_for(info.action).to_string(),
            source: self.source_for(info.action),
            description: info.description,
        }).collect()
    }
}
```

- [x] **Step 4: 加模块声明** — `crates/arbitrate-core/src/lib.rs`

Edit: old `pub mod imgrefs;\npub mod pathop;` → new `pub mod imgrefs;\npub mod keymap;\npub mod pathop;`

- [x] **Step 5: 运行测试确认通过**

Run: `cd /Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs && cargo test --workspace --locked --test keymap`
Expected: PASS — 8 passed

- [x] **Step 6: 门禁 + 提交**

```bash
set -e
cd /Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs
cargo fmt --all
cargo fmt --check --all
cargo clippy --workspace --locked -- -D warnings
cargo test --workspace --locked
git diff --exit-code Cargo.lock
git add crates/arbitrate-core/src/keymap.rs crates/arbitrate-core/src/lib.rs crates/arbitrate-core/tests/keymap.rs
git commit -m "feat(core): keymap action enum + default table + data-layer queries (plan 4 task 9)"
```
Expected: 门禁全绿，258 passed + 1 ignored（250 + 8），Cargo.lock 零变化，提交成功。

---
### Task 10: core::keymap — TOML 子集解析 + Load/Check + 警告

**Files:**
- Modify: `crates/arbitrate-core/src/keymap.rs`（追加 parse 层）
- Test: `crates/arbitrate-core/tests/keymap_parse.rs`

**Interfaces:**
- Consumes: Task 9 `Action`/`Action::parse`/`KeyMap`（pub(crate) 字段）/`all_actions`/`is_reserved_key`/`is_known_action`
- Produces（M2 `arbitrate keys` CLI / M3 TUI 启动接线）:
  - `pub fn load(path: &Path, warn: &mut dyn Write) -> (KeyMap, bool)`
  - `pub fn load_bytes(data: &[u8], path: &str, warn: &mut dyn Write) -> (KeyMap, bool)`
  - `pub fn check(path: &Path) -> Vec<String>`
  - `pub fn parse_toml_bindings(input: &str) -> Result<BTreeMap<String, String>, String>`

- [x] **Step 1: 写失败测试** — `crates/arbitrate-core/tests/keymap_parse.rs`

```rust
//! core::keymap parse 层契约测试（规格：上游 internal/keymap parse.go，Rust 自有重写）。
//! 手写 TOML 子集（禁 BurntSushi，D4-13）；警告形状逐字 + 品牌适配（D4-4/D4-14）。
//! swap 硬化为 arbitrate 有意偏差（D4-14）。虚构键值，不触真实 ~/.arbitrate。

use arbitrate_core::keymap::{Action, KeyMap, check, load, load_bytes};

mod common;
use common::TempRoot;

fn warn_string(data: &[u8], path: &str) -> (KeyMap, bool, String) {
    let mut buf: Vec<u8> = Vec::new();
    let (km, ok) = load_bytes(data, path, &mut buf);
    (km, ok, String::from_utf8(buf).unwrap())
}

#[test]
fn missing_file_defaults() {
    let root = TempRoot::new();
    let p = root.0.join("nope.toml");
    let mut buf: Vec<u8> = Vec::new();
    let (km, ok) = load(&p, &mut buf);
    assert!(ok); // 无文件 → 静默默认
    assert!(buf.is_empty());
    assert_eq!(km.resolve("enter"), Action::Submit);
}

#[test]
fn syntax_error_rejects() {
    let (km, ok, w) = warn_string(b"[bindings\nsubmit = \"x\"", "f.toml");
    assert!(!ok);
    assert!(w.contains("toml syntax error"), "{w}");
    assert!(w.contains("(using defaults)"));
    assert_eq!(km.resolve("enter"), Action::Submit); // 全回退默认
}

#[test]
fn unknown_action_soft() {
    let (km, ok, w) = warn_string(b"[bindings]\nsubmmit = \"ctrl+x\"\n", "f.toml");
    assert!(ok); // 软错误：丢该行，其余仍应用
    assert!(w.contains("unknown action \"submmit\""), "{w}");
    assert!(w.contains("available: submit, steer"));
    assert_eq!(km.resolve("ctrl+x"), Action::None); // 未应用
}

#[test]
fn empty_value_soft() {
    let (km, ok, w) = warn_string(b"[bindings]\nsubmit = \"\"\n", "f.toml");
    assert!(ok);
    assert!(w.contains("action \"submit\" has empty value"), "{w}");
    assert_eq!(km.key_for(Action::Submit), "enter"); // 仍默认
}

#[test]
fn reserved_key_soft() {
    let (km, ok, w) = warn_string(b"[bindings]\nclear-screen = \"tab\"\n", "f.toml");
    assert!(ok);
    assert!(w.contains("reserved key \"tab\" cannot be rebound"), "{w}");
    assert!(w.contains("clear-screen = \"tab\""));
    assert_eq!(km.key_for(Action::ClearScreen), "ctrl+l");
}

#[test]
fn unrecognised_key_soft() {
    let (km, ok, w) = warn_string(b"[bindings]\nsubmit = \"weird key with spaces\"\n", "f.toml");
    assert!(ok);
    assert!(w.contains("unrecognised key"), "{w}");
    assert!(w.contains("arbitrate keys actions"), "品牌适配命令名");
    assert_eq!(km.key_for(Action::Submit), "enter");
}

#[test]
fn valid_rebind_reverse() {
    let (km, ok, w) = warn_string(b"[bindings]\nsubmit = \"ctrl+x\"\n", "f.toml");
    assert!(ok);
    assert!(w.is_empty());
    assert_eq!(km.resolve("ctrl+x"), Action::Submit);
    assert_eq!(km.resolve("enter"), Action::None); // 旧键反查移除
    assert_eq!(km.key_for(Action::Submit), "ctrl+x");
    assert_eq!(km.source_for(Action::Submit), "user");
}

#[test]
fn case_insensitive_key() {
    let (km, ok, _) = warn_string(b"[bindings]\nsubmit = \"Ctrl+X\"\n", "f.toml");
    assert!(ok);
    assert_eq!(km.resolve("ctrl+x"), Action::Submit); // normalizeKey 小写
}

#[test]
fn conflict_rejects_whole_file() {
    let (km, ok, w) = warn_string(b"[bindings]\nsubmit = \"ctrl+x\"\nsteer = \"ctrl+x\"\n", "f.toml");
    assert!(!ok);
    assert!(w.contains("bound to multiple actions"), "{w}");
    assert!(w.contains("rejecting whole file"));
    assert_eq!(km.resolve("ctrl+x"), Action::None); // 全回退
    assert_eq!(km.key_for(Action::Submit), "enter");
}

#[test]
fn swap_hardening() {
    // D4-14：上游 apply 循环 delete-oldKey 在交换绑定时丢反查条目（无上游测试）。
    // arbitrate 硬化：apply 后从 by_action 重建 by_key。
    let (km, ok, w) = warn_string(b"[bindings]\nsubmit = \"alt+enter\"\nsteer = \"enter\"\n", "f.toml");
    assert!(ok);
    assert!(w.is_empty());
    assert_eq!(km.resolve("enter"), Action::Steer);
    assert_eq!(km.resolve("alt+enter"), Action::Submit);
    assert_eq!(km.key_for(Action::Submit), "alt+enter");
    assert_eq!(km.key_for(Action::Steer), "enter");
}

#[test]
fn check_good_empty() {
    let root = TempRoot::new();
    let p = root.0.join("kb.toml");
    std::fs::write(&p, b"[bindings]\nsubmit = \"ctrl+x\"\n").unwrap();
    assert!(check(&p).is_empty());
}

#[test]
fn check_bad_lines() {
    let root = TempRoot::new();
    let p = root.0.join("kb.toml");
    std::fs::write(&p, b"[bindings]\nsubmit = \"ctrl+x\"\nsteer = \"ctrl+x\"\n").unwrap();
    let lines = check(&p);
    assert!(!lines.is_empty());
    assert!(lines.iter().any(|l| l.contains("rejecting whole file")));
}

#[test]
fn inline_display_path() {
    let (_, _, w) = warn_string(b"[bindings]\nsubmmit = \"ctrl+x\"\n", "");
    assert!(w.contains("<inline>"), "{w}");
}

#[test]
fn other_sections_ignored() {
    let (km, ok, w) = warn_string(b"[other]\nfoo = \"bar\"\n[bindings]\nsubmit = \"ctrl+x\"\n", "f.toml");
    assert!(ok);
    assert!(w.is_empty());
    assert_eq!(km.resolve("ctrl+x"), Action::Submit);
}

#[test]
fn duplicate_key_rejects() {
    let (_, ok, w) = warn_string(b"[bindings]\nsubmit = \"a\"\nsubmit = \"b\"\n", "f.toml");
    assert!(!ok);
    assert!(w.contains("toml syntax error"), "{w}");
}

#[test]
fn quoted_keys_trim() {
    let (km, ok, _) = warn_string(b"[bindings]\n\" submit \" = \"ctrl+x\"\n", "f.toml");
    assert!(ok);
    assert_eq!(km.resolve("ctrl+x"), Action::Submit); // 键 trim 后匹配
}

#[test]
fn warning_order_sorted_actions() {
    // 软警告按动作名排序（BTreeMap）：cancel < steer < submit
    let (km, ok, w) = warn_string(
        b"[bindings]\nsubmit = \"tab\"\ncancel = \"\"\nsteer = \"bad key\"\n", "f.toml");
    assert!(ok);
    let lines: Vec<&str> = w.lines().collect();
    assert_eq!(lines.len(), 3, "{w}");
    assert!(lines[0].contains("\"cancel\""), "{}", lines[0]);
    assert!(lines[1].contains("\"steer\""), "{}", lines[1]);
    assert!(lines[2].contains("\"submit\""), "{}", lines[2]);
    assert_eq!(km.key_for(Action::Submit), "enter"); // 全软丢，默认保留
}

#[test]
fn rebind_onto_default_conflict() {
    // D4-14 硬化（评审 P2-4）：单项重绑撞另一动作的未改默认键 = 真冲突。
    // submit→alt+enter 与 steer 默认 alt+enter 同键；查终表（默认+覆盖）拒全文件。
    let (km, ok, w) = warn_string(b"[bindings]\nsubmit = \"alt+enter\"\n", "f.toml");
    assert!(!ok);
    assert!(w.contains("bound to multiple actions"), "{w}");
    assert!(w.contains("rejecting whole file"));
    assert_eq!(km.key_for(Action::Submit), "enter"); // 全回退默认
    assert_eq!(km.resolve("alt+enter"), Action::Steer); // 默认反查未被覆盖
}

#[test]
fn illegal_multibyte_escape() {
    // 评审 P2-5：\u 后非 ASCII hex（界界）→ 字节级先验拒绝，不 panic（UTF-8 中间切片）。
    let (km, ok, w) = warn_string("[bindings]\nsubmit = \"\\u界界\"\n".as_bytes(), "f.toml");
    assert!(!ok);
    assert!(w.contains("toml syntax error"), "{w}");
    assert_eq!(km.resolve("enter"), Action::Submit); // 全回退
}

#[test]
fn quoted_section() {
    // D4-13 承诺 quoted 段名（评审 P2-5）：["bindings"] 与 [bindings] 等价收集。
    let (km, ok, w) = warn_string(b"[\"bindings\"]\nsubmit = \"ctrl+x\"\n", "f.toml");
    assert!(ok);
    assert!(w.is_empty());
    assert_eq!(km.resolve("ctrl+x"), Action::Submit);
}
```

- [x] **Step 2: 运行测试确认失败**

Run: `cd /Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs && cargo test --workspace --locked --test keymap_parse`
Expected: FAIL — `unresolved import arbitrate_core::keymap::{check, load, load_bytes}`

- [x] **Step 3: 写实现** — 追加到 `crates/arbitrate-core/src/keymap.rs` 末尾

```rust
// ---- parse 层（Task 10）----

use std::collections::HashSet;
use std::io::Write;
use std::path::Path;

/// 手写 TOML 子集解析（禁 BurntSushi crate，D4-13）。仅 [bindings] 段收集；
/// 其余段语法校验但忽略。值必须字符串（basic/literal）；非字符串值拒绝
/// （文档化天花板：keybindings 只需字符串）。重复键/段拒绝。BTreeMap 排序迭代。
pub fn parse_toml_bindings(input: &str) -> Result<BTreeMap<String, String>, String> {
    let mut bindings = BTreeMap::new();
    let mut seen_sections: HashSet<String> = HashSet::new();
    let mut seen_keys: HashSet<(String, String)> = HashSet::new();
    let mut current = String::new();
    for (idx, raw) in input.lines().enumerate() {
        let lineno = idx + 1;
        let line = strip_comment(raw).trim();
        if line.is_empty() { continue; }
        if let Some(rest) = line.strip_prefix('[') {
            let inner = rest.strip_suffix(']')
                .ok_or_else(|| format!("line {lineno}: unterminated section header"))?.trim();
            // bare 或 quoted 段名（D4-13 承诺 quoted；评审 P2-5）
            let name = if inner.starts_with('"') || inner.starts_with('\'') {
                let (n, remainder) = parse_key(inner, lineno)?;
                if !remainder.trim().is_empty() {
                    return Err(format!("line {lineno}: trailing content after section name"));
                }
                n
            } else {
                if !inner.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.') {
                    return Err(format!("line {lineno}: invalid section name"));
                }
                inner.to_string()
            };
            if name.is_empty() { return Err(format!("line {lineno}: empty section name")); }
            if !seen_sections.insert(name.clone()) {
                return Err(format!("line {lineno}: duplicate section [{name}]"));
            }
            current = name;
            continue;
        }
        let (key, value) = split_key_value(line, lineno)?;
        if !seen_keys.insert((current.clone(), key.clone())) {
            return Err(format!("line {lineno}: duplicate key {key}"));
        }
        if current == "bindings" {
            bindings.insert(key, value);
        }
    }
    Ok(bindings)
}

/// 去注释（# 仅在字符串外）。basic 串处理 \\ 转义跳过，literal 串无转义。
fn strip_comment(line: &str) -> &str {
    let b = line.as_bytes();
    let (mut in_basic, mut in_literal) = (false, false);
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if in_basic {
            if c == b'\\' { i += 2; continue; }
            if c == b'"' { in_basic = false; }
        } else if in_literal {
            if c == b'\'' { in_literal = false; }
        } else {
            match c {
                b'"' => in_basic = true,
                b'\'' => in_literal = true,
                b'#' => return &line[..i],
                _ => {}
            }
        }
        i += 1;
    }
    line
}

fn split_key_value(line: &str, lineno: usize) -> Result<(String, String), String> {
    let (key, rest) = parse_key(line, lineno)?;
    let rest = rest.trim_start().strip_prefix('=')
        .ok_or_else(|| format!("line {lineno}: expected '=' after key"))?.trim_start();
    let value = parse_string_value(rest, lineno)?;
    Ok((key, value))
}

fn parse_key(line: &str, lineno: usize) -> Result<(String, &str), String> {
    if let Some(rest) = line.strip_prefix('"') {
        scan_basic(rest, lineno)
    } else if let Some(rest) = line.strip_prefix('\'') {
        scan_literal(rest, lineno)
    } else {
        let end = line.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.'))
            .unwrap_or(line.len());
        if end == 0 { return Err(format!("line {lineno}: empty key")); }
        Ok((line[..end].to_string(), &line[end..]))
    }
}

fn parse_string_value(s: &str, lineno: usize) -> Result<String, String> {
    let (v, rest) = if let Some(rest) = s.strip_prefix('"') {
        scan_basic(rest, lineno)?
    } else if let Some(rest) = s.strip_prefix('\'') {
        scan_literal(rest, lineno)?
    } else {
        return Err(format!("line {lineno}: only string values are supported"));
    };
    if !rest.trim().is_empty() {
        return Err(format!("line {lineno}: trailing content after string value"));
    }
    Ok(v)
}

/// basic 串：转义 \b \t \n \f \r \" \\ \uXXXX \UXXXXXXXX。返回 (值, 闭引号后余串)。
fn scan_basic(s: &str, lineno: usize) -> Result<(String, &str), String> {
    let b = s.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'"' => return Ok((out, &s[i + 1..])),
            b'\\' => {
                i += 1;
                if i >= b.len() { return Err(format!("line {lineno}: dangling escape")); }
                match b[i] {
                    b'b' => { out.push('\u{0008}'); i += 1; }
                    b't' => { out.push('\t'); i += 1; }
                    b'n' => { out.push('\n'); i += 1; }
                    b'f' => { out.push('\u{000C}'); i += 1; }
                    b'r' => { out.push('\r'); i += 1; }
                    b'"' => { out.push('"'); i += 1; }
                    b'\\' => { out.push('\\'); i += 1; }
                    b'u' | b'U' => {
                        let n = if b[i] == b'u' { 4 } else { 8 };
                        let start = i + 1;
                        if start + n > b.len() { return Err(format!("line {lineno}: short unicode escape")); }
                        // 先验 ASCII hex 字节（b 是 &[u8]，字节切片不劈 UTF-8）；全 ASCII →
                        // start+n 是 char 边界，str 切片安全（评审 P2-5：\u界界 走 Err 不 panic）
                        let hex_bytes = &b[start..start + n];
                        if !hex_bytes.iter().all(|c| c.is_ascii_hexdigit()) {
                            return Err(format!("line {lineno}: bad unicode escape"));
                        }
                        let cp = u32::from_str_radix(std::str::from_utf8(hex_bytes).unwrap(), 16)
                            .map_err(|_| format!("line {lineno}: bad unicode escape"))?;
                        out.push(char::from_u32(cp).ok_or_else(|| format!("line {lineno}: invalid unicode scalar"))?);
                        i = start + n;
                    }
                    _ => return Err(format!("line {lineno}: unknown escape")),
                }
            }
            _ => {
                let ch = s[i..].chars().next().unwrap();
                out.push(ch);
                i += ch.len_utf8();
            }
        }
    }
    Err(format!("line {lineno}: unterminated basic string"))
}

/// literal 串：无转义，到首个 ' 止。
fn scan_literal(s: &str, lineno: usize) -> Result<(String, &str), String> {
    match s.find('\'') {
        Some(i) => Ok((s[..i].to_string(), &s[i + 1..])),
        None => Err(format!("line {lineno}: unterminated literal string")),
    }
}

fn display_path(path: &str) -> &str {
    if path.is_empty() { "<inline>" } else { path }
}

fn normalize_key(key: &str) -> String { key.to_lowercase() }

fn known_actions_list() -> String {
    all_actions().iter().map(|i| i.action.as_str()).collect::<Vec<_>>().join(", ")
}

/// 宽松启发式：拒明显垃圾（空段/非修饰符前缀/末段含空白），接受 modifier+name 形。
fn looks_like_known_key(key: &str) -> bool {
    if key.is_empty() { return false; }
    let parts: Vec<&str> = key.split('+').collect();
    for (i, p) in parts.iter().enumerate() {
        if p.is_empty() { return false; }
        if i < parts.len() - 1 {
            match *p {
                "ctrl" | "alt" | "shift" | "cmd" | "meta" | "super" => continue,
                _ => return false,
            }
        }
    }
    let last = parts[parts.len() - 1];
    !last.contains(' ') && !last.contains('\t')
}

/// 解析文件 + 应用用户覆盖。警告逐行写 warn（不返回 error，总返回可用 KeyMap）。
/// bool = 文件被应用(true) / 全拒回退默认(false)。硬拒：toml 语法错 / 键冲突。
/// 软错（未知动作/保留键/不可识别键/空值）只丢该行 + 警告。
pub fn load(path: &Path, warn: &mut dyn Write) -> (KeyMap, bool) {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (KeyMap::new_default(), true),
        Err(e) => {
            let _ = writeln!(warn, "keybindings: read {}: {e} (using defaults)", path.display());
            return (KeyMap::new_default(), false);
        }
    };
    load_bytes(&data, &path.to_string_lossy(), warn)
}

/// 从字节解析。path 仅用于警告前缀（"" → <inline>）。语义同 load。
pub fn load_bytes(data: &[u8], path: &str, warn: &mut dyn Write) -> (KeyMap, bool) {
    let text = match std::str::from_utf8(data) {
        Ok(t) => t,
        Err(e) => {
            let _ = writeln!(warn, "keybindings: toml syntax error in {}: {e} (using defaults)", display_path(path));
            return (KeyMap::new_default(), false);
        }
    };
    let raw = match parse_toml_bindings(text) {
        Ok(m) => m,
        Err(e) => {
            let _ = writeln!(warn, "keybindings: toml syntax error in {}: {e} (using defaults)", display_path(path));
            return (KeyMap::new_default(), false);
        }
    };
    let mut km = KeyMap::new_default();
    let mut accepted: Vec<(Action, String)> = Vec::new();
    // BTreeMap 排序迭代 → 警告序与冲突 first-wins 确定（D4-14）
    for (name_raw, value) in &raw {
        let key = value.trim();
        let action_name = name_raw.trim();
        let action = match Action::parse(action_name) {
            Some(a) => a,
            None => {
                let _ = writeln!(warn, "keybindings: unknown action {:?} in {} (available: {})",
                    action_name, display_path(path), known_actions_list());
                continue;
            }
        };
        if key.is_empty() {
            let _ = writeln!(warn, "keybindings: action {:?} has empty value in {}", action_name, display_path(path));
            continue;
        }
        let key = normalize_key(key);
        if is_reserved_key(&key) {
            let _ = writeln!(warn, "keybindings: reserved key {:?} cannot be rebound ({} = {:?} in {})",
                key, action_name, key, display_path(path));
            continue;
        }
        if !looks_like_known_key(&key) {
            let _ = writeln!(warn, "keybindings: unrecognised key {:?} for action {:?} in {} (see `arbitrate keys actions` for examples)",
                key, action_name, display_path(path));
            continue;
        }
        accepted.push((action, key));
    }
    // 冲突检测：终表（默认 + 用户覆盖）同键被两动作占用 → 全文件拒（PRD §4.4 rule 4）。
    // D4-14 硬化（评审 P2-4）：查终表而非仅 accepted——单项重绑撞另一动作的未改默认键
    // 也是真冲突（上游漏检致反表静默覆盖）。swap 终表键互异 → 不拒。
    let mut final_map: BTreeMap<Action, String> = km.by_action.clone();
    for (a, k) in &accepted { final_map.insert(*a, k.clone()); }
    let mut multi: BTreeMap<String, Vec<Action>> = BTreeMap::new();
    for (a, k) in &final_map { multi.entry(k.clone()).or_default().push(*a); }
    let mut conflicts = false;
    for (key, actions) in &multi {
        if actions.len() > 1 {
            let names: Vec<&str> = actions.iter().map(|a| a.as_str()).collect();
            let _ = writeln!(warn, "keybindings: key {:?} bound to multiple actions ({}) in {} — rejecting whole file",
                key, names.join(", "), display_path(path));
            conflicts = true;
        }
    }
    if conflicts { return (KeyMap::new_default(), false); }
    // 应用覆盖
    for (a, k) in &accepted {
        km.by_action.insert(*a, k.clone());
        km.source.insert(*a, "user");
    }
    // D4-14 硬化：从 by_action 重建 by_key（上游 delete-oldKey 在交换绑定时丢反查条目）。
    // 单重绑行为不变；交换正确。arbitrate 有意偏差，验收汇总登记。
    km.by_key.clear();
    for (a, k) in &km.by_action { km.by_key.insert(k.clone(), *a); }
    (km, true)
}

/// 干跑校验（`arbitrate keys check` / CI）。返回完整警告行（空 = 有效）。
pub fn check(path: &Path) -> Vec<String> {
    let mut buf: Vec<u8> = Vec::new();
    let (_, ok) = load(path, &mut buf);
    let text = String::from_utf8_lossy(&buf);
    let trimmed = text.trim();
    let out: Vec<String> = if trimmed.is_empty() {
        Vec::new()
    } else {
        trimmed.split('\n').map(|s| s.to_string()).collect()
    };
    if !ok && out.is_empty() {
        return vec![format!("keybindings: {} rejected for unknown reason", display_path(&path.to_string_lossy()))];
    }
    out
}
```

注：`{:?}` 对 ASCII 串与 Go `%q` 逐字同形（D4-14 文档化）；非 ASCII 动作名/键的引号转义细节为天花板（keybindings 实际 ASCII）。parse 层的 `use std::collections::HashSet;`/`use std::io::Write;`/`use std::path::Path;` 置于追加代码块开头（模块内 use 可出现在任意位置，合法；rustfmt 不跨 item 移动 use，保持原位即可，门禁 fmt --check 校验）。Task 9 顶部已有 `use std::collections::{BTreeMap, HashMap};`，不重复声明。

- [x] **Step 4: 运行测试确认通过**

Run: `cd /Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs && cargo test --workspace --locked --test keymap_parse`
Expected: PASS — 20 passed

- [x] **Step 5: 门禁 + 提交**

```bash
set -e
cd /Users/deep/Documents/workspaceB/JueHArbitrate/arbitrate-rs
cargo fmt --all
cargo fmt --check --all
cargo clippy --workspace --locked -- -D warnings
cargo test --workspace --locked
git diff --exit-code Cargo.lock
git add crates/arbitrate-core/src/keymap.rs crates/arbitrate-core/tests/keymap_parse.rs
git commit -m "feat(core): keymap TOML-subset parser + load/check warnings + swap hardening (plan 4 task 10)"
```
Expected: 门禁全绿，278 passed + 1 ignored（258 + 20），Cargo.lock 零变化，提交成功。

---
## 验收汇总

### 测试统计

下表记录十个任务完成时的原计划测试口径。终审修订后的实际验收见下节。

| 模块 | Task | 新增测试 | 累计套件 |
|------|------|---------|---------|
| i18n（catalogue/Bundle/formatter） | 1 | 13 | 177 |
| i18n（resolve/global/并发） | 2 | 5 | 182 |
| diff | 3 | 10 | 192 |
| imgrefs | 4 | 8 | 200 |
| filerefs | 5 | 11 | 211 |
| projectmd | 6 | 12 | 223 |
| childenv | 7 | 13 | 236 |
| pathop | 8 | 14 | 250 |
| keymap（数据层） | 9 | 8 | 258 |
| keymap（parse 层） | 10 | 20 | 278 |

新增合计 **114**；起始 164 passed + 1 ignored（计划 3 末 HEAD 4a4d734）→ 终态 **278 passed + 1 ignored**。pathop 的 `cfg(not(windows))` 测试（path_contains_dir_posix + nonwindows_noops）仅 mac/linux 跑，mac/linux tally 计 14；Windows 平台跳过这 2 个 → 12（真实 Windows 后端无单元测试，绝不触真实注册表/PATH，正确性靠 Windows CI 首验 + 替身覆盖纯算法）。keymap parse 层 20 = 原 17 + 评审新增 3（rebind_onto_default_conflict / illegal_multibyte_escape / quoted_section）。每 Task 门禁 `set -e` 全绿 + Cargo.lock 零变化 + 独立提交。

### M0.4 验收映射（plans/m0-scaffold-core.md 模块行）

- **i18n**：参数数量/顺序/类型/转义语义显式断言（format_message KAT + Go 错误标记 `%!s(int=7)`/`%!s(MISSING)`）；en/zh 同步核对（14 键双语 catalogue + fallback 链）；lang.persist_failed 路径品牌适配 ~/.arbitrate。✅ Task 1–2
- **diff**：精确 LCS tie-break + splitLines + makeHunks 窗口合并 + MaxHunks=8 + oldStart 恒 1；10 手算 KAT。✅ Task 3
- **imgrefs**：go_ext（dotfile ".png"→".png"）+ cleanToken 循环剥离 + 8 扩展名 + HashSet 去重 + stat is_file 门。✅ Task 4
- **filerefs**：rich: 前缀绑定 + verbatim 回退（stat 注入缝替代 Chdir，并行安全）+ go_ext/cleanToken 独立重复（上游 PRD D-F2 包独立）。✅ Task 5
- **projectmd**：absolute + NotFound/NotADirectory 上溯（MAX_ASCEND=5）+ 65536 字节截断 + 3 轮 UTF-8 丢弃循环 + 精确标记格式。✅ Task 6
- **childenv**：名称敏感谓词（AUTH/USER/ACCOUNT 非敏感）+ 顺序保持 scrub + keep 精确 + 环境序 withheld（安全语义不简化）。✅ Task 7
- **pathop**：测试注入注册表/通知替身，不修改开发者真实 PATH；Windows 纯算法跨平台 stub 验证；真实注册表 FFI 后端 M0 建（std-only `unsafe extern`，edition 2024 两层 unsafe 合规，cfg(windows) 仅 Windows CI build+clippy+test 首验）；CLI/交互提示 M2/M3 接线。✅ Task 8
- **keymap**：M0 用 Rust 重写纯数据用例（Action 枚举 + 默认表 + Resolve(&str) + TOML 子集 parse + Load/Check 警告）；无 bubbletea。✅ Task 9–10

### 延后登记（执行末追加 docs/deferred.md「计划 4 末」段）

- **pathop Windows 交互 PATH 提示**：CLI 交互流程（提示用户确认写入 PATH）→ M3。底层注册表 FFI（RegOpenKeyExW/RegQueryValueExW/RegSetValueExW/RegCloseKey）+ WM_SETTINGCHANGE 广播（SendMessageTimeoutW）**M0 已建**（std-only `unsafe extern`，RealBackend，去 NotYetImplemented；edition 2024 两层 unsafe 合规——`unsafe extern` 声明 + unsafe fn 内显式 `unsafe {}` 块 + SAFETY 注释；cfg(windows) 段 mac/linux 门禁不编译，正确性靠 **Windows CI 首验 build + clippy `-D warnings` + test** + 替身覆盖纯算法）。**UNC host/share 段的 `..` 保护**仍延后 M3（当前词典 helper 尚不保护这两段）
- **keymap TUI 集成**：bubbletea KeyPressMsg→&str 规范键转换 + update_key dispatch 接线 + `arbitrate keys` CLI 子命令 → M2/M3（M0 仅数据层）
- **TOML 子集天花板**：非字符串值拒绝、仅 [bindings] 段收集、重复键/段拒绝（keybindings 只需字符串；完整 TOML 规范超范围，禁 BurntSushi）。bare/quoted 段名均支持（评审 P2-5）；`\uXXXX`/`\UXXXXXXXX` 转义先验 ASCII hex 字节，非法（含多字节 UTF-8）→ 语法错误拒绝不 panic
- **i18n quote() IsPrint 近似**：is_control 近似 Go strconv.IsPrint（Unicode 可打印判定天花板；ASCII 逐字同形）
- **i18n formatter 多余参数偏差**：extra args 忽略（Go fmt 追加 `%!(EXTRA ...)`；arbitrate 文档化偏差， catalogue 调用均参数齐备）
- **projectmd absolute 无词典 Clean**：std::path::absolute 不做 lexical Clean（".." 段残留天花板；M2 调用方传已解析 cwd；m0 表 paths 依赖 = M2 接线级）
- **pathop fold Unicode 天花板**：eq_ignore_ascii_case 近似 Go EqualFold（Unicode 简单折叠；PATH 实际 ASCII）
- **keymap {:?} vs %q**：ASCII 逐字同形，非 ASCII 引号转义细节天花板

### 上游注释漂移（钉死实际行为，非注释）

- **childenv Withheld "sorted"**：上游注释称排序，实际环境序（withheld_env_order_not_sorted 测试钉死）
- **pathop userPathContains "%VAR%"**：上游注释称展开 %VAR%，实际 os.ExpandEnv 是 $-风格（expand_env_table + user_path_contains_expand_fold 钉死 %VAR% 字面不展开）

### 上游行为偏差（有意硬化，arbitrate 自有，需验收登记）

- **keymap swap 反查重建（D4-14）**：上游 apply 循环 `delete(byKey, oldKey)` 在交换绑定（submit↔steer）时删除前一动作刚设的新反查条目，致 Resolve 丢键（上游 keymap_test.go 无 swap 覆盖）。arbitrate 硬化：apply 后从 by_action 重建 by_key。单重绑行为不变；swap_hardening 测试为 Rust 自有规格。
- **keymap 冲突警告确定性（D4-14）**：上游冲突警告遍历 byKey map（Go 随机序）；arbitrate 用 BTreeMap 排序键迭代，多冲突警告序确定。单冲突行为不变。
- **keymap 默认键碰撞硬化（D4-14，评审 P2-4）**：上游冲突检测仅查 accepted 覆盖表，漏检单项重绑撞另一动作的未改默认键（submit→alt+enter 撞 steer 默认 alt+enter），apply 重建反表时静默覆盖（Steer 盖 Submit），文件却报成功。arbitrate 硬化：查终表（默认 + 用户覆盖）同键多动作 → 拒全文件。rebind_onto_default_conflict 测试为 Rust 自有规格。

### 品牌适配清单（D4-4，仅以下项，无全局字符串替换）

- i18n `lang.persist_failed`：~/.seek → ~/.arbitrate（en + zh 仅路径变更）
- keymap Interrupt 描述："Quit seek" → "Quit arbitrate"
- keymap unrecognised-key 警告：`seek keys actions` → `arbitrate keys actions`
- **保留不动**：SEEK_LANG / SEEK_* 环境名、keys 标识符、placeholders、glyphs、DeepSeek/attribution/协议标识符（wire 级契约）

### 零依赖确认

计划 4 不触任何 Cargo.toml / Cargo.lock。core 外部依赖仍仅 sha2 / serde / serde_json。i18n mini-formatter、diff LCS、TOML 子集解析器、pathop clean/expand_env、Go os.Expand 端口均 std-only 手写。每 Task 门禁 `git diff --exit-code Cargo.lock` 校验零变化。

### 执行交接

计划评审通过后，按 subagent-driven-development（推荐）或 executing-plans 逐 Task 执行。起始 HEAD 4a4d734（main，无 remote，identity repo-local bluedeep9）。每 Task：task-brief 提取 → haiku implementer 全代码转录 → review-package（cwd=arbitrate-rs，控制器移包至外层 .superpowers/sdd/）→ sonnet reviewer（spec + quality 双裁决）→ 控制器门禁重跑 → minor 登记 → 下一 Task。10 Task 全完成后 opus 整分支终审 → finishing-a-development-branch → docs/deferred.md 追加「计划 4 末」段 → M0.4 验收。

### 实施结果

- 十个任务在 `feat/m0-plan4-batch-bd` 实施并逐项评审。上面的代码块保留为施工草案，最终接口和行为以实施分支代码为准。
- 终审修复覆盖环境过滤的非 Unicode 输入、Windows PATH 写入前校验、注册表返回类型与 UTF-16 验证，以及 TOML 单层段和根级键的语法校验。`WindowsBackend::process_path` 改为返回 `Option<OsString>`，以区分缺失值与不可转换值。图片和文档用例使用独立进程工作目录，已有真实文件检查在含空格临时目录下通过。
- 单独修复既有 HTTP 测试辅助服务的连接读取竞争（`60e004d`）。其回归新增 1 项测试，终审补充 11 项，最终 macOS Rust 全套 **290 通过、0 失败、1 项原有忽略**。
- Rust 1.90.0 下格式检查、严格 clippy、Windows GNU 目标含测试的 check 与严格 clippy，以及 Linux musl 目标 check 均通过。`Cargo.lock`、依赖清单和冻结资产相对 `4a4d734` 无改动。
- 三平台远程 CI 和 Windows 原生链接、运行仍待有 runner 后记录。下游接线和已知边界登记在产品仓库 `docs/deferred.md` 的「计划 4 末」。
- 复评修复三项 P2：Windows 写入前检查进程 PATH 的 UTF-16 长度，原生进程写入可报告错误及持久化部分成功；字面 `$` 路径优先匹配，保留原有变量展开；键位校验拒绝解码后的控制字符，保留默认绑定。新增 4 项 Rust 回归，macOS 工作区复测 **294 通过、0 失败、1 项原有忽略**。格式、严格 clippy、Windows GNU 目标含测试的 check 与严格 clippy、Linux musl 目标 check 均通过。
