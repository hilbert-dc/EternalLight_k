# 作者主导创作 AC-0／AC-1 实施方案与五角色评审

> 现行覆盖规则：本方案中的旧 schema、旧数据、双读、迁移、回填、备份和降级要求已由 [测试阶段工程策略](TEST_STAGE_ENGINEERING_POLICY.md) 废止。当前实施只面向当前格式和空环境初始化；历史段落不得生成兼容任务。

版本：1.1 · 日期：2026-09-13 · 状态：五角色实施复核中

实施进度：[AC01_IMPLEMENTATION_STATUS.md](/Users/hilbert/private/codex/eternallight/novel-codex/AC01_IMPLEMENTATION_STATUS.md) 记录已落地纵向主路径、测试证据与尚未完成的强化项。

关联产品文档：[PRODUCT_AUTHOR_DIRECTED_CREATION.md](/Users/hilbert/private/codex/eternallight/novel-codex/PRODUCT_AUTHOR_DIRECTED_CREATION.md)。评审角色均为独立 AI 子任务，不是真人签字。

## 1. 变更概览

- 背景：当前工程能保存 Brief、四字段 Outline、近章 Plan 与章节候选，但层级规划没有进入章节上下文；正文采用后的反馈只按事实键影响未来章纲，不能指向单元纲、卷纲和全书大纲。任务目标、完成判定、计划来源和上下文裁剪还有已知可靠性缺口。
- 目标：交付一条可运行的 AC-0／AC-1 纵向闭环：可跳层创建全书／卷／单元规划，章纲绑定明确上级节点和修订，按已采用规划生成或手写章节，采用正文后把确认的实际变化反馈到章纲及各级大纲，由作者审阅调整。
- 非目标：AC-2 双试写、自动撰写改纲候选、完整语义影响推断、任意字符锁定、AG-1 连续多章、跨卷全文重构、读者反馈导入和文学评分。
- 用户可见结果：大纲页从四字段变为可折叠层级卡片；近期计划显示其服务的上级节点；章节页显示精确规划依据；正文与事实采用后，受影响的章／单元／卷／全书节点显示待复核，作者手动修改并采用后可关闭对应反馈。
- 约束：保留正式／候选／审阅／采用边界；Agent 不能正式采用；测试数据可按当前 schema 重建；小说领域实现留在本工程，不修改 `codex-core`。

## 2. 当前行为与影响范围

- `OutlinePayload` 只有 opening／midpoint／climax／ending；`RollingPlanPayload` 的章节没有上级节点关系。
- Draft 只存自由字符串 `sourcePlanRevisionId`，未验证其属于当前作品、是当前正式计划并包含目标章。
- Writer 上下文只取当前章 commitments，不含相关全书／卷／单元节点；场景级读取可能带入当前章后文；字节裁剪会删除 forbidden 和 commitments。
- `complete_task` 对 draft 接受任意历史 sceneComplete／chapterComplete，未严格要求最新、非空、与任务所需完整度一致；Planner 必需产物没有统一完成校验。
- Electron 自由文本依靠中文正则决定 Planner／Writer；显式章节按钮只有 Writer，规划按钮仍回到自由文本入口。
- 当时 schema 中存在未接通的 AG-1 表。本轮目标只定义当前 v5 形状，不保留或转换该中间态。

开工基线实测（2026-09-12）：`npm run build` 通过；Runtime 32 项中 31 通过、迁移备份名断言 1 项失败；Agent 25 项中 24 通过、1 项因当前沙箱禁止监听 `127.0.0.1`（`EPERM`）失败。后者须在允许 loopback 的环境复验，不能直接当成产品回归。

影响文件预计为：`runtime/creative_workspace/{schema,service,generation,context_limits}.py`、`runtime/{server,mcp_server}.py`、Runtime 测试；`electron/{creative-task-contract,main,preload}.mjs|cjs` 与测试；`src/{types,CreativeWorkspaceView,App,styles}.tsx|css`；工作流 Skill、架构与产品状态文档。

## 3. 候选方案

| 方案 | 优点 | 缺点／风险 | 结论 |
| --- | --- | --- | --- |
| A. 每个层级独立表、独立修订与基线 | 最细粒度版本与并发 | 首批迁移、采用和 UI 复杂度过高；会与现有 artifact revision 形成两套权威 | 不采用 |
| B. Outline 作为含稳定节点 ID 的版本化树；Plan 引用 Outline 修订和节点；Draft 引用两者 | 复用现有候选／审阅／采用事务；一次修订提供一致层级快照 | 同一 Outline 内仍是粗粒度 CAS；节点级历史需从版本差异计算 | 采用 |
| C. 仅在 Prompt 中拼接卷纲／单元纲 | 代码量小 | 无身份、来源、校验、反馈目标或恢复，无法满足产品验收 | 拒绝 |

## 4. 推荐方案

