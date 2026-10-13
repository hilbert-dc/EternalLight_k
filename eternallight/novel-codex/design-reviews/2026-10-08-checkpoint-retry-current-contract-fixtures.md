# Checkpoint retry 当前模型决策合同同步

日期：2026-10-08。五项为评审视角，不表示五位独立人员。

## 问题与实施范围

当前冻结 8 的已编译 Runtime 集成测试 `checkpoint_retry_tests` 实际 exit 101、0/3，原 stdout/stderr/exit 完整保留于 `/tmp/autonomous-v3-native-group1-current8/`。总 exec 209 秒，业务测试 0.41 秒；不是超时或模型质量失败。

`work_decision_generation.rs:569–585` 当前 `/next` 先返回 advisory 的真实 Step。两个 rebind fixture 创建该 advisory，却将 brief 输出写入它，`tasks.rs:593–597` 正确返回 TaskOutputForbidden；另一个 fixture 将 advisory target 与手写 generateBrief operation 拼接，`task_attempts.rs:119–124` 正确拒绝原 Step operation 不符。旧 `/generation-runs/:id/materialize` 路由已不存在；当前章节由模型选择 writeNextChapter 后 Native 物化。现行 Run stage 是 coordinate；不是旧固定 brief/draft 流程。

目标只同步 `native/story-runtime/tests/checkpoint_retry_tests.rs` 的准备过程：读取实际 Work decision 各页，显式模拟模型选择可用的实际 actionKey；章节选择包含当前冻结 acceptance；取得真实 original-author 原话及必要来源，沿当前 Native 返回 target 执行真正 primitive。初始预算、时限、正文、原稿保护、重绑定、事务回滚、作者暂停、未退还预算、旧 task/attempt 不被重新开启、迟到旧结果拒绝、精确 CAS 与重放断言保留。新增 coordinator 成本计入原 Run，不另授权或提高限额。重试只核对真实原写入 Step 的 attempt 历史，不把必需 coordinator 当成额外作者业务步骤。

不修改生产代码、共享 helper、MCP、Host 或主实施记录，不构建任何二进制。旧组测试继续使用原已编译产物，修后认证由主任务统一构建和运行；未经修后终态不称通过。

## 实施前五视角

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 测试明确选择真实 Native domain primitive；不重新引入固定生产流程或第二完成权威。 |
| 技术 A | APPROVE WITH CONDITIONS | 复制真实 Step target/actionKey/version/digest；作者原文全文回读与 hash 核对，验收标准不放宽；原预算与权限维持。 |
| 技术 B | APPROVE WITH CONDITIONS | 保留原 0/3、所有业务负例和全对象比较；只更新已废除阶段/路由的当前合同。不得因新失败猜改生产。 |
| 产品 | APPROVE WITH CONDITIONS | 作者编辑和暂停的强语义不变，旧 attempt 永不因测试准备被重开；模型决策成本真实计入同一授权。 |
| UI/UX | APPROVE | 本增量只同步当前测试准备，无新界面或重复作者消息。 |

综合：主任务已授权在确证当前 typed contract / 3.0 前提遗漏后同步本 fixture。零兼容门禁 PASS；不添加 fallback、迁移、双轨或降低断言。

## 实施后证据

已同步本 fixture 的真实模型选择准备：advisory 先读当前 Native decision 的全部 action 页，复制实际 actionKey/version/digest；第一次写章选择 writeNextChapter，由 Native 物化并返回 generateChapter，移除已不存在的 Run materialize 路由。章节选择包含稳定作者标准和显式无篇幅边界；同 intent 修订复用当前 acceptance。主任务与 writer 通过真实 context/source API 连续读取投影原文，逐页核对 source/hash、连续 byteRange、完整字节数、重组 JSON 的 SHA256 与真实最初作者原话。source 目录本身不计阅读证明。

