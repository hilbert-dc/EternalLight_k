# 三章自主选用与修订闭环 fixture

日期：2026-10-08。五项为评审视角，不代表五位独立人员。主任务方案参考 `2026-10-08-authoring-run-model-decision-fixtures.md`。

## 目标与范围

只新增独占 `native/story-runtime/tests/support/authoring_selected_chapters.rs` 和本评审，不编辑主测试或既有 helper、不构建。复用当前已验证 `continuous_recovery.rs` 的真实 Runtime loopback、Native决策/独立报告/issue resolution/Engine grant结账/finish接口。原作者“记忆逐渐消失的港城”，3章、24 attempts、64 requests、4000000 tokens、600秒不变；随后作者实际新范围第4章原7 attempts额度。

用真实完整原文阅读→实际保存候选→独立报告 unmet→当前issue驱动模型选择修订→新正文及原稿历史→重新独立报告 supported/精确resolutionOf→实际finish交付三章。保存不等于采用正文或确认设定；每次原生 attempt按实际grant结账，Work/root/预算/当前source和报告身份严格核对，拒绝旧报告或越范围动作。

## 覆盖重分配

该组20左右attempt即可闭环；强制另加scene-plan、知识抽取与scene评估至少再6，超原24。旧逐章必经 brief/outline/plan/model/extract/scene策略删除，不扩大授权或伪造stage/status/ledger。场景resolve-feedback原子事务/作者decision、承诺提案/知识/实际事件负例继续由专门Runtime全量验证；本helper不冒称覆盖这些领域路由。

有附言走真实 `/generation-runs/{id}/future-intents`，按当前领域规范记录原话和真实source，自第2章生效；HTTP输入保留空格/newline，由Native合同决定持久文本。无附言不append、不新增作者message或feedbackDecision，模型仍基于实际unmet issue修订。`task_intents`只用于conversation Work，不能偷用该接口给generation扩大权限。

## 实施前五视角

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 模型选用真实Native目录，方法不替代领域权限，删除固定流水线而不加入fallback。 |
| 技术 A | APPROVE WITH CONDITIONS | 全原文/hash/来源/独立报告/current/同scope和原grant逐次计费，旧稿/history及原Work/root保持。 |
| 技术 B | APPROVE WITH CONDITIONS | 两个原调用保留，有/无附言均验证；模块静态冻结后由主任务集中构建和默认12项运行。 |
| 产品 | APPROVE WITH CONDITIONS | 自主修订不需要作者重复长提示；作者附言实际落库且生效范围准确，空附言不虚构。 |
| UI/UX | APPROVE | 无界面、冗余审批或用户理解步骤增量；控制模型fixture不声称文学质量验收。 |

零兼容预审 PASS：无迁移、双轨、旧schema回退、预算/超时提升或权限扩大。综合允许独占实现，当前实际终态尚待集中验收。

## 实施后复核

新增 `authoring_selected_chapters.rs`：用现行Runtime真实loopback路由、全决策分页/current version/sourceDigest/actionKey选用、全原文UTF-8分页/byteRange/payloadBytes/SHA256核对。执行首稿真实保存→独立unmet报告和actual issue→无新作者输入的当前generateChapter选择→新正文和旧稿历史→独立supported报告；resolutionOf逐项等于第一次真实报告的observationId集合，实际issue resolved后才能finish。第2、3章按需直接写作并独立读前章/当前稿，不调用未选择的创作方法。

作者附言经 `/future-intents` 精确传入、Native原话落库和真实authorMessage source检查，空格/newline不自行trim，自第2章生效；第1章修订仍只读原版本。无附言没有append、authorMessage或feedbackDecision。该helper的判定集中于原记忆消失要求及附言真实来源/范围传播；不把它冒称场景resolve-feedback作者决定测试，后者继续由领域专项覆盖。

原三章24 Attempt/64 requests/400万tokens/600秒保持：每个实际Task原grant由Engine结账，最后finish用原剩余grant执行真实reserve/settle耗尽64请求，强验累计448tokens/unknown0/overrunfalse/每attempt一份真实settled grant和ledger charge，未手写费用或状态。完成Run与Work root、三章当前独立acceptance报告、未采用候选、初稿历史均检查；新真实原7 Attempt作者范围续第4章，前Work/Run/前三章候选全对象保持。