### 4.1 权威模型与数据契约

AC-1 只设一套规划权威：一次采用的是完整、不可变的 `OutlineSnapshot`，不支持节点局部采用；节点只是快照内部的稳定身份。Artifact 的 formal/candidate/review/accepted 是发布状态，节点的 `certainty` 是作者对内容成熟度的判断，两者不能互换。正式 Outline 快照允许包含 `open` 或 `tentative` 节点。

`OutlinePayload` 目标形状：

```text
{ nodes: OutlineNode[] }
OutlineNode = {
  id, kind: book|volume|unit, parentId?, position,
  title, publicTitle?, purpose, summary, dependencyKeys[],
  certainty: open|tentative|settled,
  protection: free|preserve-intent|preserve-arrangement,
  writerGuidance?
}
```

新节点和新章纲条目由 UI/Agent 提交请求内唯一的 `clientKey`，同批子节点通过 `parentClientKey` 引用；Runtime 原子生成正式 `nodeId/chapterPlanId`、替换全部引用并返回规范化 payload。已存在对象必须提交正式 ID，不接受 clientKey 冒充。幂等重试返回相同映射；重复 clientKey、未知 parentClientKey、混用 clientKey/正式 ID 均拒绝。Scene ID 也由 Runtime 在保存 SceneGroup 时生成，调用方不得自造。

规则：恰有一个 `book` 根；正式节点 ID 在后续快照中保持；父节点同一版本内存在且无环；`volume` 只属于 book，`unit` 可属于 volume 或 book，允许跳过卷／单元；同父节点 position 为从 0 开始的连续唯一整数；open 节点允许 purpose／summary 为空。`writerGuidance` 是允许进入 Writer 的显式投影；`purpose/summary` 默认仅 Planner/Critic 可见，不能靠模型猜测脱敏。节点保护只约束 Agent：`preserve-intent` 不得修改 purpose，`preserve-arrangement` 不得改 parent/position/kind；作者可以显式改动。精确文本锁仍使用现有 Artifact 整体锁与 SceneGroup 锁，不再新增节点 `locked`，避免多重锁权威。

`{nodes}` 是唯一受支持格式，不提供旧 Outline 转换或双读路径。上限统一为 200 节点、单字段 4,000 个 Unicode 标量、每节点 50 个依赖键、编码后的单个规划产物总计 48 KiB；HTTP 在读取 body 前拒绝超过 512 KiB 的请求，HTTP、MCP 与 UI 使用同一校验错误。Context 工具总预算有硬上限，结果拆成每项不超过 9,000 UTF-8 bytes 的内容块，既容纳一个合法核心产物，也不产生超过模型单项上限的工具片段。

`RollingPlanPayload` 使用 `sourceOutlineRevisionId`；章纲包含稳定的 `chapterPlanId`、唯一叶锚点 `primaryNodeId`、`publicTitle?`、`povCharacterId?`、`storyTime?`、`writerGuidance?`、`publicCommitments[]`。`title/goal/conflict/dependencyKeys/allowances` 为 Planner/Critic 私有字段，不进入 Writer；Writer 只接收显式公开字段。祖先只能由 Runtime 从同一 Outline 修订推导，payload 不接受含混的 `parentNodeIds[]`。保存时验证来源是当前作品的当前正式 Outline；没有正式 Outline 时不能创建正式 Plan，但作者仍可走 localOnly 直接写章。下一章详细、后两章粗是 UI 默认，不作数据库硬限制。

新增 `chapter_plan_bindings(project_id, chapter_plan_id, chapter_id, materialized_by_commit_id)`，数据库同时约束 `UNIQUE(project_id, chapter_plan_id)` 与 `UNIQUE(project_id, chapter_id)`，关系严格一对一。章纲可以先于实体章节存在；author-only `materializeChapterFromPlan` 产生 `materialize-chapter` 工作区提交，原子创建实体章、binding 并推进 project revision；相同幂等键或已有 binding 返回同一提交结果。物化成功后 UI 才创建 Writer task，因此旧任务按 revision 失效。章节号与标题不作为关联依据；同作品重复章节号拒绝。

同一逻辑章跨 Plan 快照必须保留 chapterPlanId，不得改绑或复用；重排只改 position/number。已绑定条目在 AC-1 不允许删除或换 ID，未来拆并章另设 author-only 迁移提交；未绑定条目可删除。`generateChapter` 以 chapterPlanId 为入口，Runtime 解析为唯一 chapterId，Draft 来源永远由实际 binding 生成。

