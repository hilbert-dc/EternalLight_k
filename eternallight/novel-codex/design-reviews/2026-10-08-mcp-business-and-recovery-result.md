# MCP 业务结果与精确恢复证据使用同一表示

日期：2026-10-08。五项为评审视角，不代表五位独立评审人员。

## 变更概览

实际 Core scoped 集成失败证据：`/tmp/autonomous-v3-current-source-fixtures-20261008.log`。写入已真实提交，但模型收到的 `save_draft_checkpoint` 结果只有 `recoveryOperation`，缺少 `saved/revisionId/checkpointId`。当前 Codex Core `protocol/src/models.rs` 优先采用非空 `structured_content`，这符合 MCP 模型结果语义；当前 Novel MCP 把业务结果放在 content，却用只有恢复证明的对象设置 structuredContent，导致正常业务结果与 Native 错误被替换。

目标：在 Novel MCP 工具调用的 HTTP 响应分支中，业务响应与精确恢复证明组合成一份实际结果；content 文本和 structuredContent 是同一 JSON。HTTP 非成功仍有 `isError=true` 和真实业务错误，不因存在恢复证明而假装保存成功。Root 与 Native 产生的既有 requestHash、callId、turnId、operationId、outcome、receiptRef 原样保留。

范围：`native/story-mcp/src/main.rs` 响应表示与当前 MCP RPC 合同测试；实际 Native+MCP 子稿测试及实际 Core scoped 原有强断言复验。不改 Codex Core 优先规则、模型判断、授权、读取、数据库与 Host。运行失败日志保留。

当前工具业务响应为 JSON 对象，在其顶层添加保留的 recoveryOperation；若协议传输为非对象 JSON，使用明确的 result 字段保存该原值并附同一恢复证明，使表示仍为合法 structuredContent 对象。这是一般 JSON 值表示，不识别旧版本、不读取旧字段、不迁移、不回退到已丢失的文本。

## 零兼容门禁

PASS：一个当前 wire 结果；没有版本判断、旧字段 fallback、迁移、回填、双读双写或降级。文本和结构化内容直接来自同一当前结果。

## 实施前五视角评审

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | Novel MCP 自己完整表示当前业务结果；Core、Engine 的当前优先语义不改。恢复证明属于结果附加事实而不能替代业务数据。 |
| 技术 A | APPROVE WITH CONDITIONS | 全部 proof 字段保持原哈希与绑定；非 2xx 仍报错，未知/未派发结果不制造 committed。 |
| 技术 B | APPROVE WITH CONDITIONS | 通过实际 handle_message+HTTP 测试证明成功 checkpoint、非成功状态与既有 schema feedback；实际 Core 必须读到业务 revisionId，保留原强断言。 |
| 产品 | APPROVE WITH CONDITIONS | 模型能区分实际保存、普通步骤完成和最终交付拒绝；不会因审计元数据误报完成。 |
| UI/UX | APPROVE WITH CONDITIONS | 不新增常驻界面或技术提示；现有成果投影继续读实际业务字段，内部恢复证明供详情及排障。 |

综合批准上述最小生产增量，无兼容例外。实际 DeepSeek 文学质量、真实应用新作品生成仍另验。

### 当前执行器工具清单一致性（实施前增量评审）

MCP 包首轮实际结果 88/89；新增真实 RPC 测试通过，唯一失败 `bundled_names_match_every_compiled_tool_once` 为 bundled 45 名与当前 Native 53 工具不一致。当前 `multimedia_tools::specs()` 已有 8 个真实媒介工具。本增量直接同步执行器可信名字资产为当前 Native 工具；保留精确集合、唯一性及 64 工具上界断言，不改变执行身份、每操作工具范围或 Native 权限。

五视角均 APPROVE WITH CONDITIONS：架构要求可信目录与当前实现唯一一致；技术 A 要求仅注册实际编译的工具名、不将名称清单视作执行授权；技术 B 要求原一一一致测试继续生效并重测；产品要求模型能发现当前实际能力而非残缺名称；UI/UX 不增加界面内容。零兼容 PASS，无旧名单兼容分支。由 Root 明确分配本增量。

### 派发后字符串错误诊断（实施前增量评审）

只读审计发现同一根因仍存在于 Rust Err 分支：若 Runtime 已派发但响应格式错误或回执校验失败，精确 ledger 可以返回 committed/unknown proof，而 Core 所取 structuredContent 只剩 proof，原错误诊断被隐藏。Root 明确批准本增量。只有没有结构化 schema feedback 时，在当前结构化结果保留 `error.message` 为原诊断；已有 schema feedback 字段保持原结构，附 proof 后文本与结构化结果一致。全部结果仍 isError，不据诊断猜测 ledger 状态。

