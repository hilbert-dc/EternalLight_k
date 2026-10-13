# 3.0 Host 持久事件唤醒与活动归属评审

日期：2026-10-08。实施前评审。该记录的五角色是职责视角，并非五名独立人员。

## 目标与边界

结果保存后、后续模型尚未接回时，应用重启应从 Native 持久事件回到原 Work 和真实原对话。Host 只取得当前独占归属并调用既有 Work 决策执行器；文学行动、方法和交付判断仍由模型组织，权限、来源、核账与保存证据仍由 Native 核验。活动 turn 自己消费工具结果，idle consumer 不创建并行 turn。

修改范围：新 `electron/work-execution-events.mjs`，`electron/main.mjs` 生命周期与归属接线、当前 main handler fixture，以及定向 Host 单元/真实 Native HTTP 验收。Native 事件事务和 author-only 接口由并行任务负责；不改 App、作者输入解释或文学流水线。

## 方案

- Host 每个进程有随机 session；普通活动 flow 在确定真实 Work 后、模型发送前取得 Native execution-owner，10 秒续期，flow 结束按精确 lease 释放。续期失败阻止新发送并请求现有 Engine 取消；不得把取消或超时当作已证明未发送。
- idle consumer 在启动恢复与核账之后启动，读取当前项目目录和每个项目的有界持久事件。单一 drain 和当前 busy 检查避免进程内并行；Native claim/CAS/lease 避免跨 Host 重复领取。目录不包含正文，原作者输入不重发为新消息。
- 唤醒前核对模型配置与当前资格、真实原对话，再读取当前 Work/工作区并 claim。Native 阻断暂停、取消、pending intent、未知保存、未知费用和额度失效。Host 不能调用作者恢复以绕过阻断。
- 恢复 flow 借用 claim 的精确 lease，不能提前释放事件归属。完成后只有 Native 验证同 Work 后继 Attempt 已实际 attach，或真实最终闭环，才能 ack；queuedStep、内部回复和模型声称完成都不能作为消费证据。失败只 release，事件仍 pending。completed Work 的待处理 generation handoff 沿用现有 proposal 消费 CAS。
- 关闭先停止扫描和续期并释放精确归属，再关闭 Runtime。无法确定的请求由现有恢复账本处理。资格恢复后重新扫描，不保存第二执行状态机或费用账本。

候选：仅内存回调缺少重启证据；Host 自行依据操作/输入关键词选择流程会复制 Native 和模型职责；采用持久事件 + 原 Work 执行器。

## 五角色预审

| 视角 | 结论 | 条件与验收 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 单一 Native 归属/事件，模型组织创作；不新增目标、作者消息、Work 或预算 |
| 技术 A | APPROVE WITH CONDITIONS | 活动 turn 零新 turn；租约失效阻断派发；跨 session、暂停、未知结果、核账和 CAS 拒绝；精确释放 |
| 技术 B | APPROVE WITH CONDITIONS | 当前接口一次同步；真实 Host→Native 证明领取、attach、ack 与重启，mock 只覆盖 Host 控制流 |
| 产品 | APPROVE WITH CONDITIONS | 简单作者输入驱动整体；重启后自动接续原授权，无额外确认/重发整段目标 |
| UI/UX | APPROVE WITH CONDITIONS | 原对话可见真实 Attempt 进展，错误简洁；不增加内部事件菜单、技术预览或重复作者气泡 |

## 零兼容门禁

PASS：无历史迁移、回填、fallback、字段探测、双读双写或降级链路。当前 schema 的不可确定状态、重启、并发和跨平台验证属于可靠性要求。必须同步当前 fixture；不能让旧 mock 默默把未知 endpoint 当完整 Work。

## 验收计划与实施后复核

Host 单元：busy/资格失效不领取、不发模型；真实原 Work/thread 验证；claim 与 ack 精确身份；queued 不消费；后继实际 Attempt 消费；续期失败取消现有 turn；停止时归属释放；分页上界与公平性。

真实 HTTP：当前 Native 从空库建项目/任务/预算，完成真实 Attempt 得事件；Host claim 后调用原 Work 决策、attach 后继 Attempt，再 ack；跨 session 与 candidate 凭据拒绝；重开库 pending 事件与原身份保持。生产 main handler 定向测试验证活动 flow 确实接线。

实施后仅按实际退出码和保存日志追加结果，当前设计批准不等于已实现或已验收。

## 当前版本恢复绑定与编辑占用增量预审

