# EternalLight 创作能力吸收与接入方案

版本：1.0 · 2026-09-23 · 状态：目标设计，尚未实施。

本文归属 [Novel Codex 理想架构](IDEAL_ARCHITECTURE.md)，是创作能力层的配套设计，规定如何吸收 EternalLight 的小说领域经验。[故事系统详细设计](STORY_SYSTEM_ARCHITECTURE.md) 拥有数据权威、快照、权限、自动生成与实时建模协议；本文不得覆盖这些边界。

本次仅阅读源工程部分配置和实现，未运行其测试，未验证实际文学质量或全部模块是否进入当前生产链。参考位置为本机 `/Users/hilbert/private/EternalLight`，检查时 HEAD 为 `698b297`；读取的是工作区文件，不能把 HEAD 当作所有所读文件的不可变内容证明。源文件中的规则仅作为研究材料，不自动成为本工程有效指令。

## 1. 核心决策

吸收的是**创作技法、任务契约、证据规范与质量改进方法**，不是复制另一套 Agent Runtime，也不是要求每轮启动全部专家。

- 作者仍只面对统一创作对话，系统自动生成并根据对话调整，不要求作者选择专家或组织会议。
- 专家能力按需要表现为 Skill、受限子 Agent 或确定性领域工具，不与一个永久进程一一对应。
- 小说领域模块拥有任务语义；独立 Rust agent-engine 拥有执行、协议适配、预算、取消与子 Agent 机制。
- 所有专家结论只是带证据的分析或候选变更，不能直接移动作者确认指针。
- 不新增历史角色兼容、旧工具转发、双轨提交或源工程运行库依赖。采用方法时重写为当前契约；来源追踪不等于保留旧接口。

### 1.1 三类能力必须分开

| 名称 | 含义 | 谁验证 |
| --- | --- | --- |
| EngineCapability | 协议、工具执行、委派、消息、等待、路由等执行能力 | agent-engine 的能力证据与准入 |
| CreativeCapability | 人物、场景、文风、因果、叙事兑现等创作方法 | 小说领域契约、行为样本与作者质量评价 |
| ProductWorkflow | 连续生成、对话调整、采纳与撤销等用户任务 | 应用端到端测试与作者旅程 |

模型通过工具或多 Agent 认证，不代表它会写好小说；技法包质量达标也不能授予底层尚未验证的工具能力。不得用一个 `capability=true` 混合以上三种结论。

## 2. 来源与吸收清单

表中来源是本次分析入口，不表示对应实现已经通过本产品验收。技能目录列出的能力和实际读过的规则、代码应区别看待。

