# 持续执行状态可见性

根因：Renderer 将 Host 后台接续健康优先返回，掩盖 Native 正在执行的步骤；固定状态条仅给笼统阶段，没有绑定当前 attempt 的实际进展。属于状态投影设计与实现缺陷。

责任：Renderer（B03/B06/B08），Native Work/step/attempt 为执行事实，Engine 流事件为实际过程，Host health 为后台核对能力；互不覆盖。公开合同不变，不调整模型、授权、预算、保存或调度，不修改真实资料。当前运行步骤+健康异常同时展示；断线/读取失败如实报告；排队不得伪装运行；阶段、耗时在原对话固定状态区展示，最新模型进展仅来自绑定当前 attempt 的公开 commentary，不显示 reasoning。

前审（同人五视角）：架构 APPROVE；技术A APPROVE WITH CONDITIONS（身份匹配、暂停/断线反例）；技术B APPROVE WITH CONDITIONS（React 重渲染/计时与构建）；产品 APPROVE（无需翻找长对话）；UI/UX APPROVE WITH CONDITIONS（实际界面验收）。验证定向状态/对话测试、类型构建、架构门禁、实际应用。零兼容 PASS，无 schema 或迁移。后审待补。

## 实施与后审

状态区使用 Native 当前步骤名称与起始时间；流式阶段/公开进展仅在 attemptContainsRun 匹配当前步骤时显示，不绑定其他执行，不显示 reasoning。后台 health 作为单独提示保留，不覆盖已证实运行的步骤或当前 Host 流事件。保留断线、读取失败、排队、暂停等边界。固定区域加入进展摘要，无新调度、模型调用或迁移。

验证：状态判定与实际 Electron React 更新共 7 项通过，包括计时、流事件重渲染、跨 attempt 隔离、排队无假计时、空闲清除；相邻 Work 命名/对话 10 项通过；完整前端/Host 类型构建、architecture:check、发布 6 项门禁通过。首轮 UI 测试因沙箱无法启动 Electron 中止，获系统批准后隔离运行通过。打包首次缺少 Native 路径、第二次使用了包内重命名而非构建文件名，均在 staging 停止；改用实际构建路径后完整发布通过，未绕过门禁。证据在 `.release/tianheng-migration-20261011/continuous-status-validation/`。

实际应用：为更新通过正式 UI 安排当前步骤结束后暂停，确认暂停后退出旧应用。更新后点击原任务重试入口恢复；未提交新文学要求，未改数据库。现场状态条显示“核对第 15 章章节检查结果与后续安排”，从“正在写回复”变为“正在分析”，耗时 5 秒增至 17 秒；后台健康提示单列，未掩盖当前执行。已有第 1—15 章正文保留。不声称后台旧事件调度根因已解决，本改动保留真实告警。

同人五视角后审：架构 APPROVE（事实权威不变）；技术A APPROVE（隔离与停止状态验证）；技术B APPROVE（真实 React、类型与发布验证）；产品 APPROVE（持续状态固定可见）；UI/UX APPROVE（实际应用阶段变化与计时核验）。B01–B10 PASS；零兼容 PASS。