真实 Native 重启验收发现：已批准且未执行的 task action 的 Work Step 原目标不携带授权元数据。Host 统一追加 `resumeStepId` 会绕开其原 action 绑定，Native 正确拒绝 `TaskBindingInvalid`。这是当前版本恢复缺陷。方案按真实 `Step.sourceTaskId` 读取作者接口的 source Task、所属 Work、动作和历史授权证据，核对原 Work/真实原对话、源 Attempt、操作、目标、指令、输入版本及原许可，再仅构造 `requestedByTaskId` 与 `requestedActionId`；两种来源绑定不可同时发送。已有 `workDecisionAction` 原生路径保持不变。知识分析原动作仍按当前已保存章节源修订建立执行 batch，不复制未知字段，不再批准授权、不造新作者消息、Work 或预算。Native 的 attach 与 finish 是最终权限和结果权威。

自动 idle 唤醒沿用界面当前 `agent:set-workspace-context` 的 dirty 占用状态：未保存编辑时零 claim/零模型发送，恢复 clean 后再扫描；不依据项目名称或输入词语猜测。普通 blocked 纠错仅在 Native 已 claim 的真实终态失败、同 Work/原权限/核账/资格都通过时进入既有模型 decision，不调用作者 control resume、不补新预算；generation blocked、paused/cancelled 和未知状态继续停止。

| 视角 | 结论 | 验收条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 原批准动作精确恢复其单一 action 绑定；当前模型 decision 绑定保持完整；不增加权限或执行状态机 |
| 技术 A | APPROVE WITH CONDITIONS | foreign Work/thread/source/action/许可与已有 Attempt 在发送前拒绝；Native TaskBindingInvalid 不减弱；dirty 不 claim |
| 技术 B | APPROVE WITH CONDITIONS | 同步当前两个纯 fixture 与 scope production fixture；新独立负例及真实 completed/failed 重启 2 项完整退出码；保留原失败 |
| 产品 | APPROVE WITH CONDITIONS | 原任务在原授权内恢复，无重复作者输入；用户正在编辑时不抢占工作区 |
| UI/UX | APPROVE WITH CONDITIONS | 不新增内部授权或 event UI；真实保存与恢复进度使用原对话，编辑占用由现有 UI 状态提供 |

零兼容门禁：PASS。无版本探测、旧格式 fallback、迁移或回填。同步的是当前 source/action 与 UI 状态合同。新增范围仅 `work-step-continuation.mjs`、其精确定向 fixture/测试、现有 main 接线和真实 Host→Native 验收。

## 实施后复核

当前已落地：Native 独占 execution-owner 的普通 flow acquire/renew/release；Native 持久终态事件的有界、公平 idle consumer；启动核账后接线与关闭释放；资格、活动 flow/turn、未保存编辑等占用阻断；原真实 Engine thread 与原 Native Work 校验；原 claim 租约借用；仅实际新增后继 Attempt 或 Native 最终证据 ack；普通终态失败经精确 claim 接回既有模型 decision。作者暂停/取消、generation blocked、未知费用或写入、额度失效继续停止。当前 task action 恢复按原 source/Work/action/permission 校核，不再误用 resumeStepId；知识源 batch 沿用实际章节修订合同。

证据均为最终进程退出，而非中间通过：

- `/tmp/autonomous-v3-wake-host-unit-current.log`：Host event/lease、offline 与原预算组 39/39，exit 0。新 event 21 项证明 busy/不合资格零发送、暂停/取消、Native blocked/settled、跨 thread、claim 身份、仅新附着后继消费、queued 不消费、停止围栏、租约续期失败、四项目×20事件上界与轮转。
- `/tmp/autonomous-v3-wake-action-binding-current2.log`：当前 Host event 21、action binding 6（含 29 种错误绑定/已有 Attempt 负例）、decision driver 12、conversation analysis 19、scope 7，共 65/65，exit 0。同步 fixture 只使用当前 Native 源、action 与许可；Native TaskBindingInvalid 与作者约束不变。
- `/tmp/autonomous-v3-wake-host-native4.log`：真实 Host→Native 重启 2/2，exit 0。完成结果从实际原批准 action 恢复，失败结果从 Native claim→模型 decision 纠错恢复，两者均实际保存候选并经 Native 强最终 deliveryReview 闭环。跨 Host 活租约、实际作者 pause 和 dirty 未保存编辑不发送；dirty 时不 claim。原 Work/root thread/goal/预算授权保持；隔离库仅只读对比真实作者消息 ID、intent revision 和完整原文，前后完全相同。实际新 Attempt 消费原事件，消费后再次扫描零 turn；Host 未调用作者 control resume 或发送新 initialBudget。
- 当前 `node --check` main/continuation 与作用域 `git diff --check` 通过。新 Host 与绑定测试已加入 `test:autonomous-creation` 当前统一入口。

