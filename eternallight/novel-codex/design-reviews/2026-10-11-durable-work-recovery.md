# 持续目标与统一接续判定

## 方案与责任

Work 保存目标、作者授权、领域成果和整体状态；Attempt 的失败保留为执行证据，不等于目标作废。Native 原有完成/写入/预算/作者控制合同继续作为唯一权威。统一补全已有 WorkExecutionEvent admission 契约，不新建第二套调度器或作品账本。

已证实根因：事件消费检查排在当前 activeAttempt/费用等安全门后，使实际已有后续执行的旧事件反复等待；已消费 generation handoff 没有被 finished 识别为交接事实，导致 ack 无真实后继报错；另一个 Work 占用同一对话、快照竞争作为异常进入通用失败退避；blockedReason 被 Host 丢弃导致等待原因不可见。设计与实现均有缺口。

Native：先核对既有交接/后继事实以结束旧通知，此举不授予执行、不改变原 Work 成果/账本/暂停；只有新派发才执行全部当前身份、作者、版本、写入、用量、所有权门禁。版本变化返回等待重新读取，活动归属返回等待释放；真实协议/存储故障保持错误。Host：消费结构化等待原因，明确等待条件、需作者决定、工程阻塞；不将正常等待计入异常失败次数，退避核对不发模型；接续未创建实际后继时仍保留事件且显示等待，不能虚报完成。UI：运行进度、目标暂停和恢复原因并列，不能把所有停止解释为要作者输入。

契约：现有 claim 的 admission=claimed/blocked/settled 不变，blockedReason 增加 snapshotChanged/conversationBusy/continuationPending；Host executionHealth 同步增加 reason/category（condition/author/blocked），旧实现字段一次性更新，没有旧版本兼容。不改数据库 schema、不迁移作者资料、不清零费用、不扩展权限，不从错误正文推断恢复授权。

## 状态判定

- 已有同 Work 实际后继或持久化已消费交接：旧通知 settled；后继自己的保存/费用和失败继续独立核对。
- 仅有提案/排队步骤，无实际后继：保持 pending，不伪造消费。
- 暂停、取消、待授权、未知写入、未结算、明确额度耗尽：新派发仍拒绝，状态原因明确。
- 版本/CAS 竞争或对话有其他 owner：等待条件，不派发、不记为用户任务失败。
- 真实异常：保留错误健康记录及退避；同条件重复创作失败仍由原 Native 无进展保护限制。
- 完成/失败/断网后重开：从持久化事件和当前账本重新核对，原目标与历史失败不删除。

## 五视角前审（同人）

架构 APPROVE（B03/B05/B06/B08，事实所有者不变）；技术A APPROVE WITH CONDITIONS（事件消费不放宽派发、幂等并发）；技术B APPROVE WITH CONDITIONS（实际 Native+Host 集成）；产品 APPROVE（普通等待不要求反复输入继续）；UI/UX APPROVE WITH CONDITIONS（清晰原因、活动和暂停优先）。零兼容 PASS。验证：Rust 当前状态矩阵、Host 条件/异常/退避测试、实际 HTTP 重启接续、类型/架构/发布门禁及真实应用观察。不得把设计计划记作已验证结果。

补充同一根因：Host acquireWorkFlowOwner 把已确认 ownerInvalid 与外部身份不匹配合并为同一错误，造成前置中断无法走既有恢复合同。将本机已失效归属明确取消当前执行句柄，保存 execution_interrupted 证明；外项目/外 Work 仍拒绝，供应商错误文本不能伪造本机中断。前审补充：架构/技术A APPROVE WITH CONDITIONS（验证 trusted 本机句柄与持久化结算），不增加自动授权。Host 原 fixture 漏传 PATCH method，已同步传递真实 HTTP 形状，避免假终态。

## 已完成验证

- Native 初轮 19 项通过：相同/不同 owner、真实后继、生成交接对应真实目标执行、跨 thread 拒绝、作者暂停/取消、未知写入、用量/额度、幂等及过期租约。原 activeAttempt 用例改为核对“已有后继的旧通知可消费”，仍保留租约续期和外部续期拒绝；无后继的阻塞用例保留。
- Host/状态定向 41 项通过；真实 Electron 组件回归已通过（包含流进展、计时、排队/对话隔离）；预算/终态 21 项通过，owner 前置失效持久化 trusted interruption，供应商文本不能伪造。
- 实际 Native HTTP + 生产 Host 重启集成 5 项通过：完成后接续、可纠正失败接续、生成可重试失败、可信中断、真实阻塞。保持原 goal、author message、scope、budget 和费用回执。已加入对 activeOwner 条件等待的断言；原 fixture 立即第二次扫描改为推进 Host 注入时钟至约定检查时间，Native 仍真实释放租约，没有跳过等待或伪造权限。
- 类型构建与 B01 架构门禁通过。证据保存在 `.release/tianheng-migration-20261011/durable-recovery-validation/`。系统加载延迟曾经采样确认 `_dyld_start`，未关闭系统保护。

