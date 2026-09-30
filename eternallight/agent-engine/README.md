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

## Rust 构建前置

四协议编解码、流状态机、预算及持久化、会话编译、证据与认证规则、供应商 HTTP、App Server 进程通信、行为评测调度与恢复日志状态由独立 Rust Node-API 库提供。先运行 `npm run build:native`；
`npm test` 和委派集成测试会自动构建它。Rust 库测试运行 `just test`。本地 Bridge、worker IPC 和 Host 上层编排仍在迁移中。
没有原生二进制时请求明确失败，不回退 JavaScript。完整迁移进度见 [RUST_MIGRATION.md](RUST_MIGRATION.md)。
