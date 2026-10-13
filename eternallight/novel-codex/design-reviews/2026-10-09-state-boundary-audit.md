# 状态、权限与恢复边界全面审计

审计时间：2026-10-09 14:52（Asia/Shanghai）。基线 HEAD：`d46bf6f403d8ad6aa44fdf1b8b77c8d8d843f329`，结论针对当时包含未提交改动的工作区，不代表已发布版本。

## 结论与计数口径

**确认 9 项仍需代码修复的问题，另有 3 项待验证风险。** 此外，前序工作已修正的 4 项相关问题单独登记，不计入这 9 项；其中部分仍缺应用重载后的验收证据。

以“可独立修复的错误判断或信息丢失链路”为一项。同一错误在多个组件显示不重复计数；搜索命中、合理的拒绝、模型偶发生成错误和测试环境失败不计为工程缺陷。9 项中：2 项有真实创作记录，5 项通过局部生产函数或生产 SQL 的受控输入复现，2 项由明确源码调用链确认。局部复现不冒充完整应用验收。

主要问题在自建产品与 Engine 的状态合同和消费方式：不同事实被压缩成相同错误或状态，后续模块无法选择正确行动。模型可能触发错误，但模型不能补回工程没有提供的失败位置、结算事实或恢复准入。

## 范围与方法

贯通检查八条相关链路：

| 链路 | 已检查的责任边界 |
| --- | --- |
| 能力发现与加载 | MCP 资源目录、任务范围、Engine 当前请求 schema、发现回执来源 |
| 读取与写入授权 | Native task capability、动作授权查询、候选写入、知识提交后的终结门禁 |
| 任务与用量 | Native grant、Engine 请求账本、已确认/未知费用、有限/无限上限 |
| 长篇决策 | Work/Step/Attempt、Generation Run、`/decision`、`/next`、接续提案 |
| 失败传递 | 供应商/流式错误、Engine 本地拒绝、任务失败记录、UI 错误解释 |
| 保存与交付 | 未知写入核对、真实保存回执、评估引用、成果入口与完成投影 |
| 自动恢复 | 持久事件、执行租约、作者暂停/取消、重试与停止条件 |
| 用户界面 | 对话重试选择、任务面板、长篇状态栏、接续失败提示 |

方法：静态调用链审查、原应用日志与只读数据库核对、受控故障注入、受影响合同测试。未写真实作品数据库，未新增创作指令，未点击重试推进小说，未修改官方 Codex Core。多媒体专属服务执行器及全库所有功能未做穷举验收，不能据此保证其无缺陷。

## 待修复问题清单

P1 表示优先解决持续创作/恢复断点；P2 表示影响准确诊断、模型自纠或用户操作。当前没有由本次证据确认的越权执行或数据丢失 P0。

| ID | 优先级 | 独立问题 | 证据层级 | 责任 |
| --- | --- | --- | --- | --- |
| S01 | P1 | 首个请求失败被后续用量保护错误覆盖 | 真实创作记录 | Engine/Host 失败关联 |
| S02 | P2 | 评估引用拒绝缺少具体出错位置 | 真实创作记录 | Native 输出校验 |
| S03 | P1 | 停止纠错的指纹不能区分执行条件改变 | 生产 SQL 局部复现 | Native 恢复准入 |
| S04 | P2 | 长篇状态仍直接显示内部停止码 | 源码调用链 | Renderer 状态投影 |
| S05 | P2 | 读取任务失败与结算失败都显示待核账 | 生产函数局部复现 | Host → Renderer |
| S06 | P2 | 额度耗尽、未结算与身份错误共用错误码 | 源码调用链 | Native → Runtime → UI |
| S07 | P2 | 待恢复可能只有费用问题，却被解释成未知写入 | 生产 SQL 局部复现 | Native → Host/UI |
| S08 | P1 | 后台接续失败只写日志，缺少可见恢复状态 | 生产类局部复现及日志 | Host 生命周期反馈 |
| S09 | P2 | UI 按整段错误中的关键词判断本地状态 | 生产函数局部复现 | Renderer 错误来源识别 |

### S01：保留首个失败与当前阻塞原因

