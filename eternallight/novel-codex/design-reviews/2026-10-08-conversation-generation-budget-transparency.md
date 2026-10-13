# 对话创作批准前的用量说明

## 变更与实施前五视角评审

目标：作者批准新章节创作前，在现有提案卡中准确看到“当前未设用量上限，可随时暂停或停止”。只补充当前策略的展示，不设置有限默认额度，不新增RPC、预算框架、控件或创作步骤。

责任模块：Renderer，适用B01/B05/B08/B10。范围仅为 `src/ConversationGenerationHandoff.tsx`、现有对话提案UI fixture和本记录；不修改Shared策略、Host、Native、调用许可、费用或保存合同。

当前事实与公开链路：

- `shared/generation-permit.mjs` 的 `generationPermitForChapters` 验证原新增章数，返回 `maxTaskAttempts: null`；当前公开声明同样为 `null`。
- `App.tsx` 的提案批准调用 `generateWork`，保留真实原话、章数、proposalId、版本、快照与sourceDigest；`agent:generate-work` 到 `electron/main.mjs:startGenerationRun` 使用同一Shared permit。
- 当前Host发送 `maxTaskAttempts: permit.maxTaskAttempts`，`maxProviderRequests`、`maxProviderTokens`、`deadlineAt` 均为 `null`。这条新范围入口仅计量实际用量，没有此次批准的有限上限。Native负责授权和领域额度，Engine负责实际请求与结算；UI不创建额度事实。
- 卡继续执行真实来源核对、重复点击隔离与原禁用条件；工作稿仍不自动采用或发布。

展示使用现有Shared permit，不在Renderer复制一个预算数值或另建策略。只对有效提案读取该函数；无效提案继续禁用批准，不能因展示计算抛错或获得新权限。此文案描述当前新范围入口，不概括显式有限额度的其他任务；未来该公开入口策略改变时，必须按B10同步文案与验收。

模型仍自行选择阅读、方法、起草、协作、检查及修订。工程继续保证当前来源、权限、身份、费用和结果；本改动不把创作请求固定成流水线。

| 视角 | 实施前结论 | 必须保持 |
| --- | --- | --- |
| 架构 | APPROVE | 仅投影现有策略；复用Shared公开函数，不建立第二事实源。 |
| 技术A | APPROVE | 原CAS、飞行隔离、禁用、取消和执行身份全部保持，不修改任何额度。 |
| 技术B | APPROVE WITH CONDITIONS | 在现有实际App UI fixture中断言批准前可见说明且尚未启动生成；保留全部原行为断言，Root统一验证。 |
| 产品 | APPROVE | 作者看得到计量而无上限的实际含义，不制造额外确认或技术参数输入。 |
| UI/UX | APPROVE | 在原说明段落中用简明文字提示，不新增面板；继续说明按实际计费、不会自动采用或发布。 |

零兼容门禁：PASS。无旧字段fallback、版本分支、迁移、双读写、旧行为保护或权限例外。一个连贯改动使用本记录。

## 最小验证与证据边界

Root统一执行 `npm run architecture:check`、`npx tsc --noEmit`、`node --test electron/tests/conversation-generation-handoff-ui.test.mjs`。现有UI测试检查真实App渲染的提案、原范围与来源传递、取消/过期/双击、草稿与选区保留、单一作者消息及未启动恢复；只增批准前说明断言，不更改原快照或弱化门禁。

本次没有模型或领域执行行为改动，不要求扩大为完整Native、全Node或付费模型测试。实际短输入创作QA由Root独立记录，展示检查不能证明文学质量或长篇完成。

## 实施后复核

实际变更：有效提案读取现有 `generationPermitForChapters`，仅在原说明段落中补充当前无上限说明。现有实际App fixture在首次批准前断言完整说明可见且生成调用仍为零；原取消、过期、双击、精确身份、草稿/选区、未启动恢复与快照断言全部保留。fixture中已有媒体初始化及问题汇总合同同步保持原样，不属于本次增量。

| 视角 | 实施后静态复核 | 结论 |
| --- | --- | --- |
| 架构 | Renderer引用原Shared公开函数，未改策略或引入第二额度来源；B01/B05责任边界保持。 | PASS |
| 技术A | 来源核对、飞行隔离、禁用与回调逐行保持；仅有效提案计算permit，无新写入或调用。 | PASS |
| 技术B | 现有UI fixture新增批准前可见性和零生成断言，未改快照或删原断言；尚待Root实际运行。 | APPROVE WITH CONDITIONS |
| 产品 | 原批准动作仅多了真实现行策略说明，没有新确认、预算表单或流程。 | PASS |
| UI/UX | 同一说明段落展示无上限、可停止、实际计费及候选边界，无新增控件。 | PASS |

静态冻结身份：

