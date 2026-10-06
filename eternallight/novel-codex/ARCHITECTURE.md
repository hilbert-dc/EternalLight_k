# Novel Codex 当前架构

> 2026-09-23：新的总体目标统一为 [Novel Codex 理想架构](IDEAL_ARCHITECTURE.md)，围绕现有独立引擎重构小说领域层。本文保留阶段性实现背景；下文旧状态不代表新目标已完成，最新领域设计与验收以总纲链接的详细方案为准。

> 2026-09-20 审查后更正：下文“本地纵向闭环已落地”“唯一状态权威已明确实现”等完成判断不再有效。分层方向保留，实际预算、资格、取消、恢复与交付仍有缺口；当前状态与重构顺序见 [优化与重构方案 v3](ENGINE_REFACTOR_PLAN_V3.md)。本次仅更新方案，未修改实现。

版本：0.9 · 2026-09-20（通用 Agent Engine 本地纵向闭环已落地）

通用引擎目标见 [理想技术方案（v4.6）](MODEL_AGNOSTIC_CODEX_ENGINE_DESIGN.md)，具体整改和证据见 [修复与局部重构实施方案（v2.2）](ENGINE_REMEDIATION_PLAN.md)。44 项 review 问题已按纵向链路整改：F01–F41 与 F44 达到本地验证，F42–F43 已实现发布门禁但仍待签名双平台最终产物。本文区分本地闭环和真实供应商/目标安装终验，不用 fixture 结论替代外部报告。

实施顺序为 M0 可信基线 → M1 Chat Completions 完整链路 → M2 Messages 检验公共抽象 → M3 四协议覆盖 → M4 高级能力与独立交付。M0–M3 已由当前 CLI + 可控端点纵向测试覆盖；M4 的签名安装和真实端点属于外部终验。Codex、Host、资格仓库、预算账本、Runtime 各自拥有明确的唯一状态权威；原生与转换共用准入门禁，但不强求内部语义统一。

## 产品边界

当前版本只实现小说创作工作区。用户在中央编辑作品产物，在右侧与 Novel Agent Engine 协作；漫画、分镜、图像和视频不在本阶段范围内。

作品打开后直接进入章节，按作品恢复仍存在的最近章节；章节内阅读位置沿用编辑器会话书签。作品名称菜单提供只读作品信息，标题与简介来自作品元数据，创作目标来自现行简报并标明采用状态。查看信息不卸载正文或对话；进入简报编辑仍经过未保存保护。无作品显示创建入口，有作品无章节时可讨论构想或直接建首章。没有独立总览或第二份正文预览；全书分析沿现有对话按需执行。

本工程处于测试阶段，直接维护目标架构，不保留旧 Workflow、ChangeSet、浏览器演示数据或伪 Agent 回退路径。

## 仓库与上游边界

Novel Codex 采用“独立产品、Codex Fork 单仓库”结构，而不是抽离成失去上游历史的独立源码仓库：

```text
OpenAI Codex upstream/main
             │ direct git merge
             ▼
产品 Fork main
├── codex-rs/                   # 上游 Agent 内核
└── eternallight/novel-codex/  # 独立 Novel Codex 产品
```

- `origin` 是可写的产品 Fork；`upstream` 固定为 OpenAI Codex 官方仓库。
- 所有上游更新先进入 `codex-sync/YYYY-MM-DD` 分支，保留标准 merge 提交与冲突，不执行 reset、自动 stash 或覆盖。
- 正常产品差异限制在 `eternallight/**` 和产品专用 CI；上游内核修复应形成可独立贡献的 Codex 提交。
- `repo:check:strict` 是保护分支与发布门禁；完整流程见 [UPSTREAM_SYNC.md](UPSTREAM_SYNC.md)。
- 源码历史共享不代表运行时共享：Novel Codex 仍使用自己的应用 ID、安装包、数据目录、Provider 凭据和发布流程。

## 运行分层

```text
Electron + React Renderer
        │ 窄 IPC（无 token）
        ▼
Electron Main / Author Gateway
        ├── author token ───────────────┐
        └── Novel Agent Engine          │
              └── private pinned        │
                  App Server core       │
                    │ stdio MCP         │
                    ▼                   ▼
              candidate tools ──► Rust Story Runtime
                                      │
                                      ▼
                              Rust Story Core + SQLite
```