五视角均 APPROVE WITH CONDITIONS：架构坚持诊断与 ledger 是两个并存事实；技术 A 保留精确 unknown/committed 的原来源、不重试或扩权；技术 B 要求实际 RPC malformed-response 与 unknown ledger 负例；产品要求模型知晓响应失败且真实结果未知；UI/UX 保持详情中的真实可读诊断，不把错误显示为完成。零兼容 PASS。

当前 schema 测试 helper 同步：原先只从 structuredContent 拆 proof，假设 text 只有 feedback；在当前同一表示下先强断言完整 text/structured 等值，再验证既有 beforeDispatch proof，最后拆出原反馈核对全部原字段及原 1600-byte 反馈预算。没有删除 proof 断言、抬高原反馈上界或放松未派发/精确 CAS 断言。五视角沿上述条件批准本测试表示同步；实际 malformed/unknown 与完整反馈测试仍必需通过。

## 实施后复核

当前最小生产增量已实现：HTTP 业务 JSON 与原样恢复证明组合为同一结果，content 文本与 structuredContent 一致；非 2xx 保留真实错误与 isError。新增真实 handle_message+本机 HTTP 用例覆盖成功 checkpoint、实际完成回执、Native 拒绝及 accepted/unknown ledger，核对所有精确哈希/调用绑定字段和文本/结构化等值。非对象 JSON 原值以 result 表示并测试。

首轮 `/tmp/autonomous-v3-mcp-complete-business-result-20261008.log` 为 88/89，新合同测试通过、工具名字资产不一致失败，原失败保留。补齐当前 8 个实际媒介工具后，`/tmp/autonomous-v3-mcp-business-result-current-tools-20261008.log` 为 89/89 通过，包含原精确集合和唯一性测试；当前工具 53 个。`/tmp/autonomous-v3-mcp-business-result-bin-20261008.log` 首轮生产二进制构建 exit0。

上述 bin 的实际 Core scoped 已 1/1 exit0：`/tmp/autonomous-v3-mcp-current-core-scoped-20261008.log`，业务测试 97.09 秒。保留原强业务断言，覆盖精确 author scope 修订、受保护正文、当前保存回执、独立评估、建模、GenerationRun 续章和所有历史答复读取。末轮字符串诊断增量已经 Root 批准并独立评审；实际 RPC 同时验证 malformed JSON 诊断与 accepted/unknown、明确 committed ledger，以及完整 schema-before-dispatch feedback。

字符串诊断增量首轮 `/tmp/autonomous-v3-mcp-business-and-string-diagnosis-20261008.log` 为 87/91：新增两项通过，四个 schema 用例共用 helper 的旧 text/structured 处理方式失败，原反馈字段未丢失。同步 helper 为完整 text=structured 等值、验证原 proof、拆出原 feedback 的原 1600-byte 上界后，`/tmp/autonomous-v3-mcp-current-complete-result-final-20261008.log` 为 91/91 通过。最终 bin 构建 `/tmp/autonomous-v3-mcp-current-complete-result-bin-final-20261008.log` exit0（24.16 秒）；已通知 Root 使用当前 `native/target/debug/novel-workspace-mcp`。此前实际 Core 成功路径证据保留；最终统一链由 Root 执行。

| 复核视角 | 结论 | 证据/限制 |
| --- | --- | --- |
| 架构 | APPROVE | 一个当前结果表示；未修改 Core 优先语义、领域保存/交付门禁或数据库。 |
| 技术 A | APPROVE | 所有 proof 字段保持原来源，Native rejected 与未知 ledger 均不能变成成功；当前工具名不扩张执行权限。 |
| 技术 B | APPROVE | 最终包 91/91，当前生产二进制构建通过；真实 RPC 结果/错误/精确 ledger/反馈长度与原业务 CAS、实际 Core 成功路径通过。最终统一链由 Root 独立验收。 |
| 产品 | APPROVE WITH CONDITIONS | 返回实际保存与拒绝信息；机制修复不能被当作真实 DeepSeek 文学质量或任务全部完成。 |
| UI/UX | APPROVE | 不增加常驻技术界面，现有模型答复和成果投影使用实际业务结果。 |

零兼容复核 PASS。上层 Native/Host 自主闭环和真实模型任务分别验收，不由本修复制造成功。
