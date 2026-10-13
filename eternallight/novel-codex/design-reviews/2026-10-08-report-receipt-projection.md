# 章节检查报告保存回执的界面投影

## 变更与责任（实施前）

实机样本 3 的 `submit_revision_outcome` 调用 `call_00_LuxcfDW98LIdvflMmZ9S5897` 返回报告 `revision-outcome-0faa62688dd3465e899d960ffeb8c7cb`、`status:candidate`、实际 `writerTaskId`，以及同调用的 `runtimeLedger` / `committed` / `runtime:candidate:<revisionId>` 回执。Native 持久 operation 的 call、报告、requestHash、receiptRef 一致。技术证据为 `/tmp/autonomous-v3-current5-sample3-terminal-observation.json` 与该 critic 会话第 157、160 行的原调用/结果；检查只抽取技术字段，不复制正文、引文、系统提示或凭据。

Renderer 将该工具分类为 write，但 `operation-presentation.ts` 缺少此报告保存结果映射，因此即使真实回执完整，也会由既有摘要及展开项显示“保存结果待核实”。本次只修正 Renderer 纯状态投影与必要的同调用增量身份保留，不修改 Native、MCP、Engine、Codex Core、Host、业务完成条件或布局。

责任规则为 B03、B04、B08、B10：报告保存与报告检查通过、作者采用、最终交付分开；回执事实唯一权威仍为 Native。Renderer 消费真实 MCP 结果，不能从模型回复、工具名或 completed 状态推断成功。权限、来源、预算和任务版本已由 Native 校验，本次不复制第二套鉴权或在浏览器重算 Native requestHash。

公开合同为当前 `mcpToolCall` 的 server、item id、tool、arguments、result，以及 `submit_revision_outcome` 已有返回字段和 `recoveryOperation`。保存投影要求实际 `novel_workspace` MCP 调用、同 item id / proof callId、对应 toolName、runtimeLedger committed、有效 operationId、candidate 状态、writerTaskId 与该调用参数一致、receiptRef 与返回 revisionId 精确匹配。所有条件缺失或不一致时维持既有待核实/失败状态。

当前增量 completion 可以省略 arguments。仅为此报告提交保留同调用的 `reportWriterTaskId`，完成项在相同 id、相同工具且参数确实省略时复用；参数显式替换、不同工具或不同调用不能借用。该字段是事件投影缓存，不是新领域事实，不保存正文、权限或请求参数。现有摘要、展开项和显示快照消费同一 outcome，无新 UI。

模型仍自主决定是否检查、如何组织评估与创作；本次不增加固定流水线、重试、确认或创作指令。accepted、rejected、错误、取消及未知响应不会升级为已保存，幂等重复事件不会创造新作品版本。

## 零兼容与五视角前审

| 视角 | 结论 | 条件与边界 |
| --- | --- | --- |
| 架构专家 | APPROVE | 修复归 Renderer 真实事实投影；Native 权限和完成权威不变，不新增跨层依赖。 |
| 技术专家 A | APPROVE WITH CONDITIONS | 精确匹配同调用、writer 和报告回执；缺失、错误及 accepted/rejected 保持未证实；不同调用不得继承身份。 |
| 技术专家 B | APPROVE WITH CONDITIONS | 覆盖完整结果、同调用省略参数、显式替换、不同 call/tool、错误、重复事件及快照；Root 唯一运行验证。 |
| 产品经理 | APPROVE | 消除真实已保存报告仍显示待核实的困惑；不把报告保存或 supported 误写成正文采用/发布/交付。 |
| UI/UX 设计师 | APPROVE | 使用既有“已保存章节检查结果”状态，无新面板/按钮/技术字段；未知和失败仍清晰保留。 |

综合结论：按上述条件实施。无兼容例外、迁移、回填、版本判断或双轨；增量合并针对当前同调用事件，不能恢复旧版本或跨调用身份。

## 最小验证及证据边界

Root 执行 Renderer 定向合同、类型检查与 `architecture:check`；本子任务不构建、不测试、不操作应用或模型。合同以当前生产结果字段、真实 MCP 事件形状与真实回执为准，覆盖 saved / unknown / failed 及同调用合并。已有真实模型样本证明报告保存发生；静态和受控事件测试不能独立证明文学质量、完整交付或多章可靠性。修复后由 Root 在实际应用检查该状态呈现，不另发创作指令。

## 实施后复核

实际改动仅为 `src/operation-presentation.ts`、`src/agent-events.ts`、现有 `electron/tests/conversation-value.test.mjs` 的一个新增合同用例。原工具结果映射、摘要与展开项复用既有 saved outcome，不修改 UI 组件。新增测试放入现有 test:ui 已包含的文件，没有新增未接入门禁的测试入口或 package.json 修改。

同调用增量合并仅保存有效 writerTaskId；显式 null/空对象/替换参数不复用，不同 item id 或不同工具不继承。保存判断限定 novel_workspace 的 mcpToolCall，核对当前业务报告与 Native 同调用 committed proof。Native 身份、来源、版本、预算、保存与完成权威不变；Renderer 不解释报告文学结论、不重算请求摘要、不调用额外 API。

| 实施后视角 | 静态结论 | 依据及待验收边界 |
| --- | --- | --- |
| 架构专家 | PASS（待 Root 实际验证） | 仅 Renderer 纯投影与事件缓存；没有跨层依赖、模型流程或第二事实权威。 |
| 技术专家 A | PASS（待 Root 实际验证） | 保存依赖真实候选报告和精确同 call/tool/writer/report 回执；缺失、错配、accepted/rejected、错误、取消仍不能 saved。 |
| 技术专家 B | PASS（待 Root 实际验证） | 定向合同覆盖直接结构化/文本结果、增量参数省略与显式替换、跨 call/tool、重复完成、快照及私有正文不留存；接入原 test:ui。 |
| 产品经理 | PASS（待 Root 实际验证） | 只修“已保存检查结果”；不提升检查结论、作者采用/发布或领域最终交付。 |
| UI/UX 设计师 | PASS（待 Root 实际验证） | 复用原摘要、展开及历史呈现，未知与失败保留；无额外 UI 或作者技术参数。 |

独立只读子代理静态复核结论 PASS，未发现必须修正问题；未修改文件、运行构建/测试、操作应用或模型。零兼容门禁静态 PASS：无迁移、回填、旧字段 fallback、双轨或版本判断。只含旧 outcome 摘要而无原回执的 DisplayExecution 不被此改动反向认定成功；实际历史重建必须消费真实工具结果。

Root 实际执行：`node --test electron/tests/conversation-value.test.mjs electron/tests/agent-events.test.mjs` 32/32、exit0（`/tmp/autonomous-v3-current6-report-receipt-renderer-tests.log`）；`npx tsc --noEmit` exit0（`/tmp/autonomous-v3-current6-renderer-types.log`）；架构检查332文件/7crate/0诊断、exit0（`/tmp/autonomous-v3-current6-report-receipt-architecture.log`）。

实际应用观察：HMR后的旧过程摘要仍显示“保存结果待核实”；刷新后当前章保持839字符、“当前稿检查通过”和可打开的已保存正文，历史过程已收起且未提供原报告回执。没有以Work completed反向填充旧摘要，也没有重发报告。该观察证明旧资料未被冒认成功，尚不构成新回执在实际界面显示saved的证据；下一次应用自产报告须现场验证。纯投影合同、类型与边界验证通过，真实新报告展示验收仍待完成。
