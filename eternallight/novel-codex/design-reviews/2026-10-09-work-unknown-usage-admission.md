# 未知用量的统一执行准入

## 实际故障、事实权威与责任

current11 本轮写手task-ffe942e…的公开Engine回执为17请求、577391已知tokens、44708未知tokens；Native grant已结算、charge及Work汇总完全相同，0活动预留。正文draft-c3c386b…真实committed，未知保存为0。相同thread日志保留网络断流与随后重试；公开历史没有可按原请求身份补结算的真实usage，不能靠汇总trace推测或归零。只读证据 `/tmp/autonomous-v3-current11-unknown-usage-diagnosis-final.json`。

现Native event observer在unknownUsage时停止，但Host同轮直接接力和手动resume仍可创建新Task/grant，已实际执行后续planner/critic/retryplanner。reserve/read_budget只检查exhausted/has-charge/unsettled；unknown只扣有限剩余额度，null额度不会因此停止。现HTTP回归甚至将已结算unknown之后继续执行写成成功标准，与B06“未知费用保持未知，必须核对后才能重试”及自动observer不一致。不是收费账本错配，也不是作者明确批准了新的不确定性契约；当前接口不存在这种合同。

责任在Story Core的执行准入，适用B03/B05/B06/B08/B10。Engine仍拥有真实服务商请求与费用证据；Native只消费真实receipt并控制同Work执行权限；Host/Renderer只消费拒绝原因及状态，不自行解释已settled为费用已知。

## 最小实施方案与边界

从同一Native ProviderBudget事实导出明确的执行阻断原因，区分unknownUsage与真正额度耗尽。现行reserve和read_budget统一拒绝新的grant/provider执行；需要新模型执行的Work /decision返回unknownUsage且不创建步骤；automatic observer使用同一纯领域判定。Work读取通过已有resumeBlockReason精确投影非terminal工作的unknownUsage，当前类型与UI标签同步，提供“模型请求用量尚未核对，暂不能继续执行”的清晰说明。

新拒绝应经过当前Runtime错误合同明确返回409及静态原因，不变成invalid_request、服务不可用、预算耗尽或保存未知。生产影响与helper位置待Native精确方案/独立前审记录。没有新数据库格式、版本探测、回填、兼容分支、费用核定/清零或作者风险确认入口；不修改规则，也不将现有普通resume解释为这种确认。

历史正文、保存回执、评审来源和已完结结果仍可读取；纯汇总已存事实的闭合不消耗新模型grant，因此不借费用未知否定既有成果。不能将全Work未知费用加进revision_outcome的历史provenance校验。无预算授权时保持现权限拒绝，不将缺账本当0用量。原失败、费用、作者输入、同Work标识及queued步骤保留，不重开预算或作品绕过限制。

模型继续组织文学任务和修订；工程只决定实际执行现在是否获准。Engine逐请求证据留存不足已登记，但本修复不扩成新供应商对账平台，也不伪造缺失的44708真实回执。

## 五视角前审

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 准入由Native同一账本判定，邻层只显示/停止，不复制费用政策。 |
| 技术A | APPROVE WITH CONDITIONS | 有限与null额度同样阻断未知，新Task/grant事务无副作用；已存事实仍可读。 |
| 技术B | APPROVE WITH CONDITIONS | 真实Engine未知receipt→Native已结算→手动/直接/自动三条公开路径拒绝，known回执正常继续；更新固化错误行为的既有正例。 |
| 产品 | APPROVE | 明确用量需核对，不让作者重试、换Work或填技术参数绕过。 |
| UI/UX | APPROVE WITH CONDITIONS | 使用精确领域原因，保持正文入口和作者消息，不误报保存失败或进行中。 |

独立前审确认关键早返回：`prepare_work_decision`必须在复用queued决策之前阻断新执行，停止时不得先reconsider待执行计划；`reserve/read_budget`为最终事务栅栏。observer保留未知写入/未结费/身份/暂停取消安全检查，将费用原因放在真实finished/successor无模型结闭之后。预算缺失保持未授权语义；读取合法可选账本投影不新增授权。综合批准此Native及必要Host/UI同步的窄范围，无兼容例外。

