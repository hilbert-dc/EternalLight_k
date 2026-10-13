# Product Next 落地方案与五角色评审记录

> 现行覆盖规则：本文件中的旧数据库升级、历史数据回填和兼容要求已由 [测试阶段工程策略](../TEST_STAGE_ENGINEERING_POLICY.md) 废止，不得作为当前实现或验收项。

版本：0.5 · 日期：2026-09-12 · 状态：代码阶段已实施，评审阻断项已闭环；发布验证待完成

关联需求：[PRODUCT_NEXT.md](../docs/designs/PRODUCT_NEXT.md)；讨论依据：[PRODUCT_DISCUSSION.md](../docs/designs/PRODUCT_DISCUSSION.md)。

## 1. 变更概览

- 变更名称：单作者小说创作闭环（M0～M3 第一条完整纵向切片）。
- 背景/问题：现有 Runtime 只有固定四步执行计划、基础上下文和四种 ChangeSet，UI 主要展示 Canon、章节与 Agent 事件，无法把大纲、近期计划、草稿、候选事实、采用和局部重规划串成产品流程。
- 目标：在当前 Electron/React + Python/SQLite 架构中，完成可运行的“直接写章或建立大纲 → 近期计划 → 草稿 → 候选事实审阅 → 原子采用 → 撤销 → 标记受影响计划”闭环。
- 非目标：一键生成整本、多候选并列综合、多人协作、漫画、自动文学评分；不把无模型时的模板内容伪装成 AI 生成。
- 假设与约束：工程处于测试阶段，直接替换旧契约和旧业务逻辑，不维护双轨兼容层或一次性数据升级；测试数据库可重建；macOS/Windows；所有写入按 projectId、来源修订和幂等键校验。
- 影响范围：SQLite schema、Python Runtime/API、Electron IPC、React 类型与中央编辑工作区、测试与架构文档。

## 2. 开工状态（历史）

- `runtime/storage.py` 保存项目、Canon、Scene/Beat、章节正文、知识和依赖，但正文只有当前值，知识记录会被覆盖。
- `runtime/novel_workflow.py` 的 `create_story_plan` 是 Execution Plan；`apply_change_set` 无基线版本冲突和撤销。
- `electron/main.mjs` 只把工作包注入 Agent，不提供创作产物专用 IPC。
- `src/App.tsx` 已有左侧导航、中央内容、右侧对话，但没有大纲、近期计划、草稿审阅工作台。

## 3. 候选方案

| 方案 | 优点 | 缺点/风险 | 结论 |
| --- | --- | --- | --- |
| A. 扩充现有四种 ChangeSet | 文件少 | 把执行日志、草稿和正式事实混为一体，生命周期继续含糊 | 不采用 |
| B. 新增 Creative Workspace 唯一领域写边界；旧正式写入口删除或改为创建草稿 | 状态边界清楚，可独立测试，UI 可直接消费 | 新增数据表、MCP 与 API；需要一次纵向集成 | 采用 |
| C. 先做纯前端模拟 | 快 | 刷新丢失，无法证明原子采用、版本冲突和隔离 | 不采用 |

## 4. 推荐方案

### 4.1 架构与模块边界

- `runtime/creative_workspace/`：按 `planning`、`drafting`、`review`、`context`、`impact`、`repository` 分模块；`service.py` 只编排用例。它是唯一领域写入口，不把逻辑继续堆进 `storage.py`。
- `storage.py`：只增加 schema、初始化和项目状态投影；保留现有项目和正文数据。
- `server.py`：统一 stdlib/FastAPI 路由语义，新增 `/creative-workspace`、`/outline-nodes`、`/chapter-plans`、`/drafts`、`/review`、`/accept`、`/undo`。
- Electron preload/main：映射结构化 Runtime API；IPC 校验类型、长度、枚举和 ID，Runtime 再校验归属和业务不变量。
- `src/features/creative-workspace/`：client、状态协调器、Outline、Plan、Draft、Review 分组件；中央编辑，右侧对话和当前审阅包联动。

### 4.2 数据模型