无规划直写使用同一个 generateChapter 的 `localOnlyTarget` 分支。author-only `materializeStandaloneChapter` 先创建不带 binding 的实体章与结构提交，再创建 Writer task；Runtime 生成局部快照 `{chapterId, projectRevision, localOnly:true}`，不伪造 Outline/Plan。独立稿采用、重试、撤销遵循普通 Draft/commit 规则。随后作者有三条明确出口：保持独立；通过 author-only `associateStandaloneChapterToPlan` 绑定尚未绑定的 chapterPlanId；或从正文打开 Planner，创建新的 Outline/Plan 候选。关联操作推进 revision、双向唯一且可在无下游依赖时撤销。

`DraftRevision` 增加 `source_plan_refs_json`，规范化保存 `{ planRevisionId, outlineRevisionId, chapterPlanId, primaryNodeId, ancestorNodeIds[] }`。Agent 写入只能从任务 manifest 派生，作者手写时只能二选一：采用当前正式规划，或 `localOnly=true`；不能由调用方自由声称任意 revision。SceneGroup 使用 Runtime 生成的 sceneId 和同章唯一连续 position；继续场景只允许读取 position 小于目标的组。

所有 Artifact/Draft 候选写入强制显式提供 `expectedCandidateRevisionId`（首次保存为 `null`）和 `idempotencyKey`。Runtime 在 `BEGIN IMMEDIATE` 内同时比较正式 project revision 和 candidate pointer；任务 manifest 固定启动时的候选头，旧任务不得覆盖作者或另一任务后来保存的候选。`candidate_write_requests` 按 author 或具体 task 记录幂等键、request hash 与结果；不同任务即使使用相同键也不会复用对方结果，同一作用域同键异参拒绝。任务完成还要确认最后输出仍是对应 artifact/chapter 的当前 candidate。

新增 `planning_impacts`，不再扩张旧表：记录稳定 `impact_chain_id`、`supersedes_impact_id`、`source_commit_id`、`source_draft_revision_id`、`source_state_event_id`、最初 `source_planning_revision_id`、`target_artifact_kind=outline|plan`、`target_revision_id`、`target_node_id`、`target_label`、`dependency_key`、`severity=affected|locked-conflict`、`state=open|superseded|resolved-kept|resolved-updated|withdrawn`、解决说明与提交。数据库约束 `UNIQUE(supersedes_impact_id)` 与 `UNIQUE(impact_chain_id, target_revision_id)`，事务校验每条链最多一个 open 叶节点；source event 与最初 baseline 在后继链保持不变。删除被 open impact 指向的节点会被拒绝，除非作者先明确 keep 或由后续 impact 迁移取代。

`planning_impact_links(impact_id, candidate_revision_id, linked_by_author_at)` 是 author-only 命令元数据，不写进 Outline/Plan 领域 payload。Agent 不能声明或建立 link；作者保存候选或随后执行专用 link 命令时，Runtime 比较目标正式节点与候选节点，只有目标确实变化且 kind/node 完全匹配才建立 link。采用该候选后才能转为 resolved-updated。resolved-kept 只能由作者单独提交。重复确认同值不会生成无限 impact。

正式操作统一使用扩展后的 `acceptance_commits`：重建 CHECK 以支持 `accept|materialize-chapter|materialize-standalone|associate-plan|resolve-impact|undo`，receipt 记录章节/binding、impact 前后状态和本次创建的 impact ID。materialize-standalone 幂等创建空独立章；associate-plan 原子建立双向唯一 binding。两者都推进 revision，并且只有在没有 Task/Draft/Review/passage 或其他下游依赖时才能按最新提交 undo，否则返回 undo_has_dependencies。`kept` 推进 project revision，可在它是最新正式提交时撤销。现有锁操作沿用既有语义，本阶段不承诺锁操作可撤销。

schema v5 是唯一受支持数据库形状。建库逻辑、fixture 和测试直接使用 v5；schema 改变时重建测试数据库，不实现 v1/v4 升级、历史任务回填、备份矩阵或数据库降级。

### 4.2 任务输入、完成和上下文

TaskRun 增加 `operation` 与 Runtime 生成的 `input_manifest_json`。首批唯一矩阵为 `generateBrief→planner/brief/project`、`generateOutline→planner/outline/project`、`generatePlan→planner/plan/project`、`generateChapter→writer/draft/chapter`；删除未定义产物的 reviewDraft。operation 是唯一权威，调用方不能另传 purpose/output kind。请求为判别联合：前三种只接受 intent；generateChapter 只接受 writerInstruction，并要求 `plannedTarget{chapterPlanId}` 或 `localOnlyTarget{chapterId}` 二选一。Manifest v1 固定 `{ operation, projectId, projectRevision, target, planningBaseline?, localOnlySnapshot?, expectedCandidateRevisionId, permissionPolicy, branchContext:null }`，在创建任务的同一事务从正式指针生成。

`PlanningBaseline` 是 manifest 内不可变的 `{ outlineRevisionId, planRevisionId }` 对，不是第三套可修改的正式指针。每个规划相关对象同时拥有两个正交维度：`lineageStatus=current|historical` 表示使用当前正式规划或章节绑定的历史快照，`impactState=clean|review-required` 只表达哪些稳定节点语义待复核。“无关节点不受影响”仅指不产生语义 impact、不改写内容，不表示旧快照仍是 current。

