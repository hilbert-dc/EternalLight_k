# 执行详情动作名称

## 变更概览

- 目标：用已登记动作、准确来源步骤和讨论要求替换重复的「讨论与规划」，降低定位成本。
- 非目标：改变展开层级、执行流程、结果判定或增加模型调用。
- 影响模块：WorkTaskPanel、展示名称函数及 Electron 测试。
- 验收：真实选择的动作显示「安排第 N 章写作」；未选择动作的协调步骤根据准确 sourceTaskId 显示「核对第 N 章写作结果与后续安排」；普通讨论显示要求摘要。不能推断保存成功、目标完成或未登记的下一章。

## 方案

纯展示函数读取当前 WorkTaskSummary。协调步骤只认可同任务 workDecision；已登记动作必须同时绑定该 decisionStepId 和 decisionTaskId。没有登记动作时只沿准确 sourceTaskId 寻找来源，不按相邻顺序猜测。普通讨论使用 instruction 或作者 goal；超长要求显示简短摘要、最多两行，完整名称通过 title 和可访问名称保留。其他 operation 名称保持现行用语。动作名称与状态分离，不读取模型回答来推断动作。

## 测试阶段零兼容门禁

- [x] 无版本判断、旧字段适配、双写、迁移、回填或降级。
- [x] 无假设旧格式或旧调用方需求。
- [x] 无接口/schema 变化；当前 fixture 和测试同步。
- [x] 测试数据使用当前契约。
- [x] 保持安全、状态真实性、失败可恢复、展开身份与键盘交互。
- [x] 无兼容例外。

## 五角色实施前评审

以下为本次单代理按五个角色视角的自审，不代表独立代理评审。

### 架构专家
- APPROVE：展示投影依赖当前结构化记录，无执行层写入，无兼容层。
### 技术专家 A
- APPROVE WITH CONDITIONS：校验同任务、同决策尝试的身份，拒绝串用其他步骤；标题不包含内部标识，不宣称完成或保存。
### 技术专家 B
- APPROVE WITH CONDITIONS：覆盖登记动作、来源结果、普通讨论、无足够信息以及错误绑定；实际 Electron 验证原有展开键与长标题布局。
### 产品经理
- APPROVE：只优化检索名称，用户明确要求保持展开结构，不扩大任务。
### UI/UX 设计师
- APPROVE WITH CONDITIONS：名称明确对象与用途；过长要求有完整可访问文本；保持现有焦点、状态、层级与折叠默认值。

## 综合决策

- APPROVE WITH CONDITIONS；无 P0/P1 阻断项。
- 无兼容例外。
- 实施范围：执行详情名称与针对性验证。

## 实施后复核

- 实际变更：新增纯名称投影函数，替换 advisory 固定标签；已登记动作验证 task/step/attempt 三个身份，未选动作按准确来源命名；普通讨论引用实际要求。两层 details、展开身份、状态和回复位置保留。长标题最多两行，完整名称留在 title/aria-label。
- 与方案偏差：无。没有登记动作或来源信息不足时显示「核对任务进展与后续安排」，不猜测章节或已完成目标。
- 架构专家复核：APPROVE；仅 Renderer 投影，无 Runtime、IPC、schema 或执行权限变更。
- 技术专家 A 复核：APPROVE；不同 task/step/attempt 的动作均被拒绝，准确来源不受相邻其他章节干扰，状态变化不会变成保存成功声明。
- 技术专家 B 复核：APPROVE；4 项名称契约测试与 4 项实际 Electron 回归全部通过，新测试已纳入 test:agent 和 test:ui 对应入口。
- 产品经理复核：APPROVE；已登记写作显示「安排第 2/3 章写作」，结果核对显示具体章节，普通讨论显示要求；仍为通用规则。
- UI/UX 设计师复核：APPROVE；实际窄屏截图已查看，300px 容器不溢出、长名称最多两行；外层及内层默认折叠、刷新保留展开和原 summary 焦点能力均验证。
- 构建证据：npm run build 通过（tsc 与 Vite）；原有大型 bundle 提示不涉及此次名称变更。
- 测试证据：node --test electron/tests/work-step-presentation.test.mjs（4/4）；creative-workspace-ui.test.mjs 选择 execution names、conversation unifies、conversation accurately、task list serializes（4/4）。截图：/tmp/execution-step-names/execution-step-names.png。
- 零兼容门禁复查：PASS。
- 剩余项：无。