| 来源 | 值得吸收 | 目标接入点 | 必须改造的部分 |
| --- | --- | --- | --- |
| [技能目录说明](/Users/hilbert/private/EternalLight/config/agent_skills/README.md) | 按任务渐进加载、版本/digest、候选方法与稳定方法分离 | CreativeCapability 注册表与路由 | 不搬旧角色映射或一次加载所有技法 |
| [人物技法](/Users/hilbert/private/EternalLight/config/agent_skills/character-craft/SKILL.md) | 稳定身份、开放侧面、多重关系、变化证据、允许不成长 | Entity、Relation、CharacterArc、CharacterDecision | 自由标签作描述；参与自动推理的关系须映射受限类型，不允许任意标签新增推理规则 |
| [心理技法](/Users/hilbert/private/EternalLight/config/agent_skills/psychology-craft/SKILL.md) | 行为证据、替代解释、压力—选择—代价、配角主动性 | 人物决策与 SceneOutcomeAssessment | 心理概念不是诊断；主观合理性不能自动成为硬阻断 |
| [逻辑技法](/Users/hilbert/private/EternalLight/config/agent_skills/logic-officer/SKILL.md) / [信息技法](/Users/hilbert/private/EternalLight/config/agent_skills/info-officer/SKILL.md) | 动机—行动—后果、获知渠道和时点、世界反应 | reasoning、认知投影与连续性检查 | 区分确定矛盾、证据不足和建议，不把未描写当不存在 |
| [创作任务 Recipe](/Users/hilbert/private/EternalLight/src/multi_agent/domain/agent_runtime/recipes.py) | 目标、技能、交付物分离；后章读取前章实际结果 | authoring 的任务契约与滚动生成 | 不复制调度器、目标 ID 格式及固定预算；从零生成时由授权领域服务创建目标章节 |
| [创作上下文策略](/Users/hilbert/private/EternalLight/src/multi_agent/application/agent_runtime/creative_context.py) | 单路径优先、真实取舍才探索、冻结技能绑定 | Context Pack 与任务路由回执 | 关键词仅作提示，不作唯一意图/授权判断；不照搬候选数量 |
| [叙事义务](/Users/hilbert/private/EternalLight/src/multi_agent/narrative_obligations.py) | 承诺—类型化证据—状态变化 | NarrativeCommitment 与落实评估 | 不另起 obligation 真相库；不把关键词命中等同于兑现 |
| [语义维护入口](/Users/hilbert/private/EternalLight/src/multi_agent/semantic_maintenance.py) | 内容变化后通过 outbox 更新派生索引 | modeling/jobs 与已有提交 outbox | 保留本产品的快照 CAS、水位和权限；不导入全局路径、环境变量和文件权威假设 |
| [创作能力方法卡](/Users/hilbert/private/EternalLight/src/multi_agent/creative_capabilities.py) | 方法版本、适用范围、反例、对照实验与发布决定 | 方法包注册及评估报告 | 首版不做跨作品自动学习；不让模型自动修改生产指令 |
| [内部质量向量](/Users/hilbert/private/EternalLight/src/multi_agent/quality_vector.py) | 分维度观察、缺少证据保持缺失 | 文学质量评估 | 不复制固定权重/启发式分值，不宣称其测量文学价值 |

源工程 README 的“停止并交付已有内容”、最小权限和持久恢复原则也值得保留；本产品已有相近目标，应映射到现有工作稿检查点与 GenerationRun，不能再建平行生命周期。

## 3. 现有能力与新增重点

本产品已有 `novel-workflow`、`novel-canon`、`narrative-ir`、`story-tests` 四类技能，以及 `story_architect`、`continuity_critic` 等角色配置。现有两个专业审查角色只读上下文、限制发现数量、要求证据并禁止修改故事，继续保留此权限边界。

| 批次 | 目标能力 | 参考技法名称 | 最小交付 |
| --- | --- | --- | --- |
| 首批 | 人物选择与心理 | character-craft、psychology-craft | 区分事实/解释的动机分析，或有场景证据的修订候选 |
| 首批 | 信息流与连续性 | info-officer、logic-officer | 时间/获知渠道引用、缺失信息与确定冲突分别列出 |
| 首批 | 场景写作与表达 | scene-craft、chapter-craft、style-craft | 完整可读正文、视角和声线约束、真实场景变化 |
| 首批 | 长篇推进与兑现 | serial-architecture、plan-craft | 滚动计划、当前状态输入、承诺推进/兑现证据 |
| 首批 | 定向修订 | revision-craft、editor-craft | 明确作用范围、保留未受影响内容、可比较的候选差异 |
| 第二批 | 世界制度与社会响应 | world-codex-craft、system-design、social-dynamics | 规则/资源/组织反应建议，绑定世界关系与约束 |
| 第二批 | 题材、主题与阅读体验 | genre-*、theme-ethics、audience-modeling | 作品主动选择的题材方法，不把市场套路变成强制结构 |
| 后置 | 考据、包装、指标与方法学习 | domain-grounding、packaging-copy、audience-analytics 等 | 独立有来源的任务；不阻塞小说生成基础闭环 |