- `project_revisions(project_id, revision)`：每次正式采用递增，用作乐观并发基线。
- `creative_briefs`：作品简述与风格/禁改项。
- `outline_nodes`：Story/Volume/Arc 节点，稳定 ID、父节点、顺序、正文、锁定、状态与版本。
- `chapter_plans`：章节号、目标、POV、冲突/选择/后果、信息边界、场景概要、细化程度、锁定、失效原因、版本。
- `draft_revisions`：章节、正文、来源计划版本、base project revision、draft/review/accepted/rejected 状态、父修订。
- `fact_proposals`：来源 draft、事实键、世界事实/角色信念/未知类型、状态、故事时间、阅读位置、证据、candidate/confirmed/rejected。
- `acceptance_commits`：幂等键、基线与结果修订、正文前后、事实变化、受影响计划、applied/undone；联合提交和撤销均可追溯。

### 4.3 控制流与错误恢复

1. 作者直接新建空章/计划/草稿，或通过右侧 Agent 调用候选权限的本地 MCP 工具结构化生成并保存；无模型时只建立空产物，不伪造正文。
2. “进入审阅”冻结草稿版本，按显式事实候选运行硬检查；作者可修正、拒绝、标未知/信念。
3. “采用”在单事务内校验 project、baseRevision、草稿版本、锁定与候选证据，写正式正文、确认事实、提交记录、revision+1、标记依赖计划过期。
4. 相同幂等键返回原回执；冲突返回结构化 409，不覆盖手改。
5. 最近且无后续提交的采用可撤销；撤销是新操作，恢复正文与事实并保留记录。搜索索引事务后重建，失败显示 stale，不冒充最新。

### 4.4 UI 信息架构

- 左侧保留作品导航；中央新增“故事大纲 / 近期计划 / 正文”切换。
- 空态提供“直接写第一章 / 设计大纲 / 粘贴已有章节”；按钮只创建真实空产物或将指令送入 Agent。
- 大纲：树状节点、行内编辑、锁定和前后修订。
- 近期计划：默认三章卡片，下一章可展开 Scene/Beat；锁定和失效状态不只靠颜色。
- 正文：正式稿/草稿切换、保存状态、版本冲突；审阅抽屉列出候选事实及证据，提供拒绝/未知/信念/确认。
- 采用按钮明确“采用正文与事实”；草稿保存不等于正式采用。加载、空、失败、成功、撤销均有文案；Enter/Shift+Enter 保持现状并尊重输入法。

### 4.5 安全、性能、跨平台和观测

- 所有读取和写入用 projectId 复核关联关系；正文和 Prompt 有长度上限；不把 API 密钥写入项目。
- SQLite 开启外键并使用事务；列表按项目和顺序建立索引；检索范围先过滤后 limit。
- 路径继续使用 Electron `userData` 与 Python `Path`，不引入平台 shell 语法。
- API 错误返回 `code/message/details`；任务和提交记录可定位版本，不把模型回复当作提交回执。

## 5. 分段实施与测试

### Stage 1（本轮必须完成）

- M0：版本化草稿、候选事实、联合采用、幂等、冲突、撤销、时间态读取、索引状态。
- M1/M2 纵向闭环：简述、大纲、未来三章计划、空章节/草稿、中央工作区、联合审阅。
- M3 基础：按显式依赖将关联章节计划标为需调整，保留锁定与无关计划。

### 后续增强（不阻塞本轮可运行）

- 自动识别自然语言中的倒叙时间、多候选并列综合、整本导入、真实作者盲评；显式故事时间和真实 Agent 结构化回写不延期。

### 测试与验收

- Runtime 集成测试覆盖：跨项目隔离、历史知识 as-of 查询、旧草稿冲突、重复采用、事务失败零写入、采用后局部失效、撤销、拒绝梦境事实仍可采用正文。
- Server 测试覆盖 stdlib/FastAPI 契约与错误状态一致。
- Electron 测试覆盖 IPC 参数和销毁 webContents 安全。
- 前端执行 `npm run build`；Runtime 与 Electron 测试全通过；用独立 Electron 进程验证三栏核心旅程。

## 6. 第一轮五角色评审

