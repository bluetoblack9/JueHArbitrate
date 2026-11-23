# M2 存储/行为场景清单（M0 登记，M2 用 Rust 构造器执行）

规格来源：总纲约束 4/6/7、docs/prd（参考归档）、上游测试文本只读。
全部用 Rust 新建数据，SEEK_HOME 临时根；不导入旧 Go 数据。

1. session：新建→首行元数据→流式追加→重启→resume 重物化（含图片/文档资产按名引用）
2. session：continue 取最近更新；-list 输出；no-save 不落盘
3. memory/soul：写入→decay-GC→重启读取；soul ~500 token 上限（M5 dream 接线）
4. checkpoint：创建/restore/prune；keep-checkpoints 跨 session 保留；undo/redo
5. plan 快照：transcript 重建（analyze 拒写/approve 执行/resume 重建）
6. 损坏输入：截断 JSONL、未知 schema 版本、非法首行 → 明确报错不静默置空
7. 资产：缺失引用报错；名穿越（../、分隔符）拒绝；32MiB 上限；原子写失败恢复
8. 项目标识：规范化路径稳定、项目隔离、无旧哈希兼容
9. 权限：双轴全组合矩阵；拒绝即工具结果；子代理收紧（M4）
10. 工具：24 静态工具注册裁剪；-tools 白名单未知名 exit 2；MCP schema 冻结
11. distill 基础能力：mock provider 提炼→写入→重启读取
12. -install：Windows PATH 幂等/错误、非 Windows 拒绝、无 key 短路（pathop 替身）