同人后审：架构 APPROVE（通知消费与新执行授权分离）；技术 A APPROVE（幂等、租约、费用/写入保护）；技术 B APPROVE（实际 HTTP 重启闭环）；产品 APPROVE（原目标保留、条件等待无模型重试）；UI/UX APPROVE WITH CONDITIONS（发布后实际任务确认待完成）。零兼容 PASS，B01–B10 PASS。

实际应用补充：旧 task-5db… 的提案未消费，但后续新提案已经恢复同一显式 resumeRun。增加 Native 通知已覆盖证明：同项目/对话、指向同一目标的新提案有 consumed_run_id，来源序号更晚，目标 Work 有在新来源之后的实际 Attempt。仅消费旧通知，不伪造旧提案执行、不修改作者指令。前审补充：架构/技术A APPROVE WITH CONDITIONS（缺少目标执行时继续拒绝，原 proposal 不改写），对应新增 Native 回归。

最终增量验证：Native 20 项通过（新增旧 resume 提案被后续已消费交接及真实目标 Attempt 覆盖，原提案 consumed_run_id 保持空）；最终二进制的实际 HTTP 重启集成 5 项通过；状态/Electron 界面 8 项通过；类型构建和架构检查通过。后台同类健康记录合并展示；退避时间只是最早可检查时间，不再承诺某一秒一定可占用执行资源。

真实运行中已观察：恢复后自动从第16章处理推进至第17章写作，没有再追加文学指令。为安装最终 Native 增量，通过 UI 安排写作步骤结束后暂停；第17章正文已保存、未采用/发布。仍待最终包核对历史 work-event-7d829… 消费结果，不能仅凭受控测试宣称此现场告警消退。

最终现场根因补充：正式恢复入口确实发出请求，但 Authoring `refresh_checkpoint` 将 `replans.len() >= 8` 与 CAS 失配合并为 AuthoringRunStale；原运行恰有 8 条有效历史，永久无法恢复。历史条数不是权限、费用或无进展证据。责任仍在 Authoring 当前检查点用例：删除固定历史上限，保留全部历史及来源/CAS、实际用量、作者控制、未完成写入门禁；不扩展预算、不自动发起新创作。公开接口/schema 不变，Host/Renderer 调用不变。五视角前审（同人）：架构 APPROVE；技术A APPROVE WITH CONDITIONS（第9次之后保留原身份和用量、旧请求仍拒绝）；技术B APPROVE WITH CONDITIONS（真实 HTTP 回归+现场恢复）；产品 APPROVE（长期任务恢复能力不随正常暂停耗尽）；UI/UX APPROVE（不再以状态过期误报历史次数）。零兼容 PASS。

历史次数限制回归通过：真实 HTTP 的 generation_replan_tests 2 项通过，包含连续 12 次暂停/rebind、完整历史与作者身份/目标/费用不变、旧请求拒绝，以及实际额度耗尽/取消不能恢复。Native 构建、架构检查通过。现场旧 work-event-7d829… 已消费，最终包重启后未再出现该作品旧接续异常；第17章保存后的暂停未被后台绕过。正式恢复曾在第9次被8条历史上限拒绝，已纳入此次修复，仍需用最终包验证。

最终交付：`release/package-report.json` 的 staging/runtime-verification/frontend-build/packaging/packaged-resources/application-smoke 六项全部通过，最终应用已启动。现场通过原任务恢复按钮完成第9次检查点恢复，原 generation-d6db… 变为 running、version 283，创建第17章结果核对 Attempt task-e195…；UI 显示具体步骤、等待执行/接收内容以及动态时长，保存的17章目录仍在，原目标100章不变。旧事件异常在最终启动后未复现。前100章尚未完成，16章已结项、第17章正文已保存且后续核对已启动；不是目标完成回执。

最终五视角后审（同人）：架构 APPROVE（Native事实与Host调度边界保持）；技术A APPROVE（幂等/CAS/作者控制/费用/未知写入仍有效）；技术B APPROVE（真实HTTP跨12次恢复、重启集成及发布门禁）；产品 APPROVE（实际原任务第9次恢复成功，无需新增文学指令）；UI/UX APPROVE（实际步骤和持续活动可见，历史等待单独提示）。零兼容 PASS；无schema迁移、无原文改写、无预算重置、无发布。
