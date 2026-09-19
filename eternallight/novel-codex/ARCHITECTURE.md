# Novel Codex 当前架构

版本：0.5 · 2026-09-19

## 产品边界

当前版本只实现小说创作工作区。用户在中央编辑作品产物，在右侧与 Novel Agent Engine 协作；漫画、分镜、图像和视频不在本阶段范围内。

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
              candidate tools ──► Creative Runtime
                                      │
                                      ▼
                                    SQLite
```

- Renderer 不持有 Runtime token、API Key、Node 能力或任意文件能力。
- Electron 主进程持有 author token，只为用户明确操作调用采用、撤销和锁定。
- Novel Agent Engine 内核运行在独立 `CODEX_HOME` / `CODEX_SQLITE_HOME`，只获得 candidate MCP 能力；这些是私有实现目录，不与 Codex CLI 或桌面应用共享。
- Runtime 监听随机回环端口；candidate/author token 通过 stdin 交给 Runtime，不进入参数、环境、磁盘或日志。
- 私有 App Server 内核可在自身进程中读取显式选择的 Provider 密钥和 candidate MCP 环境，但它启动的 shell 使用 `inherit = "core"` 并排除 `NOVEL_CODEX_*`、OpenAI/DeepSeek/Codex 凭据；模型不能通过 shell 回显这些秘密。
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
- `submit_fact_proposals`
- `propose_plan_revisions`
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
- 已有章节在正文主列显示“生成当前章”；请求明确展示目标章和作者意图，任务状态区分进行中、部分完成、失败和候选可审阅。
- 审阅展示阻塞检查、场景选择、事实证据与决定；成功、失败、过期、索引滞后都有文字状态。
- 中央和右栏独立滚动；macOS 当前窗口已完成三栏视觉走查。

## 独立环境

```text
Electron userData/
├── codex-home/                 # Novel Agent Engine 私有配置、线程与日志（不含桌面登录态）
│   └── sqlite/
└── creative-runtime/
    └── novel-codex-v2.sqlite3  # 当前 schema 的小说领域数据
```

- 开发环境只解析项目专用 CLI 或当前仓库构建产物，不从 PATH 或 ChatGPT.app 取 CLI。
- Novel Agent Engine 与 Creative Runtime 都剥离继承的 Codex、OpenAI 和其他 provider 凭据。DeepSeek Key 只进入引擎私有 App Server 进程，并且不会写入配置文件、Runtime 或日志。
- Creative Runtime、普通 Codex CLI 和 Codex 桌面应用不共用配置、会话、认证或数据库。

## 当前验证

- `npm run build`
- `npm run test:runtime`：70 项
- `npm run test:agent`：48 项
- Electron 主进程、适配器与 Python 模块语法检查
- macOS 独立 Electron 进程三栏视觉走查，包括章节生成卡片、正文主列和后续表单滚动

Windows 实机核心旅程、真实模型从空作品完成单章生成，以及文学质量盲评属于发布门槛，不由当前自动测试替代。试写分支与连续多章仍未实现。

## 非目标

- 不把对话历史当作 Story Canon。
- 不允许 Agent 直接修改正式状态或任意数据库字段。
- 不用 UI 模拟代替真实 Runtime/MCP 结果。
- 不在当前阶段实现漫画、图片、视频、多人实时协作或整本一键生成。