- `src/ConversationGenerationHandoff.tsx`：`e9caa82032374dc75f4a3c8697ef4c1d1d6bfdf4989a65a96bc733a1c62bbd6f`。
- `electron/tests/fixtures/conversation-generation-handoff-ui.tsx`：`dd05cefa696683d73aa18038bc709023d15b3793e976e53c17e6c7aa9f9c1ea2`。
- 原Shared实现只读核对：`87a22bc20549ff23686da46480b8ae081098be95ca694bd62e0279b557cbba5d`；声明：`855229b61f989717049dc2f4c105a143d8d54f136cf80feb4d8424e6ebb772d6`。

零兼容复核：PASS。B08/B10展示与现行合同一致。本代理未运行build、测试、UI或模型；以上仅静态复核，不声明测试通过。Root实际验证待补，原失败证据及独立真实创作QA不改写。

## 当前历史读取合同的fixture增量前审

Root实际UI已启动业务并失败：`/tmp/autonomous-v3-current2-budget-ui-local.log`，exit1、0/1，`Second proposal missing`；仅一次send，页面明确显示历史不可读取。该原失败保留。

确切遗漏：当前 `App.tsx:497` 调用公开 `agent.readConversation(projectId, threadId)`，历史error会按原准入禁用发送。旧fixture没有该方法，且 `listThreads` 永远返回空数组；首轮 `thread/started` 选中root-thread后历史读取必然失败。这不是模型或生产权限错误，不能通过放宽Composer门禁处理。

限定修正：只在同一fixture实现当前线程目录和回合历史的公开契约。仅由该fixture的实际send登记对应作品、真实root-thread与作者回合，再发布事件；目录返回已登记记录，历史读取核对作品/线程，未知或跨作品ID明确拒绝，不将任何未知对话伪装为空成功。类型采用当前公开ThreadDetail；不改生产状态或伪造Native权限。

| 视角 | 增量实施前结论 | 必须保持 |
| --- | --- | --- |
| 架构 | APPROVE | 只同步测试适配器，真实生产历史与发送边界不改。 |
| 技术A | APPROVE | 历史必须绑定已登记的project/thread，未知身份仍拒绝。 |
| 技术B | APPROVE WITH CONDITIONS | 原四场景、预算展示/零生成断言及快照全保留；Root重验，原失败不能擦除。 |
| 产品 | APPROVE | 继续验证正常对话接力，不把fixture缺方法误判为需要重验模型。 |
| UI/UX | APPROVE | 不新增UI，不绕过历史error禁用发送，仍保留输入与阅读状态。 |

零兼容：PASS。这是当前公开契约的一次性fixture同步，没有旧接口fallback或生产迁移。仍由Root唯一执行测试。

## 当前历史合同fixture增量后审与重新冻结

实际仅修改owned fixture：在实际send中登记同一project/root-thread及原作者回合后发布事件；当前 `listThreads` 返回该目录，`readConversation` 只回读已登记且作品归属一致的完整记录，未知或跨作品身份直接抛错。历史包含本fixture实际收到的原话，没有补造模型回复、作者批准或Native执行许可。

| 视角 | 实施后静态复核 | 结论 |
| --- | --- | --- |
| 架构 | 当前公开ThreadDetail/目录契约仅在fixture同步，未改生产历史权威或准入。 | PASS |
| 技术A | 初始无登记记录，send后再可读；错误身份仍拒绝，无通用空成功fallback。 | PASS |
| 技术B | 原四场景、预算文案/零生成、全部精确来源与草稿/恢复断言及原快照保持；待Root复验。 | APPROVE WITH CONDITIONS |
| 产品 | 修正测试适配器的缺方法，仍从实际App入口验证第二轮正常发送。 | PASS |
| UI/UX | 生产历史error文案及禁用保持，既有视图/确认卡行为不改。 | PASS |

当前重新冻结：fixture SHA256 `1c28b6f8f062d03ed410466ec6bf93629d021a5a456614b22f4cb62dc1314961`。原快照SHA256 `6067384013c7d8c69919a8da5f950c3622099df952fcb6ff3e90c96357e63621`，未编辑；生产卡仍为前述 `e9caa82032374dc75f4a3c8697ef4c1d1d6bfdf4989a65a96bc733a1c62bbd6f`。

Root已报告本轮原类型检查实际exit0；UI原业务失败仍保留，不能据此声明本次fixture修正通过。本代理仍未执行build/test/UI/model。Root最小复验仍为 `node --test electron/tests/conversation-generation-handoff-ui.test.mjs`，不扩大回归或提高超时。

## 启动失败回执与当前任务目录的fixture增量前审

Root第二次实际UI结果保留于 `/tmp/autonomous-v3-current2-budget-ui-local2.log`：exit1、0/1，实际完成4次send与2次generate，第四场景在“Initial generation pre-task proof was treated as success or stayed busy”失败。DOM另有两次“回复归属不匹配”。前3场景已进入原断言，不把此次失败写成通过。

两处明确的fixture遗漏：