保留真实失败链：`wake-host-native1` 0/2 为旧 fixture 误期望 candidate HTTP 403（当前 author-only 是 401）；`native2` 0/2 揭示 lookup metadata 无 projectId（改为读取真实完整作者任务再校核）；`native3` 1/2 揭示当前原批准 action 恢复丢授权来源（修生产 continuation 后 `native4` 2/2）。`wake-action-binding-current` 63/65 的两个失败为 scope fixture 初始 root 被未定义的 action 比较误命中；精确同步后 `current2` 65/65。原失败日志不删、不改、不计为通过。

| 视角 | 实施后结论 | 核验 |
| --- | --- | --- |
| 架构 | APPROVE | 持久事件/预算/权限/Attempt 由 Native 唯一负责；模型选择方法和动作，Host 仅运行原已授权 Work；没有第二目标、账本或作者消息 |
| 技术 A | APPROVE | 活动 turn 不另开；精确租约与事件围栏；foreign/已 attempted、paused/cancelled、dirty 与 unknown 保持阻断；恢复来源用真实记录，Native 最终校验未弱化 |
| 技术 B | APPROVE | 当前接口、生产 Host、原动作 continuation、fixture、统一入口和文档一致；定向当前 65/65 与真实 Native 重启 2/2 退出码可审计；不声称全量已完成 |
| 产品 | APPROVE | 用户原简单创作指令在原任务中继续，无额外输入或重新批准；真实失败可交原模型决策纠错，用户未保存编辑不被自动任务抢占 |
| UI/UX | APPROVE | 沿用原对话和现有编辑状态；只发实际 Work 更新，不新增内部事件、授权术语或技术面板；Renderer 刷新由主任务另行验收 |

零兼容门禁复查：PASS。无历史格式、迁移、回填或降级。实际测试的 AppServer/model 输出是受控 fixture；它证明生产 Host 与真实 Native/预算/持久数据库合同，不证明外部 DeepSeek 文学效果或实际 Core 模型能力。真正 Core 自主创作、Renderer 展示与最终全量冻结验收由主任务提供独立证据，不能以本组定向结果替代。

## Generation 双层状态增量预审

真实 Native HTTP 验证最终澄清：标准 generation 失败使 Step blocked，`refresh_work_task(kind=generation)` 保持 Work running，原 Run 仍 running；非 retryable 失败经 `/next` 后 Run 与 Work 才真实 blocked。初步讨论中“可纠正失败必然令 Work blocked”的判断已被实际源码与 HTTP 推翻，验收不以 SQL 改状态制造该正例。真正 Run blocked、作者暂停/取消、未知费用/写入与额度失效必须由 Native 拒绝。Host 不再按孤立 Work 标签猜测准入；只有实际 claimed 事件、精确借用 lease 与原来源核对通过，才直接调用原 `continueGenerationRun` 的 Native `/next`。所有 durable generation 接续都不走作者 Run resume、retry、重绑或任何新授权。该调整替代此前文中未区分 Step/Work/Run 的“generation blocked 全部停止”。

五职责视角增量预审（各项 APPROVE WITH CONDITIONS）：架构要求 Native 唯一准入；技术 A 要求 true Run blocked 零 turn、可纠正结果同原 Run/Work/thread/预算且作者新 pause 立即停止；技术 B 要求真实 HTTP + production Host 运行两个相反场景并重新冻结/后审；产品要求合法纠错自动接回原任务，真正阻断保持停止；UI/UX 沿用原进展与作者暂停，不增加恢复状态术语或手动点击。零兼容 PASS：同步的是当前两层真实状态合同，无版本分支、fallback 或回填。

## Generation 增量实施后复核与最终 Host 冻结

最终实现不按 generation Work 的标签猜测准入。普通与 generation 都先取得 Native 实际 claim；所有 generation durable wake 只在原来源、真实 Work/root thread、精确 claim/lease 验证后调用原 `/next`，不走作者恢复、retry、输入重绑或预算授权。

- `/tmp/autonomous-v3-wake-action-binding-current3.log`：当前 66/66，exit 0，0 失败/取消/跳过。含 Host event 22、action binding 6、decision driver 12、conversation analysis 19、scope 7。generation 真阻断必须由 Native 拒绝，明确 claimed 的当前原来源方可唤醒。
- `/tmp/autonomous-v3-wake-host-native7.log`：Root 唯一 `pretest:autonomous-creation` 当前全 bins 构建之后，真实 production Host→Native 重启 4/4，进程实际 exit 0，19.27 秒，0 失败/取消/跳过。前两项完成真实候选保存和 Native 强最终交付审核；generation retryable 项证明 Step blocked/Work running/Run running 的真实已结账结果在重启后 claim→原 `/next`→原 root thread 新 advisory Attempt，之后实际作者 pause 仍停止，原事件 pending 不假 ack；true Run blocked 项 Native 拒绝且零模型、零 `/next`。后两项是安全恢复与停止证据，**不是长篇单章文学交付证据**。
- generation 原 authorMessageId、完整 intentRevisions、真实作者 SQLite 消息 ID/版本/原文、原 Work/Run/root thread 与 goal 均不变；原 16 requests/200000 tokens/10 attempts/null deadline 与原真实费用 charge 保留。只对当前 generation Run 合同作判断；普通 Work 的 budget-authorization API 明确不适用于 generation。Host 无新 initialBudget、预算授权、control 或 rebind 写入。