五个独立 AI 角色均给出 `BLOCK`。共同 P0/P1：真实 Agent 工具链被错误延期；正式写入可被旧 API 绕过；作者采用权限未隔离；时间态、迁移、幂等、撤销、TaskRun、影响修订和 UI 恢复状态不完整。本文件 0.2 通过以下第9～16节逐项修订；修订后必须由五角色复审，不把第一轮意见伪装为批准。

第二轮及条件修订后的最终结论（均为独立 AI 角色评审，不是人类签字）：

| 角色 | 最终结论 | 代码前 P0/P1 |
| --- | --- | --- |
| 架构专家 | APPROVE | 无 |
| 技术专家 A | APPROVE | 无 |
| 技术专家 B | APPROVE | 无 |
| 产品经理 | APPROVE | 无 |
| UI/UX 设计师 | APPROVE | 无 |

## 7. 综合决策

- 最终结论：APPROVE；0.2 已解决第一轮全部 P0/P1。
- 批准后实施范围：第15节 S1～S4；发现实质偏离时更新方案并重新评审。

## 8. 实施后复核

- 实际修改：新增不可变 Brief/Outline/Plan/Draft 修订、ReviewBundle、FactProposal、TemporalStateEvent、Acceptance/Undo Commit、PlanImpact、TaskRun、用途隔离 Context 和候选 MCP；删除旧 `novel_workflow.py`、ChangeSet/apply 路由、正文直写与浏览器伪回退。中央工作区已支持结构化编辑、正式/候选比较、场景级选择与保护、事实决定、采用、受依赖约束的撤销，以及 Brief/Outline/Plan 和 Chapter/Scene/Beat 的分层锁定。
- 测试阶段策略：旧 API、旧 Agent 分支、旧写入路径和旧数据库升级逻辑直接删除，不提供兼容适配；fixture 和测试数据按当前模型重建。
- 唯一写边界：Agent App Server 只获得 candidate MCP；author token 只在 Electron 主进程；Runtime token 经 stdin 传递。Shell 子进程由独立环境策略剥离 candidate token 与 Provider 凭据；Renderer 导航与所有 IPC sender 都经过精确来源校验。正式写入只能由经过审阅的 author 操作推进。
- 与方案偏差：首个版本使用按产物的不可变 JSON revision，而未立即拆成完整 `OutlineNode` 树表；正文保护粒度为稳定 SceneChangeGroup，未实现任意字符范围锁定；倒叙故事时间仍依赖显式输入；自动语义文学检查和整本导入未进入本轮。
- 验证结果：`npm run build` 通过；Runtime 27 项通过；Agent/MCP/Electron 安全 23 项通过；主进程和 Python 语法检查通过；macOS 独立 Electron 已从最终源码重新启动并验证 Runtime/App Server 就绪、三栏界面、未保存保护与 Composer 联动。
- 剩余发布门槛：Windows 实机核心旅程、真实模型新作品/改稿样例、A09 完整故障注入、A11 跨重启场景恢复、A15 语义样例和真实作者盲评。未完成这些验证前不声明 A01～A17 全面发布完成。

## 9. 0.2 聚合、不变量与状态词汇表

正式读取模型仍由 `chapters/passages`、Canon 和计划当前指针提供，但只能由 Creative Workspace 提交服务更新。新建项目从当前 schema 直接创建初始 DraftRevision 与 TemporalStateEvent，不定义旧数据转换流程。

```text
CreativeProject(revision, indexTargetRevision, indexedRevision)
├─ Brief ─ currentRevision → BriefRevision*
├─ OutlineNode ─ currentRevision → OutlineRevision*
├─ ChapterPlan ─ currentRevision → PlanRevision* ─ ScenePlan* ─ Beat*
├─ Chapter ─ acceptedRevision → DraftRevision*
├─ TaskRun ─ Checkpoint* ─ output Draft/Outline/Plan/FactProposal revisions
└─ ReviewBundle(source revisions, checks, decisions)
   └─ AcceptanceCommit → TemporalStateEvent* + PlanImpact*
```