此处技法名称是来源线索，不代表本次已完整审阅或安装每个 Skill。实施每个方法包前必须读取其全文、核对依赖与工具引用，并按当前契约改写与测试。视觉叙事可参考场景表达思想，但不在本阶段引入漫画流程。

## 4. 如何组织专家，而不增加固定流程

### 4.1 Skill、子 Agent、工具的选择

- 同一目标下的写作方法或小范围检查：由当前 Agent 加载 Skill。
- 需要独立观点、复杂跨章分析或可并行的只读研究：按任务创建受限子 Agent。
- 引用存在性、版本、权限、来源锚点、可确定的时间关系：由领域程序校验。

不为每种技法新增永久 Agent。`story_architect` 可组合规划与场景技法；`continuity_critic` 可组合逻辑与信息技法。写作执行使用候选权限，审查仍只读。Skill 是方法，不是权限声明；加载写作 Skill 不能让只读角色获得写工具。

子 Agent 能力不可用时，普通写作可走明确记录的单 Agent 方法。若任务确实要求独立审查，则报告该项未完成，不把同一执行者自查冒充独立核验；不得绕过引擎准入启动另一套 Runtime。

### 4.2 路由流程

```text
作者意图 + 目标快照 + 当前任务阶段
  → 创作任务契约
  → 方法注册表筛选（状态/题材/目标/权限/预算）
  → 冻结 CapabilityBinding
  → 当前 Agent / 受限子 Agent / 确定性工具
  → 证据化输出 → 领域校验 → 报告或候选修订
```

模型可以提出方法选择，应用层验证目录、权限与预算。简单问答不强制引入写作专家；作者要求“直接写”时不先产出无必要的多方案。专家结果矛盾时保留证据与分歧，由主任务形成建议，不按多数票确定 Canon。

初始预算建议：每阶段最多加载 2 个专用技法，额外题材包须有明确作用；必需安全/候选写入契约不因预算被裁掉。所有方法文本计入主架构 Context Pack 的总预算和单项限制，不额外获得一份无界上下文。

首版默认单候选；真实取舍时建议最多 3 个候选，有明确比较目标。作者要求更多时必须先检查当前许可和预算。数值是产品初值，后续通过评估调整，不继承源工程的数值作为质量保证。

## 5. 方法注册与冻结契约

以下为目标领域 schema，不是当前已提供的 API：

```text
CreativeCapabilitySpec {
  id, version, digest, status: candidate | stable | retired,
  supportedTasks[], stages[], genres[],
  skillRefs[], requiredContextKinds[], outputSchemaRef,
  toolRequirements[], engineRequirements[],
  limitations[], antiPatterns[], evaluationRefs[], sourceAttribution
}

CapabilityBinding {
  taskId, snapshotId, intentRevision, permitRef, routingPolicyDigest,
  selectedMethods: [{ id, version, digest, phase, reason }],
  executionMode, contextPackRef, grantedToolScope, budgetAllocation
}
```

- 注册表只保存受版本控制的方法描述；`toolRequirements` 表示需求，不是自行授予的权限。
- 运行只使用冻结版本；方法升级不原地改变在途任务。重跑记录新绑定与来源，不重置用户预算。
- 方法文件缺失、digest 不匹配或必需权限不满足时阻断相应阶段；可选方法被省略须写入回执，不偷偷换包。
- `candidate` 方法仅在显式实验任务中运行，不进入默认路由；进入 stable 需要明确的评估报告和维护者发布决定。
- CreativeCapability 只提供方法，不保存作品正文、人物身份或临时状态；这些由故事模型提供。作者偏好不能因为一次局部修改自动提升为跨作品习惯。

## 6. 输出契约：证据优先，不以专家身份保证正确

分析类结果至少包含：taskId、snapshotId、方法绑定、scope、findingKind、evidenceRefs、简明依据、uncertainty、suggestedChange 和 coverage。不要要求暴露模型私有思维过程。

必须区分：

