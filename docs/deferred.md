# 延后登记（计划 1 末）

- core 15 包实现与测试：计划 2（批A）/ 计划 3（批C）/ 计划 4（批B+D）
- 类型化 core 配置/消息样例：随计划 2–4（D12）；场景清单已登记 docs/m2-scenarios.md
- state 重启/resume、distill 基础能力、sysprompt 适配、-install flag 与短路测试：M2
- keymap 集成场景：M2/M3；Windows 交互 PATH 提示：M3
- friction/fileroute：M2；schedule_wakeup/routines：M4
- 冻结前端契约提取与接口适配：M5（开工前置）
- CI 三平台远端结果：首次 push 后确认；本地门禁与远端 CI 分别登记真实结果，尚未运行记未验证
- 旧 Go 文件/记录兼容测试：明确排除（总纲约束 4/11），不移植
- YAML crate 定案：不引入（计划 2 D2-5）——上游 frontmatter 为手写 4 形状解析器，Rust 移植 std-only
- agenttypes load 逐条目目录 IO 错误静默跳过（filter_map(Result::ok)）：计划 2 Task 6 评审 Important 发现，用户裁决按计划接受——上游 Go os.ReadDir 整目录报错，Rust 单条目错误罕见且新分支不可移植测试；需加强启动诊断时改 match 计入 errors

# 延后登记（计划 3 末）

- Message/ChatResponse 等消息领域类型：M1 provider 层（D3-2 YAGNI）
- opencatalog 刷新安装入口接线：M1（set_model_rates/set_limits 已建 + 合成 id 测试）
- Tracker 生产接线（agent TurnEnd → record/set_base/seal/record_flat_cost）：M2
- 锁定成本持久化接线（M2 保存 cumulative()+cumulative_cost()，resume 经 set_base_locked 恢复；平价成本折叠进总额）：M2（D3-9 契约已建）；token-only 近似路径（set_base）仍丢失平价成本——上游天花板保留
- adopt_child 生产调用方（subagent spawn）：M4
- TTSBillableChars/TTSMaxBillableChars 计数助手：M1 deepseek
- TurnRecord 的 model/tier 审计字段：M2 会话导出需要时再加（D3-5）
- core Cargo.toml 尾换行：M1 批（计划 2 遗留 hygiene 组）
- serde_json dev-dep 冗余声明清理：M1 批（计划 3 Task 1 评审 minor，normal deps 已对集成测试可见，dev-dep 条目为功能性 no-op；与尾换行同批 1 行改动）
- seal() extra_cost≠0 且 ≥2 轮时浮点重结合 ~1 ulp 显示漂移：计划 3 终审 minor（brief 逐字上游形状，D3-7 左结合已满足）；该路径变用户可见时补 doc 注释或容差回归测试

# 延后登记（计划 4 末）

- Windows `-install` 接线归 M2，交互 PATH 提示归 M3。M0 已实现平台辅助和可注入后端。UNC host/share 段的 `..` 保护归 M3。PATH 比较目前采用 ASCII 大小写折叠，Unicode 路径的比较规则待 Windows 接线时确认。
- keymap CLI 和 TUI 集成归 M2/M3。M0 解析范围限于单层段与字符串键值，仅加载 `[bindings]`。完整 TOML 语法和非 ASCII 警告引号细节暂不纳入。
- i18n 格式化器忽略多余实参，`%q` 的 Unicode 可打印判定采用 Rust 近似。后续调整语言解析时补 `config_lang` 与 `LC_ALL` 均有效且冲突的用例。
- projectmd 调用方在 M2 接线时传入已解析的工作目录，并明确非常规文件、输入读取上限与截断原字节计数。后续调整查找时补第五个候选可见、第六个不可见的边界用例。
- diff 的 LCS 矩阵占用与两端行数的乘积成正比。在编辑工具接线前确定输入上限。pathop 若引入异步工作或变更写入顺序，再给替身增加完整的操作顺序断言。
- 本地验收代码 `b9a7009` 在 Rust 1.90.0 的 macOS 工作区通过 290 项测试，1 项原有生成器测试保持忽略。格式、严格 clippy、Windows GNU 目标含测试的 check 与严格 clippy、Linux musl 目标 check 均通过。三平台原生 CI 结果待 runner 记录。
- 复评追加的回归使 macOS 工作区测试增至 294 项通过、0 失败、1 项原有忽略。Windows `-install` 接线应处理 `ProcessPathUpdate` 的部分成功状态：注册表若已写入便保留，按请求发出通知，重试可补齐当前进程 PATH。原生 Windows 验收项继续保留。