| 正式提交 | accepted Plan | Plan candidate | 活动 Task | Draft candidate | ReviewBundle | 可执行动作 |
| --- | --- | --- | --- | --- | --- | --- |
| accept Outline | sourceOutline 不同则 lineage stale；依赖闭包命中的条目 review-required | 基于旧 Outline 的 candidate stale | 引用旧 Outline 的全部 cancelled | 基于旧 baseline 的 draft stale、只读 | 基于 stale artifact/draft 的 review stale | 先生成并采用新 Plan，或新建 localOnly Draft |
| accept Plan | 新 Plan current；命中项可 review-required | 旧 candidate stale | 引用旧 Plan 的 Writer task cancelled | 引用旧 Plan 的 draft stale、只读 | 引用旧 Plan 的 review stale | 基于新 baseline 新建 Writer task |
| materialize chapter | accepted 内容与 baseline 不变 | 物化前已有 candidate 全部 stale、只读 | 物化前同 revision 的旧任务 cancelled | 物化前已有 draft candidate stale、只读 | 物化前已有 review 全部 stale、只读 | 物化提交后按新 project revision 创建 Writer task |
| accept Draft | PlanningBaseline 不变 | 不改变 lineage | 同 project revision 的活动任务 cancelled | 被采用 draft current | 对应 review accepted | 为命中稳定节点创建 open impacts |

Plan 只有在 sourceOutlineRevisionId 等于正式 Outline 时才是 current。Context 和写入只接受同一 frozen baseline，禁止混用当前指针与历史 ID。语义影响通过稳定 ID 和 dependencyKeys 选择性产生；lineage stale 是形式一致性的全局边界。

materialize-chapter 允许撤销，但仅当该章没有 Task、DraftRevision、ReviewBundle、正文 passage 或其他 binding 依赖；否则返回 `undo_has_dependencies`，禁止依赖级联删除。成功撤销删除空章节和 binding，并按 receipt 恢复 revision/状态。

默认主旅程是可恢复的显式状态机，任何复合 CTA 都不能代替作者采用：

| 当前状态 | 主 CTA | 完成后 | 失败/关闭应用后的恢复 |
| --- | --- | --- | --- |
| 只有一句构想 | 生成创作方向 | Brief candidate | 查看已保存候选或重试；结果未知则先刷新 candidate pointer |
| Brief candidate | 审阅并采用方向 | Brief accepted | 本地编辑保留；CAS 冲突可比较/复制为最新版 |
| Brief accepted | 生成层级大纲 | Outline candidate | 展示 partial/candidate，不自动进入下一步 |
| Outline candidate | 采用大纲并生成近三章 | 先完成 author accept，再启动独立 generatePlan | 后半步失败时停在“正式大纲/待生成章纲”，可继续 |
| Plan candidate | 审阅并采用近三章 | Plan accepted | stale 时可复制为新 baseline candidate |
| Plan accepted | 开始写第一章 | materialize commit → 新 Writer task | 物化成功而模型失败时保留空章，提供继续/撤销物化 |
| Draft candidate | 审阅本章 | 事实与方向决定齐备 | partial、未知结果、stale 各自显示继续/查看/复制 |

无规划空态提供“直接写章”：materializeStandaloneChapter → generateChapter(localOnlyTarget)。页面明确显示“无上级规划，结构反馈只来自作者手动关联”，并在采用后提供保持独立／关联章纲／从正文生成规划三条出口。

所有 stale 候选提供“复制为基于最新版的新候选”，通过三方差异展示旧 baseline、最新版和作者候选；不静默重挂。稳定 nodeId/chapterPlanId 保留，作者只修改冲突项。CAS 409 不清空表单，提供比较、复制、重新加载。反馈处理的推荐顺序是先采用 Outline 修改，再从旧 Plan 复制到新 Outline baseline，只改受影响章并采用；核心演示必须真实走完该顺序。

UI 为 Brief／Outline／Plan 提供专用生成入口，章节使用章纲入口。自由 Composer 只做对话／建议；不能以中文关键字授予领域写入用途。`propose_plan_revisions` 从 AC-1 的允许工具中移除，待未来重新按 impact command 契约设计。

新增不可变 `task_outputs(task_id, ordinal, output_kind, revision_id, completeness)`；ordinal 在任务事务内单调递增，Agent 不能自报。每次成功候选保存后由 Runtime 写一条；`output_refs_json` 只作显示兼容。`complete_task` 只检查最大 ordinal：项目、目标、类型、当前 candidate、非空正文以及所需 completeness 都匹配才完成；最新 partial 必须让任务保持 incomplete。