新增 coordinator 使用原 Run/Work，provider 记账读取真实已授权 budgetId 与限额，没有新 initialBudget。原 12/9 次 attempt 上限、256 次 provider 请求、4,000,000 tokens 和 600 秒业务期限未改。简报 retry 的真实消耗为 coordinator + 首次 writer 共 2 次，重试 writer 后共 3 次；验证精确 2 个 Step，其中 coordinator 只有一次 attempt，原 generateBrief Step 保留两次 attempt。原旧 task 全对象不变、外部编辑拒绝 retry、CAS、迟到旧写拒绝、两个 rebind 场景的事务回滚/作者暂停/原费用不退/原稿不被采用等断言保留。stage 和阻断原因按现行 coordinate / executionBlocked 合同同步，不把它们当作旧流水线阶段。

`rustfmt --edition 2024` 仅解析并格式化本测试文件，exit 0；它不是类型检查或业务测试。源码复核没有发现要为这三项失败放宽生产的依据。未构建、重跑新二进制或修改共享 helper；当前修后测试终态由主任务集中构建后验证。原已编译 group 1 的 0/3 结果独立保留；不以源码修改冒充通过。

| 实施后视角 | 结论 | 证据边界 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 当前 Native 决策与原 Work 仍是唯一行动入口；fixture 明确模拟模型选择，不新增生产路线。需主任务最终验证。 |
| 技术 A | APPROVE WITH CONDITIONS | 保留精确原 Step/target、真实完整来源、author hash、当前冻结 acceptance 与实际 budgetId；无权限扩大、退款或新授权。尚待真实修后二进制执行。 |
| 技术 B | APPROVE WITH CONDITIONS | 原 0/3 失败保留；格式解析和作用域差异核对完成，不声称编译/业务已通过。所有原关键拒绝及全对象比较保留。 |
| 产品 | APPROVE WITH CONDITIONS | 重绑定、作者暂停与中断写入的目的保留；coordinator 成本计入原范围，不替作者重写原要求。 |
| UI/UX | APPROVE | 没有 UI 改动或伪造重复作者消息；原技术恢复合同只在测试中显式执行。 |

零兼容复查 PASS：只支持当前 typed contract，没有 fallback、迁移、双读双写、超时提升或断言降级。修后自动化状态为“已修改，等待主任务统一验证”。

## current 9 HTTP 诊断增量实施前评审

主任务集中编译成功，但 current 9 真实二进制再次 exit 101、0/3，业务 0.36 秒，原 call 的 JSON unwrap 掩盖了非 JSON HTTP 错误。原 stdout 与二进制/源码 hash 保存于 `/tmp/autonomous-v3-native-current9-fixtures/`。先仅将 call 解析失败诊断改为包含真实 method、URI、HTTP status 和原始响应 body 的 panic，仍拒绝非 JSON，不添加成功 fallback。使用已编译 current 9 同一目标默认全部断言及 RUST_BACKTRACE=1 独立记录调用栈，不重构或访问外部模型。实际输入合同错误只在确证后同步，生产疑点先向主任务报告。

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 暴露真实 typed contract 失败，不把错误转为成功值或新增生产容错。 |
| 技术 A | APPROVE WITH CONDITIONS | 保留 status/body/当前 run 输入，使用临时数据库受控 loopback；不读真实应用、密钥或私有小说。 |
| 技术 B | APPROVE WITH CONDITIONS | current 9 原 0/3 不覆盖；新输入修复必须有源码/调用栈证据，再由主任务集中构建。 |
| 产品 | APPROVE | 诊断仅在测试，原稿、暂停、退款和旧 attempt 业务语义不变。 |
| UI/UX | APPROVE | 无界面或用户错误文案变化。 |

零兼容预审 PASS。主任务已再次授权本 fixture 诊断所有权；未扩大到生产。

已编译 current 9 默认全部三项独立带栈复现 exit 101，日志 `/tmp/autonomous-v3-checkpoint-current9-backtrace.stdout.log`。三项栈都在 current_model_action 原第 925 行的首次 POST /tasks，而非 source 回读。`native/story-authoring/src/lib.rs:39–78、181–189` 当前 GenerationRun 没有 authorIntent 字段，原话由 intentRevisions[].instruction 保存。fixture 错读不存在字段得到 JSON null，违反 CreateTaskHttpRequest.author_intent:String，Axum typed rejection 不是 JSON。只将 fixture 改为明确读取实际全部 intentRevisions instruction，非旧字段 fallback。