| 视角 | 后审结论 | 证据与边界 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 真实模型选择与领域证据边界；没有生产固定流水线/fallback；当前二进制待集中运行。 |
| 技术 A | APPROVE WITH CONDITIONS | 原来源全页/hash/精确report+issue/current/Scope/Work/root/Engine费用和旧稿断言；静态可核对，实际HTTP终态待主任务。 |
| 技术 B | APPROVE WITH CONDITIONS | 仅新增helper，既有共享helper和主测试未改；原两调用与12项由主任务保留并集中默认验收；未自行构建。 |
| 产品 | APPROVE | 修订不要求重复提示，无附言不虚构原话，有附言真实持久和准确生效；新范围显式授权。 |
| UI/UX | APPROVE | 无UI扩张；受控模型fixture不冒称真实供应商或文学质量验收。 |

rustfmt当前新模块 actual exit0，限定diff-check actual exit0。静态冻结后通知主任务集中构建。零兼容后审 PASS；未增加额度、超时、权限或生产回退，无兼容例外。剩余项为当前全部12项实际二进制验收，不将源码实现视为通过。

## current10d完整作者来源合同预审

主任务当前4目标实际authoring 3/12，原失败 `/tmp/autonomous-v3-authoring-current10d-nodebug/authoring_run_tests.stdout.log` 保留。本helper两项首先失败于裸原话递归等值断言；现行唯一完整来源是 `{"authorText":"[要求版本 0] 记忆逐渐消失的港城","authority":"frozenAuthorIntent","intentRevision":0}`，包含Native冻结的完整生成要求，而非另一个原message表投影。

仅修本helper来源断言：全raw/hash/字节/分页检查不变，authorText严格等于真实run.intentRevisions+current chapter经当前generation_author_intent生成的完整文本；Task inputManifest.authorIntent也精确等值。实际Author DB的裸原话/真实message/source与有附言空格newline断言保持，不做source字段fallback，不用substring替代整份检查。

五视角预审：架构 APPROVE（唯一Native作者源）；技术A/B APPROVE WITH CONDITIONS（完整rendered等值与原message持久证明，全部两项当前binary待集中验）；产品/UI APPROVE（不要求作者重输，没有UI变化）。零兼容 PASS，Root明确授权仅此增量；无生产/共享helper/主测试改动。

实施后：coordinator和writer的authorRequirementsSource完整authorText/authority与真实run适用history生成结果精确等值，Task冻结authorIntent同时等值；实际message原话、原范围、全raw/hash、空附言零新增和原预算断言保持。五视角后审同前审，技术条件为集中当前两项实际执行；rustfmt/diff-check actual0，不build，10d原失败保留，源码再次冻结，零兼容 PASS。

## current10e完成阶段精确合同预审

当前主任务authoring 10/12，helper两项已经真实完成三章、修订与独立评估，先停止于235行stage旧期望completed；status234行completed已通过。`RunStage::Finished`和 `work_decision_delivery.rs:120–121` 当前状态是Run status completed/stage finished，Work仍completed。本轮只同步stage并增加第四章同stage finished断言，不把status改成不存在的finished；第四章仍原新范围和7 Attempt，不修改正文/范围/预算/完成proof。

五视角预审：架构/产品/UI APPROVE（Execution stage与Work状态明确）；技术A/B APPROVE WITH CONDITIONS（原精确所有完成/来源/费用断言保持，后续第四章全流程仍需实际跑）。原 `/tmp/autonomous-v3-authoring-current10e-nodebug/authoring_run_tests.stdout.log` 10/12保留，零兼容 PASS。

实施后：仅三章stage现行finished和第四章同stage强断言；Run status及Work completed未改，原全部闭环/账本/作者/正文/范围断言保留。五视角后审同预审，技术条件为集中重建实际完整两项；rustfmt/diff-check actual0，无build，源码再次冻结。

## current10f实际终态后审

主任务当前完整Native链中的authoring_run_tests默认12项 actual 12/12、0 failed/ignored/filtered，耗时22.99秒；日志 `/tmp/autonomous-v3-native-full-current10f-nodebug.stdout.log:765–779`。原两项有/无附言的真实3章保存→首稿unmet→当前修订→独立重新评估/精确issue resolution→原64请求结账→完成→第4章原7范围接续全断言已运行通过；完整作者原话来源、实际持久空格newline、无附言零message、Work/root/旧稿/未采用/原Run和前3章保存状态保护一并通过。