增量失败原日志保留：`wake-host-native5` 2/4，为新 fixture 误查 generation 不允许的普通 Work budget API（Native 正确 400 TaskBindingInvalid）；`native6` 2/4，为末尾断言不存在的 `Run.authorIntent`，以及错误把 deadline 放进 providerBudget 的预设。同步当前 Run.intentRevisions/authorMessageId/Run.deadlineAt，并前置验证所有实际预算字段与原 charge 必须存在，避免两个 undefined 误通过；`native7` 4/4 后冻结。没有修改 Native、生产权限、费用或来源门禁以迎合 fixture。

五职责视角最终后审：架构、技术 A、技术 B、产品、UI/UX 均 APPROVE。Native 权限、来源、核账、最终保存与消费为唯一权威；模型继续决定组织方法和实际任务；Host 没有新文学流程或输入关键词；用户编辑/暂停/取消与未知状态继续优先停止。当前实现、fixture、测试链和现行文档同步完成，零兼容 PASS。作用域最终 hash 存 `/tmp/autonomous-v3-durable-wake-host-frozen-final.json`，全工程冻结、真实 Core 自主创作、UI 与完整测试入口仍由主任务独立验收。

## 统一入口剩余作者全文 fixture 增量预审

Root 当前完整 Node 201 项实际 200 pass/1 fail（exit 1），唯一失败 `agent-configuration.test.mjs` 的候选恢复 fixture 缺当前 action Step.inputRevision/currentAttemptId/attempts 及原 source Task/action/许可。生产 `WORK_ACTION_BINDING_INVALID` 拒绝正确；完整链的新 4 个真实 Host→Native 已全部通过。

五职责视角均 APPROVE WITH CONDITIONS：架构只同步当前 Native 原 action 合同；技术 A 不改生产或降低来源/授权门禁；技术 B 只改该精确 fixture，补真实形状的原 advisory Attempt、action/许可和实际附着状态，保留完整作者原话/只一次/原输入不冒充作者追加要求的全部原断言；产品保持原简单目标与追加意图传到模型；UI/UX 不增加重复用户消息或技术内容。零兼容 PASS，无 fallback、迁移或旧形状保护。定向整文件退出成功再后审、冻结，原完整 Node 失败日志不改不删。

同次修正消费注释事实（无行为变更）：未知写入、费用和 active Attempt 等安全准入在 Native claim 阶段检查；ack 核对精确 owner/claim、作者控制、同 Work/source 之后的实际附着 Attempt 或既有 final/handoff 证据。consumed 只代表原结果已转交实际后继，不能代表后继已完成、费用已结或最终交付。未知后继仍留在原执行账本；不能给 ack 添加完整 claim safety 而阻断合法的活跃后继。五视角均批准该事实修正，Native/Host 生产 ack 控制流不改。

实施后五视角复核均 APPROVE，零兼容 PASS：仅目标 fixture 使用当前原 advisory/source Attempt、queued action、历史 permit 与实际 child attachment；源 author history 完整保留，发送一次、原模型输入不冒充作者追加要求及不使用过期 Work goal 的全部原断言保留，并新增精确 requestedByTaskId/requestedActionId 目标断言。`/tmp/autonomous-v3-agent-configuration-action-current2.log` 整文件 40/40，session 58639 实际 exit 0，10.36 秒，0 失败/取消/跳过。首次定向 `/tmp/autonomous-v3-agent-configuration-action-current.log` 39/40 为 fixture 改实际 child ID 时忘同步 `budget-${childId}`；当前预算身份门禁正确拒绝，同步精确当前 budget ID 后通过，原失败保留。生产仅修改消费注释事实，控制流和 Native 权限无变化。

最终 Host 当前 12 文件摘要（含这一个 author history fixture）改存 `/tmp/autonomous-v3-durable-wake-host-frozen-final2.json`；原 final 摘要保留供比较。Root 完整 Node 201 项 200/1 的旧失败日志仍是失败证据，须以当前统一入口完整终态独立验收，定向 40/40 不代替全量 exit 0。
