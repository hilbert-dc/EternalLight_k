# 恢复费用与真实模型决策阶段的当前 fixture 合同

日期：2026-10-08。五项为评审视角，不代表五位独立人员。

## 原失败与源码确证

冻结 8 `generation_provider_budget_tests` 实际 2/3，`restart_recovers_an_active_task_and_unknown_budget_before_author_retry` 原第 482 行期望 blocked/executionBlocked，但实际返回 running/advisory。原日志 `/tmp/autonomous-v3-native-group3-current8/generation_provider_budget_tests.stdout.log`。fixture 仅产生 Engine 的 recovered_unknown provider 费用：实际 budget-{taskId} 正确，20 tokens 已计入 unknownTokens；没有任何 Native 工具调用或未知业务写入。

`native/story-authoring/src/lib.rs:479–537` 仅允许已收费、未超授权、真实 failed/cancelled task 的作者显式 retry。`native/story-core/src/authoring_runs.rs:783–815` 严格拒绝 state=accepted 的未决 Native 写调用；无该调用时，仅重排原 blocked Step，旧 failed Task 不被重开。`native/story-core/src/work_decision_generation.rs:517–564` 返回实际 queued advisory Step，没有生成正文、扩大预算或自动重放工具写入。费用未知与写入未知是不同事实，原 fixture 的“unknown writes”注释没有实际 ledger 支撑。

另 `chapter_deletion_http_tests` 实际 1/2，唯一失败是删除后的 generation-start-position.firstStage 旧 rollingPlan 对当前 coordinate。原日志 `/tmp/autonomous-v3-native-group2-current8/chapter_deletion_http_tests.log`。`native/story-core/src/authoring_runs.rs` 当前 start-position 模型决策阶段统一 coordinate；章节编号 frontier 仍为 3，不允许删除后重写旧章。

## 最小实施范围与验收

仅当前两个独占测试及本评审，不改生产、shared helper、主实施记录或预算/业务时限，不构建。

- 恢复费用 case 使用实际 Native grant；未结算拒绝、准确费用、charged-once 和原授权不变仍保持。显式作者 retry 后只能返回同一 Step 的 advisory，旧失败 task 全对象不变、尚无新 attempt/正文/请求。只有 Native 的后续真实能力 claim 才建立新 attempt。
- 新增同一准备流程的真实未知工具调用分支：通过当前 Host-only bind_recovery_operation 记录已准入 root-call 的准确 project/task/thread/turn/call/hash，模拟 accepted 后未返回的崩溃边界；没有终态证明，receipt 保持 null。费用结算不会使此写入变成 committed/rejected；作者 retry 必须失败，Native next 仍 blocked，无新 Task/Step/attempt、无预算退款或旧 attempt 重开。只测真实当前 ledger，不直接手写 outcome 或清除未决行。
- 章节删除只把 firstStage 更新为 coordinate，完整位置全对象比较、实际删除、历史 snapshot、源引用、权限、CAS、锁定、当前 task、知识目录和预算断言全部保留。

## 实施前五视角

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | Native 费用和工具结果各自权威；模型恢复仅在原作者预算内，未知写入不得推定完成。 |
| 技术 A | APPROVE WITH CONDITIONS | 真实 ledger root identity/hash/未决状态必须强检查；费用、旧 task、owner、attempt 数和原授权精确保留。 |
| 技术 B | APPROVE WITH CONDITIONS | 保留原 2/3、1/2；补实际未决写入负例，不能仅删旧 blocked 断言。同步 current coordinate，不回退固定流水线。 |
| 产品 | APPROVE | 作者可恢复纯计算/规划中断，未知业务结果必须核对；删除后不降低历史和正文保护。 |
| UI/UX | APPROVE | 仅测试，无重复状态、技术字段或界面改动。 |

零兼容预审 PASS：没有 fallback、迁移、双轨、限额提升或权限扩大。主任务已授权两个 fixture 所有权，修后实际验证仍由主任务集中构建。

## 实施后复核

实际变更已完成并冻结：`generation_provider_budget_tests.rs:368–697` 用两个独立测试执行相同真实恢复准备流程，分别证明“只有保守计入的 provider 未知费用”与“另有真实准入 Native 调用、无 receipt”两个边界。后者通过公开 Host bind API 准入，强校验完整 root identity/hash/accepted/null receipt；作者 retry 返回 TaskNotWritable，next 保持原 blocked Run 全对象且无 action，原 ledger/Task/Work/attempt/owner 行和费用不变。前者仅由作者显式 retry 接回原 Step advisory，不退款、不重开旧 failed Task、不分配新的 attempt。owner 检查是实际 owner 行前后不变和 Work root thread 身份保持，不声称此 fixture 建立了非空 Host lease。