| 视角 | 当前后审 |
| --- | --- |
| 架构 | APPROVE：真实模型按需选能力和领域证据边界成立，没有加入固定创作流水线或fallback。 |
| 技术 A | APPROVE：来源分页/hash/当前报告/原问题观测resolution/Work/root/预算/历史/新范围全部实际断言通过。 |
| 技术 B | APPROVE：当前新增helper和主任务原12项默认全部执行，旧10d/10e失败保留，没有跳过或增额。 |
| 产品 | APPROVE：原简输入可完成修订与交付；附言真实生效、空附言不虚构、新范围仍显式授权。 |
| UI/UX | APPROVE：无界面扩张；受控模型功能验收不冒称真实供应商小说文学质量。 |

零兼容 PASS。此后审仅该当前12项和此模块，完整workspace Native链仍在运行并已有其他目标失败，不能据此宣称全工程验收通过。源码保持current10f冻结。

## 同Work后继修稿任务报告隔离增量预审

主任务授权只修改本独占helper和本评审，为普通Work专项无法合法派生generateChapter的覆盖重新建立真实生成Work路径。原first critic已通过Native保存revisionOutcome、完成任务并用原Engine grant结账；修稿writer已由原generation Work的实际模型决策派生。新增断言必须在该writer处于当前活跃准备状态、保存修稿之前执行，不能靠终态权限关闭得到拒绝。首次方案将其Task状态误写成running；current12证明实际Task为queued，Work/Step为running，详见后文当前合同修正。

协调者先从自己的实际read_context.taskWorkspace sourceId分页定位objectKind=revisionOutcome，精确匹配当前负评revision和真实producer；通过实际raw task-source全页连续字节与hash读取，完整saved content与实际revision payload等值。只读核对producer completed/outputRefs/ledger settled、同原Work/root及实际attempt sequence，修稿writer sequence必须在producer和协调reader之后。修稿writer自己的真实workspace目录必须items空且pagination完成；拿协调者已知sourceId调用raw task-source与parsed task-source-content，都必须精确409 task_source_not_in_catalog及当前task身份/recovery对象。

仅新增原已运行闭环上的来源读取与断言，不加Task、Attempt、作者输入、授权或provider预算，也不修改生产、主测试和共享helper。不SQL改身份/许可/状态；DB读取只核对真实输出内容、既有归属、顺序和实际费用。普通非生成Work专项继续保留合法报告保存/同Work后继reader完整回读与foreign负例，交叉引用本真实生成writer的隔离证明。

| 视角 | 前审结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE | 同一真实generation Work的领域权限和任务阅读范围分别验证，不伪造普通Work可用writer动作；Native保持唯一authority。 |
| 技术A | APPROVE WITH CONDITIONS | 报告先真实保存/完成/结账；合法reader全原文/hash与实际payload强等值；writer处于活跃准备状态、目录真空和两种读取精确拒绝。 |
| 技术B | APPROVE WITH CONDITIONS | 保留current10f原12/12作为增量前证据，增量后须Root集中构建重跑原默认12项；所有原预算、修订/独立评估、历史和第4章新范围断言保持。 |
| 产品 | APPROVE | 修稿不需要新增用户指令或预算；独立评估报告不会因同Work身份自动成为writer可读正文依据。 |
| UI/UX | APPROVE | 仅受控测试权限验证，无界面改动或作者交互增量；不宣称文学质量验证。 |

零兼容预审PASS：无fallback、双轨、旧schema、权限/预算/超时提升。允许实施上述owned增量；实际修后终态由Root唯一集中构建执行。

## 同Work后继修稿任务报告隔离增量后审

已在原write修稿调用显式传入原first critic与真实failed_report。协调者当前decision的reportRevisionId精确等于该负评revision；由它自己的当前read_context.taskWorkspace分页定位该producer/revision，验证savedReport/nativeSavedCheckpoint、真实completed producer、ordinal/version，再通过完整raw source字节与SHA256读取。完整原文wrapper和数据库实际revision content_json及原报告assessment均全对象等值。producer真实outputRefs指向该revision，原Run ledger含其terminal实际1请求/7 tokens/unknown0；真实grant settled=1、work_task_id仍原Work。

实际writer由该协调者完成原决策后取得。Author只读Work接口和实际attempt sequence证明producer、reader与writer属于同一个原generation Work，根线程等于该Work最初attempt的真实thread；sequence严格producer<reader<writer。writer尚未保存新稿时，从其自己的read_context.taskWorkspace完整分页objectKind=revisionOutcome，items必须完全为空、末页coverage明确searchScopeCompleteFromCursor=true。使用reader确切已读sourceId访问writer的raw与parsed两路，均强验409及完整task_source_not_in_catalog错误对象、当前writer身份与read_context recovery参数；Run完整对象前后不变。首次实现误断言writer running，current12实际到达此处后失败为queued；这不代表Host已启动provider。报告读取与拒绝均不靠SQL修改许可、终态关闭或额外新任务制造。

