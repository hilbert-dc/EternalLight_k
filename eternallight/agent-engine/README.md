# Eternal Light Agent Engine

面向多个产品的 Codex 引擎接入层，负责协议适配、执行准入、路由、预算、隔离、恢复与诊断。Agent 循环及原生子 Agent 调度由 Codex 内核负责。

目标与验收基线：[多模型 Codex 引擎理想架构](IDEAL_ENGINE_ARCHITECTURE.md)。模型原语、引擎执行与产品功能分别定义，当前优先建设底层引擎。

已落地的基础边界：

- 模型原语与 Codex 引擎能力分别声明，产品操作不进入模型目录。
- SessionPlan 只采用有效证据，并冻结证据引用、分类依赖摘要、路由及产品扩展授权；配置编译阶段不能追加权限。
- Responses、Chat Completions、Messages、Generate Content 通过统一 Codec 接入，Bridge 执行预算、租约、取消与认证撤销。
- 通用 Engine Host、App Server 客户端、资格 runner/MCP、版本化行为任务执行器、证据/报告存储、恢复日志与有向路由位于本包。
- `startRun` 可同时指定相对 `maxDurationMs` 和绝对 `deadlineAtMs`；Rust 签发租约时取两者及 SessionPlan 到期时间的最早值，跨进程启动不能延长上层许可。
- 行为评测固定任务集并明确 `authorizationEffect: none`，不会反向授予执行能力。
- 测试证据仅从 `@eternallight/agent-engine/test-support` 导出，生产入口不能构造基线通过证据。

行为评测输入显式使用 `mode: "execute"` 或 `mode: "samples"`：

```bash
npm run evaluate:model -- evaluation-input.json report.json
```

真实供应商与平台验收仍以生成的认证、行为和构建报告为准；代码测试通过不等于某个外部模型已经通过认证。

## 模型切换与指令角色

适配由连接的协议合同选择，不按供应商名字分支。Responses 连接可显式设置 `protocolOptions.developerRoleMode`：

| 值 | 出站处理 |
| --- | --- |
| `native`（未设置时亦如此） | 保留原 `developer` 角色；接入方须核实供应商实际支持其指令语义。 |
| `system` | 原 `developer` 文本在原位置映射为 `system`。 |
| `instructions` | 仅将对话前连续的 system/developer 文本按原顺序和角色标签加入 `instructions`；历史中后插入的 developer 在原位置映射 system。 |
| `unsupported` | 有 developer 消息的请求明确拒绝。 |

此选项只适用于 Responses；其他协议由各自 Codec 处理并验证角色合同。角色映射不修改原请求或历史，不提升作者、引用、工具结果的权限，无法表示的非文本指令明确拒绝。`system`/`instructions` 不能保留独立的 system 与 developer 优先级，不能宣称严格等价。

适配配置参与连接及环境指纹；切换使用目标连接的有效证据，不能复用其他配置的认证。语言偏好、作者要求属于产品对话上下文，不能因换模型重置；原任务身份、权限、保存回执和累计用量仍由各自权威管理。私有推理状态和不同协议间的历史可移植性需单独核验，本项角色适配不证明任意模型间可以无损热切换。

回归入口：`tests/developer-role-portability.test.mjs` 验证切换、不变性与拒绝边界；产品 `electron/tests/author-language.integration.mjs` 验证真实 Core/Bridge 的新回合、恢复和追加要求。`novel-codex/scripts/probe-live-language.mjs --confirm-cost --connection=<目录连接ID>` 用相同六场景检查目标模型真实公开文本，结果须独立审阅，不以请求成功或规则已传入判定语言合格。

## Rust 构建前置

四协议编解码、流状态机、预算及持久化、会话编译、证据与认证规则、供应商 HTTP、App Server 进程通信、行为评测调度与恢复日志状态由独立 Rust Node-API 库提供。先运行 `npm run build:native`；
`npm test` 和委派集成测试会自动构建它。Rust 库测试运行 `just test`。本地 Bridge、worker IPC 和 Host 上层编排仍在迁移中。
没有原生二进制时请求明确失败，不回退 JavaScript。完整迁移进度见 [RUST_MIGRATION.md](RUST_MIGRATION.md)。

### Trusted per-request reasoning selection

A Responses connection may declare `protocolOptions.reasoningEffort` (default)
and `allowedReasoningEfforts` (nonempty set containing the default). The set and
default belong to the frozen connection definition and qualification fingerprint.
An individual selection inside that set does not change the SessionPlan.

Both `ModelBridge` and `ModelBridgeProcess` accept a trusted Host callback:

```js
requestConfigurationProvider: ({ connectionId, runId, threadId }) =>
  selectionFor(connectionId) // null, or { reasoningEffort: "high" }
```

The callback is evaluated once per incoming model request, after route/identity
admission and before provider dispatch or budget reservation. It can only select
an effort inside that route's declared set; extra fields, out-of-range choices,
and callback failures reject that request. It cannot change the model, endpoint,
author input, tool permissions, lease, or budget. Return null for routes without
a mutable reasoning policy. Request observations include the encoded
`reasoningEffort` for verification without recording private input.

Changes affect subsequent requests, including permitted child requests using the
same connection. Already dispatched requests retain their snapshot. The Host
owns persistence and user choice; Engine owns wire validation and accounting.

### Input capacity diagnostics

Local complete-input admission failure returns HTTP 400 with
`error.code = "context_input_budget_exceeded"`, `error.retryable = false`, and
`error.contextCapacity`. The response observer receives the same
`contextCapacity`, correlated with the original run and request even when the
encoder rejects before dispatch:

```json
{"boundary":"route","estimatedInputTokens":11000,"inputLimitTokens":10000,"excessTokens":1000}
```

`boundary` is `connection` (declared context minus output allowance) or `route`
(the resolved input allowance including policy reserves). Counts use the
Engine's conservative estimator, not provider billing. Rejection happens before
provider dispatch and budget reservation. A smaller request must pass admission
again under the same execution and budget; the error does not authorize an
unchanged retry, a new task, or extra allowance. Diagnostics contain no input,
credentials, or provider-supplied prose. Core owns compaction; this error alone
does not promise automatic resumption of a failed application task.

Item admission separately protects buffering with a 256 KiB serialized UTF-8
byte bound. It does not impose a second fixed token window per message,
reasoning item, tool argument, or tool result. Bounded high-token items retain
their full text and opaque state through output and subsequent input; the
complete encoded request must still fit the connection and route allowance.
Oversized output items return `provider_output_item_limit` before executable
output is released, with byte-only size diagnostics and normal usage settlement.
The independent tool-parameter schema compiler bound remains 40,000 bytes;
its shape, depth, pattern, and argument checks remain in force.

### Portable discovery lifetime

Loaded tool identities belong to the trusted caller session, not an individual execution lease. A new lease clears pending discovery receipts and execution counters. Every request selects schemas from its current policy-admitted catalog: unchanged tool declarations remain loaded, removed or changed declarations lose activation, and discovery policy changes invalidate the selection. Revoked tools do not regain activation merely by reappearing. Threads and sessions remain isolated; process restart and explicit reset require discovery again. Historical tool output never supplies authorization or schemas. Embedders should expose essential state readers as initial tools within the current admitted catalog.
