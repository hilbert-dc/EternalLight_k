# 作者属性规则与状态投影

## 目标与实施阶段

推进 S1 世界规则与状态演进，不以模型的 single/multiple 声明代替作者的规则。
第一阶段交付版本化规则、Author Gateway 保存、审阅采用/撤销、当前与历史投影，
以及真实 HTTP 验证。作者规则审阅 UI 与对话提案现已接通，有限事件效果及条件
解释仍需继续。阶段划分服务可审查实施，不缩小 IDEAL/STORY 的完整完成条件。

## 领域契约

`propertyRules` 为既有修订库中的 `artifact-propertyRules`，只在 main 保存。
作者直接保存 source=author 且无 Task 绑定；模型 source=agent 必须绑定
generatePropertyRules 根任务及 propertyRules 产物。其他操作、伪造来源与子 Agent
不能保存。来源与权威分开，模型生成候选也必须由作者明确审阅采用才生效。
payload 必填 definitions 与 eventRules 两个数组；definitions 每项为 predicate、subjectKind、schema、
description、exceptions。schema 复用 state 的有限类型；主体种类限定既有实体种类。
同一主体种类/谓词最多一条定义，最多 64 条，总 payload 不超过 16 KiB；说明
最多 512 UTF-8 字节，例外最多 8 条，每条最多 256 字节。无代码、DDL 或任意 DSL。
eventRules 最多 16 条、每条 4 KiB，使用有深度/节点/类型限制的 all/any/not 与
状态比较表达式，引用同份属性定义及事件角色。规则整数常量限 JSON 安全整数。
完整契约见 [事件条件设计](EVENT_CONDITION_DESIGN.md)。属性编辑保留事件规则；
删除被引用的属性必须同时明确修改或移除相应事件条件，不能悄悄丢弃。

作者保存只建立候选；审阅不得混入事实、选择或建模批次。现有 Author Review
采用才赋予结构规则权威，原采用事务同时检查对象、规则形状、锁和作品版本。
撤销沿原补偿提交。规则版本是不可变 revisionId，不另建一套批准指针或数据库。

既有 artifact 保存链读取上一候选版本时，缺失、损坏、非法版本及溢出明确失败；
不再吞掉读取错误并将版本默认为第一版。首次保存无需上一版本，重放仍沿既有幂等回执。

当前规则取 accepted head；历史取冻结快照祖先链上的采用/撤销，不能从快照的
候选成员猜测批准。损坏或错归属的已采用记录报错，不退回模型声明。

## 投影语义

只有无位置限制的 Author/World 查询应用全局规则。Reader、Character 及任务绑定
的 ThroughChapter 查询明确采用有限视角政策，不加载规则正文或修订信息。

原 assertion/value/schema/evidence 不改变。匹配规则时额外提供规则修订及评估：
compatible、declarationMismatch、typeMismatch 或 requiresInterpretation。无例外且值
符合类型时 effectiveStateSchema 使用作者规则；声明不一致仍保留诊断。类型不合
不做强制转换，未知的实体种类不忽略。存在自然语言例外时不假定例外永远为假，
不授予有效基数，报告适用性尚待解释。上述未应用规则计入覆盖不足。

`compatible` 只表示本切片的类型、基数声明及值形状兼容，不代表模型主张真实或
正文满足说明里的文学/数值限制。description 是解释文字，不是被执行的附加约束；
范围、数值上下界、因果与事件效果仍须后续类型化契约和独立证据，不能从说明推导。
Author/World 披露正式 eventRuleCount 与 eventConditionsPolicy=declaredNotInterpreted；
有限视角仍由原 propertyRulesPolicy=withheldForLimitedPerspective 统一声明两类规则
未披露，不返回事件规则数或重复规则声明；不能用零冒充没有规则。表达式合法或规则已采用
不是条件成立证明；全来源与反证条件解释、状态演进仍继续实施。

上下文预算沿原整包裁剪。空的可选 workingKnowledge 投影在预算不足时仅省略
重复作品 ID、空身份/时间参数和全空选择器；保留活动视角、阅读/认知位置、
规则披露政策、规则修订及覆盖计数。明确 metadataTruncated、完整性为 false 和
contextKnowledgeMetadataBudget。冻结任务、方法绑定、目标正文及事实不能因此删除，
仍放不下时必须拒绝；不提高预算、不对带主张的投影省略来源元数据。
检查可选数组必须采用不创建字段的 get_mut 链；缺失或 null 的可选目录/投影
保持原形状，不能因检查预算被可变索引变为 entries:null 的对象。

冲突比较有效 schema：正式 single 找到重叠区间的不同值，正式 multiple 允许不同
成员并存；无正式规则仍报告模型 schema 分歧。未知时间、类型不符或待解释不能
伪装无冲突。规则权威与主张确认状态独立，任何一方都不覆盖来源或自动确认值。