| 视角 | 后审结论 | 证据边界 |
| --- | --- | --- |
| 架构 | APPROVE | 合法模型修稿动作在原generation授权中派生；相同Work归属不会扩大writer报告阅读范围，Native权限不变。 |
| 技术A | APPROVE WITH CONDITIONS | 真实producer output/settled账本、同Work/root/sequence、完整报告原值/hash、活跃准备中的writer空目录和两路精确拒绝均有强断言；待集中实际执行。 |
| 技术B | APPROVE WITH CONDITIONS | 原12/12是增量前证据，增量后二进制尚未构建；默认12项需统一重验。原修订/重新独立评估/64请求/24attempt/作者附言/旧稿/第4章原7范围全部保留。 |
| 产品 | APPROVE | 不增加作者输入、授权、执行额度或交互；既有报告仍可作为合法协调者的实际判断来源。 |
| UI/UX | APPROVE | 仅fixture范围验证，无UI或用户理解成本增量，不冒称文学质量验收。 |

rustfmt（edition2024、skip_children）与限定git diff --check实际exit0；未build、未运行旧或新binary、未访问实际应用/外部供应商。仅本helper及独立评审改动，主测试和共享helper未编辑。源码静态冻结SHA256：0e4ed4576186a1de17b95a0d01596378586f9763a7e70f8f269c046fc9e02b0b。零兼容后审PASS；剩余项仅Root集中重建和全部默认断言真实终态。如writer出现报告或可读取原source，保留失败并报告生产事实，不能规避或放宽此断言。

## current12活跃准备状态合同同步预审

Root集中实际authoring 10/12，两helper都通过合法报告全页/hash与完整payload、真实producer completed/settled、同Work/root/sequence、writer目录空和raw/parsed两路完整409及Run全等，最终484行只因queued与误期望running失败。原stdout/stderr和manifest保留在 `/tmp/autonomous-v3-three-targets-current12-nodebug/`，三目标合计25pass/3fail、actualexit1；不声明并行期间全工程源码未变，owned helper/bin hash以manifest为准。

确证当前代码：support/continuous_recovery.rs:131–171的claim只用真实Author POST /tasks，不PATCH状态或绑定Engine turn；story-core/tasks.rs:555写Task queued/preparing/pending，task_attempts.rs:162同时把原Step置running/current_attempt_id，authorize_task_capability仍对ACTIVE_STATUSES（queued/running/waiting）开放精确许可（tasks.rs:689–701）。queued是合法原Task活跃准备阶段，不能虚称paid模型已运行，也不需要额外provider调用才能证明报告范围隔离。

只同步本helper：读取完整真实Task作为queued基线，精确前后全对象等值；Work必须running，唯一writer currentStep为running且currentAttemptId/attempt thread/status/target/inputRevision与该writer完全绑定。使用writer自己当前context.workspace的实际draft条目，raw全页/字节/hash成功，完整正文与真实当前冻结原稿payload及expectedCandidateRevisionId精确等值；再证明报告目录空和两路完整409，原Run与Task都不变。不改任意状态集合、不删除状态断言、不PATCH或SQL改变状态，不新增Task/Attempt/作者输入/预算。

五视角预审：架构APPROVE（Task准备/Step运行/Host provider生命周期分开，不新增生产分支）；技术A APPROVE WITH CONDITIONS（精确queued全Task基线、真实currentStep identity和合法全文成功共同排除终态关门）；技术B APPROVE WITH CONDITIONS（原current12两失败保留，Root统一原12项重建验收，全部报告拒绝及24Attempt/64requests/旧稿/第4章断言保留）；产品/UI APPROVE（无作者交互、预算或界面变化，不误报paid执行）。零兼容PASS。Root明确批准此唯一owned合同同步，修后实际终态仍待统一运行。

## current12活跃准备状态合同同步后审

已精确校验writer读取前queued且完整实际Task等于原Native返回Task；合法正文读取、报告目录为空和两路完整409之后，writer仍精确queued且全部Task字段与原基线全等，Run全等保留。Work必须running，唯一currentStep.currentAttemptId等于writer.id、Step为running，operation/inputRevision及全部domain target精确一致。当前Native标准resumeStepId绑定直接等于该Step.id；仅按task_attempts.rs:77–91去除这个传输绑定字段后与完整Step target全等，不能任选Step或忽略其他字段。当前Attempt必须唯一、queued且threadId精确等于writer。