- 证据：真实任务 `task-0f518729e4854cb295f6c810b125209b` 的日志先出现 `engine HTTP response stream failed`，随后出现 `task_provider_usage_unknown`；Native 任务最终 `error` 仅保存后者。日志 `/tmp/twenty-resume-application.log` 第 7、8 行可关联；审计只读取固定标识和错误字段。
- 源码：[bridge_exchange.rs](/Users/hilbert/private/codex/eternallight/agent-engine/native/engine-core/src/bridge_exchange.rs:681) 每次记录当前 failure，并在后续请求预算拒绝时输出预算错误。首个原因虽在底层日志，未成为任务恢复可消费的失败链。
- 影响：用量保护正确，但作者和恢复模型只知道“费用未知”，不知道首先发生了响应流失败。解除结算障碍不等于解决最初故障。
- 修复：在原执行身份下关联首个原因、后续保护原因、请求/写入/结算事实；上层消费结构化链路。不得用估算费用、清空未知记录或盲目重放请求来消除报错。优先在自建 Engine/Host 公开合同修正；若确需改官方 Core，先依 B09 完整同步上游并复验。
- 验收：同一次执行先流中断、后费用保护时，两原因均保留；已保存内容和未知费用不消失；模型能区分待结算与传输恢复。

### S02：引用校验正确，反馈信息不足

- 证据：第 6 章评估任务 `task-81176d69215c4655820be43ddeb2460e` 两次 `submit_revision_outcome` 返回 `invalid_request / evidence_quote_not_found`，`details={}`、`recovery=null`，未提供 `instancePath`。原 rollout 第 196、210 行可复核。任务后来完成，因此这里是自纠成本和反馈缺陷，不是永久失败。
- 源码：[revision_outcome_validation.rs](/Users/hilbert/private/codex/eternallight/novel-codex/native/story-core/src/revision_outcome_validation.rs:45) 遍历 outcome/evidence 时，直接传播 [source_evidence.rs](/Users/hilbert/private/codex/eternallight/novel-codex/native/story-core/src/source_evidence.rs:27) 的通用错误。
- 影响：模型知道某条引文无效，但不知道哪项评估、哪段证据失败，容易重读、盲改整份报告。
- 修复：领域包装错误补充 outcome/evidence 路径、合法对象/版本引用及精确原因；不放宽原文匹配，不泄露无关正文，不由工程替模型改文学判断。
- 验收：多条证据只有一条错误时返回准确位置；未找到、歧义、锚点不符分别可判定；拒绝不保存无效报告。

### S03：两次历史失败不等于同条件重复失败

- 源码：[work_decision.rs](/Users/hilbert/private/codex/eternallight/novel-codex/native/story-core/src/work_decision.rs:329) 指纹只使用作品 revision 与 source digest；SQL 累计相同指纹的失败 advisory，达到 2 次即 `recoveryStalled`。未区分失败原因、连接/工具环境或中间成功进展。
- 局部复现：抽取生产 SQL，在隔离内存数据库放入同来源的 `unauthorized_tool_call` 与 `provider_connection_failed` 两个失败，计数即为 2。执行条件更改不是该判断输入。
- 影响：防无限重试的方向正确，但不同障碍和已改变的执行条件可能仍被当成原地重试。当前 [WorkTaskPanel.tsx](/Users/hilbert/private/codex/eternallight/novel-codex/src/WorkTaskPanel.tsx:29) 建议换模型后继续，底层没有对应的条件变化证据。不能据此断言所有显式新接续都失败；完整换模型恢复路径尚需应用复验。
- 修复：明确阻塞原因与恢复条件，区分连续无进展和历史失败累计；受信执行条件改变可触发重新评估，普通定时唤醒不能清零账本或绕过作者暂停。
- 验收：同一障碍无变化会停止；合法条件变化后原 Work 重新准入；不同失败原因不盲目共用同一计数；重复事件不重复执行。

### S04：同一停止事实在界面被不同解释