同时 `read_current_inputs` 对 writer 省略真实 target 的 readingChapter/sceneOrder/povCharacterId/storyTime；`server.rs:1116–1156` 要求 query 与绑定 writer position 一致，必须逐字段复制实际当前 target，不能猜第一章或沿用旧 POV。此项即使不是本次最早失败，也是已确证的准备遗漏。五视角对该增量继续有条件批准：架构要求唯一当前 DTO，技术 A 要求真实位置逐字段与 Native 授权一致，技术 B 要求输入缺失直接失败而非 fallback，产品保持原授权完整和原稿强断言，UI/UX 无产品变更。仍只目标 fixture，待主任务集中编译后确认。

## current 9 增量实施后复核

已明确读取 next.run.intentRevisions 全部真实 instruction，不再读取不存在的 authorIntent；空列表或类型错误直接失败。writer context 从实际当前 target 复制全部四个位置字段，原错误 POV/time/scope 的负例仍保留。所有非 JSON HTTP 响应继续失败，但 panic 包含 method/URI/status/body，帮助下一次真实终态揭示 typed rejection。来源回读增加原文 2,000 bytes、响应 6,500 bytes 限额检查，返回真实 source 元数据及完整 JSON，供最后一项 plannedEffects 内容全对象强等值比较；同时确认它对应 writer_context 的确切 sourceId/textDelivered:false。未绑定 Author 的原 plannedEffects 内联比较仍保持，不恢复绑定入口内联。

原 current 9 的 0/3 和追加调用栈 0/3 均保留；追加复现只运行原已编译测试，无构建或外部网络。增量 rustfmt exit 0、作用域 diff-check exit 0，仅是源码检查。需要主任务重新编译 checkpoint_retry_tests；当前源码冻结，尚无增量修后业务终态。

| 增量后视角 | 结论 | 证据边界 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 当前作者要求 DTO、Native source 和真实 writer position 是唯一输入；无生产双轨。 |
| 技术 A | APPROVE WITH CONDITIONS | 完整原文、来源/hash/页限额及真实位置绑定保留；错误不被当作成功，等待新编译验证。 |
| 技术 B | APPROVE WITH CONDITIONS | 原全部强业务断言保留；0/3 原失败、调用栈和格式检查分开记录。 |
| 产品 | APPROVE | 原作者完整要求及所有追加 instruction 保留，原稿/暂停/费用语义未改。 |
| UI/UX | APPROVE | 仅测试诊断更明确，无界面变更。 |

增量零兼容后审 PASS：无 fallback、迁移、放宽授权或预算/超时增加。

## current10b可信Host结算元数据修正预审

主任务 current10b 三项真实失败均为完成Task后 GET `/tasks/{id}/provider-budget` 返回 TaskBindingInvalid；模型终态权限关闭正确。原日志 `/tmp/autonomous-v3-fixtures-current10b-nodebug/checkpoint_retry_tests.stdout.log` 保留。现有charge_budget在complete/failed后请求模型可用预算，准备顺序不符合当前合同。

仅此fixture charge_budget改为可信Host直接读取真实 `story_task_provider_budgets_v1.grant_json`，严格反序列化当前TaskProviderBudget并校验实际taskId、workTaskId/runId、预算ID，按原grant创建真实Engine PersistentBudget和 terminal receipt，再走原Native provider-usage接口结账。该元数据是Host结算证明，不能作为模型读取原文、来源或授权扩大的证据，不开放终态模型访问，不新建虚构预算。

五视角预审：架构 APPROVE（Host结算和模型终态权限边界）；技术A APPROVE WITH CONDITIONS（原grant身份/额度与Enginereceipt，原暂停不退款、旧attempt不重开、原稿/作者编辑保留）；技术B APPROVE WITH CONDITIONS（三项完整默认断言集中复验，原失败保留）；产品 APPROVE（用户不用为工程结算重复提供内容）；UI/UX APPROVE（仅fixture，无UI增量）。零兼容 PASS，无production/门禁/预算或超时修改，允许最小同步。

