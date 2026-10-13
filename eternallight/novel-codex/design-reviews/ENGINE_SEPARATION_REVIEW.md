# Novel Agent Engine 独立化设计评审

版本：0.1 · 2026-09-19 · 状态：实施前门禁通过，实施后待复核

## 变更概览

- 目标：将面向产品的 Agent 运行层一次性切换为 `Novel Agent Engine`。底层可以复用固定版本、固定摘要的 Codex App Server 协议内核，但不得读取或依赖 ChatGPT/Codex 桌面应用的认证、配置、会话、审批 UI 或订阅额度。
- 模型边界：当前只允许 Novel Codex 自有的 DeepSeek provider/key；用量由模型提供商独立计费和限流，不能也不会绕过提供商额度。
- 权限边界：所有领域 turn 使用空执行环境和 `approvalPolicy=never`。command/file/network 等软件权限请求由主进程引擎立即拒绝，不进入 Renderer；只保留 `requestUserInput` 作为作者与模型的结构化问答。
- 非目标：重写 Codex Agent 内核、绕过模型服务商认证或配额、让模型获得 shell/文件/网络权限、保留旧 IPC/mode/UI 兼容层。
- 影响模块：App Server adapter、Electron Main/Preload、Renderer 类型与对话 UI、事件 reducer、测试、架构与运行时文档。
- 当前版本验收：不存在产品侧权限批准按钮；软件权限请求自动拒绝；作者问题仍可回答；状态只显示 Novel Agent Engine；环境不含 Codex/ChatGPT 凭据；构建和 Agent 测试通过。

## 测试阶段零兼容门禁

- [x] 不保留旧 `codex-app-server` mode、审批 IPC 或权限审批 UI。
- [x] 一次性同步 adapter、IPC、类型、fixture、测试与文档。
- [x] 不新增版本判断、fallback、双轨、迁移或 deprecated 路径。
- [x] 保持密钥隔离、IPC sender 校验、项目/线程/TaskRun 绑定和跨平台路径处理。

## 角色评审

### 架构专家

- 结论：`APPROVE WITH CONDITIONS`
- 模块边界：产品 API 只能暴露 Novel Agent Engine；Codex App Server 是私有协议内核，不能成为认证、额度或审批的所有者。
- 条件：软件权限请求必须在主进程边界终止，不能先发送给 Renderer 再自动点击。
- 是否存在无授权兼容层：否。

### 技术专家 A

- 结论：`APPROVE WITH CONDITIONS`
- 可靠性与安全：自动拒绝必须回写 JSON-RPC 结果，避免 turn 永久挂起；`requestUserInput` 必须继续验证问题和回答长度。
- 条件：增加自动拒绝且 turn 正常完成的集成测试。

### 技术专家 B

- 结论：`APPROVE WITH CONDITIONS`
- 实现与测试：删除 approval IPC/type/UI，统一新 mode；保留固定 CLI 摘要和隔离环境测试。
- 条件：禁止用 Codex/ChatGPT token 作为 provider fallback。

### 产品经理

- 结论：`APPROVE`
- 用户价值：用户只处理创作问题，不再理解底层软件权限；模型配置与用量明确属于 Novel Codex 自选 provider。
- 范围：不承诺“无限额度”，只承诺“不消费 Codex/ChatGPT 软件额度”。

### UI/UX 设计师

- 结论：`APPROVE WITH CONDITIONS`
- 流程：保留作者问题卡片，删除命令、文件和网络授权卡片；状态文案改为 Novel Agent Engine。
- 条件：被引擎拒绝的软件权限仅作为折叠诊断事件，不打断创作对话。

## 综合决策

- 最终结论：`APPROVE WITH CONDITIONS`
- P0/P1 阻断项：上述条件必须在同一变更中完成。
- 兼容例外：无。
- 实施范围：一次性替换 mode/IPC/UI，主进程自动拒绝软件权限，保留结构化作者问答和 provider 用量统计。

## 实施后复核

- 实际变更：产品 mode 已切换为 `novel-agent-engine`；主进程拦截所有 `requestApproval` 并回写 `decline`；Renderer 只接收 `requestUserInput`；旧 approval IPC、类型和可操作授权卡片已删除；架构与运行时文档已同步。
- 与方案偏差：无功能偏差。私有实现仍复用固定版本 App Server 协议与内部 `CodexAppServerAdapter` 类名，但不作为产品 API、认证、额度或审批边界。
- 构建与测试证据：`npm run build` 通过；`npm run test:agent` 69/69 通过；`npm run test:ui` 3/3 通过。新增集成测试证明软件权限请求不会进入用户问答回调、会产生折叠诊断事件并且 turn 可正常结束。
- 零兼容门禁复查：旧 `codex-app-server` mode、`agent:approval-request`、`agent:resolve-approval`、`onApprovalRequest`、`resolveApproval` 均已从活动 Electron/Renderer 边界删除；未增加兼容别名或 fallback。
- 剩余项：无阻断项。模型服务商 DeepSeek 自身的认证、余额、计费和限流仍然生效，这属于明确产品边界而非缺陷。