- 源码：[CreativeWorkspaceView.tsx](/Users/hilbert/private/codex/eternallight/novel-codex/src/CreativeWorkspaceView.tsx:1684) 对多数停止原因输出 `已阻断（内部码）`；[App.tsx](/Users/hilbert/private/codex/eternallight/novel-codex/src/App.tsx:1578) 的活动提示也直接插入 `blockedReason`。任务面板已有更清楚的部分解释。
- 影响：作者从不同入口看到不同说明，`recoveryStalled`、`executionBlocked` 本身不能说明该等待、重验连接还是处理保存记录。
- 修复：所有展示入口消费同一受信停止事实和恢复动作；未知原因显示“原因尚待确认”，不猜权限问题。技术码保留在详情。
- 验收：同一运行状态在对话、任务和长篇状态栏含义一致；不会把暂停、失败或保存中误报完成；按钮只对应当前合法动作。

### S05：状态读取失败被表述为费用结算失败

- 源码：[main.mjs](/Users/hilbert/private/codex/eternallight/novel-codex/electron/main.mjs:1272) 的一个 catch 覆盖任务 GET 与 provider-usage POST，统一产生 `usageUnsettled=true`；[CreativeWorkspaceView.tsx](/Users/hilbert/private/codex/eternallight/novel-codex/src/CreativeWorkspaceView.tsx:1685) 统一提示“模型用量待核账”。
- 局部复现：直接执行抽取的生产函数，分别注入任务读取连接失败与结算失败；两者输出相同布尔状态，前者从未读取到终态，更未发出结算请求。
- 影响：保守暂停是合理的，但无法读取状态并不证明费用未结算，错误提示给出不准确的处理对象。
- 修复：保留读取与结算阶段、是否取得权威结果以及对应重试动作；不将读失败改成默认安全放行。
- 验收：任务读失败、尚在执行、结算失败、已结算四种输入分别投影；读取恢复后重新核对原任务，不重复创作。

### S06：预算错误的原因在 Native 出口已丢失

- 源码：[task_provider_budget.rs](/Users/hilbert/private/codex/eternallight/novel-codex/native/story-core/src/task_provider_budget.rs:30) 将 `BudgetExhausted` 与未结算 grant 等不同情况返回 `TaskBindingInvalid`；`read_budget` 的到期等分支也如此。[server.rs](/Users/hilbert/private/codex/eternallight/novel-codex/native/story-runtime/src/server.rs:3936) 将其映射为通用 `invalid_request`。
- 影响：接收方不能区分“身份不匹配”“额度确已耗尽”“仍有在途账务”。UI 的现有解释会把这些归为作品/任务状态变化。默认无上限时通常不会触发耗尽分支，不能把它当成本次停止的全部原因。
- 修复：由 Native 返回独立且有限的结构化原因，保留实际准入条件；不新增作者必须填写的额度表单，不删除已有显式限制。
- 验收：真实预算接口分别验证身份错、已结算、待结算、确已耗尽与过期；每类拒绝仍零派发，并保留累计用量。

### S07：费用待核对不等于写入未知

- 源码：[work_decision.rs](/Users/hilbert/private/codex/eternallight/novel-codex/native/story-core/src/work_decision.rs:308) 将 accepted 工具操作与未结算 provider grant 做 OR，统一为 `pendingRecovery`；[work_decision_generation.rs](/Users/hilbert/private/codex/eternallight/novel-codex/native/story-core/src/work_decision_generation.rs:519) 同样合并。
- 局部复现：生产 SQL 在没有任何工具操作、只有一条未结算 grant 时仍返回 true。
- 消费者：[work-step-continuation.mjs](/Users/hilbert/private/codex/eternallight/novel-codex/electron/work-step-continuation.mjs:5) 称“保存结果还需要核对”，[semantic-work-continuation.mjs](/Users/hilbert/private/codex/eternallight/novel-codex/electron/semantic-work-continuation.mjs:20) 与任务面板称“未知写入”。
- 影响：相同停止码要求接收方猜测应核对作品还是账务；这是恢复对象混淆，与 S05 的“读取阶段未知”是两个独立来源。
- 修复：保留待核对项类型，允许同时包含写入和费用；模型/作者只得到真实存在的障碍。未知写入依旧禁止重复写入。
- 验收：仅费用、仅写入、二者皆有、二者皆无；核对其中一项不能错误地宣称全部恢复。

