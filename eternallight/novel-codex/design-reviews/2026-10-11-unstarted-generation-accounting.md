# 连续创作未发送步骤结算

原任务新版已通过第12章核验；后续协调 task-5683513b9f9f4e908d200e2f94c08cab 在归属失效后、budget binding 前失败。Native 已分配 grant，Engine 无预算/模型请求，generation provider-usage 只读取 Engine receipt，未实现普通 Work 已有的严格未发送证明。模型登记原范围交接后被 unsettled results 拒绝。归属最初失效的原因证据不足，不关闭/扩大租约保护。

归类：实现缺陷（协调任务与写作任务预算绑定次序不同），设计缺陷（generation 与 ordinary 结算合同分叉）。责任：Host 在有 Native task 后先建立可结算 Engine 预算再检查执行归属；Runtime 仅在 Engine 明确返回 run not found 时调用 Story Core 的严格未发送证明；Core 核当前任务/grant/Work/run 身份、终态、无 turn、无 outputs、无任何 tool operation。原 Engine 记录存在或不可读/损坏时不得判零。相邻 ordinary 行为保持，无数据库迁移。列表已有核账路径使用正式 provider-usage 接口恢复。

判定表：确切 Engine 缺席+Native 终态且无执行证据→允许零请求结算；Engine 存在→只用真实账本；账本损坏/缺文件→拒绝；活动任务/已绑定 turn/任意工具操作/已有输出→拒绝；外项目或错误 run→拒绝。均不重发模型、不改原费用、不取消保护、不新建 Work。

五视角同人前审：架构 APPROVE（事实权威不变）；技术A APPROVE WITH CONDITIONS（正反证据回归）；技术B APPROVE WITH CONDITIONS（实际 HTTP + Host 顺序）；产品 APPROVE（恢复既有任务，不扩大原要求）；UI/UX APPROVE（已有核账状态，无新参数）。零兼容 PASS。验证：generation HTTP 未发送/已发送拒绝；ordinary 同根因回归；Host 定向核验；architecture 和发布门禁。后审待补。

## 落地验证与后审

Host 定向 14 测试通过；generation 真实 HTTP 全文件 5 测试通过（未启动允许且幂等；活动/turn/output/operation/Engine 活动/损坏拒绝）；ordinary 既有未启动证明回归通过。架构检查和发布 6 项门禁通过，发布完成于 2026-10-11T11:41:57.254Z。已有 unresolved native call 用例的恢复阶段断言同步为当前终态观察器的 executionBlocked，仍保留完整计费状态不变、不可重试、操作未丢弃等断言。证据见 `.release/tianheng-migration-20261011/unstarted-generation-validation/`。

实际应用启动后，只读核验原 run version 186、completedChapters 12；原失败 task-5683513b9f9f4e908d200e2f94c08cab 的 settled 从 0 变为 1，通过已有列表核账接口完成，无直接数据库写入。UI 已提交原范围恢复指令，实时续写验证进行中。

五视角同人后审：架构 APPROVE（统一使用 Core 未启动证据）；技术 A APPROVE（严格反例与幂等验证）；技术 B APPROVE（Host 顺序与真实 HTTP 覆盖）；产品 APPROVE（保留原目标、正文与用量）；UI/UX APPROVE WITH CONDITIONS（实际接续与目录更新待观察）。原始执行归属失效原因尚无充分证据，本变更不声称修复该诱因。

### 实际接续核验

2026-10-11 11:49 UTC：原 run 已恢复 running，currentChapterNumber=13，已完成 12 章保持不变。新 generateChapter task-19121ee9b7d948958251dcd81efd7a77 已由 queued 转为 running，thread 01a12ac9-4012-7810-8365-0729c1bc52ae 有实际模型 token 事件及 read_source_content 成功回执。UI 目录出现第 13 章，对话显示恢复结果和第 13 章起草安排；正文尚未保存。没有直接改写数据库。UI/UX 后审条件在目录/对话及实际执行层面满足，后续全章完成不在本次已验证结果中。
