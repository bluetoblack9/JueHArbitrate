# 延后登记（计划 1 末）

- core 15 包实现与测试：计划 2（批A）/ 计划 3（批C）/ 计划 4（批B+D）
- 类型化 core 配置/消息样例：随计划 2–4（D12）；场景清单已登记 docs/m2-scenarios.md
- state 重启/resume、distill 基础能力、sysprompt 适配、-install flag 与短路测试：M2
- keymap 集成场景：M2/M3；Windows 交互 PATH 提示：M3
- friction/fileroute：M2；schedule_wakeup/routines：M4
- 冻结前端契约提取与接口适配：M5（开工前置）
- CI 三平台远端结果：首次 push 后确认；本地门禁与远端 CI 分别登记真实结果，尚未运行记未验证
- 旧 Go 文件/记录兼容测试：明确排除（总纲约束 4/11），不移植
- YAML crate 定案（候选 serde_yaml_ng）：计划 2 开工