Writer Context 根据 task manifest 固定作品 revision、PlanningBaseline、目标章、sceneOrder、POV、storyTime 及来源清单；任一不匹配直接 stale 错误，不静默向下取整。返回 `planningContext`：当前章计划、primary 节点及其祖先的 `writerGuidance` 和精确修订 ID；不注入全树全文。当前章 source excerpts 按已验证的 scene position 截到目标位置，无法证明位置时不提供当前章正文。

适用于任务的全部 `hardConstraints`、当前章 commitments、保护安排和人工编写的中性 forbidden policy 先完整计算，再作为不可裁剪区；不允许 SQL LIMIT 先行截断，预算不足在模型调用前 fail-closed。候选事实和原文摘录可按优先级删减并报告。秘密记录整行、predicate、答案、摘要均不得进入 Writer 查询或错误。

安全威胁模型覆盖模型可见 prompt/MCP 结果、工具调用、同应用其他任务历史、记忆、日志/错误/缓存；不覆盖已入侵的本机用户账户或直接读取进程内存。每个领域生成任务启动独立、短生命周期的 Codex app-server＋MCP 进程和全新线程，完成/取消即终止；turn/start 传 `environments: []`、`approvalPolicy: never`。专用 config 同时设 `web_search="disabled"`，禁用 collab/multi_agent/multi_agent_v2、tool_search、插件建议、记忆、skills 与动态工具，只配置 novel_workspace 一个 MCP server。

发布物包含 `codex-runtime/supported-cli.json`，逐平台固定 Codex CLI version、SHA-256、允许的 provider/model 组合与 model-metadata 指纹；发布暂存步骤必须为当前目标平台复制 CLI、Runtime sidecar 和模型目录，并在发布资源内写入真实 bundled 摘要。同时携带经过审计的只读 `model-catalog.json` 和 Codex 格式的 `codex-model-catalog.json`，基础 instructions 控制在 1K token 以下，专用 config 以 `model_catalog_json` 固定模型工具能力，领域生成不读取远端动态 catalog。Electron 取得单实例锁后才启动 Runtime 和回收孤儿任务；每次启动原子覆盖隔离 config/catalog。领域生成前校验真实路径、版本、摘要、provider/model 与本地 catalog 指纹，未知/漂移组合拒绝生成；DeepSeek 等组合未逐项审计前只允许自由对话，不允许领域写入。MCP 候选写入成功只向模型返回修订、检查点和 CAS 元数据，不回显刚提交的正文或完整 payload。本地契约测试验证这些拒绝路径；捕获真实模型 `/responses` 仍是发布 canary 证据。

预期完整工具集合（使用该版本实际 canonical 名）为：generateBrief=`mcp__novel_workspace__read_context|mcp__novel_workspace__save_brief|mcp__novel_workspace__complete_task`，generateOutline=`mcp__novel_workspace__read_context|mcp__novel_workspace__save_outline|mcp__novel_workspace__complete_task`，generatePlan=`mcp__novel_workspace__read_context|mcp__novel_workspace__save_chapter_plans|mcp__novel_workspace__complete_task`，generateChapter=`mcp__novel_workspace__read_context|mcp__novel_workspace__save_draft_checkpoint|mcp__novel_workspace__submit_fact_proposals|mcp__novel_workspace__complete_task`。MCP sidecar 依 operation 只在 tools/list 发布该集合，本地契约测试要求 tools 与集合全等。发布 canary 还要捕获每个获准 provider/model 的真实 `/responses` 请求；出现 core utility、web、collaboration、资源枚举、未知或新增工具即不允许登记新支持组合。运行时不允许用户为领域生成切换任意 CLI/catalog/model。

taskCapability 使用首轮调用前顺序激活：Electron 生成 256-bit capability，把明文只注入该任务的 MCP sidecar 环境；此时 Runtime 没有对应 hash，凭证不可用。app-server 创建 thread 后，author gateway 在同一 `createTask` 事务写入 capability hash、30 分钟 expiry、project、thread、operation、input revision 与 manifest；成功后才发送首个模型 turn。Renderer、模型和 prompt 均看不到明文；sidecar 在每次 Runtime HTTP 请求的专用 header 附带它。任务完成、失败、取消、过期或规划结构使任务失效后 capability 立即失效。启动、thread 创建或 task 创建失败时不发送模型回合并终止／轮换 sidecar。Context 只通过 read_context 一条路径注入，handoff 不嵌入 Context。

Writer 投影是机械字段白名单：OutlineNode 增加 `publicTitle`（默认空）与 writerGuidance；ChapterPlan 增加 `publicTitle`、`publicCommitments[]`、writerGuidance、POV、storyTime。Writer 只接收这些字段；所有未在白名单中的字段默认不输出，尤其不输出内部 title、purpose、summary、goal、conflict、dependencyKeys、allowances、Impact label/元数据或 author intent。generateChapter 只接受 writerInstruction，并明确提示作者：主动写入该字段的内容会发送给 Writer，不属于“系统保存的私密信息不泄露”保证。先在其他 Planner 线程、记忆和本地状态植入 canary，再检查新 Writer 的完整 input、tools、MCP、错误和日志。任一校验无法满足时模型调用前 fail-closed。

