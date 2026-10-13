# 来源绑定的属性状态与区间投影

## 目标

推进理想架构 S1 的类型化状态与 S2 的证据查询：位置、身份描述、物品集合等属性不能只保存任意 JSON，也不能以最后一条记录覆盖前文。新增 `kind=state` 候选，沿既有建模发布、快照、事实审阅、查询和索引链落地，不建立第二知识库或供应商执行通道。

普通 assertion 是有来源的命题/观察，event 是发生记录，relation 是方向关系，epistemic 是人物态度，state 是有显式值规则和世界时间适用区间的属性主张。这些不是旧格式兼容分支；不同语义不得相互默认转型。

## 当前契约

state 必须包含 subjectId、predicate、value、stateSchema、validFrom（整数或 null）、evidence。可带 validUntilExclusive 和同源 validUntilEvidence；结束必须与已知开始构成严格递增半开区间。state 不接受 storyTime、认知时间或任意扩展字段。结束表示此主张不再适用，绝不推导反值、死亡、复活或空集合。

predicate 与状态查询选择器统一为最多 80 UTF-8 字节；单条主张最多 8 KB，值与锚点均有独立限额。非法批次不得部分发布，重建只替换派生索引，不写来源或正式状态。

stateSchema 是有限类型声明：cardinality=single/multiple；valueType=boolean/integer/text/entity。entity 指定已有有限实体种类，value 只含 entityId，且同批次有正确种类的实体证据；文字不超过 512 UTF-8 字节，不接受 null、浮点、数组、任意对象、代码或 DDL。schema 没有授权含义，模型无法借 single 建立全局世界规则。

schema 与每条候选一同冻结在不可变建模来源中。它始终是 modelDeclared 解释，不是假装已确认的全局属性注册表。不同来源的同主体同谓词若声明不同 schema，应报告规则分歧，不能任意择一。同一显式 single schema 的不同值在已知重叠区间构成待审冲突；multiple 的不同成员可以并存。未知时间不得确认重叠或不存在冲突，须报告覆盖不足。人物已知或模型宣称排他都不能把候选转为真相。

## 读取与演进

知识查询复用当前/历史身份、来源、章范围、授权及同一读事务。SQL 在来源 LIMIT 前按 state 的 validFrom/validUntilExclusive 做世界时间过滤，保留未知时间及坏区间供权威验证，避免静默隐藏损坏数据。

响应保留原始 value/schema，并提供只读 canonicalStateValue（entity 值按同一快照身份决议投影）；冲突保留两条独立来源和证据，不做 latest-wins。通用端点索引将 entity 值作为派生目标，发布与全量重建使用相同提取规则，不能借任意文本值制造实体引用。

世界演进为 state 主张和 stateExpired 两种记录；到区间结束，after=null 且 interpretation=claimNoLongerApplies，不生成 false 状态。旧快照不会因后来改稿或身份裁决而重写。Reader 仍按章节披露范围查询，Character 不取得世界属性；POV 不因为新增类型放宽知识证明。

模型建模任务通过现有 Codex 执行。宿主 handoff 说明有限契约；publish_knowledge_batch 在原事务校验，非法结果不产生索引、知识头或成功任务证据。作者仍可经现有 Review 确认具体有源主张；确认主张不等于授予全局 schema 权威。

审阅提案增加 stateProperty 的有限只读说明（类型、基数、开始/结束和 modelDeclared），单章及多对象提案选择共用显示组件。确认记录仍保留完整 modelingAssertion，而不是抹平状态语义。Context Pack 的通用 stateFacts 不装配这种带区间的确认记录；通过 typedStateFacts 缺口和有界计数说明需要类型化查询，不把缺口解释为属性不存在。计数在原章/Writer/POV过滤之后累计，不借诊断暴露未来或世界知识。

## 实施前自检

由单一实现者进行五视角自检，按用户要求不组织独立专家评审。

- 架构：有条件通过；类型语义、来源和投影归 Story Core，现有执行/提交链不变。
- 技术 A：有条件通过；严格形状/值/端点/区间/锚点校验，同事务发布和重建，未知与损坏不能伪装有效。
- 技术 B：有条件通过；同步当前模型说明、索引、选择器、状态、冲突、演进及 HTTP/领域测试，不增加迁移或双轨。
- 产品：通过；区分位置迁移、多物品并存、不同来源冲突、未知时间；模型规则仍可质疑。
- UI/UX：有条件通过；沿现有知识/审阅视图保留 stateSchema 及原始证据，不新增假完成或自动冲突修复。

零兼容门禁 PASS。只支持当前 schema/协议和重建测试数据库，无获授权兼容例外。

## 验收计划

真实发布/索引/查询链验证不同类型、错误类型与端点、Unicode 锚点、区间边界、未知时间、schema 分歧、single 冲突与 multiple 并存；旧快照与来源失效、Reader/Character/Writer 范围、派生索引核对/重建及 Author-only HTTP 需对应证据。完整构建、Clippy 与回归后最终格式化。

本切片不是全局正式属性注册表、事件效果状态机、人物全部认知/披露证明或文学质量认证；这些仍须按 S1–S5 原目标完成，不能用此新类型或测试通过代替整体验收。

## 实施与验证记录

已实现 Story Core 校验、发布/重建端点提取、查询/冲突/演进、确认记录和 Context Pack 分型、模型 handoff 及审阅 UI；未增加供应商路径、调度器、DDL 或历史兼容层。

五项真实 HTTP 链路测试通过：有限值和区间、回滚、来源失效及冻结历史、实体值检索与 POV 隔离、作者确认仍保持 schema 声明性质且不成为永久 Context Fact。Story Core 105 项库测试通过（包括新属性冲突预算、同一身份不同来源、状态索引修复回滚及实体值差异端点）；231 章选择器三项回归通过，证明早部/后部状态按时间过滤后仍可命中，损坏区间不被静默过滤。

完整 `CARGO_NET_OFFLINE=true npm run verify` 退出码 0：边界与前端构建、两轮 native 回归、180 项引擎测试、2 项真实 Codex/子 MCP 委派集成、7 项发布门禁、146 项宿主测试和 12 项 UI 测试全部通过。真实工作区 UI 用例包含属性状态的未知时间和规则声明提示；委派集成的模型后端为测试环境，不是供应商文学质量或全部模型能力认证。

严格 `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` 退出码 0。原有前端大块打包提示仍存在，不将其视为已优化。最终格式化后仅做差异/上游路径检查，不再运行测试。

实施后五视角自检在本切片范围通过，由单一实现者完成，不宣称独立专家签发。全局正式属性注册表、事件效果状态机、真实模型语义/文学质量和 Windows 实机旅程仍未验收；本轮回归不能替代 S1–S5 的总体完成条件。