- Renderer 不持有 Runtime token、API Key、Node 能力或任意文件能力。
- Electron 主进程持有 author token，只为用户明确操作调用采用、撤销、锁定和章节删除。
- Novel Agent Engine 内核运行在独立 `CODEX_HOME` / `CODEX_SQLITE_HOME`，只获得 candidate MCP 能力；这些是私有实现目录，不与 Codex CLI 或桌面应用共享。
- Runtime 监听随机回环端口；candidate/author token 通过 stdin 交给 Runtime，不进入参数、环境、磁盘或日志。
- 下游 Provider 密钥通过 IPC 注入独立 Bridge，不交给 Renderer、Creative Runtime 或 Codex 子进程。私有 App Server 只持有短期 Bridge token 和 candidate MCP 环境；shell 环境策略排除产品凭据。统一原生透传与转换路径的租约门禁、可信路由及权限链已通过 W5/W8 本地纵向测试，最终签名安装包另按 W9/W10 验收。
- 产品 API、IPC 和 UI 只暴露 `Novel Agent Engine`。底层内核不读取 Codex/ChatGPT 登录态，不消费其订阅额度，也不把软件权限请求交给用户批准。
- 当前 provider 为 DeepSeek；认证、计费和限流由 Novel Codex 自有 Provider 配置承担，模型服务商自身额度仍然有效。
- 开发窗口只允许精确 Renderer origin，生产窗口只允许唯一打包入口；重定向、外部导航和新窗口均被拦截，所有 IPC 还会二次校验 sender WebContents 与 frame URL。
- 没有 App Server 或模型时仍可手工编辑候选并运行确定性检查，但不会生成假正文或假成功结果。

## 权威领域模型

```text
CreativeProject(revision, index watermark)
├── Story Canon
│   ├── Character / Relationship / Location / WorldRule / PlotThread
│   └── TemporalStateEvent（append-only）
├── Creative Artifacts
│   ├── StoryBrief Revision
│   ├── Outline Revision
│   └── Rolling ChapterPlan Revision → ScenePlan → Beat
├── Narrative IR
│   └── Scene → Beat → Canon constraints / state changes
├── Novel Edition
│   └── Chapter → immutable DraftRevision → SceneChangeGroup
├── FactProposal → ReviewBundle
├── AcceptanceCommit / UndoCommit
├── PlanImpact
└── TaskRun(project, thread, purpose, target, input revision, checkpoints, output refs)
```

`project_state()` 暴露的 `canon / narrativeIr / novelEdition` 是正式读取投影。候选产物不会进入正式投影，只有 ReviewBundle 通过后，author 采用事务才会移动 accepted 指针。

## 唯一写入协议

```text
手工编辑或 Agent 工具
  → immutable candidate revision
  → version-bound ReviewBundle
  → Story Tests + selection/fact decisions
  → BEGIN IMMEDIATE
  → accepted pointer + temporal facts + impacts + project revision
  → stable receipt
```

关键不变量：

- Brief、Outline、Plan、Draft 修订创建后不可变；编辑产生新修订。
- Agent 只能创建候选、事实建议和任务输出，不能采用、撤销或移动正式指针。
- 每个候选绑定 project、TaskRun、thread 和输入 project revision；跨作品、过期任务和旧审阅均被拒绝。
- Draft 可按稳定 `SceneChangeGroup` 部分采用；未选场景保留原正式内容，未选场景的事实不能确认。
- 事实按时间追加；Writer、Planner、Critic 使用不同信息策略，Writer 不读取未来计划正文和作者秘密。
- 采用以 project revision 做 CAS，并使用 request hash + idempotency key；回执丢失后重试返回同一结果。
- 撤销生成新提交和新 revision，恢复准确的前一正式指针与事实状态，不删除历史。
- 正式 Brief / Outline / Plan 可由作者显式锁定；Plan 内 Chapter / Scene / Beat 与正文 SceneChangeGroup 也有稳定 ID 和独立锁。锁定节点的修改、删除或移动在候选保存和采用两个边界都被 Runtime 拒绝；作者必须先提交并采用只改变 `locked: true → false` 的单独解锁修订。
- 搜索索引是可重建派生数据；水位落后时 UI 显示 stale，不能声称检索最新。
- 单章生成的 target 由中央章节页结构化传入并持久化；Writer 只能向目标章节保存草稿，不能写入 Planner/Critic 产物。
- `complete_task` 不信任模型声明的引用，而是从任务检查点与 DraftRevision 重新计算有效输出；只有 `sceneComplete/chapterComplete` 可满足交付契约。

## Context 与信息边界

`ContextQuery` 至少绑定：

- `projectId / projectRevision`
- `purpose = planner | writer | critic`
- `readingChapter / sceneOrder`
- 可选故事时间与 POV
- 条目预算与 UTF-8 序列化字节硬预算

Planner 可以读取相关未来承诺和作者秘密；Writer 只读取当前表达所需事实、当前章承诺和禁止泄露项；Critic 读取候选、约束及证据。切换作品或 purpose 时，主进程不会复用可能含有更高权限上下文的 Agent thread。

Writer 的前文摘录来自已采用 DraftRevision，每段带 chapter、revision、source 与截断标记。candidate 请求的 purpose 必须等于 TaskRun purpose；目标章的 readingChapter 必须一致。预算不足时按低优先级内容降级并返回 omissions，硬约束仍放不下时拒绝请求。

## Novel Agent Engine MCP