不变量：Draft/Brief/Outline/Plan 修订创建后不可变，当前候选指针与 accepted 指针单独移动。ReviewBundle 是按 `artifactKind=brief/outline/plan/draft` 区分的联合类型，固定绑定对应 artifact revision 及依赖修订；Draft 类型额外绑定 FactProposal 与 SceneChangeGroup。编辑产生新修订并使该 artifact 的旧 ReviewBundle stale；Agent 只能创建候选；只有作者权限的 `AcceptReviewBundle` 能移动 accepted 指针。接受 Brief/Outline/Plan 同样产生 AcceptanceCommit 与新 project revision，但不会把计划事件写成已发生事实；接受 Draft 才可追加 TemporalStateEvent。撤销创建新 commit 和新 project revision，不倒退版本号；锁定对象或受保护正文范围在来源版本变化后不能被静默覆盖。

生命周期分别为：Task `queued/running/waiting/completed/failed/cancelled`；Artifact `draft/review/accepted/rejected`；FactProposal 的 `reviewDecision` 仅为 `candidate/confirmed/rejected`；PlanImpact `affected/unchanged/locked-conflict/resolved`。事实另有正交字段 `factKind=world/reader/character/author-secret` 与 `epistemicState=unknown/suspected/known/false`。这些状态不可互换。

## 10. 时间态与 ContextQuery 契约

`temporal_state_events` 为 append-only：`project_id, id, project_revision, fact_kind, subject_id, predicate, value_json, epistemic_owner_id, epistemic_state, story_time, reading_chapter, scene_order, source_draft_revision_id, evidence, supersedes_id, reverted_by_commit_id`。FactProposal 与 ReviewBundle 另存 `reviewDecision`；命题类别、主体、认知状态与审阅决定为正交字段。

`ContextQuery(projectId, projectRevision, purpose, storyTime, readingChapter, sceneOrder, povCharacterId, budget)` 返回 `ContextSnapshot`：硬约束、当前章节/前场、相关事实、远期承诺、证据来源、遗漏说明和水位。查询先按 project/revision、readingChapter、sceneOrder 过滤，再按 purpose 应用策略：Planner 可读作者秘密和未来约束；Writer 只读当前可表达信息及禁止揭示项；Critic 同时读取候选与约束。显式 `storyTime` 决定世界状态，`readingChapter/sceneOrder` 决定读者与角色已知；不以章节号替代故事时间。自动解析倒叙延期，但 UI/API 必须允许显式填写，A05/A06 固定样例验证。

模型线程必须绑定 `project_id`。切换作品新建或恢复该作品已绑定线程；不得把另一作品线程设为当前线程。TaskRun、MCP 工具和恢复操作均校验同一 project。

## 11. 权限、MCP 与唯一写边界

Runtime 使用回环随机端口，并在每次启动生成 `candidateToken` 和 `authorToken`。两者只存在于进程内存：Agent App Server/MCP 环境只得到 candidateToken；Renderer 两者都得不到；Electron 主进程持有 authorToken。所有 Runtime 路由要求认证。

- Candidate 权限：MCP 服务内部持有进程级 candidateToken，但每个工具（包括 `read_context`）都必须携带 `taskId + projectId + threadId`，Runtime 验证三者与 TaskRun 的持久绑定后才读取限定项目的 ContextSnapshot，或创建/修订 Brief、Outline、Plan、Scene/Beat、Draft checkpoint、FactProposal 和局部计划修订候选；不能采用、撤销或移动正式指针。模型本身不获得 candidateToken，Runtime 不接受只有 candidateToken 而没有有效任务绑定的领域调用。
- Author 权限：是 Candidate 能力的超集。无模型时，Renderer 的手工编辑请求经 IPC 交给持有 authorToken 的主进程，主进程可以创建/修订候选产物；在 Renderer 明确点击后还可提交 ReviewBundle 选择、采用和撤销。Renderer 永远得不到 token，Runtime 仍执行版本、归属、锁定、证据和幂等校验。
- MCP：隔离 Codex `config.toml` 配置本地 stdio `novel_workspace` 服务，工具为 `read_context`、`save_brief`、`save_outline`、`save_chapter_plans`、`save_draft_checkpoint`、`submit_fact_proposals`、`propose_plan_revisions`、`complete_task`。主进程先用 authorToken 为当前 project 和当前 Agent thread 创建 TaskRun，再把非秘密的 taskId/projectId/threadId 放入工作包。所有工具必须携带这三个 ID及 expectedRevision，并返回稳定 artifact/revision ID。MCP 不暴露 accept/undo。

