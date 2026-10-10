# 3.0 持久执行事件与单一唤醒归属评审

日期：2026-10-08。实施前状态：设计3.0第10.3的通用Work持久事件消费尚未落地；已有章删除事件消费者、恢复账本和显式继续接口不能代替该能力。本记录按五个职责视角评审，不冒称五名独立人员。Native增量由本任务负责，Host接线由并行受委派任务负责；全部当前调用方和测试完成前不得标记闭环。

## 目标与当前缺口

实施前 `finish_attempt` 只更新Work/Step/Attempt，启动只恢复孤立Attempt和核账。结果已经保存、后续模型尚未接收到它时，重启不能自动把这个确证结果交回原Work决策。新增持久事件仅记录结果发生与消费身份，复用原Work、预算、作者源及模型决策；不增加第二任务系统、文学调度器或作者消息。

活动Engine turn继续处理自己的工具结果及子任务通信。Work活动flow持有Native独占短租约；idle consumer不能抢占它。只有不存在有效活动归属且Native证明可以继续的事件，才触发原Work的一次决策。未知写入、未知费用及作者暂停/取消保持停止；租约超时不证明旧模型请求未发出。

## Native当前接口

所有接口仅进入Runtime现有author router，candidate凭证不能访问，不向模型开放。JSON拒绝未知字段；Host会话和租约身份逐项校验。Native时间决定固定30秒租约，Host每10秒续期。事件目录不包含正文、工具参数、密钥或重复作者输入。

- `GET /api/projects/{project}/work-execution-events?cursor=&limit=20`：`{projectId,items,nextCursor}`。items包含eventId、workTaskId、sourceStepId、sourceAttemptId、rootThreadId、kind、createdAt、workVersion、state、claimId、hostSessionId、leaseExpiresAt；按持久sequence keyset分页。createdAt与expiresAt均为Native Unix秒。
- `POST .../work-execution-events/{eventId}/claim`：`{hostSessionId,expectedWorkVersion,expectedProjectRevision,expectedSourceDigest}`。返回 `{event,lease,work,admission,blockedReason}`；lease为 `{leaseId,hostSessionId,workTaskId,rootThreadId,expiresAt}`，work仅身份/状态摘要。admission为claimed/blocked/settled，不决定文学操作。
- `POST .../{eventId}/ack`：`{hostSessionId,claimId,leaseId,consumerStepId}`，consumerStepId明确可空。非空必须对应source之后同Work真实已attach的后续Attempt；仅queuedStep不能消费。空仅允许Native已经证明最终完成或确实无后续的终态。
- `POST .../{eventId}/release`：`{hostSessionId,claimId,leaseId}`。只释放该claim，事件保持pending；不重新开放暂停/取消或额度。
- `POST /api/projects/{project}/work-tasks/{work}/execution-owner`：`{hostSessionId,expectedWorkVersion}`，取得活动flow归属。
- `POST .../execution-owner/renew` 和 `.../release`：`{hostSessionId,leaseId}`。只更新精确租约；迟到释放不能清除新的owner。

`finish_attempt` 在结果原事务中记录唯一 `attemptSettled` 事件，sourceAttemptId唯一；不回填历史事件。已保存generation handoff由其完成Attempt定位，执行仍使用现有proposalId和消费CAS，不建立第二handoff账本。owner/event记录是域内执行元数据，作品事实和额度仍在既有表。

## 准入与崩溃规则

claim核对真实Work/project/rootThread、当前来源CAS、原作者源摘要、终态sourceAttempt及未被替代的关系。作者源或pending intent改变时停止自动领取。活动Attempt、accepted且无receipt的写入、未结或unknown provider使用量、缺失/过期/耗尽额度均不能启动模型。已完成且没有待处理handoff或交付义务的事件可确定性结清，不能为它另开模型。

Host收到claim后复用 `prepare_work_decision` / generation next及原Work；不调用作者resume以改变暂停状态。原函数继续检查范围、来源、幂等、费用和模型资格。模型选择行动与方法，事件消费者只传递实际结果。