Novel Agent Engine 的私有 App Server 内核只注册 `novel_workspace` 本地 stdio MCP：

- `read_context`
- `save_brief`
- `save_outline`
- `save_chapter_plans`
- `save_draft_checkpoint`
- `publish_knowledge_batch`（仅知识建模任务）
- `submit_planning_review_report`
- `complete_task`

所有工具都要求 `taskId + projectId + threadId + expectedRevision`（读取工具按其契约携带版本和用途）。MCP 不提供 accept、undo、lock 或任意 SQL/文件写入。

## UI 信息架构

```text
┌─────────────┬────────────────────────────────┬──────────────────┐
│ 作品与导航   │ 中央作品文档                    │ 创作引擎协作栏     │
│             │ Brief / Outline / Plan / Draft │ 当前对话与回复     │
│ 任务历史     │ 候选与正式版本比较              │ 折叠执行过程       │
│ Canon/IR    │ Review / Accept / Undo / Lock  │ Composer/Inspector│
└─────────────┴────────────────────────────────┴──────────────────┘
```

- Enter 发送，Shift+Enter 换行，并尊重输入法合成。
- 当前回复是主体；计划、工具、作者问答和文件变更作为同一回合的可折叠过程。软件权限请求在主进程自动拒绝，不显示审批按钮。
- Brief、大纲、近章计划和章节草稿均在中央结构化编辑。
- 空作品可以手工或通过创作引擎建立第一章；保存只创建候选，采用后才成为正式稿。
- 已有章节的明确修改、规划、规则和伏笔修订请求，由对话在同一 Work 内执行有限候选任务，不重复要求批准生成；提问和讨论只读取，目标歧义先澄清。原作者意图、源版本、场景锁定与预算约束仍有效，发布和正式设定采用独立。
- 每项执行前核对源版本；前项保存导致后项来源过期时保留已保存结果并停止后项，不自动重新绑定来源。特殊改稿修复讨论仍核对独立范围。
- 角色页以人物简介、按章故事线及独立的后续规划为主。`useCharacterStory` 读取当前 IdentityCatalog，并经 `runtime:get-character-knowledge` 按真实 canonicalId 查询当前 Author 知识；前后 workspace revision/sourceDigest 与 identityRevision、逐条正文来源核对，防止混入不同版本。不能用冻结的 head snapshot 代替当前分析，因为分析完成可能只推进知识头。对话结束后只读刷新，失败保留同作品资料并允许完整重读，换作品或人物隔离迟到结果。身份识别不等于正式事实确认；未分析、读取不完整及规划建议均明确标注。同名管理折叠，讨论入口交由现有对话任务处理，不因浏览页面启动模型或写入。
- 审阅展示阻塞检查、场景选择、事实证据与决定；成功、失败、过期、索引滞后都有文字状态。
- 中央和右栏独立滚动；macOS 当前窗口已完成三栏视觉走查。

### 对话内伏笔作者决定（2026-10-03，真实闭环完成）

本阶段将当前章节已保存的伏笔提案接回对话内作者决定，不移动整个承诺管理页面，不新增模型工具、IPC 或领域 schema。执行过程仍由 ConversationTimeline 只读展示；决定入口在同一对话栏的当前章结果区，与历史回复分开。

结果卡绑定作品、章节、正文修订、提取报告修订和提案 ID。现有读取接口返回章节最新报告，因此入口称“本章当前建议”，不猜测某条历史回复对应的报告。摘要与兑现标准直接展示，原文依据按需展开；未决定的建议不会自动确认，也不伪装成持久拒绝。

“确认保留这条伏笔”是明确的作者确认，复用 establishCommitment → getCommitment → createCommitmentReview → acceptReview 当前接口完成。独立决定模块负责单飞、精确来源、分阶段回执和每次后续写入前的 owner 校验；候选建立成功后读取失败应恢复原对象，确认回执不确定时先只读核对，不能重复建立或冒称确认成功。撤销仅在权威 authorControls 允许时显示可用。调整入口复用现有 CommitmentEditor，完整目录保持按需折叠。

正文脏状态或任务执行阻止决定；决定不改写或发布正文，不调用模型，不改变模型历史。作品/章/源修订切换使旧决定失效。默认结果展示只陈述当前权威已读状态，不把“已建立”推断为“已确认”。自动展开应响应新的当前结果，作者收起同份报告后不因普通保存强制重开。

已建立但尚未读取确认状态的卡片提供只读“查看确认状态”，不把其他卡片打开或关闭当成取消既有确认。自动展开只响应入口和作品/章切换，同份结果不随脏状态变化强制展开。独立模块保存建立请求、检查请求与接受回执，错误文字区分候选已建立、检查未核实和确认结果未核实；自动恢复只读，作者再次明确操作才继续原请求。