实施后：charge_budget通过真实task/project绑定JOIN读原grant_json，deny_unknown_fields当前DTO反序列化并严格核对task、原generation Work和budget ID，Engine沿用全部原限额出具terminal receipt后原provider-usage结算。所有原3项正文/history/编辑/暂停/退款/旧attempt/CAS断言保持。五视角后审：架构/产品/UI APPROVE，技术A/B APPROVE WITH CONDITIONS（当前binary全3项待Root集中重建执行）；rustfmt/diff-check actual exit0，不build，原current10b 0/3未覆盖。零兼容 PASS，源码冻结。

## current10e真实观察时点与暂停边界预审

当前实际checkpoint 1/3，原日志 `/tmp/autonomous-v3-authoring-current10e-nodebug/checkpoint_retry_tests.stdout.log` 保留。暂停case第551行首先是故意把Work kind改conversation后，真实GET返回400 TaskBindingInvalid，但fixture丢掉先前status后强填200。保存并严格校验先前完整(status,body)，rebind事务失败后必须完整相同；还原kind后原Run/Work/Task/binding/workspace/receipt全对象回滚证明保持。

运行case第722行在renew之后已真实调用current_model_action：合法coordinator已经消费一个原attempt、留下真实zero-usage terminal charge和lastTask/token/version，再返回queued writer，不能对非法writer请求比较coordinator之前的renewed。baseline改成当前next.run完整对象；另明确校验原授权/intent/source/current章节不变、attempt只加1、原charge全部前缀不变、只新增对应lastTaskId实际charge，无退款。每个非法writer请求对当前Run/Work和所有Task/Step/Attempt计数完整等值不变。

加强暂停边界：原rebound取消Task结账后保存完整Run/Work/原Task和计数，再调用next必须action null、paused且各对象/计数/账本完全不变；只有后续原测试明确作者POST replan才能恢复running。迟到旧写、作者旧稿、POV/time/scope拒绝和最终sourcePlanRefs/未采用证明仍全覆盖。

五视角预审：架构 APPROVE（模型合法后继的真实费用与暂停无派发分开）；技术A/B APPROVE WITH CONDITIONS（只改合法观察baseline，保留全对象/计数/旧任务/账本/原稿负例，当前全部3项集中重建）；产品/UI APPROVE（不偷偷恢复用户暂停或重复原话，无UI改动）。零兼容 PASS，无生产放宽、预算/权限/超时变化。

实施后：malformed Work前真实HTTP完整tuple保存、paused分支严格400+完整TaskBindingInvalid对象，事务失败后同tuple等值；恢复kind仍与原Work/Run/binding/Task/receipt全等。暂停原cancelled grant结账后完整Run/Work/Task/计数经next不变且action null；后续作者明确replan才恢复。合法coordinator后current next.run做每项非法writer请求baseline，同时强验原全部授权/历史/来源字段不变、usedAttempt只+1、旧charge逐条前缀不变、新charge严格等于lastTask/原budget/terminal/zero usage、不退款、version/token确有真实进展。原scope/位置/迟到旧写/候选原稿与未采用全部保留。

五视角后审：架构/产品/UI APPROVE；技术A/B APPROVE WITH CONDITIONS（原10e 1/3保留，全部3项集中重建默认实际终态待确认）。rustfmt/diff-check actual0，无build；源码冻结，零兼容 PASS。

## current10f省略位置字段的当前合同预审（尚未实施）

完整Native链当前checkpoint仍1/3，两个rebind都已通过此前paused/原Task/baseline/账本断言，首次失败在861行：省略povCharacterId期望403，但真实200。原日志 `/tmp/autonomous-v3-native-full-current10f-nodebug.stdout.log:813–848` 保留。`server.rs:1116–1133` 当前读取frozen Task target派生缺省readingChapter/sceneOrder/storyTime/POV；显式不同字段仍严格403 writer_scope_mismatch。Native能力合同明确位置由Runtime派生，不能让作者/模型重复输入一个已冻结字段才能读原范围。