### 4.3 正文反馈到各级规划

数据流：章节候选 → Draft Review 中作者确认实际变化／事实 → 采用正文及被选事实 → Runtime 以 Draft frozen baseline 的显式 dependencyKeys 计算 impacts → 工作区按目标层级显示“待复核” → 作者打开节点手工修改 → author-only 命令选择拟处理 impacts 并建立 link → 审阅采用且服务端确认目标真实变化后，只关闭 link 对应 impacts。Agent 保存候选不能建立或暗示已处理 link。

Draft Review 必有“实际发生了什么”步骤：模型建议与作者新增分栏，逐项显示正文证据、事实种类与“建议/作者确认”状态；作者必须逐项确认或忽略，也可显式选择“本章没有要确认的变化”，空列表不能伪装已检查。Runtime 只采用作者确认项，语义推测仍标建议。依赖键没有命中时显示“尚未找到关联”，作者可选择具体章纲、单元、卷或全书节点建立 `author_fact_dependency_links`；该 link 记录 author actor、fact proposal、target revision/node 和时间，在正文采用事务中产生 impact。验收覆盖无预设键、键不一致、多节点命中、手动补链与拒绝误报。

在采用正文前，Runtime 基于候选事实与 frozen baseline 生成 provisional impact preview。所有已识别的 locked-conflict/重大方向改向必须有 `DirectionChangeDecision=return-draft|accept-and-open-impact|false-positive`；未决定时阻止采用。选择 accept-and-open-impact 会与正文采用写入同一幂等 receipt，选择 false-positive 留作者说明；undo 一并恢复。普通 affected 可采用后再处理。系统明确只阻断已识别项，不宣称发现全部语义冲突。

反馈是双向的：正文事实向未来 Plan、单元、卷、全书产生 planning impacts；而 Outline/Plan 采用按状态矩阵使旧 lineage stale，并通过稳定 node identity、祖先边和 dependencyKeys 只给命中内容标 review-required。树的 parent 边只表达结构，dependencyKeys 表达语义依赖，不能互相代替。

open impact 固定指向产生时的 target revision。每次采用新 Outline/Plan 时，同一事务迁移旧 revision 的 open impacts：已 link 且目标真实改变则 resolved-updated；未 link 且稳定目标仍存在则旧 impact superseded，并创建带 `supersedesImpactId`、指向新 revision 的后继 open impact；目标仅移动但 ID 不变按“仍存在”处理；目标被删除则除非已 link 且明确修改、或已 kept，否则拒绝采用。后继链保留原 source event，唯一约束覆盖 supersedes 链，保证无关修订后反馈仍存在。

影响卡展示：来源章节和正文证据、变化 predicate、当前规划节点、为何命中、保护状态。对全书／卷／单元的重大变化必须单独处理；采用正文不能顺带改规划。作者可以：修改规划、保留规划并把反馈标记为已审阅／不调整，或退回正文。首批不让模型自动写改纲候选；“无调整”必须留下作者决定和时间，不能由 Agent 自行消除。

为支持“保留规划”，增加 author-only `resolvePlanImpact(impactId, resolution=kept, note, expectedRevision, idempotencyKey)`。`updated` 不是公共参数，只能在采用已 link 且真实修改目标的新 Outline／Plan时由事务写入；`kept` 是单独作者提交并可按最新提交撤销。若 impact 为 locked-conflict，kept 只表示作者知情保留，UI 永久保留冲突标识，不伪装为语义一致。

### 4.4 API、MCP 与 UI

- HTTP／IPC：扩展 saveArtifact／saveDraft payload；新增 materializeChapterFromPlan、materializeStandaloneChapter、associateStandaloneChapterToPlan、分页 planningImpacts、author-only resolvePlanImpact；显式 Agent 请求使用判别联合 `PlanningGeneration{projectId,operation,intent}` 或 `ChapterGeneration{projectId,operation:'generateChapter',writerInstruction,target:plannedTarget{chapterPlanId}|localOnlyTarget{chapterId}}`。保持 Renderer 无 token、Runtime 权威校验。
- MCP：更新 outline／plan／draft schemas；任务 operation 限制工具；移除 AC-1 对 `propose_plan_revisions` 的授权。
- UI：Outline 页为分层卡片，支持添加全书／卷／单元、选择父级、排序、状态、保护和依赖键；不要求补齐全部层级。Plan 页为每章选择关联上级节点。Chapter 页显示“本章依据”及反馈事实。Outline／Plan 页分组显示本层 impacts，提供“编辑后处理”与“保留原规划”。
- 状态：空态提供“从构想规划／直接写章／继续作品”；加载、保存候选、审阅、采用、影响待处理、冲突、失败、恢复都有文字；状态不只依赖颜色。树形控件使用语义列表／组和明确 label，首批不实现复杂拖拽，以按钮排序保证键盘操作。
- 写作前展示“本章将使用的公开规划”预览；writerGuidance/publicCommitments 全空时必须由作者确认“无规划指导写作”才可启动，不能因为技术上存在 revision 引用就伪装获得了创作指导。