五角色 PRE 已由根代理确认通过。冻结源码的产品/UI POST 无剩余 P0/P1；根代理真实 DeepSeek 提取单条候选后一次作者确认、再撤销已完成，正文和发布状态保持。最终 UI 快照复核仍由根代理收口。整章提取首轮证据引用无效仍是已知可靠性限制，不能用单条成功宣称模型引用错误已解决。测试与实机结果记录在 CHAPTER_EXPERIENCE_REVIEW_2026_10_02.md。

## 独立环境

作者登录是本机身份，不是 Codex/ChatGPT 或模型服务商认证。Electron 在平台 appData/novel-codex/author-session.json 原子保存作者资料摘要和登录状态；显式 NOVEL_CODEX_USER_DATA_DIR 时使用该隔离目录。Renderer 仅通过可信异步 IPC 读取/修改，不再使用页面 localStorage 保存作者会话。启动成功恢复时直接进入工作区，无登录页闪烁；默认持久登录，正常关闭/重启、开发页面地址变化及打包入口切换不要求再次登录。主动退出保持退出，读写错误明确提示并支持重试，不隐式创建新身份或宣称保存成功。作品与引擎数据目录不因作者会话路径调整而改动。

```text
Electron userData/
├── codex-home/                 # Novel Agent Engine 私有配置、线程与日志（不含桌面登录态）
│   └── sqlite/
└── story-runtime/
    └── novel-codex-v2.sqlite3  # 当前 schema 的小说领域数据
```

- 开发环境只解析项目专用 CLI 或当前仓库构建产物，不从 PATH 或 ChatGPT.app 取 CLI。
- Novel Agent Engine 与 Creative Runtime 剥离继承的 Codex、OpenAI 和其他 provider 凭据。显式配置的下游 Key 由 Host 管理并通过 IPC 提供给 Bridge，不注入 App Server、Renderer、Runtime 或日志。
- Creative Runtime、普通 Codex CLI 和 Codex 桌面应用不共用配置、会话、认证或数据库。

## 当前验证

模型接入采用“显式连接配置 + 发布前离线认证 + 运行时轻量账号检查”，详细流程见 [模型接入与发布认证](MODEL_ONBOARDING_AND_CERTIFICATION.md)。认证文件会转换为现有 CapabilityEvidence，SessionPlan 仍是唯一执行资格来源；不会按供应商创建第二套调度器或连接状态机。

- `qualify:model` 是无副作用诊断入口：运行协议、Core 和独立的 spawn/message/wait/route 场景，只落本地报告。
- `certify:model` 是显式发布入口：只有所有场景及每项能力证据同时通过，才原子更新认证文件与策略摘要；聚合能力已满足但任一场景失败时仍判定失败。

- Agent Engine 62/62；Creative Runtime 122/122；Electron/App Server 102/102；Electron UI 5/5；生产构建通过。
- `repo:check:strict` 通过；当前 macOS arm64 CLI 已生成并校验 BuildIdentity。
- Codex Core 的可信 MCP caller/root/parent/role/depth/turn 元数据定向测试 1/1；本轮未运行完整 Core 套件。
- 四协议均经过真实 App Server → Bridge → Provider fixture → MCP 回环；Chat 路线另覆盖同连接/异连接子 Agent、可信身份、取消和 Runtime 权限。
- QualificationContext 绑定 CLI、基础指令、Agent Engine 源码、认证 runner/MCP、模型目录、契约和 Codec 版本；任一变化使旧证据失效。

仍未完成的是目标环境验收，而不是隐藏的兼容回退：真实供应商矩阵、签名后的 macOS/Windows 安装旅程、upstream 同步演练和文学质量报告。只有这些固定组合的 G1–G10 报告齐全，才宣布理想目标达到 `VERIFIED_TARGET`。

Windows 实机核心旅程、真实模型从空作品完成单章生成，以及文学质量盲评仍属于发布门槛，不由当前自动测试替代。试写分支与连续多章仍未实现。

## 非目标

- 不把对话历史当作 Story Canon。
- 不允许 Agent 直接修改正式状态或任意数据库字段。
- 不用 UI 模拟代替真实 Runtime/MCP 结果。
- 不在当前阶段实现漫画、图片、视频、多人实时协作或整本一键生成。


## 对话驱动的工程约束（2026-10-03）

主对话是作者意图入口。日常生成、讨论、固定快照假设与专业多视角规划检查使用同一作者意图和任务链。以下为当前工程约束。页面是作品的可编辑视图、资料投影与精确决定界面；页面不能另起一个要求重复输入创作意图的模型入口。规划页的预填按钮与章节讨论、人物讨论、影响检查使用同一 composer，保留未发送文本并退出预备 Writer 模式。直接手写、场景排序和保护、发布、撤销、账号和导出仍是明确的作者操作。