- `getTaskFinalReply` 返回裸 `{ state: "absent" }`，不含当前非空回执必需的task/project/thread/turn归属。`SavedTaskReply.tsx:20–23`正确拒绝；当前公开API允许无回执返回`null`，已验收的 `unfinished-output-ui.tsx:162` 使用该合同。仅将真正不存在的回执返回`null`，不伪造成功、文本或保存证明。
- 第四场景返回 `work-generation-not-started` 的可信暂停回执，但fixture任务查询仍只有第三场景两条已完成Work。`App.tsx:2119`按真实任务ID和当前root-thread显示错误，因此查不到该任务。Host生产路径先真实暂停并回读Work才返回回执（`electron/main.mjs:1220–1224`）；`work-resume-not-started.mjs`还要求该首Step保持queued且无Attempt。只在fixture为实际第四次原输入和sourceTaskId登记相同project/root的原作者Work，以及该原话派生的paused generation Work：queued首Step、currentAttemptId为null、attempts为空、精确originTaskId；回执直接引用该查询对象的ID/版本/归属。

责任仍为受影响Renderer合同的测试适配器，适用B04/B06/B08/B10；Native/Host是生产事实权威，fixture只提供它模拟的当前接口结果。生产App的归属校验、错误范围过滤、忙碌清理、许可消耗和草稿保留均不修改。模型组织、权限、额度、作者输入与供应商调用不受影响。

| 视角 | 增量实施前结论 | 必须保持 |
| --- | --- | --- |
| 架构 | APPROVE | 同步当前接口表示；不让Renderer凭回执越过实际任务查询或另造事实。 |
| 技术A | APPROVE | 暂停对象精确绑定原project/root/source；无Attempt、无新执行事件、无重放或许可。 |
| 技术B | APPROVE WITH CONDITIONS | 原第四场景必须显示未启动失败并结束busy；原4场景、预算说明、快照和草稿/选区断言全保留，Root唯一重验。 |
| 产品 | APPROVE | 真实暂停任务仍可理解失败；不能将未启动改为成功来通过测试。 |
| UI/UX | APPROVE | 不改UI或增加控件；无回执不再伪装成归属错误，原作者消息不因接力重复。 |

零兼容前审：PASS。当前公开合同一次性同步，无旧字段fallback、生产放宽或未知身份空成功。最小复验仍只为现有对话提案UI测试。

## 启动失败回执fixture增量后审与冻结

实际仅改owned fixture：无保存回执返回公开合同的`null`；第四轮提案使用实际send返回的proposal/sourceTaskId和已登记原作者turn，保存该原作者Work及同project/root的paused generation Work，再从同一个paused对象构造启动失败回执。首Step精确为queued、无Attempt；没有发布新workflow/turn事件、没有重放、没有追加作者输入或批准。

原“未启动必须显示失败且结束busy”、草稿DOM与选区、许可不可恢复、无自动重放和生成次数断言保持。另明确断言无生成Attempt、无错误的最终回复归属提示、两次实际创作要求分别只显示一条Work作者消息。原4场景快照未编辑，预算展示和批准前零生成断言保持。

| 视角 | 实施后静态复核 | 结论 |
| --- | --- | --- |
| 架构 | 只同步公开结果与目录合同；原生产事实、归属筛选和权限不改。 | PASS |
| 技术A | 暂停回执和查询来自同一对象；origin绑定实际第四次作者task/turn；queued无Attempt不伪称运行。 | PASS |
| 技术B | 全部原断言与快照保持，并补精确无Attempt/不重复作者/无伪归属错误断言；实际重验待Root。 | APPROVE WITH CONDITIONS |
| 产品 | 未启动仍失败，不能借无回执、空目录或成功话语掩盖原暂停事实。 | PASS |
| UI/UX | 生产UI未修改，已保存草稿/选区和原任务的失败可见性继续验收。 | PASS |

静态冻结：fixture SHA256 `d9329fca8098ab5578ff5b2a48d82fe6b684fc6cf3f5488d76815fb36b3712ce`；原快照 `electron/tests/conversation-generation-handoff-ui.test.mjs.snapshot` SHA256仍为 `6067384013c7d8c69919a8da5f950c3622099df952fcb6ff3e90c96357e63621`。生产卡SHA仍为 `e9caa82032374dc75f4a3c8697ef4c1d1d6bfdf4989a65a96bc733a1c62bbd6f`；外层test源未编辑，当前SHA为 `4be642f2ecad05a59231cd1e65ec75a2de5e026f903e1bd44043bf2a6b939d4e`。

零兼容后审：PASS。B04/B06/B08/B10责任边界无偏移。两次Root实际失败日志继续保留；本代理未执行build/test/UI/model，不能将静态复核当作实际通过。Root最小复验为 `node --test electron/tests/conversation-generation-handoff-ui.test.mjs`；不提高超时，不新增测试范围，不修改production/App。