本轮体验修复采用显式场景编辑：新增时生成稳定身份，正文编辑与换序不按内容猜测身份；普通保存和生成期间锁定相关编辑操作。历史任务归属使用 author-only、有界批量 `tasks/lookup` 查询，与最近任务展示窗口分离。局部场景任务只提交授权前缀，由 Runtime 保留不可见尾部；有尾部时前缀不得缩短以改变可读边界，候选 HTTP/MCP 回包只含元数据。实际回归见 [作者体验修复记录](AUTHOR_EXPERIENCE_REPAIR_REVIEW.md)。

前端为 Outline、Plan、Draft 各维护独立 `EditorSession{baseCandidateRevisionId,buffer,dirty,serverMetadata}`。后台刷新和任务完成只更新 serverMetadata，不覆盖 dirty buffer；切换页/章/项目和 Electron 关闭窗口时提供“保存候选／放弃／留在此处”。任何会使候选 stale 的 materialize/associate 操作先列出受影响候选，存在 dirty session 时阻止。409 显示旧 baseline、服务端最新版、本地稿三方比较，并可直接以服务端最新版为 baseline 保存本地稿为新候选；重新加载前仍可复制或导出本地稿，保存成功不抢焦点。

默认 UI 不暴露 dependency key 字符串：用“关联的事实／安排”搜索选择器展示自然语言标签和全书→卷→单元→章路径，技术 key 仅在高级详情折叠区。影响卡固定提供“打开并修改节点／规划不调整／返回正文”；打开节点后在领域 payload 外维护待处理选择，持续显示“本次拟处理 N 条反馈”，采用前再确认。无匹配态提供节点选择或“不相关”，显示总数、分页、各层 open/locked-conflict 计数，任何冲突不能因折叠或分页隐藏。

生成任务创建后固定显示“停止生成”。停止只取消任务、撤销 capability 并终止独立进程，不删除已保存 checkpoint；界面区分“已停止／部分稿已保存”“结果未知／正在核对”“失败／无有效输出”。停止后提供继续此章、审阅已保存部分、撤销空章；结果未知必须先按 task/idempotency key 查询任务和 candidate head，再允许重试。应用重启恢复同样先核对，不盲目重复生成。

层级编辑使用语义嵌套列表、标题和普通按钮，不模拟复杂 ARIA tree。新增节点后聚焦标题；移动后焦点留在该节点；删除后聚焦下一兄弟，否则父节点；移动按钮的 accessible name 包含节点与方向。422/413 关联具体字段并设置 aria-invalid/aria-describedby，提交失败聚焦首错且顶部错误摘要可跳转；禁用原因以邻近可读文本/aria-describedby 表达，不依赖 title；保存与任务状态用 aria-live 宣告。

### 4.5 安全、性能、错误与恢复

- 所有 revision／node／chapter／impact 引用在 Runtime 校验项目归属、当前正式指针和 target；Agent 只能保存候选。
- 循环／重复位置／跨项目引用返回 422，过期正式版本、候选头或 planning baseline 返回细分的 409；payload/body 超限返回 413；unmaterialized chapter plan 返回专用 409。
- 采用与 impact 解决均使用 `BEGIN IMMEDIATE`、request hash 和幂等键；索引失败不回滚正式提交。外部模型调用不承诺 exactly-once。
- 当前 schema 的建库和事务必须通过外键检查。索引失败不回滚已完成正式提交，而写入 `index_recovery_queue` 并在启动时重试。Windows／macOS 路径不硬编码，Runtime 测试可在 Linux 运行。
- 日志只记录 ID、版本、操作、目标、长度和错误码，不记录私密正文、秘密或凭据。
- 默认只保存在本地的产品事件记录：规划各阶段耗时、stale candidate 复制、正文实质重写次数、impact 命中/手动补链/误报/kept、改纲处理耗时、localOnly 去向和中途放弃阶段；不记录正文或秘密。设置页可查看并导出匿名汇总，用于验证是否降低返工，而非以字数冒充创作质量。
- 2026-09-13 度量口径细化：当前以固定本地聚合实现。阶段耗时具体为采用候选的保存到采用历时；正文返工以作者内容变更保存为代理，stale 操作以旧基线候选续存为代理，不推断实质重写或复制动作。放弃由作者明确记录；统计设置解释这些限制。无正文的聚合不足以判定文学质量，该目标仍需作者试用验证。详见 [交接、导出与指标验收](AUTHOR_HANDOFF_EXPORT_METRICS_REVIEW.md)。

