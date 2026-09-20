# DeepSeek 工具协议修复设计评审

版本：0.2 · 2026-09-19 · 状态：实施后复核通过

## 变更概览

- 目标：让 DeepSeek 通过标准 Responses function calls 直接调用 `novel_workspace` MCP 工具；禁止 Codex Code Mode/DSML 与多 Agent 指令进入 Novel Agent Engine；协议模板泄漏时失败关闭而不是显示为正常回复。
- 非目标：解析或执行模型输出的 DSML 文本；恢复 shell、Node REPL、文件、命令或网络工具；修改 Story 数据 schema；保留错误的 Code Mode 路径。
- 影响模块：私有模型目录、目录摘要、App Server adapter、fixture、Agent 测试、运行文档。
- 当前版本验收标准：模型目录固定 `tool_mode=direct`、`multi_agent_version=disabled`、`node_repl_disabled=true`；DSML 泄漏不进入 Renderer 且本轮返回协议错误；真实咨询任务产生结构化 `novel_workspace` 工具事件和可读答案。

## 测试阶段零兼容门禁

- [x] 直接删除错误的 Code Mode/多 Agent 配置，不保留双轨或 fallback。
- [x] 不解析、猜测或执行文本形式的工具调用。
- [x] 同步更新目录摘要、adapter、fixture、测试与现行文档。
- [x] 当前版本的失败恢复、安全、跨平台配置和 UI 错误状态有明确验证。
- [x] 无兼容例外。

## 角色评审

### 架构专家

- 结论：`APPROVE WITH CONDITIONS`
- 模块边界与依赖方向：模型只看到 App Server 直接暴露的 `novel_workspace` MCP 工具；不允许通过通用 `exec` 间接分派，也不引入 Codex 多 Agent 层。
- 条件：模型目录同时关闭 Code Mode、Node REPL 和多 Agent，避免仅修其中一项后继续注入内部协议。
- 是否存在无授权兼容层：否。

### 技术专家 A

- 结论：`APPROVE WITH CONDITIONS`
- 可靠性、安全、并发与性能：DSML 是不可信模型文本，禁止在客户端解释或执行；adapter 必须抑制泄漏事件、终止成功语义并返回可诊断错误。
- 条件：覆盖 completed item 与 turn payload 的泄漏路径，保留正常文本流行为。
- 是否把当前版本可靠性误写成历史兼容：否。

### 技术专家 B

- 结论：`APPROVE WITH CONDITIONS`
- 实现、测试与一次性切换完整性：目录应使用协议枚举支持的 `direct`/`disabled`；更新 SHA-256 固定值；fixture 增加真实泄漏形状。
- 条件：增加目录深度相等断言和 adapter 集成回归测试。
- 是否遗留双轨、fallback 或半迁移状态：否。

### 产品经理

- 结论：`APPROVE`
- 用户价值与范围：咨询必须真正读取作品上下文并回答；协议失败不能伪装成“本轮完成”。
- 是否把旧行为保护误列为产品需求：否。

### UI/UX 设计师

- 结论：`APPROVE WITH CONDITIONS`
- 流程、状态、错误与可访问性：隐藏 DSML 原文，显示“模型未按结构化工具协议返回，请重试或检查模型配置”的错误；保留失败轨迹而非成功徽标。
- 条件：Renderer 只接收净化后的 `agent/error`，不得收到含 DSML 的 agent message 或 turn payload。
- 是否保留无价值的旧交互双轨：否。

## 综合决策

- 最终结论：`APPROVE WITH CONDITIONS`
- P0/P1 阻断项：direct function calls、禁用多 Agent/Code Mode、DSML 失败关闭和回归测试必须同批落地。
- 获得明确授权的兼容例外：无。
- 实施范围：模型目录、adapter 协议守卫、目录哈希、fixture/测试、运行文档和真实端到端复测。

## 实施后复核

- 实际变更：DeepSeek 改用标准 Responses 请求（`use_responses_lite=false`）与 direct 工具模式；关闭 Code Mode、Node REPL、多 Agent、Apps 与 Plugins 指令；`novel_workspace` 作为受能力凭证、任务操作和 Runtime 校验约束的本地候选工具，被明确配置为 `default_tools_approval_mode="approve"`；adapter 对 DSML 文本失败关闭。
- 与方案的偏差：实施时发现仅切换 direct 不足。Responses Lite 会把工具目录放入 OpenAI 专用的开发者消息，DeepSeek 无法据此形成函数调用；切换标准 Responses 后又发现全局 `approvalPolicy="never"` 会拒绝默认需审批的 MCP 调用。因此补充了标准 Responses 和本地 MCP 明确预批准两项配置。命令、文件、网络权限仍保持拒绝。
- 构建与测试证据：`npm run build` 通过；Agent 测试覆盖目录策略、配置生成、DSML 泄漏和正常 App Server 回合；真实 Electron 回合成功产生 `tool_search_call`、带 `namespace=mcp__novel_workspace` 的 `function_call(read_context)`、完成态 `McpToolCall`，并基于返回上下文生成可读答案，界面显示“本轮完成”。
- 零兼容门禁复查：未保留 Responses Lite、Code Mode 或 DSML 解析 fallback；工具失败不会伪装成成功；审批豁免只作用于能力受限的 `novel_workspace` MCP，不扩展到 shell、文件或网络。
- 剩余项：无 P0/P1 阻断项。后续可单独优化回答长度与冻结上下文覆盖度，不属于本协议故障。
