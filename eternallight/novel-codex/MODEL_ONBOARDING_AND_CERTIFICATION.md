# 模型接入、引擎准入与行为评测

状态：目标机制实施中，2026-09-20。技术基线见 [多模型 Codex 引擎基础层技术方案](../agent-engine/ENGINE_FOUNDATION_TECHNICAL_DESIGN.md)。

## 能力边界

模型目录只描述连接、协议选项、模型原语和限制。文本、结构化工具调用、流式终态及 usage 属于模型能力；Codex 根工具循环、子任务生命周期、取消恢复及有向路由属于引擎能力；小说生成和固定多席评审属于产品能力。

模型声明不能授予引擎能力。产品操作不写入模型目录，Novel Codex 在 `electron/operation-requirements.mjs` 统一声明当前操作所需能力。

## 接入流水线

1. 在 `model-catalog.json` 登记连接、明确协议、协议选项、模型原语支持方式和 token 限制。
2. 用协议 fixture 验证 Codec 的结构化调用、callId、namespace、流终态、错误及状态保真。
3. 执行 `npm run qualify:model -- <connection-id> --profile <profile>`。基础工具画像必须分别通过模型结构化工具原语和真实 Codex 根工具回环；委派画像另外逐项验证子任务能力与精确有向路由。
4. 每项能力独立形成证据；基础通过、委派失败、恢复未运行可以同时存在。失败不会抹除无关通过项。
5. 运行时仍只从 `CapabilityEvidence → SessionPlan` 解析执行资格。模型目录、UI 或行为评分不能绕过 resolver。
6. 用独立开放任务集执行 `evaluate:model`。质量、延迟和费用用于推荐或产品质量标准，不直接授予工具或子 Agent 权限。

## 证据要求

协议证据绑定连接、Codec 和协议配置；引擎证据绑定 Codex 构建、工具契约、平台和测试条件；路由证据明确区分 A→B 与 B→A。所有执行证据包含观测断言、依赖、有效期、报告引用和替代关系。

确定性准入测试允许规定工具步骤，以证明执行链路。开放行为评测不能把统一的 spawn/send/wait 顺序当作唯一正确策略。35 次和 Wilson 下界只是一种可选质量发布策略，不是执行能力开放条件。

## 当前状态

- 四类模型协议 Codec、Bridge、预算/租约、恢复记录和部分细粒度子任务证据已经进入工程。
- `toolLoop` 已从模型的 `structuredToolCalls` 中拆出，产品工具任务需要真实根工具回环证据。
- 固定规划评审由产品启动隔离根任务，只要求对应根工具能力，不要求模型自主创建子 Agent。
- `qualify:model` 与 `evaluate:model` 已分开；行为结果带 `authorizationEffect: none`。
- DeepSeek 的真实发布证书仍为空。已有运行不能证明全部子任务生命周期、双平台构建或开放行为质量已经达标。

完成状态以能力报告、行为报告、平台构建报告和上游同步候选报告为准，不能从模型名称或单次成功推断。