## 实施者五视角检查（非独立评审）

- 架构：原 Revision/Review/Acceptance/Snapshot 为权威，独立私有规则模块负责解释。
- 技术 A：Actor 边界、写入/CAS、接受再验、历史祖先及失败关闭；不增加执行通道。
- 技术 B：候选与批准、改规则/撤销/历史、类型/例外/作用域、坏 payload 真实 HTTP 回归。
- 产品：作者批准规则不等于批准全部内容；单值/多值、类型矛盾及例外可解释。
- UX：阶段一不将未接入的 UI 标为完成；后续从模型候选提出规则供作者明确审阅。

零兼容检查：无新表、迁移、旧协议 fallback 或历史数据修复。只支持当前契约。
综合结论：按本契约实施；整体 S1–S5 完成仍须原全部验收证据。

## 验收记录

2026-09-27：第一阶段已实现，未新增表、迁移、依赖或上游内核代码。

- 新增 3 项规则 Core 回归：类型/主体作用域、有限定义预算、批准回执与损坏数据；
  包含撤销目标丢失不能被解释为没有规则。原生 Core 共 108 项通过。
- `state_property_tests` 共 7 项真实 HTTP/仓储测试通过，其中新增两项覆盖候选
  不生效、采用、修改、撤销、历史规则、Writer/Reader 隔离、类型不符与条件未知。
- 新增两项裁剪单元回归，Runtime lib 共 3 项通过；原 20 KB HTTP 预算保持不变，
  验证任务/冻结方法/目标正文保留、覆盖与精确字节数、完整摘要重算及不足预算拒绝。
  完整验证曾发现预算回归，新增单测进一步发现可变索引创建空字段；已修复并重测。
- Electron 生产处理器回归验证规则保存的幂等键原样转发、纯规则审阅，以及任务
  四段分页游标与 Rust 同步。未增加旧格式兼容路径。
- 最终 `CARGO_NET_OFFLINE=true npm run verify` 退出 0：边界、构建、两轮 native、
  引擎与委派、发布门禁、147 项 Agent/Host 和 13 项真实 Electron UI 全部通过。
  边界报告 strictReady=true；构建仍有既有的大包体警告。
- Rust 工作区所有目标的离线、锁定依赖严格 Clippy（`-D warnings`）通过。

上述是 macOS 自动化与受控模型端点证据，不是新规则 UI 已实现，不替代真实
供应商、Windows 实机、语义金标或文学效果验收。规则对话提案、规则逐项披露、
条件解释、数值/因果约束与事件状态机仍按完整 S1–S5 目标继续，不宣称整体完成。

## 作者工作区后续落地

作者规则 UI 已接入“世界与规则”：可选有限表单、候选与正式版本展示、纯规则
审阅、采用和原账本撤销。未知保存结果冻结原请求，已确认回执后的读取失败只读
核对；用户设置与关闭均经过统一未保存保护，不自动放弃本地输入。

2026-09-27 最终完整验证退出 0，147 项 Agent/Host、14 项 Electron UI 通过。
真实 Runtime 回归验证采用后同键重放原保存回执及同键改载荷拒绝，UI fixture
遵守候选保存不增加正式作品版本。详细契约、故障与布局回归见
[作者规则工作区](PROPERTY_RULES_UI_DESIGN.md)。此更新仅替代上面的“新规则 UI
尚未实现”状态，不改变其余 S1–S5、真实供应商、双平台与文学验收的未完成结论。

## 对话提案执行契约

对话显式修改请求沿原 request_candidate_task、WorkTask/Step/Attempt、Host 和
Codex/MCP 登记 generatePropertyRules。propertyRulesBase 提供冻结候选优先基线的
描述；根任务用 read_property_rules 的有界 UTF-8 JSON 页核对并重组完整基线，
自身保存后也不把新 head 冒充原输入。详见 [冻结规则读取](PROPERTY_RULE_INPUT_PAGING_DESIGN.md)。
没有有效持久产物不能 complete_task。源或候选变更拒绝迟到写入，新工具沿原
recovery operation 与候选事务提交证明，不以宿主消息猜测保存成功。模型提案
在原规则 UI 展示来源，采用/撤销和正式投影仍用同一权威链。
当前动作表 CHECK 同步新操作，只改建库契约，不添加旧库迁移或兼容路径。
详见 [对话式规则提案](PROPERTY_RULE_PROPOSAL_DESIGN.md)。

分页变更后的完整验证退出 0：Core lib 109 项、MCP 11 项、Agent/Host 150 项、
Electron UI 14 项通过；实际模型可见页面证据见有界读取文档。
此更新替代第一阶段“规则对话提案未实现”的状态；模型 token 预算、逐项披露、
条件解释、数值/因果约束与事件状态机以及真实模型/双平台/文学验收仍独立待办。