`chapter_deletion_http_tests.rs:222–226` 保持完成 frontier=3 的完整位置比较，只同步当前 coordinate。删除/历史/权限/预算的原实际断言未削弱。两个文件的 earlier medium/idempotencyKey 当前建库字段已存在于冻结 8 以前，本增量不再修改。

| 视角 | 后审结论 | 证据与边界 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 费用与工具未决证明分离，恢复不替代模型决策；等待当前二进制实际执行。 |
| 技术 A | APPROVE WITH CONDITIONS | 强身份、原 Task 全对象、未决 ledger 全对象、费用、计数及实际 owner 行均保留；等待集中构建。 |
| 技术 B | APPROVE WITH CONDITIONS | 原 2/3 与 1/2 失败原位保留，新增真实未决调用负例；未用生产放宽或过滤换取通过。 |
| 产品 | APPROVE | 仅恢复无未知写入的显式作者请求，不推定写入成功。 |
| UI/UX | APPROVE | 无用户流程或界面增量。 |

rustfmt 当前两个文件 actual exit 0；限定文件 `git diff --check` actual exit 0。未构建、未宣称新 4/4 或删除 2/2 已通过。零兼容门禁复查 PASS，无权限/限额/业务超时/OS 改动。剩余项仅主任务集中重建并执行当前全部默认断言。

## 当前10b真实执行身份准备修正预审

集中二进制实际 3/4；新未知写入负例在准入前第441行失败：Task真实状态 queued，fixture断言 running。日志 `/tmp/autonomous-v3-fixtures-current10b-nodebug/generation_provider_budget_tests.stdout.log` 原位保留。Task claim本身不会伪造Host开始执行；该fixture缺真实Author/Host PATCH queued→running与 `/turn` 精确绑定。仅未知Native分支增加这些现行API调用，再重新读取Task验证 running/原thread/原turn，之后才调用公开bind_recovery_operation。

五视角：架构 APPROVE（真实Host生命周期而不放宽状态）；技术A APPROVE WITH CONDITIONS（实际turn与recovery binding一致，原旧attempt/未知写入/费用/owner负例保持）；技术B APPROVE WITH CONDITIONS（保留3/4原失败，集中重建后全4项）；产品 APPROVE（不把未开始的Task称为正在执行）；UI/UX APPROVE（无UI变更）。零兼容 PASS，无生产/门禁/预算/权限修改，获准最小独占fixture同步。

实施后：仅未知Native分支在原bind前增加实际Author PATCH running与 `/turn`；重新读取的Native Task严格验证原thread、running和turn。后续recoveryOperation完整identity/hash/accepted/null receipt及作者retry/next拒绝、原费用/Task/Work/attempt/owner断言完整保留，provider-only positive未改。五视角后审为架构/产品/UI APPROVE，技术A/B APPROVE WITH CONDITIONS（当前二进制全4项待集中实际复验）。rustfmt actual exit0；不构建，原3/4日志保留，零兼容复查 PASS。

## current10d精确拒绝合同预审

当前实际4项仍3/4，真实未知调用已经通过running/turn/bind准备，作者retry被严格拒绝。第624行仅HTTP状态旧预期400对实际409不同；真实完整响应为 `{"error":{"code":"conflict","details":{},"message":"TaskNotWritable","recovery":null}}`。日志 `/tmp/autonomous-v3-authoring-current10d-nodebug/generation_provider_budget_tests.stdout.log` 原位保留。`server.rs:3898–3902` 当前将TaskNotWritable映射409/conflict；更精确拒绝不应回退生产400。

五视角预审：架构/产品/UI APPROVE（状态冲突符合终态权限，没有产品变化）；技术A/B APPROVE WITH CONDITIONS（409和完整错误全对象等值，原pendingRecovery、未知operation全对象、原费用/Task/Work/owner/attempt断言保持，当前默认4项待重建）。零兼容 PASS，允许仅当前fixture精确同步，不降低为is_client_error。

实施后：仅该拒绝assert变为409+实际完整错误对象强等值，全部未知/计费/owner/attempt负例保持。五视角后审同预审，技术条件是集中重建默认4项；rustfmt/diff-check actual0，无build，原10d 3/4保留。零兼容 PASS，源码再冻结。

主任务current10e集中当前二进制默认4项 actual 4/4、0 ignored/filtered，通过耗时1.13秒；日志 `/tmp/autonomous-v3-authoring-current10e-nodebug/generation_provider_budget_tests.stdout.log`。含真实未知Native call 的身份/未决状态/作者retry和next阻断/全部费用及旧Task/Work/owner/attempt不变，provider-only恢复边界单独通过。原冻结8/10b/10d失败仍保留。此目标五视角当前后审条件解除均APPROVE；零兼容 PASS。该结果仅此四项，不代表整个Cargo、真实供应商创作或文学质量通过。