待完整链终态后只同步本fixture：省略POV改为合法默认导出正例，实际target.position/queryScope严格等于原显式请求；使用该响应真实source目录逐页raw回读、hash/byteRange/payloadBytes/UTF8完整证明，再与原writer_inputs的完整作者要求、当前原稿和其他来源原值逐个等值。重用当前独占原文阅读循环为本文件函数，不编辑共享helper。显式other POV以及storyTime10两个负例保持精确403/writer_scope_mismatch；原budget/Task/Work/history/暂停无dispatch和后继writer/迟到旧写/未采用等断言保持。

五视角预审：架构 APPROVE（Runtime负责冻结范围派生，无新默认authority）；技术A/B APPROVE WITH CONDITIONS（只同步当前省略字段合同，source原文完整等值，显式越界严格拒绝，当前3项实际重建仍需终态）；产品/UI APPROVE（降低重复输入复杂度，没有UI变更）。零兼容 PASS，无生产/授权/预算/超时修改。当前仅记录方案，测试源码保持current10f冻结，等待主任务确认完整链退出后统一应用。

## current10f终态后的省略POV增量实施后复核

主任务确认完整标准Native current10f已真实退出101，100个目标合计843项通过、8项失败，1524个受检源文件前后未变，并授权应用上述准备增量。原完整日志继续保留，不将本fixture修正计为已通过。已实际通过的三章自主修稿及第四章新范围helper不改动。

本fixture现对省略POV请求严格验证200及与原显式请求完全相同的target、queryScope、sourceWatermarks、snapshotId、policyDigest和contentHash。由该请求自身返回的真实source目录逐页回读原文，保留连续byteRange、完整payloadBytes、UTF8、每页大小和完整SHA256校验；逐项完整JSON等值比较原writer_inputs的全部来源，并精确比较每项sourceId、contentHash、payloadBytes和sectionPath。完整authorText仍等于冻结任务作者要求，/relevantText的全部内容仍与原当前正文来源等值，未以目录或hash存在代替正文阅读。既有回读循环仅提取为同一独占fixture内的read_pack_sources函数，未修改共享helper。

显式不同POV（linxi对绑定zhangheng）和storyTime10对绑定20的请求继续严格403及writer_scope_mismatch；原budget、Task/Work、历史、暂停无新派发、费用不退、旧attempt不重开、迟到旧写、候选原稿与未采用断言均保留。没有生产、权限、预算、超时或错误容错变化。

| 实施后视角 | 结论 | 证据边界 |
| --- | --- | --- |
| 架构 | APPROVE | Native冻结target仍是唯一位置authority；fixture验证派生范围完全相同，无新增fallback或双轨。 |
| 技术A | APPROVE WITH CONDITIONS | 全部来源原文、元数据、hash和作者/正文全值保留；显式越界仍拒绝。尚待主任务集中重建并执行全部三项。 |
| 技术B | APPROVE WITH CONDITIONS | 原current10f完整失败日志保留；合法省略字段强正例代替已失效负例，其余业务负例未减少。格式检查不冒充业务通过。 |
| 产品 | APPROVE | 确保已冻结范围无需重复输入位置字段，同时越界请求仍拒绝；作者原话、暂停与原稿保护不变。 |
| UI/UX | APPROVE | 仅测试合同同步，没有界面改动或新增重复作者输入。 |

rustfmt（edition2024、skip_children）和定向git diff --check实际exit0；未构建或执行新二进制。checkpoint源码冻结SHA256为c54689989cd6c3fda738f182cfe00b00ecaa4093ff6cc450d55d22cdd4a8d0fe。authoring_selected_chapters helper仍为5ec85295fae22e1709806ddf06eddf24e5a135ee3b963aeef0cac07427153643，其current10f真实12/12证据保持独立。零兼容后审PASS；当前checkpoint修后状态为“源码冻结，等待统一验收”。