链路为：作者原话 → 冻结作品与当前页面指向 → 有界查询／动态候选任务 → 持久执行回执 → 精确版本结果 → 作者决定。`ConversationFocus` 一次替换原章限定上下文，IPC 严格验证 view 和成对的章 ID/章号；导航上下文不赋予写权限、不代表正式版本。恢复保留原范围，禁止把过期来源自动换成最新内容。

- 当前发送可执行八类已明确要求的候选任务：简报、大纲、近章规划、已有章节写作、世界规则、故事知识、伏笔提取、已有伏笔修订。仅讨论不登记动作，依赖前一步结果的任务不得预猜来源。
- 自动候选开始前 Host 发出 `agent/candidate-started`。Renderer 仅在本次 pending send、同作品、同 advisory source task 时将重试描述切换为原任务恢复；生成中断不能通过重新发送作者原话隐式再跑模型。
- `task-results` 只投影持久 delivery evidence 的确切 step/attempt/reference。`task-result-navigation` 重新读取任务与作品，再验证产物和源修订。章节、知识批次、规划、规则、伏笔分别连接到对应视图和现有作者检查；不存在／已替换结果明确失效，不以最新版本冒充历史结果。
- 资料页读取现行工作区、历史知识快照、身份目录和独立伏笔目录，不再消费固定空的旧展示数组。时间未知就展示未知，候选与作者确认分开，截断覆盖直接可见。资料中的查看来源与讨论不执行模型或修改作品。
- 精确原文引用通过任务限定的 `resolve_story_evidence` 计算 `/body` Unicode 位置。只接受真实引用，歧义有限返回，未找到不修文本、不猜位置；Writer 不得借此读隐藏后文。引用与响应有界，不是小说正文长度限制。
- 身份与规则决定保留作者权限。界面进行同步单飞、owner/来源复核和执行中保护；采用成功与视图刷新失败分别报告，不把模型观点当正式设定。

专业能力必须明确区分：一般规划讨论不是结构化多视角检查；普通反事实推理不是固定快照假设模拟。`request_planning_review` 在原 conversation Work 内登记 2–3 个检查视角，冻结原作者要求及当前补充版本、准确规划修订、关注点和原预算。同一 source task 至多 3 组，幂等重放不另建任务。模型登记响应只返回有限身份／状态收据，不回传作者全文、检查集或报告；首次与幂等重放都受最终 9K 模型项边界约束，完整报告经 Author 读取。Host 仅执行 Runtime 已登记席位；恢复跳过已完成席位，关闭一组不取消其他组。独立专业入口显式保存 workTaskId，不靠名称推导归属。

`simulate_story_hypothesis` 仅限当前 advisory task。先读取有界基线页，再用绑定任务／意图版本／快照／页范围的 token 模拟；新增假设保留原主张，只有明确 suppress 才临时排除准确原主张。每任务至多 16 份不可变本地报告，不写正文、正式事实或人物决定。输入 8KiB、最终模型项 9K tokens；预览截断和覆盖未知直接返回，限制不改变分析语义或作者正文。

任务控制由 App 唯一 controller 承载，跨章节、资料和规划页共享单飞、错误提示、暂停、取消、继续与核验保存结果。无模型时仍可读取并核验回执；执行恢复使用原任务，不重新发送作者原话。跨作品、卸载与迟到响应都检查作用域。

专业结果从 Runtime canonical delivery 读取准确 report/hypothesis ID；模型自述或 outputRefs 不构成保存证据。对话中的只读历史分析窗口展示原报告、范围、原文依据和未完成视角，不跳转到最新报告、不卸载编辑器、不改变正文选择或滚动。四类知识冲突按当前协议分别显示。作者仍通过已有明确操作发布或确认正式设定。

资料讨论使用 typed `DiscussionReference`，scene 绑定 sceneGroup/object/revision，claim 绑定 modelingBatch/assertionIndex/source，commitment 绑定 commitment/revision；共同绑定 project/snapshot。引用与草稿按作品保留，替换来源须显式选择，Host 在任务创建前验证当前来源。目录直接查看与决定走只读精确导航，不能伪造任务结果证据或替代作者确认。

`composeAdvisoryPrompt` 由生产 Host 与实模探针共用。讨论阶段对明确委托只完成必要定位与登记，执行任务负责完整提取和证据检查。探针与生产一致，仅由 Host 关闭 advisory；worker 完成必须来自实际 MCP 与持久输出，不得由验收脚本补造。

普通明确委托的 advisory 在 read_context 已核实唯一相关正文来源时直接登记精确目标；query 是缺失/歧义或问答证据所需的按需工具，不是登记前固定步骤。relevantText 的 sceneGroups 表示不因省略重复 body 而缺文；章节身份使用 content.chapterId/chapterNumber 与同项 sourceRevisionId，不能从事实或 objectId 推导。覆盖截断不能证明目标唯一。完整提取、逐条证据与保存由 worker 执行。