claim后、实际attach前崩溃：事件仍pending，旧lease过期后重读同一原Work，已准入queued decision复用。attach之后崩溃：原Attempt恢复与账本先决定结果，新的终态记录形成新事件；不能因为lease到期重复派发旧Attempt。ack绑定真实后继Attempt或最终完成，未取得证明不得消费。

### 实测合同修正：generation Work 与 Run 的状态

真实HTTP验收发现，已结算、无产物、无未决写入的标准失败使Step blocked，generation Work及原Run仍running；`refresh_work_task`不聚合改变generation Work状态，后者由原Run控制。现有`next_generation_decision`仅在Native的`retryable`证明成立时清除activeTaskId，把失败交回同一Work的模型决策。这不需要作者resume或新额度。owner获取/续租遇到generation Work blocked时，仍检查原Run与blocked步骤的实际证明，不只靠Work标签推断：原Run仍running、作者源仍当前，且blocked步骤全部拥有现有Native标准失败与已结算证明。未知费用/写入、非标准失败、真正Run blocked、作者暂停/取消依然拒绝。活动正常Attempt取得归属不要求idle条件。实测正例使用真实Step blocked、Work/Run running；不修改SQL状态伪造Work blocked路径。

五视角增量预审：架构、技术A、技术B、产品、UI/UX均APPROVE WITH CONDITIONS；条件是正向真实HTTP证明原Run交回决策，负向真实HTTP证明非retryable失败/暂停/取消不可自动继续。此调整仅细化执行归属准入，不更改失败分类、作者授权、原Run控制或文学决策。

## 五视角实施前评审

| 视角 | 结论 | 必须证明 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 唯一Native域事实，事件附着原Work/Attempt；Host仅唤醒，Engine组织文学行动；不另造目标或预算 |
| 技术A | APPROVE WITH CONDITIONS | 两连接抢占、精确session/lease/claim、暂停取消与迟到ack、unknown费用/写入、跨身份及source/intent失效拒绝 |
| 技术B | APPROVE WITH CONDITIONS | 当前schema/API/Host合同一次性同步；真实Rust与HTTP、Host→Native及重启链验收；保留失败，不靠mock权限认证 |
| 产品 | APPROVE WITH CONDITIONS | 结果保存后重启不要求作者重发整段目标；只在实际原授权内继续；不可确定时保留成果并停止 |
| UI/UX | APPROVE WITH CONDITIONS | 无额外作者输入、角色菜单或内部步骤；事件仍投影到原对话/真实Attempt，暂停在无模型时生效 |

零兼容门禁：PASS。只新增当前建库资源和当前接口；不迁移、回填、探测旧格式、fallback或双轨。不修改实际小说库、已有未知写入证明或历史授权。

## 必需验收与实施后记录

Native：终态事件事务与重复去重；两连接争领；lease过期重领和迟到release/ack；同Work真实后继Attempt消费、queued拒绝；跨Work/project/Attempt/session拒绝；作者暂停取消/改向；未结与unknown费用、unknown写入、预算到期/耗尽；重开库事件和claim保持；有界目录；candidate凭证HTTP拒绝；普通与generation Work。

Host：活动turn不新开turn；保存后崩溃重启一次原Work决策；claim后未attach复用queuedStep；已attempted且未知不重派发；模型资格失效保留事件；原rootThread、预算和作者消息不重复；handoff沿用真实shouldExecute消费。

设计批准不代表实现或验收已经通过。以下记录仅报告实际完成的范围。

## Native实施与当前证据

生产落点：`native/story-core/src/work_execution_events.rs`集中定义当前schema、终态事务事件、目录、独占owner、claim/ack/release与证明；`work_tasks::finish_attempt`在原事务内登记，`sqlite_repository`建立当前schema，`story-runtime/src/work_execution_event_api.rs`接入7个author专用路由。终态唯一事件不回填；调用方不提供权限或预算。schema与Rust/Runtime当前合同一次同步，不改实际小说库。