旧写入口处理：`update_chapter`、`add_canon_note` 的写 IPC、`novel_workflow.py`、Workflow/ChangeSet/apply 路由和本地伪 Agent 路径已删除，不保留兼容适配。任何 HTTP/IPC/MCP 路径都不能绕过联合采用。

官方 OpenAI 文档确认 Codex 可通过 MCP 服务器扩展工具；具体本地配置形状同时以本仓库 `codex-rs/core/config.schema.json` 的 `mcp_servers` 为实现依据。正式实现验证 `tools/list` 可见上述工具及一次真实工具调用；配置失败时 UI 明确降级为手动模式。

## 12. API、事务与恢复契约

### API/IPC 命令表

| 权限 | 方法与路径 | 必填请求 | 成功结果 | 主要错误 |
| --- | --- | --- | --- | --- |
| candidate/author | GET `/api/projects/{p}/creative-workspace` | author调用：revision/page；candidate调用：taskId/threadId/revision/page | 分页元数据、当前指针、index watermark | 401/404 |
| author | POST `/api/projects/{p}/tasks` | kind,inputRevision,threadId | taskId、状态；MCP凭进程内令牌使用该绑定 | 401/409/422 |
| candidate/author | POST `/api/projects/{p}/artifacts/{kind}` | candidate必须有taskId/threadId；expectedRevision,payload | artifactId、revisionId | 401/409/413/422 |
| candidate/author | POST `/api/projects/{p}/drafts/{d}/checkpoints` | candidate必须有taskId/threadId；parentRevision,body,sceneId | 新 DraftRevision | 401/409/413 |
| candidate/author | POST `/api/projects/{p}/reviews` | artifactKind,artifactRevisionId；draft时可带fact proposals和scene groups | ReviewBundle 与对应 checks | 401/409/422 |
| author | POST `/api/projects/{p}/reviews/{r}/accept` | expectedProjectRevision,idempotencyKey,selections | AcceptanceReceipt | 401/409/422 |
| author | POST `/api/projects/{p}/commits/{c}/undo` | expectedProjectRevision,idempotencyKey | 新撤销回执 | 401/409/422 |
| candidate/author | GET `/api/projects/{p}/context` | candidate必须有taskId/threadId；purpose,time/read position/POV/budget | ContextSnapshot | 401/404/422 |
| candidate/author | POST `/api/projects/{p}/plans/{id}/revision-candidates` | candidate必须有taskId/threadId；impactIds,payload | 可比较的新 PlanRevision 候选 | 401/409/422 |

统一错误体为 `{error:{code,message,details,recovery}}`。稳定映射：400 malformed、401 auth、404 missing/wrong project、409 stale/idempotency/lock conflict、413 size、422 invalid state/check、503 database busy/runtime unavailable。409 包含 currentRevision、conflictIds 和安全恢复动作。Electron 对 project/task/artifact ID、枚举、长度和正文上限先校验，Runtime 进行权威校验；stdlib/FastAPI 调同一 service/DTO，并以同一契约测试参数化。

采用使用 `BEGIN IMMEDIATE`：验证认证与 project 归属 → 先查 `(project_id, operation_kind, idempotency_key)` 并比较规范化 request hash；已存在且同 hash 立即返回原回执，同键不同 hash 返回409；不存在时才验证 review/source/lock 与 `expectedProjectRevision` → 生成合并后的不可变 DraftRevision → 移动正文正式指针并写 diff → 追加 TemporalStateEvent → 写 PlanImpact → revision+1 与 indexTargetRevision → 保存稳定回执 → commit。提交后丢响应可用 commit/idempotency 查询恢复。索引失败不改变采用成功，`indexedRevision < indexTargetRevision` 返回 stale；启动时重建并更新水位。撤销使用同样的“幂等查询先于 revision CAS”顺序。