query MCP 前置校验与 Runtime 已有 UTF-8 字节字段边界一致，text/name 仍按 Unicode 字符；错误只说明固定字段路径和限制。整个 typed query plan 的 9000 字节最终边界继续由 Runtime 检查，不用原始 JSON 的不同序列化形状代替。

确定性工具参数拒绝与瞬时连接故障分离：引擎仅在本地codec校验点赋invalid_tool_arguments终态，不执行、不作为502/断流盲重采样。成功供应商响应已携可靠usage时即使输出被拒也结算；流式输出校验失败后仅在同一有界传输读取终态usage，不继续公开/执行输出或发新请求，缺用量仍unknown。外部HTTP相似错误文本不取得本地分类，原预算/权限/schema门禁不变。

read_context.queryPlan 与 query_story 共用UTF-8前置边界；探针将MCP失败映射为有界固定错误码及字段类别，不留原参数、正文或任意错误文字。供应商response成功不等于其后MCP操作成功，两类诊断分别保留。

#### Advisory source identity and recoverable query validation

Advisory `read_context.operationContext.delegationTargets` is a bounded source-identity index, independent of optional prose coverage. It enumerates current main-snapshot chapter targets for `extractCommitments` and `modelKnowledge`, reuses domain target validation, and preserves explicit focus/historical-reference boundaries. Its entire serialized fragment is capped at 768 UTF-8 bytes and four entries; coverage is explicit. A copied target never grants author consent or bypasses task input, intent, source, or CAS validation. Workers remain responsible for reading full prose and saving anchored outputs.

Query tools expose an object transport for `plan`/`queryPlan`, with a bounded field contract in the description. The MCP execution boundary validates the complete operation-specific schema and UTF-8 limits before dispatching Runtime requests. Invalid plans return a bounded genuine MCP `isError` with a fixed code and safe field/constraint; Core can correct the call within the existing turn and budget. This does not relax top-level task identity, malformed JSON rejection, Runtime domain validation, or bridge transport validation. No fabricated tool output, history rewriting, or extra retry budget is involved.

#### Chapter-owned scenes

Scenes are chapter structure rather than a first-level workspace view. Navigation, `ViewId`, story reference views, and conversation view labels use the same current route set; there is no scenes route alias. Scene groups and scene discussion references remain domain objects. Chapter scene controls prepare the existing composer with the exact saved project/snapshot/chapter/revision/sceneGroup identity. Dirty prose or review state blocks preparation with a save notice; locked scenes can still be discussed without unlocking. Preparation does not save, send, generate, or publish. Existing composer text, explicit source replacement, and Host source validation remain authoritative. Cross-chapter lookup uses the existing conversation/search capabilities rather than a duplicate scene directory.

#### Planning surfaces and source selection

Planning has three editing scopes: the creative brief defines direction, the outline arranges book/volume/unit structure, and the rolling plan specifies upcoming chapters. A shared `PlanningWorkspaceHeader` exposes current edit/save/adoption state and prepares the existing conversation without sending. It never reuses the previous assistant answer as an implicit new author instruction. Save and adoption preparation have one entry; adoption still requires the existing explicit author decision. Ready reviews must match both artifact revision and project revision. Working-baseline overrides and locks remain visible; detailed controls and saved-version comparisons are optional and never include unsaved prose as if persisted. Native details keep editor nodes mounted.

Plot commitments and recorded story time are reference surfaces, not planning prerequisites. Their filters operate only on loaded data; coverage, paging, and source authority remain explicit. Page-level discussion prepares the same composer; precise object discussion keeps existing typed references.

`PlanningCharacterProvider` reads the current project's identity catalogue once per planning-page refresh. Each POV option persists the raw character entry ID used by Writer's raw-source knowledge policy, never a merged canonical ID. Missing selected IDs remain unchanged; catalogue reads, failures, refreshes, and merges do not mutate plans. API/project/refresh ownership isolates late results. Selection is an explicit editor change saved through the existing plan CAS contract.

Planning conversation submission uses `usePlanningSubmission` and a content-only workspace save handler. Ordinary Send on an edited brief, outline or rolling plan first saves the current candidate and verifies its exact receipt by reading the workspace, then binds the original prompt to the unchanged conversation focus. It does not apply review decisions, select a working baseline or adopt an artifact. Precise discussion sources, repairs and retries retain their existing explicit source checks. Input/source/route/project/API incarnation changes cancel the pending send; same-scope requests remain single-flight until the save settles. A known successful save with failed readback is retried as a read of that receipt, never as another write. Successful conversations refresh once; refresh deferred by local edits resumes after save/discard, while read failures expose a read-only retry without invoking a model. This does not implement dependent multi-step planning or rebase frozen actions after a source changes.

Idle model availability is projected from the same current capability evidence and applied SessionPlan validation used immediately before execution. Local evidence reads never trigger verification or provider requests; active executions retain their bound status. Host preflight failures use MODEL_CONNECTION_UNAVAILABLE; the renderer preserves an untouched unsent draft only when no task/turn started, and isolates background availability reads by connection lifetime. The save_outline MCP payload schema now declares current editable OutlineNode fields and validates them before dispatch. Runtime remains authoritative for source/CAS and persistence; missing CAS heads are JSON null and are never coerced from text.

