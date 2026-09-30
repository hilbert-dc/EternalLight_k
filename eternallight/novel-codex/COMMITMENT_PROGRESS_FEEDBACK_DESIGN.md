# 承诺推进的规划—正文反馈链

## 目标与边界

完成 S3 叙事承诺在后续规划、实际正文、证据评估与作者决定之间的版本化链接。复用 generatePlan / assessSceneOutcome、候选修订和既有 Check/反馈，不建立第二调度器、不增加供应商调用路径，也不允许模型修改独立承诺的正式状态。

## 实施顺序与契约

1. **规划绑定**：每个 expectedEffect 必须包含 commitmentRefs 数组，没有引用时明确为空。每项引用要么是 `{type:"commitment",commitmentId,revisionId}`，要么是 `{type:"proposal",reportRevisionId,proposalId,sourceRevisionId}`；不以自然语言摘要或批次局部 ID 独立定位对象。每效果最多四项，整个规划最多六十四项，重复引用和额外字段拒绝。
2. **引用门禁**：沿任务候选写入事务复核 main 选中的修订及原文锚点来源。滚动 Plan 只能引用起始章之前的内容；报告提案须来自当前选中的有效报告、正确提案及冻结正文，已经建立的 origin 不能以旧提案替代作者对象。独立承诺必须引用当前选中的精确修订，任何过期、其他作品、未来来源或未知 ID 都拒绝，不自动补最新版本。首次实施该领域契约及真实 Runtime 生成回归。
3. **正文与评估**：原 chapterPlanId/sourcePlanRefs 携带不可变规划，Scene/Effect 的引用据此定位原意图。评估按引用返回推进、部分/全部兑现、矛盾或证据不足的候选判断，以当前正文场景的已绑定事件支撑；“预期效果实现”不自动等同“整个承诺兑现”。第二阶段已接入严格输出契约及确定性校验，文学语义正确性仍需独立验收。
4. **反馈与作者决定**：把矛盾/证据缺口接入已有 Check 及明确接受/修稿决策；对已确认承诺的状态调整仍通过根任务候选修订与 Author Review。历史判断不可被后来正文覆盖，旧来源必须显示失效。整个闭环最终需要真实连续创作、金标语义及作者体验验收。

## 实施者五视角自检

依用户要求单人推进，不组织评审或子 Agent。架构：唯一修订/任务权威；技术 A：来源、范围、CAS、事务回滚与查询预算；技术 B：形状、伪造/越界拒绝、真实仓储和 Runtime 连续旅程；产品：自动创作也保留作者确认权；UX：现有对话/场景反馈层级不新增打扰操作，明确引用及候选状态。无旧格式 fallback、迁移、双轨、供应商分支或上游内核修改。

## 验证记录

### 第二阶段实施契约

- assessSceneOutcome 只获得原章节绑定 Plan 中的明确引用，不拓宽查询计划或给 Writer 新权限。`operationContext.boundCommitmentInputs` 返回逐项冻结摘要、兑现标准和源证据，带 `referenceState`；无关任务不装配。原报告被新报告替代但原正文未变时，仍评估原计划意图；已建立作者对象、独立修订替代或正文证据变更分别标记，不自动改绑。未来章来源及跨作品引用拒绝。
- 每个 effectAssessment 必须显式包含 `commitmentAssessments`，逐项覆盖其 commitmentRefs；没有引用时为空。每项包含 reference、referenceState、outcome、rationale、actualEventRefs。outcome 为 advanced / partiallyFulfilled / fulfilled / contradicted / unchanged / insufficientEvidence。推进、兑现和矛盾必须有该效果已验证的场景内事件引用，禁止仅用计划或他章正文作证明；失效引用只能返回 insufficientEvidence。判断仍是模型候选，不证明文学正确性。
- 矛盾进入场景 deviated，证据不足进入 insufficientEvidence；与逐效果结果一起确定性汇总，现有 Check 阻断并由作者决定接受现稿或修稿。开放悬念的 unchanged / advanced 不强迫完成。保存后来源变化使旧评估失效；历史修订不被覆盖。
- 每个输入项最多 9KB、整组最多 24KB、引用最多 64 项，超预算明确失败，不截断兑现标准、遗漏必评引用或静默换来源。整组纳入 Context Pack 共享预算和摘要，MCP 继续按现有工具分片上限交付。
- 场景评估任务的 scenePlanDraftLinks 同为必需输入，不按整包预算删除场景；可选知识裁剪后仍超预算则拒绝交付。所有 Plan 写入还按各目标章节号复核前文上限，没有 planFromChapter 的规划也不能绑定同章或未来证据。
- 单人五视角自检：架构复用 Plan/Task/修订权威；技术 A 核验来源、事务、范围与预算；技术 B 真实仓储、HTTP 和连续创作回归；产品保留作者最终决定；UX 展示候选解释、证据与失效状态。代码按私有解析/校验模块拆分，不扩上游 Codex 内核。当前版本同步更新类型、提示、fixture 和测试，不保留旧评估格式。

已完成第 1–2 步的规划引用契约与原候选事务门禁，并验证不可变规划把引用传到实际场景评估上下文。真实仓储测试拒绝未来来源、错误正文版本、未知提案、已建立 origin 的旧提案，以及不匹配的独立对象/修订，逐次验证分支头和 Task 产物不变；形状测试覆盖缺失数组、重复引用、每效果上限和伪造字段。连续创作七项 Runtime 用例覆盖三章创作、修稿及第四章续作，后续 Plan 和评估读取保留原修订引用。相关十二项仓储/Runtime 专项测试通过，Host 契约及交接十项测试通过。