- Core真实仓库定向：`cargo test -p novel-codex-story-core --lib work_execution_events_tests --no-fail-fast`，实际exit0，10/10，日志`/tmp/autonomous-v3-durable-wake-core-current-20261008.log`。包含Barrier同时两连接争领、lease过期精确再领取、迟到ack/release、重开库去重、活动flow归属、当前source CAS/作者改向、暂停取消、未知写入/未结与unknown费用/额度、实际后继与queued区分、真实ack consumed后keyset续页及最终交付证明。
- Runtime真实author/candidate HTTP：`cargo test -p novel-codex-story-runtime --test work_execution_events_http_tests --no-fail-fast`，实际exit0，7/7，日志`/tmp/autonomous-v3-durable-wake-http-wire-current-20261008.log`。使用当前Router及真实capability/原作者API建立临时作品、Work、generation Run、任务、model plan、行动和handoff；不使用放宽权限的fixture transport，不直接SQL伪造generation身份或终态事件。实际Engine PersistentBudget生成零调用终态receipt，再由author provider-usage进入原累计ledger；不是伪造新额度。
- generation标准失败正例：真实failed Attempt及结算形成Step blocked、Work/Run running；claim后同一lease acquire/renew，原`/next`返回advisory决策，真实新Attempt attach到同Work与原rootThread，然后原事件ack consumed。作者消息仍1，原预算maxRequests=16、既有charge=1，未调用作者resume。仅证明恢复机制与身份，不代表真实模型理解或生成了章节。
- generation负例：非标准`provider_transport_failed`经原`/next`使真实Run/Work blocked，claim返回generationNotRunning且owner拒绝；原Run暂停/取消后claim与renew拒绝。首次rootThread未绑定拒绝owner，终态费用未结不假定已知。实际未consumed handoff保持pending且不能用空consumerStepId虚称最终完成；handoff proposal及shouldExecute仍由原CAS管理。
- 过程中保留并报告fixture错误：早期将标准失败的Run预设blocked、将generation Step blocked误认为Work blocked、将`target.workDecision.workTaskId`误读为顶层。修正为实际生产合同，未改失败分类、生成控制、写入证明或授权门禁；最后一项原失败日志保存在`/tmp/autonomous-v3-durable-wake-http-wire-assumption-failed-20261008.log`，之前错误与原输出保留在本任务记录。没有通过构造假状态宣称真实链通过。

## 五视角实施后复核

| 视角 | 当前结论 | 可复核依据与边界 |
| --- | --- | --- |
| 架构 | PASS（Native落点） | 终态事件附着原Work/Step/Attempt并原事务登记；owner仅决定消费归属，实际派发与文学行动仍走原Engine/Native决策；无第二Work、预算或handoff权威 |
| 技术A | PASS（定向17项） | 同时抢占、精确租约、实际attach消费、来源/作者/额度与unknown负例均通过；租约超时不当作模型未发送证明 |
| 技术B | PASS（当前Native合同）；统一版本验收待Root | 当前schema与7个author路由编译、Rust与原始HTTP均通过；Host已由并行任务接线，最终bin及Host→Native整体证明由Root统一构建验收，不将旧bin当作本次证明 |
| 产品 | PASS（Native恢复边界）；实际创作另验 | 重开后可恢复原事件，原消息/线程/预算保持，标准已结失败交回原模型决策，无法确证则停止；不以零调用receipt或script证明文学能力 |
| UI/UX | PASS（本增量边界）；整段体验另验 | 仅返回有界身份/状态摘要，无正文复制和新增作者输入；Host投影与真实对话/阅读控制由Root的Electron与实机验收覆盖 |

零兼容门禁：PASS。当前schema/API/fixture一次更新，无迁移、回填、旧格式fallback、双读写或历史授权推断。Native生产及上述两个fixture文件已冻结，交Root进行唯一当前bin构建和统一验收。设计3.0 A–D整体闭环、真实DeepSeek文学理解、第7章、前100章与300万字作品分别需要其自身证据；本模块的17项通过不宣称这些目标已完成。