Native范围：`provider_budget.rs`（保留exhausted原额度语义，另有类型化执行原因）、`repository.rs`（WorkUsageUnknown）、`task_provider_budget.rs`、`work_decision.rs`、`work_execution_events.rs`、`work_tasks.rs`、`work_decision_actions.rs`（保留已有幂等和finishChapter纯事实路径，对新非finish动作同规）及Runtime `server.rs`（409 work_usage_unknown静态文案）。Root同步`src/types.ts`、`src/WorkTaskPanel.tsx`、`electron/work-step-continuation.mjs`。原未知允许正例及真实HTTP回归由独立测试agent负责，保留旧事实核定/幂等断言。

实施中的必要拒绝消费者同步前审：`recovery_operations.rs`仅将WorkUsageUnknown加入现明确写前零副作用拒绝白名单，复用原精确绑定、无已提交回执及事务回滚证明。五视角批准此单变体同步，否则已受理的request_work_action会在确定性费用拒绝后仍留accepted而误造未知写入。不得使用字符串推断、泛化所有失败或核定已有提交；真实绑定操作回归须证明409、rejected、无新步骤/费用变化。

后审定位纯事实恢复的必要同步前审：已真实发布且仍current的modelKnowledge产物，failed Task可经现recoverSavedResult核对已有成果；该动作无新Task/grant。费用unknown不能一概清除这个精确模式。批准`work_modeling_recovery.rs`与当前Work投影/控制在已有源、发布回执、取消、时效、未知写入及未结费门禁全部满足时继续提供RecoverSavedResult；普通resume/newgrant仍拒。不得将有费用、队列重发、源已变或结果未确认的恢复归入该例外。Root的`conversation-retry.ts`必要同步只能消费Native明确resumeMode=RecoverSavedResult与unknownUsage组合，允许核对保存事实；普通continueExecution的unknown仍不可点。UI仍提示后续模型执行停止。相应Core真实发布恢复回归与当前类型/Host/对话重试回归必须补证；不创建新风险确认或恢复接口。

另外发现Engine在同一个活动Task内，未知结算之后仍可允许网络重试的新reservation。独立追加前审五视角有条件批准同步这一通用计费合同，否则Native gate仅证明跨Task一致性。Engine仅在`budget.rs`新reserve任何成功/等待/可信id replay早返回之前检查unknown_tokens>0，返回精确静态原因；`bridge_failure.rs`和`bridge_exchange.rs`将其明确投影为task_provider_usage_unknown、budget_unknown、不可重试，不误写预算耗尽。已有已发送请求的真实结算、not-sent、终态聚合和receipt读取保持；unknown旧结算仍幂等保留，不能以reserve重放当重发许可。无文学政策、Native作品查询、CoreCLI或新授权。

最小回归增加有限/null额度匿名/new trusted ID/已有trusted ID/等待分支的事务不变矩阵，另有已发送请求仍可结算和真实代理首SSE断流→下一请求不下发、服务商计数仍1的验证；原错误地用unknown构造“额度耗尽”的fixture拆回明确已知quota负例。Engine实际NAPI构建与该组完整回归独立留证，不冒用前次错误传输证明本栅栏。逐请求新对账/证据平台仍不在本次范围，原44708不能从这次代码修复补成已知。

## 故障与回归、应用再验

真实跨入口不一致对应有限/无限额度的已settled unknown负例：POST /tasks与provider-budget、新决策、event claim不新增执行/收费，完整账本与作品快照保持。权限/取消/输入时效原门禁继续存在。旧unknown可继续的positive按当前规则改成精确拒绝，不降低断言或删原失败证据；另测known真实回执仍可创建下一执行，以及已保存/terminal产物可读。

评审恢复身份修复的生产与HTTP证据独立记录，不能冒用为本项验证。实际同QA重启应显示用量待核对并不启动新模型请求，原26Task/5Work/7版本/5作者行与全部费用保持；不能宣称局部改稿或长篇交付通过。真实未知usage无法可靠核对这一外部证据缺失须明确登记。

## 实施后复核

第一阶段Native与Engine按已批准范围实施时，Root同步当前`resumeBlockReason`类型与UI：非terminal用量阻断在对话主线显示“用量待核对”，说明已保存内容仍可查看，不要求展开工具明细；精确领域原因消费使Host在已排队步骤前也停止，而不读取账本自行制定策略。已有对话retry通用门禁保持，新增合同证明unknownUsage不能绕过。以下阶段记录保留当时范围与结果，最终冻结范围见本节末尾，不以阶段证据覆盖后续变更。

