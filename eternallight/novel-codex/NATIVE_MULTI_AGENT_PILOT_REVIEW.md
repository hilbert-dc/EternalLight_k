# Codex 原生多 Agent 试点设计评审

版本：0.1 · 2026-09-19 · 状态：设计评审中

## 变更概览

- 目标：在不引入自研调度器或通用模型网关的前提下，验证 DeepSeek 能否通过标准 Responses function calls 使用 Codex 原生 Multi-Agent V2；根 Agent 保持唯一写入权，子 Agent 只读分析同一冻结作品快照。
- 非目标：允许子 Agent 修改 Canon 或候选稿；实现跨供应商统一网关；重写 Codex 调度；保留当前禁用多 Agent 的旧配置分支。
- 影响模块：Codex 模型目录、隔离运行配置、隔离 Agent 角色、发布资源暂存、Agent 事件时间线、fixture 与自动化测试。
- 当前版本的验收标准：主 Agent 能真实产生 `collabAgentToolCall`，至少启动两个子 Agent、等待结果并汇总；子 Agent 只能调用 `novel_workspace.read_context`；取消根任务会结束整个进程树；UI 能区分委派、等待、通信、完成和失败；单 Agent 生成与咨询链路不回归。

## 目标架构

```text
Renderer
  -> Electron task lifecycle
    -> Codex App Server / Core
      -> root agent (唯一候选稿写入者)
      -> read-only subagents (Codex 原生调度)
        -> novel_workspace.read_context
    -> Creative Runtime (任务、版本、能力凭证、候选稿权威)
```

- 使用当前标准 Responses 接口和 `tool_mode=direct`。
- 模型目录声明 `multi_agent_version=v2`；隔离配置显式启用 V2，并限制每个会话最多三个并发子 Agent。
- Novel Codex 自己管理的角色文件复制到独立 `CODEX_HOME/agents`，不读取 Codex Desktop 或 Codex CLI 的个人角色。
- `default`、`worker`、`explorer` 及小说领域角色全部设置 `sandbox_mode="read-only"`，并将 `novel_workspace` 限制为 `read_context`，确保省略或误选角色时仍无写权限。
- 根 Agent 保持现有 operation 对应的 MCP 工具范围。子 Agent 共享根任务的短期能力凭证，但只能读取该任务已冻结的上下文；本阶段不引入子 Agent 独立写任务。
- App Server 的 `collabAgentToolCall` 直接进入 Renderer 时间线，不解析模型文本中的 DSML，也不伪造子 Agent 状态。

## 测试阶段零兼容门禁

- [x] 未新增版本判断、旧字段 fallback、双读双写、升级迁移、回填、降级或 deprecated 路径。
- [x] 未因为假设中的旧调用方、旧数据或旧行为降低目标架构质量。
- [x] 配置变化会一次性同步生产代码、fixture、测试、发布资源和现行文档。
- [x] 不涉及数据库 schema 或历史测试数据迁移。
- [x] 当前版本的数据一致性、故障恢复、安全、并发与 macOS/Windows 路径有明确验证。
- [x] 无兼容代码或兼容例外。

## 待评审问题

1. V2 plain function tools 在 DeepSeek Responses 上是否能真实完成 spawn / wait；若不能，本阶段必须以明确协议错误失败，不能降级到文本解析。
2. 子 Agent 继承根能力凭证是否在只读工具白名单下足够安全。
3. App Server 终止时是否能可靠回收子 Agent，且 UI 不会把子线程 idle 误判为根轮次完成。
4. UI 是否只显示对作者有意义的委派进度，避免重新退化为原始事件日志。

## 角色评审

### 架构专家

- 结论：待评审
- 模块边界与依赖方向：待评审
- 是否存在无授权兼容层：待评审

### 技术专家 A

- 结论：待评审
- 可靠性、安全、并发与性能：待评审
- 是否把当前版本可靠性误写成历史兼容：待评审

### 技术专家 B

- 结论：待评审
- 实现、测试与一次性切换完整性：待评审
- 是否遗留双轨、fallback 或半迁移状态：待评审

### 产品经理

- 结论：待评审
- 用户价值与范围：待评审
- 是否把旧行为保护误列为产品需求：待评审

### UI/UX 设计师

- 结论：待评审
- 流程、状态、错误与可访问性：待评审
- 是否保留无价值的旧交互双轨：待评审

## 综合决策

- 最终结论：待五角色评审
- P0/P1 阻断项：真实结构化协作调用、只读子 Agent 边界、任务取消回收、根/子事件隔离。
- 获得明确授权的兼容例外：无。
- 实施范围：评审通过后按本文件“目标架构”一次性落地。

## 实施后复核

- 实际变更：待实施。
- 与方案的偏差：待实施。
- 构建与测试证据：待实施。
- 零兼容门禁复查：待实施。
- 剩余项：待实施。
