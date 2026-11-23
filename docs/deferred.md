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