Root类型编译实际exit0，两个受影响完整Node文件19/19、0失败/跳过实际exit0，日志`/tmp/autonomous-v3-current11-unknown-usage-types.log`与`/tmp/autonomous-v3-current11-unknown-usage-host-retry-tests.log`。新增两项Host真实函数路径分别覆盖Native已投影阻断的queued/无step状态以及/decision返回unknownUsage；返回后没有模型派发。原source快照`/tmp/autonomous-v3-current11-work-unknown-usage-root-before.json`。这些是Host和类型合同证据，不能代替Native/Engine准入或实际应用验收。

纯已存成果核对的必要同步已完成：保留原来源、读权限、部分产物、输入/时效与取消检查的优先级，只有精确unknownUsage或无阻断且有仍current的真实发布成果时，Native才给出RecoverSavedResult；全Work任何accepted写入或未settled grant均禁止该模式。普通继续仍拒绝，核对后未执行队列仍被未知用量阻断。最终Native10生产文件与2个Core测试冻结于`/tmp/autonomous-v3-current11-unknown-usage-native-source-freeze.json`，精确前快照、累计增量和必要追加前快照均保留；没有使用缺预算授予执行权。

Root必要消费者最终类型编译exit0，两完整Node文件20/20、0失败/跳过exit0，日志`/tmp/autonomous-v3-current11-unknown-usage-types-final.log`与`/tmp/autonomous-v3-current11-unknown-usage-host-retry-tests-final.log`。`conversation-retry.ts`只允许Native的unknownUsage+recoverSavedResult精确组合进行无新调用的核对；其余阻断和continueExecution不能重试。Host直接接力只消费Native原因，在已排队动作之前停止，不复制账本政策。

Engine通用同Task门禁独立后审通过：Root逐块核对reserve栅栏、静态错误码和budget_unknown投影，有限/null额度、匿名/可信ID重放及等待分支不产生新reservation，已有请求结算与真实账本保持。实际NAPI构建exit0、Rust完整39/39、相关七个完整Node文件63/63、真实Codex自动重试集成5/5均exit0；后两项新增有限/null断流场景的服务商计数均为1。冻结源/构建/日志见`/tmp/autonomous-v3-current11-unknown-reserve-verification-final.json`。归属限制见`/tmp/autonomous-v3-current11-unknown-reserve-source-attribution.json`：未保存三个完整before内容副本，真实pre-hash和tool read保留，不把已有共享Git差异全归于本轮，也没有事后重建旧快照。

当前与此前28个边界文件逐项核对，仅已批准的Runtime server.rs错误映射有意变化，其他27个保持；Codex CLI SHA保持。Root6源/测试、Native10+2与Engine冻结一致，证据`/tmp/autonomous-v3-current11-final-unknown-usage-boundaries-and-root-freeze.json`。最终架构检查332文件/7crates/0diagnostics实际exit0，日志`/tmp/autonomous-v3-current11-unknown-usage-final-architecture-v2.log`。

独立五视角后审确认Native纯事实恢复、Host停止派发和对话明确模式消费符合边界，指出一处B08文案矛盾：合法running+unknownUsage+pendingActionIds状态仍提示“正在安排后续创作步骤…”。该必要UI消费者已修为优先消费unknownUsage，显示“后续行动尚未执行。”，其余分支保持；真实前快照`/tmp/autonomous-v3-current11-unknown-usage-ui-pending-before.tsx`，新panel SHA `a1733b0e7cefb522bae14987ca53fd70b6ab204637103a09150401fb6b613da5`，独立后审APPROVE。没有改变领域状态、收费或增加权限策略；最终类型与架构检查另留v2/v3日志，不借旧UI哈希。

上述阶段10的Native验证随后因终态规划报告核对的必要消费者继续扩展，不能冒用阶段10结果认证阶段12。原44708未能可靠补核，当前创作交付仍未通过。

### 必要追加：暂停后的终态规划报告核对