对话展示区分阶段回复与实际交付。只有同作品、精确关联到成功advisory及其已完成子执行、且交付证据已记录的回合，才把中间公开原文与过程收进该任务既有执行详情；主表面保留最终产物回复、结果导航和错误恢复。纯讨论、待确认、失败、repair或证据不足保持外露，不按回复措辞猜测执行状态，不改写模型历史。公开表达规范由composeNovelCodexPrompt统一注入一次；作者授权与工具回执决定动作和完成声明，简洁回复不缩减工具产物或替代正式采用决定。

成功且完整交付的写作/规划主产物，按canonical结果的作品、任务、步骤和attempt精确关联收纳“结果说明”，默认突出真实完成状态和作品入口；完整模型原文可展开、复制。纯讨论/只读分析/repair/失败/部分交付不适用。持久回复只有recorded且完整的文本可收纳，读取错误、无回执、absent和truncated提示直接展示；已有阅读状态按稳定attempt身份保留，不新增模型摘要或改写模型答复。

历史交付展示按原证据区分被替换与真正部分完成：只有完整成功、原产物引用与成功attempt精确匹配且有superseded证据时，标为“结果已保存，后续已有更新”，并保留原生成说明的折叠状态。Runtime的partial值不变；该展示分类不生成TaskResultDestination、不读取最新head代替历史原文、不增加采用入口。missing/pending、错误、未结算、repair或回复截断继续保留原有问题提示。

对话滚动以实际位置与明确阅读意图管理跟随：普通点击、复制、Tab、文字键、向下/横向滚轮或缩放不会退出底部跟随。明确上翻、主动展开详情、对话正文非空选区会先固定当前锚点；实际滚离底部保持阅读，返回底部或点击“回到最新”恢复。输入控件内编辑不作为上翻指令；原生事件不被拦截。监听随作品/面板生命周期清理，沿用有界展开记忆及短加载占位的锚点恢复。

### 作品目录与单一卷纲来源

App 持有唯一 `useWorkCatalog` 状态。目录卷只持有名称、顺序、章节归属与显式卷纲节点关联，不存故事简介。关联时保存精确 outlineRevisionId/nodeId 供来源核对；阅读时 Native 在同一读事务内选择当前 candidate??accepted，以同节点身份唯一匹配 kind=volume，派生当前名称与 outlineStatus。节点删除、换身份、重复或错误类型显示 unavailable，不回退旧版本内容、不按同名猜测。独立卷显示 independent。

左侧长篇大纲与全书卷概览进入同一个 CreativeWorkspaceView、同一份大纲 artifact。卷纲只阅读指定节点及下级；手动编辑限定子树，但保存仍提交同一份完整大纲并保留其他节点，CAS、dirty、锁及采用规则保持。采用作用于整份大纲。全书页展示主线和各卷简短目标；查看卷纲后可读全文，没有第二份可维护副本。异常层级仍保留在未归入层级区域。

独立或失联卷可经对话构思，保存后显式选择已保存卷纲建立 linkOutline；该操作沿现有作者目录 CAS、任务锁和幂等控制，不自动采用大纲或改变章归属，同节点不能绑定两个目录卷。卷纲业务仅存于 outline payload，目录创建不复制目的、摘要或写作备注。当前 schema 一次切换，不实现旧库迁移或兼容路径；旧测试库不会由应用隐式删除。

章节删除是目录章行菜单中的明确作者操作，只经 Author IPC/API，不向模型开放删除 MCP。确认前读取当前工作区的精确章节与保存标题；中间章节提示后续章保留及衔接影响。事务验证 workspace revision、幂等键、任务与内容锁，原子移除章节、目录归属、规划绑定、当前正文与分析头、以原始 chapterId 绑定的 draft working baseline 和 continuation intent，并生成排除本章的当前快照。不可变历史、任务收据及独立已确认故事设定保留；删除身份阻止迟到写入重新建立章节。Renderer 仅读回 state/workspace/catalog 一致后更新目录，未知回执仅核对，仍在时须重新明确确认。删除当前章打开相邻章，末章删除进入空态，其他章不重新编号；继续创作使用现存最大章号作为编号边界。具体门禁与验证见 [章节删除设计评审](CHAPTER_DELETE_REVIEW.md)。

