# 保存核对闭环设计评审

目标：恢复提示由真实 Runtime 回执消除，用户不再被要求凭对话猜测写入是否成功。仅核对持久化记录，不重放工具、不调用模型、不扩大任务权限或预算。

前置五角色评审（本轮协作审查）：
- 架构：APPROVE WITH CONDITIONS。Runtime fenced snapshot 是结果权威；恢复索引是投影。
- 技术 A：APPROVE WITH CONDITIONS。执行与核对互斥；未知写入保留，真实回执才消除。
- 技术 B：APPROVE WITH CONDITIONS。删除手工标记完成的 IPC、类型、preload 和 UI，不留兼容入口。
- 产品：APPROVE。主操作为重新核对保存状态；不让作者承担内部事务判断。
- UI/UX：APPROVE。在所属对话显示结果；核对失败或仍未知时保留记录，禁止假成功。

零兼容门禁：不增加旧字段回退、迁移、双写或隐藏失败；当前工程数据一致性、授权边界、执行次数与模型预算保持。

实施后复核：
- 实际变更：空闲 recovery report 先执行 fenced snapshot 核对；并发读取共享同一核对，执行入口等待核对结束，活动任务读取不 fence。原人工 resolve IPC、preload、类型和 UI 已删除。
- UI：主操作为“重新核对保存状态”；回执未知或服务不可用均保留提醒，只有权威报告移除对应记录时消除提示，并只读刷新原任务状态。不会自动恢复或重放未知写入。
- 测试：recovery-report.test.mjs 3/3（真实 RecoveryIndex、生产 IPC、临时失败→未知→真实回执、活动执行保护、并发及启动互斥）；agent-configuration 全部 36/36；真实 Electron App 恢复 UI 专项 1/1（失败保留、未知保留、真实核对消除、无模型重放、所属作品隔离）。
- 零兼容门禁：PASS。无旧手工确认入口，无补偿写入或新任务。
- 角色复核：架构/技术 A/技术 B/产品/UI 条件均落实。前端生产 build 通过。本轮恢复专项通过；完整工程回归由主代理统一执行。
- 证据：/tmp/novel-recovery-report-tests.log、/tmp/novel-recovery-host-full.log、/tmp/novel-recovery-ui-tests.log。