独立后审确认另一个现有纯事实消费者：所有Work步骤完成、各席报告已保存后，Host仍先ordinary resume再finalize。新增unknown门禁正确拒绝普通执行，但现RecoverSavedResult仅识别modelKnowledge，导致这条原报告汇总恢复路径被一起阻断。普通finalize拒绝paused是原暂停保护，不能直接放开。

五视角前审一致批准窄范围追加：Native在现RecoverSavedResult的Immediate/CAS事务内核对并汇总同project/Work/run的持久终态报告；只覆盖原reports_ready的全Work步骤completed集合，不扩展局部queued运行或新模型流程。复用现作者来源、冻结资料、注册角色/席位、Task及真实报告/输出回执权威；全Work未知写入、未结grant、活动Task、取消、版本冲突及其他原阻断仍拒。按现coverage生成complete或partial，未知用量和原费用保持。普通paused finalize及ordinary unknown resume继续拒绝。direct可以依据现领域完成条件完成；conversation的未完结决策义务仍保留paused+unknown，不能因报告汇总成功冒称作者目标完成。

责任归属：`planning_reviews.rs`抽取同源纯汇总，`planning_review_work.rs`只接入内部显式保存结果核对、保留原Finished暂停规则，`work_modeling_recovery.rs`导出明确模式，`work_tasks.rs`现事务与状态消费；Root必要Host `main.mjs`在automatic paused早返、当前provider核对之后、旧conversation review loop之前执行确切recoverSavedResult并立即return。不得经过ordinary resume、预算授权、模型评审、advance或额外finalize，未授权新Task或清费。Native与Host两位独立只读前审批准这些条件；不新增API、DB或风险确认入口。

对应回归应以真实保存报告→Task完成→真实Engine finite/null unknown→Native结算→暂停或blocked→公开recover验证，覆盖direct/conversation、partial、重复核定、费用/Task/grant不变；缺报告、错绑定、活动/未结费/accepted、取消及stale CAS保持拒绝。Host明确recover只发生一次control，无模型/预算/后续派发；不以mock替代Native公开合同证据。阶段10源冻结与已启动测试另记，不冒用为四文件追加后的最终验证。

必要外层消费者同属本追加前审：`main.mjs`内层control返回completed后，外层原来仍会executeWorkContinuation并可能消费generation handoff。此次实际成功的SavedRecovery需在同一handler明确返回，跳过外层接力；不可只改内层顺序。用真实Native明确模式及本次成功control事实界定，不按作者关键词或模型答复推断。即使存在合法存量handoff，本次纯核对也不得执行它；普通执行链保持。

阶段故障日志保持：预算HTTP首轮14/21，四个新增helper错误按JSON表示比较Native整数与Engine浮点数值，三个旧成功标准未同步unknown/未结费的新门禁；必要消费者按实际Shared数值合同及真实known/unknown分支同步，不更改领域证据、不伪造用量。Work决策HTTP首轮10/12，两条unknown负例仍期待笼统executionBlocked，已同步为精确unknownUsage；原无新step/授权、取消、来源、保存与完整事实不变断言保持。对应首失败日志`/tmp/autonomous-v3-current11-unknown-usage-budget-http-first.log`、`/tmp/autonomous-v3-current11-unknown-usage-final-work-http-tests.log`及真实测试前快照均保留。不能把这些首轮失败改写成全部通过；最终冻结后重验另记。

独立仓库门禁实际未通过：`npm run repo:check`先完成architecture0 diagnostics，再被Fork路径门禁拒绝（exit1）。当前共享工作区`codex-rs/codex-api/src/api_bridge.rs`、`api_bridge_tests.rs`、`codex-rs/core/src/mcp_tool_call/conversation_history.rs`未在该门禁获准，另有release-fresh/release-verified打包产物路径被列为未批准。日志与数量摘要`/tmp/autonomous-v3-current11-unknown-usage-fork-gate-failure-summary.json`；本轮未修改这些Core路径、删除打包产物、登记虚假批准或扩门禁白名单，CLI构建哈希保持。该整体/发布阻断与本次局部合同测试分开登记，不能宣称仓库全部验收或发布就绪。

### 最终冻结后的已取得证据与待验项目