删除确认同时说明后续自动衔接检查。删除事务保存 `ChapterStructureChange`，冻结已保存、已发布和工作基准的精确修订、知识批次、相邻章节以及前后快照；未发布稿不能只靠旧快照代替。仅没有保存内容和相关来源的空章可跳过，末章不按位置跳过。Host 在当前受信作品、无未保存修改且模型合格/空闲时消费持久 pending 事件；没有可用模型时不先建运行 Work。`assessChapterStructure` 的首任务与事件唯一绑定在原标准预算事务内完成，首次任务捕获启动时的当前输入，绑定后不隐式换源。创建回执丢失时先读取精确绑定；仅本地确证尚未向模型发送、且本次授权派生的任务身份与输入完全匹配时，才终止未发送 attempt 并用原预算生成真实零请求收据和结算。已有预算记录拒绝覆写；无法核实的情况仅暂停原 Work 并保留错误。失败只显式恢复原任务，不重新删除或重建根额度。

检查任务的 MCP 和 Runtime 权限均只读故事。`read_chapter_structure_source` 只接受已绑定事件的 sourceRole 与分页游标，不能传入任意历史修订；游标绑定任务、权限、输入、事件和来源。旧原文明确标识为被移除的历史依据，当前查询沿冻结的任务输入。结果复用 `finalReply` / `completionReport`，不会保存候选、确认设定或自动修改后文；报告区分证据支持的断裂、可能风险与未检查范围。任务、实时执行和结果在对话中标为“自动检查”，待检查只显示轻量状态，不在各故事页面重复增加面板，也不覆盖未发送输入。见 [章节结构影响检查评审](CHAPTER_STRUCTURE_IMPACT_REVIEW.md)。本阶段仅覆盖删除；现有新章 API 仍是追加，不宣称已支持中间插章或重排。

### 全书与卷纲的叙事职责

全书大纲是整部故事的因果脉络：book.purpose说明核心主线（人物目标、主要冲突及代价），book.summary保留起因、主动推进、关键转折、高潮选择与后果、收束；未定部分明确留白。卷纲属于同一份outline，volume.purpose说明阶段对主线的贡献，volume.summary展开该阶段的事件与起止局面，unit继续细化局部安排。全书页面的各卷预览只展示阶段目标和精确阅读入口，不复制卷summary，也不从卷纲拼造缺失的book.summary。

outline-layer-copy统一阅读、编辑和讨论入口中的层级语义，generateOutline叶任务提示同步分工；没有新增内容字段或语义强校验，旧作者文本保留，保存仍使用完整outline和原有CAS。提示词分工不能证明模型实际写出了完整故事脉络，需另对具体保存产物做内容验收。

### 删章检查后的作者决定

当前可见自动检查完成后，普通对话携带 `conversationFocus.chapterStructureTaskId`，不复制报告正文。Host 从 Runtime 核验同作品、已完成的 `assessChapterStructure` 与完整最终回复的 task/thread/turn 后恢复该线程；新 advisory 使用新能力、预算与当前冻结输入，保留 append-only 历史。显式讨论来源及改稿来源优先，不能混用。报告只是历史建议，当前修改必须重新核对来源；“先不改”只讨论，多方案的“按建议调整”先澄清。引用失效时明确失败并保留精确重试，不切换到其他线程。


### 改稿后的按需分析

`work_decision_status` 根据当前 Work 的持久化种类和作者明确指定的 `desiredOutcomes` 决定知识建模是否是完成依赖。普通正文修改保存后，`modelKnowledge` 与伏笔提取作为可选行动提供给协调智能体；未执行可选分析不阻止作者取得已保存稿件。连续生成的章节交付，以及明确指定了效果评估的改稿，仍要求与当前正文 revision 一致的知识批次。已选择的行动仍进入原 Work 的状态、预算和回执链，失败或结果未知不得伪装成完成。后台知识 outbox 保留当前 revision 的待分析状态，但不会为有写作任务来源的 revision 在协调结束后再启动自动建模。排除条件绑定确切 revision，不占据调度队列，也不阻止其他手动编辑或同章新的手动 revision；显式分析仍可照常执行。

协调智能体从 `receipts.items[].outputs` 中正文的 `chapterId`、`revisionId` 获取实际保存对象。仅错字、措辞或标点修改复用正文进行局部核对；人物状态、时间、因果或伏笔变化才按具体需要读取相关来源、检查影响或建立候选知识。`canFinish` 说明必要执行依赖已满足，不认证文学质量。工具选择不扩大作者授权，也不自动发布、确认候选设定或改写其他章节。


### 修订评估与修复的当前契约

修订评估、修复讨论和续作都通过任务资料目录按需读取冻结内容；`revisionOutcomeInput`、`revisionRepairInput` 在 `read_context` 中是来源引用，正文由 `read_task_source` 连续分页提供。页大小约束不能作为全文截断上限。作者批准目标与完整原稿/改稿仍必须实际读取。

建模完成仅完成该步骤，评估由协调智能体通过当前 Work 的动作接口选择；动作登记、步骤入队和幂等回执必须在同一事务内。保存过的评估报告依据不可变的作品、正文版本和已发布批次验证出处，不能因当前知识头替换而变得不可读；新任务、新决定和新报告写入继续检查来源是否当前有效。