## 5. 测试、验收与发布

Runtime 集成测试至少覆盖：当前 v5 空库初始化；层级跳层／环／重复 clientKey/未知 parentClientKey/幂等 ID 映射；Plan 引用当前 Outline；binding 双向唯一、并发物化、ID 复用、已绑定项删除/重排；planned/standalone 物化与 associate-plan 的幂等回执；物化推进 revision 后所有旧 candidate/review/task stale；无依赖结构提交可撤销、有 Draft 时返回 undo_has_dependencies；候选 CAS、双连接并发与响应丢失重试；Draft 跨项目／候选／过期／错章拒绝；task output 最大 ordinal 与最新 partial；Planner 必需输出；精确 baseline；当前场景不读后文；全部硬要求不裁剪；事实分别命中未来章、单元、卷、全书；Outline 变化使 lineage stale 且只给命中节点语义影响；每条 impactChain 最多一个 open 叶；无关修订后 open feedback 仍存在；目标移动 ID 不变；未处理目标删除被拒绝；locked-conflict；真实修改才处理 impact；逻辑去重、分页、keep 幂等与最新提交撤销；索引恢复。

安全集成测试使用 canary 秘密：先写入另一个 Planner 线程、记忆和本地状态；断言生成 turn 为 environments:[]，专用 config 禁用 web/collab/tool-search/memory/plugins，实际 `/responses` tools 精确等于 operation 的 novel_workspace allowlist，handoff 不含 Context，Writer 的完整 input/MCP/错误/日志没有 canary；覆盖 taskCapability 重放、跨任务/线程/operation、取消后使用与日志脱敏。任何未知工具或空 environment 不受支持时，模型调用前失败。

Electron／前端验收：显式 operation 契约；“写卷纲”不获得 Writer 权限；从空项目完整走通构想→方向→层级大纲→近三章→物化第一章→正文；无规划直写走通保持独立/后续关联；Draft Review 走通无键命中和手动补链；重大冲突在采用前做决定；改纲后从 stale Plan 复制并沿用稳定 ID；章显示来源。另有逐步阻塞脚本：仅用键盘新增/移动/删除节点并核对焦点；制造 422/413/409 并核对字段错误、三方比较和本地文本不丢；后台刷新/切页/关窗不覆盖 dirty；停止生成后审阅 partial；重启恢复结果未知；模型完成未保存；物化成功但生成失败及撤销空章；分页外 locked-conflict 仍有全局计数和入口。首批若不引入 React 测试框架，这些都是阻塞发布的人工走查，不宣称自动覆盖。

核心演示：父女晚餐。全书、当前卷、当前单元各声明依赖 `daughter-trust`；第一章计划引用三个节点并保护“本卷不揭身份”。正文采用确认“女儿由信任转为试探”，系统同时产生章／单元／卷／全书四类待复核；作者修改单元与后两章、保留全书方向，系统只关闭明确处理项，保护安排不被改写。

验证命令：`npm run test:runtime`、`npm run test:agent`、`npm run build`，Python／Node 语法检查；有可用桌面环境时完成 macOS UI 走查。Windows 未实机验证时明确列为发布剩余项。

实现按职责拆入 `planning.py`、`planning_bindings.py`、`impacts.py`、`impact_review.py`、`impact_lifecycle.py`、`review_decisions.py` 与 `context_limits.py`；前端使用 `OutlineTreeEditor` 和创作产品纯逻辑模块，工作区组件保留页面编排。AC-0a（task output、CAS、严格完成）→ AC-1a Runtime（层级、binding、来源、context、安全域）→ AC-1a UI → AC-1b（双向反馈、处理与撤销）已按当前断代契约整体启用，不保留旧入口或兼容 feature flag。

## 6. 五角色评审

设计阶段五角色已批准本方案。实施阶段重新由架构、两位技术、产品和 UI/UX 角色审查；发现的 capability、operation 权威、Context 单项上限、Plan 元数据、Review 越权、精确 revision、无规划确认、三选离开和反馈可理解性问题均已修复。最终逐角色结论和证据见 [AUTHOR_DIRECTED_IDEAL_REPAIR_REVIEW.md](AUTHOR_DIRECTED_IDEAL_REPAIR_REVIEW.md)。

## 7. 综合决策

采用 1.1 当前方案。AC-0／AC-1 实现门禁通过；进入真实模型、目标作者、Windows 与安装包发布验收。

## 8. 实施后复核

已实施。当前代码、验证计数、桌面走查和发布证据边界见 [AC01_IMPLEMENTATION_STATUS.md](AC01_IMPLEMENTATION_STATUS.md)。