Native最终12生产文件、2个Core测试文件及原评审来源文件已冻结；Root最终5生产文件含必要Host `main.mjs`纯核对消费者，另有3个Node测试文件。`main.mjs`内层只消费明确RecoverSavedResult，外层即使控制失败或存在存量handoff也不派发模型；ordinary执行链保持。独立五视角后审批准12文件Native、5文件Root及Engine三文件的边界。当前真实前快照、逐步增量和stage10冻结保留，不把共享Git历史差异全归于本轮。

已取得的最终结果：Core273/273，Native bins，Work决策HTTP12/12及事件HTTP7/7，Host/对话重试20/20、Host报告核对19/19、类型编译、架构332文件/7crates/0diagnostics均实际exit0。日志分别为`/tmp/autonomous-v3-current11-unknown-usage-final-core-tests-v3.log`、`...-final-native-build-v2.log`、`...-final-work-http-tests-v2.log`（同次完整两个HTTP目标）、`...-host-retry-tests-final-v2.log`、`/tmp/autonomous-v3-current11-planning-pure-recovery-host-first.log`、`...-types-final-v2.log`、`...-final-architecture-v4.log`。Engine39/63/5及评审身份44的生产哈希保持，引用各自真实最终证据，不重复执行无变更目标。

预算HTTP第三轮19/23、exit101保留于`/tmp/autonomous-v3-current11-unknown-usage-budget-http-third-final.log`：三个失败共用fixture在approve之前创建新planner队列，导致简报step依赖未完成planner，`attach_attempt`正确拒绝；这不是费用门禁拒错。第四项错角色注入使用另一已被占用的seat Task，SQLite UNIQUE在到达目标断言前正确拒绝。修复测试遵守真实批准/claim/完成/结费顺序；另将本轮原应用已经存在的queued历史状态明确隔离注入到临时测试库，不冒称修复后公开链可新建该状态，也不改费用或生产兼容。错角色改用真实存在且无seat的root advisory Task，保留外键和唯一约束。真实前快照和增量为`/tmp/autonomous-v3-current11-budget-fixture-repair-before-20261009/manifest.json`和`/tmp/autonomous-v3-current11-budget-fixture-repair.diff`，编译检查exit0后第四轮完整23/23、exit0，日志`/tmp/autonomous-v3-current11-unknown-usage-budget-http-fourth-final.log`；最终manifest为`/tmp/autonomous-v3-current11-budget-and-identity-http-test-source-freeze.json`。

同一QA已真实启动，初次锁屏时的重启前后20类Native元数据、26条Engine预算记录和正文SHA完全一致，无活动Task、未结grant、accepted写入或费用预留，证据`/tmp/autonomous-v3-current11-final-unknown-usage-after-restart-locked-preservation-proof.json`。之后再次实际访问成功，完整AX确认原对话用量待核对及停止执行说明；实际截图确认第2章正文可读、1211字符、未发布和保存结果入口。两项证据未在同一截图内显示，不声称已保存本地截图文件；界面同时显示模型验证不可用或失效，未追加模型验证、Retry或作者输入。查看后的同类数据与正文亦完全保持，证据`/tmp/autonomous-v3-current11-final-unknown-usage-after-ui-preservation-proof.json`；窄范围界面事实和工具限制记录于`/tmp/autonomous-v3-current11-final-ui-observation.json`。作者记录对比仅含ID/来源/时间/字节长度，不冒称作者全文逐字核对。当前稿仍1211字符且原前文保留检查失败，第1章保持、未采用/发布；原44708及旧Work112525未知用量均未清除。此次保存保持和阻断显示检查通过，不等于局部创作交付通过。

最终统一证据`/tmp/autonomous-v3-current11-final-engineering-repair-proof.json`绑定当前源码、测试、二进制、实际退出码和日志；最终独立边界审查`/tmp/autonomous-v3-current11-final-independent-boundary-review.json`批准20个生产冻结文件，历史A–D 28文件中26未变、2项已批准重叠，无意外漂移。最终Native Runtime SHA `cbf2c7981553708129f6f7ae5f552d37c994cdab90488a5be0d5f86948520520`、Engine NAPI SHA `9f74d494f172bee26c3fedb89f095ade698eb625b7abe55f76c90a8044111810`；CLI和MCP边界保持。综合五视角后审APPROVE；工程合同及窄界面边界通过，原未知用量、当前稿独立报告/最终交付、长篇创作和整体发布仍分别未闭环。