Brief/Outline/Plan 采用时移动对应 accepted 指针并保留旧 accepted revision；候选指针仍可指向后续草案。锁定的当前 accepted revision 只有作者显式解锁后才可被新版本替换；expected artifact revision 或依赖 project revision 不匹配返回409。大纲和计划的采用不会改写正文或正式故事事实。

Draft 场景级采用以稳定 `SceneChangeGroup(sceneId, baseAcceptedRevisionId, candidateDraftRevisionId, textRangeAnchors, factProposalIds)` 为最小选择单位。未选场景从 base accepted revision 原样保留，选中场景从 candidate revision 合并，合并结果保存为新的完整章 DraftRevision；正文锚点漂移、base 不匹配或锁定范围重叠时返回409。与某场景正文存在依赖的事实候选必须和该场景组成同一依赖组：正文被选中而必要事实被拒绝时先回到草稿修正，不能提交矛盾组合。每次改变 selections 都生成新 ReviewBundle 并重跑版本、引用、锁定和依赖组检查；整章采用等价于选中全部 SceneChangeGroup。

撤销只允许最新且无后续依赖的 commit；同样走 `BEGIN IMMEDIATE` 与幂等协议，追加反向 TemporalStateEvent、恢复正式指针、产生新 revision。存在后续依赖时返回409和影响预览，不盲目删除。

当前数据库直接按现行 schema 初始化。连接启用 foreign_keys、WAL、busy_timeout；跨项目引用在事务内逐项校验。schema 调整时同步重建测试数据库、fixture 和断言，不编写旧版本迁移或回填。

正文、证据、Prompt 均有长度上限；提交引用不可变修订而非重复保存两份全文；历史列表分页。日志只记录 correlationId/projectId/taskId/commitId/revision、哈希与长度，不记录正文、秘密或 token。

## 13. TaskRun、检查点、锁定与影响修订

TaskRun 保存 project/thread、类型、输入修订、模型、阶段、输出引用、取消与重试；Checkpoint 绑定 Scene 和父 DraftRevision。取消只停止后续生成，不删除已完成 checkpoint；重试必须从同项目同输入版本继续，版本变化返回409。无模型或工具不可用显示 disabled，不创建假产物。

锁定粒度为 OutlineNode、ChapterPlan、Scene/Beat 和正文保护范围。父级锁定默认阻止 Agent 修改后代；作者直接编辑可先显式解锁。受保护正文范围用稳定锚点和来源修订保存；文本漂移时返回冲突，不猜测新范围。

DependencyEdge 包含 source/target kind+id+revision、relation、evidence、invalidation rule。采用产生实际变化集合，影响服务只遍历支持的边，输出 affected/unchanged/locked-conflict 及证据。Agent 用这些 impactIds 生成局部修订候选；中央比较前后版本，作者保存或放弃。锁定冲突只提供“调整前置安排”或“解锁后重算”，无影响时显示明确成功态。

## 14. UI 状态、导航与可访问性

左侧只负责作品对象导航；中央负责当前对象的编辑、版本和差异；右侧负责对话与当前 ReviewBundle。选择状态以 `{projectId, view, artifactId, revisionId}` 为所有者，刷新恢复；Agent 产出自动打开中央对应对象，右侧保留本轮摘要。审阅顺序固定为：阻塞问题 → 正文差异 → 事实及原文证据 → 计划影响。

| 状态 | 数据保留 | 主 CTA / 恢复 |
| --- | --- | --- |
| 空 | 项目与输入 | 直接写第一章 / 设计大纲 / 导入章节 |
| 生成中 | TaskRun + checkpoints | 取消；中央显示已完成场景 |
| 部分完成/失败 | 所有 checkpoint 和输入 | 从断点重试 / 保留草稿 |
| 草稿已保存 | DraftRevision | 继续编辑 / 进入审阅 |
| 检查过期或409 | 草稿历史与旧 ReviewBundle | 打开冲突 / 基于最新版本重检 |
| 待审阅 | 固定 ReviewBundle | 修正事实决定 / 采用正文与事实 |
| 采用成功 | commit receipt + impacts | 查看影响 / 生成局部修订 / 撤销 |
| 撤销受阻 | 正式状态不变 | 查看后续依赖 / 返回 |
| 撤销成功 | 新 commit + 恢复后的指针 | 查看恢复内容 |
| 无模型 | 手动产物可编辑 | 配置模型；生成 CTA disabled 且说明原因 |