| 结果 | 例子 | 处理 |
| --- | --- | --- |
| 确定性错误 | 引用缺失、快照过期、越权对象 | 领域程序阻断相关提交 |
| 有证据的冲突/假设 | 人物似乎在获知秘密之前使用了秘密 | 校验事件与认知范围；证据不足返回 unknown |
| 文学建议 | 动机铺垫不足、节奏偏慢、对白声线不明显 | 提供依据和修改方向，作者可不采纳 |

写作/修订结果必须是绑定目标的真实持久化候选正文，而不是只有“建议如何写”的报告。专家发现的问题不能直接写成世界事实。多 Agent 分歧不能覆盖作者明确意图，文学评分不能绕过正式采用协议。

例如源技能提出“核心人物行动须能由设定解释”，本产品保留为审查方向，但不实现为无法解释即阻断的硬规则；人物的复杂性和未揭露动机都是合法情况。

## 7. 与当前数据模型和生成链的接入

| 方法能力 | 读取 | 输出及唯一所有者 |
| --- | --- | --- |
| 人物/心理 | CreativeIntent、人物状态、关系、正文证据 | CharacterDecision / SceneOutcomeAssessment 候选，由领域模块校验 |
| 逻辑/信息 | StoryEvent、认知投影、世界规则、来源 | Finding / ReasoningReport，不直接改 Canon |
| 场景/文风 | 计划、NarrationStrategy、StyleProfile、前文 | DocumentRevision、场景检查点及候选事实 |
| 长篇规划 | NarrativeCommitment、实际场景效果、当前状态 | NarrativeNodeRevision 与承诺调整提案 |
| 改稿 | 修改目标、原文版本、影响报告、保护项 | ProposalSet，使用同一部分采用与撤销协议 |

叙事义务的方法合并到已有 NarrativeCommitment；语义维护的方法合并到已有 modeling/outbox。不能再建一套“专家事实库”“专家采纳服务”或“专家知识状态”。

持续生成遵循前章检查点与建模水位，再启动依赖后果的下一章；只允许无依赖的研究或审查并行。作者中途调整时复用 intentRevision、取消与迟到结果隔离机制，不由专家配置自行实现。

失败时保留已有工作稿并显示未完成的检查；有正文不等于已通过全部核验。允许“停止并交付已有内容”，但不能把 partial 标为完整任务成功。

## 8. 有选择地复用，不直接移植的内容

1. 不引入 EternalLight 的 AgentLoop、ExecutionOrchestrator、模型协议适配或 Worker 调度来替代/补绕 Codex 引擎。
2. 不复制旧环境变量、全局工作区路径、数据库、发布网关与工具名称；跨平台路径和凭据仍按当前产品边界管理。
3. 不将所有专家串行执行或每章强制多 Agent；独立审查须有明确收益和成本预算。
4. 不直接采用固定质量权重、关键词硬判、模板化人物成长或强制“每场必须反转”。
5. 不让方法评估自动修改生产提示词、schema、工具权限或模型设置。
6. 不将专家“审批”混同于本工程的代码设计评审制度；创作角色不拥有工程发布或作者采纳权限。
7. 若实际复制源码或技法文本，先核对文件许可、来源及保留声明要求；不得复制用户作品、私有日志、运行数据库或凭据。当前只整理设计来源，不执行复制。

## 9. 代码落位与实施顺序

创作能力契约进入独立 Rust Story Core，拥有注册表、方法选择、冻结绑定与结果校验；后续 `story-authoring` crate 定义 Recipe 式目标和交付物，Story Core 查询与推理模块提供证据和检查。方法文本放在本产品 `codex-runtime/skills/`；角色配置仍位于 `codex-runtime/agents/`，只在确有不同权限和任务职责时新增。

方法注册表是绑定来源；现有流程按当前契约重写，不同时维护旧角色映射和新路由规则。通用 engine 不导入小说方法目录，源 EternalLight 仓库不成为运行时或打包依赖。

