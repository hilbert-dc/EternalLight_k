# 删除兼容性代码：实施方案与五角色评审

日期：2026-09-12 · 状态：实施完成，复核通过

依据：[测试阶段工程策略](TEST_STAGE_ENGINEERING_POLICY.md)；模板：[设计评审模板](DESIGN_REVIEW_TEMPLATE.md)。

## 变更概览

- 目标：让工程只存在一套当前契约，删除旧数据库、旧 Outline/Plan/Draft、旧规划反馈表和旧 Codex App Server 事件别名的读取、转换、备份与测试代码。
- 非目标：不删除当前版本的 revision stale 检查、事务回滚、幂等恢复、搜索降级、本地 Runtime 模式、跨平台路径和安全校验；这些不是历史兼容。
- 当前契约：层级 Outline、绑定当前 Outline 的 Plan、结构化 `sourcePlanRefs`、`planning_impacts`、当前 app-server v2 方法名。
- 数据策略：只保证空环境按当前 schema 初始化；现存测试数据库由开发者自行删除重建，程序不迁移也不自动删除。

## 实施范围

1. Runtime schema 直接声明全部当前列，删除 schema 版本、升级备份、ALTER/回填、`migration` source、`source_plan_revision_id` 和旧 `plan_impacts`。
2. Outline/Plan 去掉 `formatVersion` 分流及旧四段/旧章纲合同；Plan 必须引用当前正式 Outline。
3. Draft 来源只由 Runtime 根据 binding/Task manifest 生成 `sourcePlanRefs`；调用方不能提交旧 `sourcePlanRevisionId`。
4. 反馈只使用 `planning_impacts`；删除旧表 fallback、旧 plan impact 解决与撤销分支。
5. Renderer 删除旧大纲展示和格式判断；MCP/TypeScript 只暴露当前合同。
6. App Server 只接受当前 `item/tool/requestUserInput`、camelCase 字段和当前方法；删除旧方法别名与旧版本静默忽略。
7. 删除迁移测试，把仍有价值的领域测试改为当前 Outline/Plan fixture。
8. Runtime 状态只保留 `canon`、`narrativeIr`、`novelEdition` 三个权威命名空间；删除重复顶层数组及 Renderer 双读模型。

## 零兼容门禁

- [x] 不新增版本判断、fallback、双读双写、迁移、回填、降级或 deprecated 路径。
- [x] 不为旧调用方或旧数据降低目标模型。
- [x] Runtime、MCP、Electron、Renderer、类型、fixture、测试和文档在同一批次切换。
- [x] 测试数据库按当前 schema 重建。
- [x] 保留当前版本的数据一致性、故障恢复、安全、并发与跨平台验证。
- [x] 无兼容例外。

## 五角色评审

### 架构专家

- 结论：`APPROVE`。
- 一套当前合同可以删除 Artifact/Draft/Impact 的双权威；初始化必须直接创建完整 schema，新项目必须原子创建 workspace 指针。

### 技术专家 A

- 结论：`APPROVE WITH CONDITIONS`。
- 不得自动删除用户机器上的数据库；旧库可以明确失败。保留当前版本的事务、CAS、幂等、索引恢复和安全边界。

### 技术专家 B

- 结论：`APPROVE WITH CONDITIONS`。
- 删除旧测试而不是改写成伪兼容测试；仍有领域价值的锁、反馈、上下文和撤销测试必须改用当前契约。必须通过 Runtime、Agent 和 production build。

### 产品经理

- 结论：`APPROVE`。
- UI 不再展示“旧大纲”或引导转换；空项目必须能直接完成大纲→章纲→正文→反馈主链路。

### UI/UX 设计师

- 结论：`APPROVE`。
- 删除旧格式条件渲染后保留层级大纲唯一编辑器、明确的缺少正式大纲状态和现有未保存保护。

## 综合决策

- 最终结论：`APPROVE WITH CONDITIONS`，上述技术条件均为本次实现与验证项。
- 兼容例外：无。
- 预计风险：本轮是有意破坏性调整，旧本地数据库不受支持；当前工程尚无数据库文件，因此不执行任何删除动作。

## 实施后复核

- Runtime：建库脚本直接声明当前 schema；已删除 schema version、备份、ALTER/回填、旧表、旧 Draft 来源列和 source=`migration`。空环境与新项目会原子初始化当前 workspace。
- 创作契约：Outline、Plan、Draft、planning impact 只剩当前结构；Draft 来源由 Runtime 根据正式 binding 生成，Plan 必须绑定当前正式 Outline。
- 桌面协议：已删除旧 requestUserInput 方法、snake_case token/status 字段、旧流式字段、旧 turn/compaction/metadata 事件名和 thread name 静默降级；fixture 改为当前 App Server v2 形态。
- Renderer：已删除旧大纲展示、格式分支、旧 impact 展示和重复顶层 RuntimeState 投影。
- 测试：`npm run test:runtime` 35/35 通过；`npm run test:agent` 29/29 通过；`npm run build` 通过。
- 零兼容扫描：生产代码中未发现 migration、ALTER/回填、旧 Outline/Plan/Draft 字段、旧 App Server 方法/字段或状态双投影。扫描命中的 `planning_impacts` 是当前反馈模型，Runtime 的本地模式、索引恢复、事务、CAS、幂等、安全和跨平台路径按非目标保留。
- 最终结论：五角色条件已满足，`PASS`；无兼容例外，无兼容后续项。