宽屏三栏；中等窗口右栏可折叠并可恢复；窄窗口左栏抽屉化、右侧为覆盖面板，中央正文保持最低 520px。移除现有 1120px 页面硬最小宽度；栏宽与折叠状态本地持久化。测试长标题和 100/150/200% 缩放。

Tabs 使用 roving tabindex、方向键、`aria-controls`；大纲使用 Tree 键盘模型；审阅 Drawer/Dialog 有焦点陷阱、Escape 关闭和关闭后焦点恢复；生成区有 `aria-busy` 和阶段播报；错误与字段关联；状态同时使用文字/图标，不只靠颜色。纯键盘可完成 A03、A07、A10。主流程用“大纲、近期计划、草稿、待审阅、正式稿、故事设定”，内部名词仅诊断可见。

导入首期流程为：粘贴 → 确认章节号/标题 → 保存不可变原文快照 → 建立可编辑草稿 → 检查；取消不保存，重复章节要求新章或新草稿，解析失败保留输入。若该流程未完整实现，则不显示入口。

## 15. 递增实施顺序与出口

1. S1 数据可靠性：当前 schema 初始化、不可变修订、ContextQuery、ReviewBundle、联合采用/幂等/撤销、索引水位；旧正式写入口直接删除。
2. S2 手工产品闭环：大纲/计划/草稿/事实审阅/影响修订 UI，可离线手工完成。
3. S3 真实 Agent 闭环：本地 MCP candidate 工具、TaskRun/checkpoint、项目线程绑定；真实模型完成空作品和直接写章样例。
4. S4 反馈与双平台：局部计划修订确认、恢复/响应丢失故障、macOS/Windows 源码开发运行核心旅程。安装包和捆绑 Python 是单独发布阶段，本轮不宣称已交付安装包。

每段通过自身自动测试后进入下一段；只有 S1～S4 全部通过才称为本 PRD 的纵向闭环。本轮发现任一段无法完成时，必须如实报告停在哪一段，不能用 UI 模拟代替。

## 16. A01～A17 验收追踪矩阵

| 场景 | 责任层 | 本轮证据 |
| --- | --- | --- |
| A01/A02 | MCP + planning + UI | 真实模型从一句设想写入大纲和1～5章计划；DB revision 断言 |
| A03 | MCP + drafting + UI | 无大纲直接写章；最小计划可查看；键盘旅程 |
| A04 | planning/review | Outline/Plan/Scene/文本锁定冲突测试，锁定对象深度相等 |
| A05/A06 | context | 显式故事时间、阅读章、场序、POV 的固定 as-of 查询 |
| A07 | review/accept | 作者纠正候选后正文与事实同 commit |
| A08 | revision CAS | 编辑后旧 ReviewBundle 409且不覆盖新稿 |
| A09 | acceptance/index | failpoint：提交前中断、提交后丢回执、索引失败、并发重复及重启 |
| A10 | undo | 最新无依赖 commit 撤销产生新 revision，完整对象比较 |
| A11 | task/checkpoint | 取消第二场、重启并从第一场 checkpoint 恢复 |
| A12 | auth/thread/project | 伪造跨项目 ID、线程恢复、MCP task 均被拒绝 |
| A13/A14 | impact/planning/UI | 只相关计划产生有证据的修订候选；锁定/无关节点深度相等 |
| A15 | review/context | 梦境/谎言拒绝为世界事实后仍可采用正文 |
| A16 | task/UI | 无模型、网络失败、预算用尽分别显示未生成/未完成/未检查 |
| A17 | context/impact | 三章窗口仍检索十章后伏笔，不自动生成十章计划 |

自动测试包括 Runtime 集成与契约、MCP 工具、Electron IPC/安全、前端组件/键盘/响应式；人工证据包括 macOS 当前环境走查、Windows CI或实机源码运行、真实模型新作/改稿各一例。当前环境若无法取得 Windows 或模型凭据，代码可以完成但 S4 不标通过，最终明确剩余验证。