### S08：后台恢复的失败没有进入用户可见状态

- 源码：[work-execution-events.mjs](/Users/hilbert/private/codex/eternallight/novel-codex/electron/work-execution-events.mjs:166) 和第 197 行只调用 failed 回调；[main.mjs](/Users/hilbert/private/codex/eternallight/novel-codex/electron/main.mjs:216) 的实际回调仅 console.warn，且无 code 时统一写 `invalid-response`。
- 局部复现：生产 Consumer 两次 tick 均遇到 Runtime 读取失败，failed 回调两次，updated 回调零次。原应用日志也有 `original Work wake held ... invalid-response`。
- 影响：后台仍在轮询，但作者看不到接续为何不能开始、是否等待自动恢复、何时需要操作；原任务错误不能代表这次后台失败。
- 修复：Host 将自身连接/调度失败作为受信运行健康信息投影，保留阶段和重试状态；Native 继续拥有 Work/写入/预算事实。暂时故障自动退避，需作者动作时明确具体动作，不凭该提示授予权限。
- 验收：断开 Runtime/原对话读取失败时，界面出现准确待恢复原因；条件恢复后同一事件接续；暂停取消、事件重复及重启不重复派发。

### S09：供应商文本可被误当成本地状态码

- 源码：[agent-error-description.ts](/Users/hilbert/private/codex/eternallight/novel-codex/src/agent-error-description.ts:24) 对完整字符串进行未限定来源的 `TaskNotWritable|task_capability_invalid`、`TaskBindingInvalid|...` 匹配。
- 局部复现：向生产函数传入 HTTP 500 供应商错误，其 message 仅提及 `TaskBindingInvalid`，UI 仍解释为“作品或任务状态已变化”；提及 `task_capability_invalid` 则解释为不能写入。
- 影响：描述文本被提升为本地事实。该函数不改变实际授权，当前证据只确认错误归因，未发现由此越权。
- 修复：消费可信错误来源与类型；自由文本只做有界诊断，不参与本地权限/版本判断。这与 S06 不同：即使 Native 细分错误，该 UI 误判仍存在。
- 验收：同一字符串来自 Native 受信码与供应商 message 时含义不同；保留凭据脱敏，未知错误不猜状态。

## 已有代码修正，单列跟踪

| ID | 前序修正 | 已有证据 | 尚未证明 |
| --- | --- | --- | --- |
| F01 | 默认仅计量时，历史未知费用不永久阻断已终止原 Work；延迟结算后可重新准入 | Native 原 Work 事件回归；实际应用曾自动启动后续 attempt，未点击重试 | 20 章持续交付仍未完成 |
| F02 | 未加载工具的失败保留有界工具名与 `not_in_current_request`，不称为作者缺权限 | Engine 单测与 9 个受控协议用例（前序证据） | 新诊断构建重载后的真实模型纠错 |
| F03 | 持久 Run 的 `recoveryStalled` 接入实际 Work 投影入口 | Native HTTP 回归已包含准确投影断言，本次 8 项事件测试通过 | 新界面/运行时重载后的统一展示；S04 仍在 |
| F04 | 对话直接重试与多候选重试共用 Native `resumeMode` 判定 | 本次 conversation-retry 5 项测试通过 | 实际界面入口复核 |

这四项不能写成“所有相关问题已闭环”。本轮审计没有重启应用加载全部新构建，也未通过额外创作指令代替应用自纠。

## 待验证风险，不计入 9 项

1. **候选接口鉴权错误过于宽泛。** `candidate_task_with_reader` 将若干生命周期错误统一为 401 capability invalid，并使用写入措辞。须用合法读取在终止、到期和撤销状态下的真实接口用例，判断是否仅是有意的不泄露鉴权细节，还是阻断合法读取。目前不能认定仍存在读写共用权限缺陷。
2. **Host Runtime 错误适配丢弃结构化细节。** `nativeRuntimeError` 只保留 code 和 diagnostic，未保留完整 reason/instancePath/recovery。当前模型工具走 MCP 的另一条返回链，不能据此宣称模型都收不到 details；需逐一确认 Host 调用者有无依赖这些字段的恢复动作。
3. **能力、角色与操作范围的多处定义存在漂移风险。** 当前发现回执与隔离测试通过，但不能证明全部新角色/所有 operation 组合一致；应补目录→当前模型输入→Native 执行的矩阵合同，而非再维护一份人工列表。

