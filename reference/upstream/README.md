# 上游参考归档（只读）

来源 commit 见 /UPSTREAM-REFERENCE.txt。本目录全部内容仅作参考归档：
- 不作为 Rust CI/测试/验收入口；eval/results 内历史输出不是基线。
- 活动说明文档在 arbitrate-rs/docs/，按 Rust 行为重写（M6）。
- builtin-skills-original/ 是 4 个内置技能原件，供适配差异审计。
- 原件由固定 commit 的 git archive 提取，tests/assets.rs 将四个原件摘要及 manifest.original_sha256 与固定值分别核对。