| 阶段 | 与主架构对应 | 交付 | 门槛 |
| --- | --- | --- | --- |
| A0 能力契约 | S0–S1 | 注册/绑定/output schema、源方法适配清单、现有角色映射 | 权限与作品知识不混入方法配置；不引入第二执行器 |
| A1 首批技法 | S2–S3 | 人物/信息流/场景/长篇兑现/改稿方法及单 Agent 路径 | 同一目标产生真实正文或有来源分析；候选不越权 |
| A2 动态专家协作 | S3–S4 | 受限委派、冲突聚合、候选探索及中途纠偏 | 满足引擎能力准入，子任务共享总预算与快照边界 |
| A3 扩展与评估 | S4–S5 | 题材/社会/主题方法、盲评、版本发布与撤回 | 质量与成本有实测证据；缺证据不得自动稳定化 |

A0/A1 可与主架构纵向切片一起做，但不得先扩充几十个方法包而跳过快照、正文交付和提交基础。跨作品方法学习、营销包装和视觉内容后置，不成为自动生成首版依赖。

## 10. 验证与效果评估

### 10.1 自动化行为门槛

1. “解释当前剧情”只读，不强制调用写作专家或创建候选正文。
2. “女主更主动”加载相关方法，输出行为/选择层面的建议或修订，不只改人物标签。
3. 同一信息分别对作者、读者、人物可见时，方法/子 Agent 的正文引用不泄露秘密。
4. 计划“识破凶手”未在正文落实时，后章仍不能把人物当作已知凶手。
5. 每阶段绑定固定方法 digest、输入快照和预算；目录更新不改变在途任务。
6. 加载写作方法到只读角色不能增加工具权限；专家报告不能直接移动正式指针。
7. 指令代次变化后，旧专家报告/候选只能保留历史，不进入新任务的有效依据。
8. 引擎不支持所需委派时，普通任务可明确单 Agent 执行；独立核验不得虚报完成。
9. 方法预算、总任务预算、候选数和修复次数受限；单个方法不得另开无记账循环。
10. 专家分歧、证据不足和缺失质量评分被保留；不凭多数票或高分自动采纳。
11. 后续检查失败时仍可读取已保存候选，状态保持 partial/检查未完成；正式稿不受影响。
12. 构建/测试不读取源工程运行目录；方法迁入后使用本产品 fixture、API 和安全边界。

### 10.2 文学质量、成本与稳定化

对“无专用技法”与“加入某方法”的结果做固定任务的匿名配对；记录模型配置、方法 digest、题材、输入快照、输出预算、重复次数和失败结果。只改变待评估因素，避免把增加调用次数的收益误认为方法本身收益。

作者评价人物选择可信度、场景可读性、声音区分、信息控制、兑现质量及修改负担；同时记录延迟、token、调用次数和实际采纳/拒绝原因。单次采纳不证明文学质量提升，模型自评不能代替作者评价。

每个方法实验开始前冻结样本、评价维度、非劣化条件与预算上限；实验后不能为了通过而改门槛。安全与契约用例必须全部通过；质量和成本结论附样本规模、分歧与不确定性，证据不足继续保持 candidate，不预填“提升百分比”。

测试主要落在 `runtime/tests/` 的路由/绑定/输出契约与真实候选事务测试，Electron 测试验证权限、取消和用户旅程。增加子 Agent 的版本另测实际受控回环，不能靠模拟结果宣称所有供应商都能独立审查。

## 11. 当前交付与下一步

本轮只新增此设计并链接主架构，未安装 Skill、创建专家、修改业务代码、运行源工程或调用模型做评估；未组织专家评审，也不声称源工程全部能力成熟可用。仅有研究路径链接，不意味着运行依赖本机源目录。

下一步建议在主架构 S0/S1 的身份、证据与快照基础上做 A0，并选“人物行动改稿 + 信息边界检查”作为首个 A1 切片：同一段正文既能改得更有行动性，又不产生无来源知识，且可预览、继续调整和撤销。