## 已排除的相似表象

- 已安装方法/角色不自动授予委派或写入权限；Engine 历史、跨线程和旧目录发现回执不能自行激活工具，7 项现有合同回归通过。
- 读取动作授权不会同时批准候选执行；Native 授权查询合同通过。不能把所有 capability 校验删掉作为读写分离。
- 知识提交收到真实终结回执后只允许 `complete_task`，属于该工作单元结束约束，当前没有证据表明是任意读取被写权限拒绝。
- `getTaskResults` 与完成投影核对真实保存/任务证据；失败任务有已保存成果不等于任务成功。
- 作者暂停、取消、未知写入和身份/版本不一致仍需挡住自动接续；这类拦截是保护边界。
- 首轮 Runtime 重启测试在受限环境无法监听，原样在允许本地监听的环境通过，不算产品缺陷。

## 验证记录

本次相关回归最终 **95 项通过**：33 项恢复/预算/重试/动作权限测试（首次 32 过、1 项监听受限，原样重跑该项通过），46 项事件/接续/成果/任务预算测试，9 项 Native HTTP 测试，7 项 Engine 工具发现测试。

`architecture:check` 通过，扫描 333 个文件、7 个 crate，静态依赖诊断为零。该数字是静态工具的扫描数，不是人工审计文件数，也不证明本清单中的语义缺陷不存在。

证据归档：`.certification-reports/state-boundary-audit-20261009/`，包含脱敏后的问题探针结果、相关测试日志和审计源码摘要。生产函数探针证明 S05/S08/S09；生产 SQL 探针证明 S03/S07 的局部判断。真实记录只摘取错误码与身份，未复制小说正文或凭据。

## 修复顺序与防复发措施

1. 先统一失败原因及待核对项合同，贯通 S01/S05/S06/S07/S09；分别保留失败阶段、首因、当前阻塞与副作用事实，生产者和调用者一次同步。
2. 修复 S03/S08：停止条件以实际障碍和条件变化为依据；后台重试/等待/需作者动作有明确投影。不得让 Host 另选创作流程。
3. 修复 S02/S04：精确可纠正错误与统一用户说明；保留完整技术详情按需查看。
4. 每个修复沿真实接口补“相邻状态不同结果”回归，再在应用里观察原任务自主处理。检查者不代替模型写正文，也不反复追加提示推动下一步。

已有 B04/B06/B08/B10 规则足以约束这些问题，单纯继续加规则文本不能闭环。需要将本清单转为对应生产合同回归和应用证据。默认不增加作者的额度管理、工程参数或重复确认。

## 五视角复核及当前结论

| 视角 | 审计结论 |
| --- | --- |
| 架构 | 问题集中于边界语义与错误合同，不足以支持推倒架构或把领域规则搬进官方 Core；后续修复遵守唯一事实权威 |
| 技术 A | 必须保留未知写入、费用、暂停取消等保护；原因更精确不意味着放宽准入 |
| 技术 B | 现有测试通过与已确认缺陷可以同时成立；补相邻状态与错误传输用例，不能仅测试成功/拒绝两个极端 |
| 产品 | 以应用自动交付和停止时的明确下一步为标准；当前 20 章目标仍未完成 |
| UI/UX | 对话、任务面板、长篇状态统一说明；错误原因与用户动作对应，内部码进入详情 |

审计记录可交付；9 项修复和相关应用验收仍为待完成。文档不修改运行行为，不批准任何新增权限，不清理原始失败记录，不新增兼容层。

后续设计见：[自主创作可靠性：状态边界与自动恢复优化方案](../docs/designs/STATE_BOUNDARY_RELIABILITY_REPAIR_DESIGN.md)。该方案提供 S01–S09 的实施映射、状态组合回归及应用验收路径，当前为设计完成、待实施，不改变本审计的问题状态。