完整 `CARGO_NET_OFFLINE=true npm run verify` 退出 0：边界检查、TypeScript/Vite、两轮 Rust 全套、180 项引擎、2 项真实 Codex 子 Agent 回环、7 项发布门禁、144 项 Agent、11 项 UI 通过。严格 native 工作区 Clippy（`--workspace --all-targets --locked --offline -- -D warnings`）退出 0。

整体验证发现并补齐工作区保存/放弃回调与承诺编辑器的接线，写入中拒绝离开和重复保存，刷新保留输入，任务分页失效重读当前页。随后收紧“继续分析本章”的按钮及 App 未保存检查，防止编辑承诺时旁路启动任务；在这项最后的前端调整后，重新运行前端构建及全部 11 项 UI 测试，均退出 0。没有删除测试以掩盖失败，也没有新增旧格式兼容或供应商专用分支。

上述是第一阶段验证记录；第二阶段进展见下文。合法引用及候选兑现判断都不是文学兑现证明。真实供应商连续创作、文学金标与作者旅程、Windows 实机仍须独立验证，S1–S5 不宣称已完成。前端包体偏大的构建警告仍存在。

最终隔离 Electron 工作区截图与回归测试已检查；当前六项系统校验入口置顶占用较多正文空间，应在后续 UX 整理中收进统一诊断分组。本轮没有以机械测试通过代替完整作者界面验收。原生工作区 `cargo fmt` 和上游仓库 `just fmt` 完成，最终 `git diff --check` 通过，未修改上游 Codex Rust 内核。

### 第二阶段实施进展与验收范围

Rust 私有 scene_commitment_context / commitment_assessment 模块已实现按冻结 Plan 引用读取及候选判断校验。独立承诺修订替代、作者建立原提案、原文源变更不自动改绑；报告替代但原源不变时仍保留原兑现标准。已保存场景评估在同一读快照检查草稿、知识、绑定及承诺输入状态，复用已有读事务而不嵌套新事务；章节重排后目录号与冻结正文/规划不匹配也视为失效。失效后保留旧内容并标为 stale。

commitmentAssessments 已同步进原生提交门禁、Context Pack、Host 普通任务与连续创作指令、Renderer 类型和真实 UI fixture，没有旧格式 fallback。矛盾和不足参与场景汇总并复用原 Check / resolve-feedback；修稿指令携带具体承诺候选理由。作者界面显示候选结论、原引用、来源状态和事件引用，接受现稿不会改变正式承诺状态。

专项验证已通过：三项真实仓储回归覆盖冻结标准、来源失效、报告替代、作者建立/修订、未来/跨作品拒绝、伪造/跨场景事件、章节重排和嵌套读事务复用，以及超过 9KB 的合法领域对象不被截断为不完整输入；Runtime 预算单测验证裁掉可选正文后仍完整保留承诺输入、必需输入不足时交由调用者拒绝。七项连续创作 HTTP 用例覆盖三章创作及第四章续作，并验证“效果 realized、承诺 contradicted”单独触发作者门禁，作者接受后完成，未自动建立独立对象；十一项 Host 契约/交接测试、TypeScript/Vite 构建及隔离 Electron 工作区回归通过。全量 verify 与严格 Clippy 的最终结果在结束后记录，不以专项结果冒充整体验收。

单人实施后五视角复核：架构保持唯一修订/任务权威；技术 A 证据边界、读快照和预算有硬门禁；技术 B 调用方一次切换、真实仓储及 HTTP 链覆盖；产品保留作者明确接受/修稿；UX 显示候选与失效，不给“已兑现”正式确认假象。未新增供应商分支、兼容代码或上游内核修改。剩余文学金标、真实供应商长篇和 Windows 验收不被本阶段结果替代。

第二阶段最终 `CARGO_NET_OFFLINE=true npm run verify` 退出 0：工程边界、TypeScript/Vite、两轮 Rust 全套、180 项引擎、2 项子 Agent 集成、7 项发布门禁、145 项 Agent、11 项 UI 全部通过。严格 native 工作区 Clippy（`--workspace --all-targets --locked --offline -- -D warnings`）退出 0。验证包括当前版本 mandatory 数组、失败回滚、预算完整比较和作者确认后的再次 Check，不删除失败用例或增加旧格式 fallback。前端包体警告及六项诊断面板置顶的布局整理仍未解决；不据此宣称 S1–S5 完成。

测试与 Clippy 完成后执行 product `cargo fmt` 及仓库 `just fmt`，均退出 0；最终 `git diff --check` 通过，上游 `codex-rs` 无工作区修改。按工程规则格式化后未重复运行测试。

### 后续作者工作区整理

以上“六项诊断置顶”是该阶段当时的剩余项，后续已按 [诊断分层方案](WORKSPACE_DIAGNOSTICS_DESIGN.md) 合并为默认折叠入口，保留运行和异常摘要、实际任务控制与正文附近的知识屏障。构建、全部 12 项 UI 回归及隔离 Electron 截图检查通过；该项不再是未实现布局缺口，包体警告和真实供应商、文学/语义及 Windows 验收仍未完成。本轮前端结果与上述 Rust/引擎验证记录分别归属各自源码阶段。