合法正文正例使用writer自己的真实workspace目录定位writerInput/frozenWriterInput实际draft，revision精确等于冻结expectedCandidateRevisionId；raw全部连续字节/页限额/完整SHA256读取，全部来源wrapper和实际revision content_json全对象等值，正文精确FIRST_UNMET。新增通用本地workspace_items只参数化Native已有合法objectKind，报告仍原revisionOutcome全页items真空；没有改动生产范围或错误合同。全部原Task/Attempt/预算/64请求/24上限/作者附言/旧稿/独立评估与第4章原7范围保持。

五视角后审：架构APPROVE（活跃准备Task与已运行Step、尚未启动paid provider边界清楚）；技术A APPROVE WITH CONDITIONS（合法正文全文/hash成功、当前Step精确身份与queued全Task不变，有力排除终态拒绝，当前新binary待验）；技术B APPROVE WITH CONDITIONS（原current12 10/12和所有artifact保留，默认12项统一重验，不以格式检查冒充通过）；产品/UI APPROVE（没有新增作者输入、状态操作、预算或界面，不再虚称running/paid调用）。零兼容后审PASS。

限定rustfmt与git diff --check实际exit0；未构建、运行旧或新binary，未访问外部供应商/实际小说应用，未改变任何SQL状态或许可。owned helper源码冻结SHA256为dc2721975b5c5f72349110d7c93de660a40304593fb3bf12dfd78fe3bf9bec0e；其实际重新验收交Root统一构建。并行媒体fixture不在本增量范围，不声明whole tree unchanged。

## final13实际12项终态后审

Root统一直接回归已完整actualexit0、43/43。`/tmp/autonomous-v3-final-direct13/manifest.json` 的authoring_run_tests使用真实编译测试产物全部默认12项，无filter/ignored/skip，actualexit0、12passed/0failed，进程运行26.73秒、测试自身25.72秒；stdout为同目录authoring_run_tests.stdout.log，SHA256为975d7e0ad55558aa6230d3f1520942136d18426ac9954eb347a9825a06218b1b。两项原有有附言/无附言三章调用均通过，包含当前新增报告隔离、queued状态与合法当前原稿全文读取。原current12 10/12及expectedrunning/actualqueued两失败完整保留，没有以通过结果覆盖历史日志。

合法协调者完整报告分页/hash/完整payload及producer已保存/完成/settled费用、同原Work/root和严格先后sequence；修稿writer自己的报告目录完全为空、raw/parsed两路完整409、读取前后Task精确queued全对象和原Run不变；同Work/currentStep准确绑定、合法冻结原稿全部字节/hash与实际revision全等，均已实际执行通过。此前三章首稿未达标→当前修订→独立重新评估/issue resolution→同原预算交付，以及有附言精确原话、无附言零新消息、原24Attempt/64requests/旧稿/未采用和新授权第4章原7范围的强断言一并保留通过。

| 视角 | 当前终态后审 |
| --- | --- |
| 架构 | APPROVE：原generation授权内真实模型决策和Native范围authority成立，报告隔离与合法来源读取没有第二路径。 |
| 技术A | APPROVE：真实报告/来源/字节/hash/同Work/root/sequence/当前Task-Step身份、双路精确拒绝和原账本全部实际断言通过。 |
| 技术B | APPROVE：当前全部默认12项actualexit0，无过滤或跳过；测试binary SHA256前后均010a4077cff1d765302d5816bb684d0066651a51fb3ebd031065d07982b255ab；原current12失败及更早失败均保留。 |
| 产品 | APPROVE：无需新增作者提示、授权额度或重复输入完成修订和交付；合法阅读与范围拒绝在活跃准备任务均成立。 |
| UI/UX | APPROVE：没有界面扩张；该受控模型/真实Runtime机制验收不替代实际DeepSeek生成和文学质量验收。 |

owned helper当前SHA256仍dc2721975b5c5f72349110d7c93de660a40304593fb3bf12dfd78fe3bf9bec0e，无源码增量或新测试。零兼容终审PASS。本次只补实际证据，不再构建或执行测试。另Root的原完整Node应用链已actualexit0、201/201、0fail/cancel/skip及无源码drift（`/tmp/autonomous-v3-node-final13-summary-proof.json`）；完整Native全量仍在运行、未终态，不能据本默认12项或直接43项提前宣称整个工程全量通过。实际外部供应商文学验收边界不变。
