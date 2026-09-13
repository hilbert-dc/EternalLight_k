# Novel Codex 第一版落地方案与四角色评审

> 当前门禁：所有新设计和实施后复核必须使用 [设计评审模板](DESIGN_REVIEW_TEMPLATE.md)，执行 [测试阶段工程策略](TEST_STAGE_ENGINEERING_POLICY.md) 的“零兼容投入”检查。本文后续章节是按时间追加的历史记录，其中关于旧版本、旧数据、迁移或兼容的要求均已失效，不得重新转化为实现项。

## 变更概览

- 目标：把当前静态 Electron + React 原型落地为可运行的小说 Codex 第一版。
- 范围：Story Canon、Narrative IR、Novel Edition、SQLite、本地 Creative Runtime、Electron typed IPC、可编辑 UI、Chapter Diff、基础 Story Tests、搜索和 Codex App Server 可选适配器。
- 非目标：漫画/动漫适配、图片/视频生成、协作服务、PostgreSQL/pgvector、复杂长任务编排。
- 约束：macOS/Windows；本地优先；未配置 Codex App Server 时应用仍可浏览、编辑和运行本地结构化操作。

## 推荐架构

```text
React Renderer
    │  allowlisted typed IPC
    ▼
Electron Main / Agent Gateway
    ├── Runtime process supervisor
    └── optional Codex App Server JSONL adapter
    ▼
Python Creative Runtime (FastAPI + Pydantic)
    ▼
SQLite: Canon + Narrative IR + Novel Edition + Diff
```

### 关键决策

1. Runtime 作为独立本地进程，由 Electron 主进程负责启动、健康检查和退出回收。
2. Renderer 不接触文件系统、子进程或任意 IPC；所有调用经过 preload 白名单。
3. SQLite 使用结构化表保存 Canon、Scene/Beat、Chapter/Passage，并在章节更新前保存版本快照。
4. UI 数据全部来自 Runtime；浏览器开发模式没有 Electron 时使用内存 fallback，方便视觉开发但不伪装成持久化。
5. Codex App Server 适配器只允许显式配置命令，按 JSONL 读取事件并转发给 UI；不可用时显示离线状态和本地模式。
6. 第一版搜索使用 SQLite `LIKE`，保留后续 FTS/语义检索替换点。

## 四角色评审

### 技术专家 A——架构与系统风险

- 结论：`APPROVE WITH CONDITIONS`
- P1：Runtime 必须绑定 `127.0.0.1`，端口可配置，不能开放局域网；IPC 必须使用固定 channel allowlist。
- P1：Runtime 子进程必须在窗口关闭和应用退出时回收，异常退出要反馈可理解的状态。
- P2：SQLite schema 初始化和 seed 必须幂等。
- 条件：实现中加入 loopback 绑定、进程生命周期管理、错误事件和幂等初始化。

### 技术专家 B——实现与质量

- 结论：`APPROVE WITH CONDITIONS`
- P1：章节更新必须有输入校验、版本快照和可查询 Diff；React 不得直接修改不可追踪的全局对象。
- P1：Runtime API、IPC payload 和 UI model 要有明确类型边界；至少覆盖 Runtime 单元测试和前端构建检查。
- P2：无 Electron 环境的 fallback 只能用于开发，必须在 UI 标明本地演示状态。
- 条件：加入 Pydantic 模型、SQLite 事务、Diff/Story Test 接口和测试脚本。

### 产品经理——目标与验收

- 结论：`APPROVE WITH CONDITIONS`
- P1：第一版验收必须是“能打开项目、编辑章节、保存、看到场景锚点、查看 Canon、看到修改记录、运行一致性检查”。
- P2：Codex 未连接时必须明确展示“本地模式”，不能让用户误以为已经调用模型。
- 条件：将 UI 静态按钮改为可验证动作，并为加载、保存、错误和空数据提供反馈。

### UI/UX 设计师——流程与体验

- 结论：`APPROVE WITH CONDITIONS`
- P1：Composer 应保持在工作区底部，不应只在总览页存在；发送中、成功、失败和离线状态要可见。
- P2：章节编辑器需要明确的保存状态，场景锚点和 Canon 上下文不能丢失。
- P2：表单和按钮需要可访问名称、键盘操作和焦点状态。
- 条件：保留 Codex 风格三栏布局，补齐全局 Composer、状态提示和可编辑控件。

## 综合决策

四个角色均允许在满足上述条件的前提下实施。本次直接在既有第一版范围内落地，不扩大到漫画和云端协作。若 Codex App Server 未安装或未登录，应用仍以本地 Runtime 模式运行，并在状态栏显示真实连接状态。

## 实施后验收

- `npm run build` 成功。
- Runtime 测试覆盖项目读取、章节保存、Diff、Story Test 和搜索。
- Electron 主进程能启动/回收 Runtime，preload 只暴露白名单 API。
- UI 能真实读取和保存章节，并展示保存状态、Scene Anchor、Canon、Diff 和 Story Test 结果。
- 最终 diff 不包含漫画、图像、视频或云端协作实现。

---

# Codex App Server 集成变更评审（第二轮）

## 变更目标

将当前仅能发送占位 JSONL 的适配器升级为可工作的 stdio App Server 客户端：自动完成初始化、线程创建和轮次启动；将 Agent 流式事件显示在 UI；对命令/文件变更审批进行显式用户确认；App Server 不可用时安全降级到本地 Runtime，并清晰标注模式。

## 方案

1. Electron 主进程维护单例 `CodexAppServerClient`，使用 `readline` 逐行解析 stdout，按 JSON-RPC id 管理请求响应，按 thread/turn 管理流式事件。
2. 首次发送前严格执行 `initialize`、`initialized`、`thread/start`；每条创作指令使用 `turn/start`，传递 `threadId` 和 `input: [{ type: "text", text }]`。
3. Preload 暴露 `agent.status`、`agent.sendPrompt`、`agent.onEvent`、`agent.onApprovalRequest`、`agent.resolveApproval`，不暴露 `ipcRenderer` 或子进程对象。
4. UI 增加连接状态、流式 Agent 消息、失败状态和审批卡片；审批默认不自动同意，用户可以接受或拒绝。
5. `CODEX_APP_SERVER_COMMAND` 可覆盖默认的 `codex app-server`；启动失败或命令不存在时转入本地 Runtime，不伪装成 AI 回复。

## 四角色独立评审

### 技术专家 A——架构、安全与可靠性

- 结论：`APPROVE WITH CONDITIONS`
- P0：不能把 App Server 的任意 server request 自动批准，否则 Agent 可能执行命令或修改文件而绕过用户同意。
- P1：必须完成 initialize 握手和 thread 生命周期；必须区分 JSON-RPC 响应、通知和 server request。
- P1：子进程只能按显式命令启动，stdin/stdout 通过主进程封装，Renderer 不接触进程；退出和错误必须清理 pending 请求。
- P2：事件内容要有大小上限，避免无限制写入 Renderer 状态。
- 条件：实现 approval request 队列、请求超时/拒绝、进程回收和有界事件缓冲。

### 技术专家 B——实现、兼容与测试

- 结论：`APPROVE WITH CONDITIONS`
- P1：`turn/start` 必须使用官方 payload 形状，不能只传 `prompt`；必须在收到 thread id 后再发 turn。
- P1：并发发送必须被拒绝或排队；响应 id 和 turn id 不能使用单一 pending 槽位混淆。
- P1：未安装 Codex 或 App Server 启动失败时，local fallback 仍必须可用并有可测试的错误路径。
- P2：需要 mock JSONL server 测试 initialize/thread/turn/delta/completed/error/approval。
- 条件：用请求 map、turn 状态机和可注入 command runner 实现，并补充 Node 测试或 smoke fixture。

### 产品经理——价值、范围与验收

- 结论：`APPROVE WITH CONDITIONS`
- P1：用户必须能判断当前回复来自 Codex 还是本地 Runtime；本地 Runtime 不能显示为“已生成内容”。
- P1：发送中、完成、失败、取消、需要授权等状态必须可理解，不能卡死 Composer。
- P2：第一版不扩展模型选择、云端账户管理和复杂多线程 UI；保持单项目单活动线程。
- 条件：验收“输入一句创作指令后可看到流式事件或明确失败原因，并能继续本地编辑”。

### UI/UX 设计师——流程、反馈与无障碍

- 结论：`APPROVE WITH CONDITIONS`
- P1：审批请求必须在 Composer 附近展示操作对象、原因、工作目录和接受/拒绝按钮，不能只显示原始 JSON。
- P1：流式输出应增量显示并保留最后状态；连接断开时提供重试/继续本地模式。
- P2：状态颜色不能是唯一信息，必须同时显示文字；审批按钮需有 aria-label 和键盘焦点。
- 条件：增加 Agent 状态栏、事件区和审批卡片，不破坏 Codex 风格三栏布局。

## 综合决策

四个角色均为 `APPROVE WITH CONDITIONS`。所有 P0/P1 条件转化为实现任务后实施：正确握手、线程/轮次状态机、审批不自动放行、显式模式反馈、错误回收、基础测试。此次只实现 stdio 单线程集成；WebSocket、远程连接、账户登录 UI 和多线程管理留在后续阶段。

## 第二轮实施后复核

- 技术专家 A 条件：已解决。适配器绑定 stdio 子进程，不向 Renderer 暴露进程；审批请求进入 UI，默认不自动批准；进程异常会清理 pending 请求；事件大小限制为 64 KiB。
- 技术专家 B 条件：已解决。实现 `initialize → initialized → thread/start → turn/start` 状态链路，使用请求 Map 和 turn waiter；补充 JSONL mock 的握手、流式完成和审批恢复测试。
- 产品经理条件：已解决。UI 显示 Codex App Server、本地 Runtime、错误和等待授权状态；Codex 不可用时返回明确的本地降级说明。
- UI/UX 条件：已解决。增加 Agent 状态栏、审批卡片、命令/目录信息、拒绝/批准按钮和键盘可操作的 Composer。
- 验证：`npm run build` 通过；`npm run test:runtime`（3 项）通过；`npm run test:agent`（2 项）通过；三个 Electron 文件 `node --check` 通过。
- 环境限制：真实 CLI 初始化检查被当前 Codex 沙箱对 `/Users/hilbert/.codex` 状态数据库的写权限阻止；这不是应用协议测试失败。用户桌面环境启动 `npm run dev` 后即可进行真实账号/权限链路验证。

---

# 完全隔离运行时变更评审（第三轮）

## 变更目标

Novel Codex 不与 ChatGPT 桌面应用或系统 Codex CLI 共用可执行文件、状态目录、配置、认证、技能、日志和会话。专用 CLI 不存在时必须安全降级到本地 Creative Runtime。

## 实施方案

1. 在 Electron 主进程中通过 `codex-runtime.mjs` 解析专用 CLI：开发环境使用项目运行时或当前仓库构建产物，打包环境使用 `resources/codex-runtime/bin/codex`；禁止 PATH fallback，并拒绝已知 ChatGPT.app 路径。
2. 将 App Server 子进程命令改为参数数组，显式传入应用专属 `CODEX_HOME`、`CODEX_SQLITE_HOME` 和环境变量；清除继承的 `CODEX_API_KEY`、`CODEX_ACCESS_TOKEN`、`OPENAI_API_KEY` 等凭据。OpenAI 由主进程在 v2 初始化后建立 ephemeral API-key 登录；DeepSeek 仅在显式选择时注入其专用 Key。
3. 将 Creative Runtime 数据库移动到 Electron `userData/creative-runtime/novel-codex.sqlite3`，与 Codex 状态目录分离。
4. 为专用 Codex 状态创建最小配置，使用 ephemeral 凭据存储，避免读取桌面版认证状态；独立登录和安全凭据 UI 留待后续版本。
5. UI 状态栏明确显示“Novel Codex 专用 CLI”或“本地 Runtime”，不再将 PATH 中的 Codex 误报为已连接。

## 四角色独立评审

### 技术专家 A——架构、安全与可靠性

- 结论：`APPROVE WITH CONDITIONS`
- P1：不得从 PATH fallback；不得继承桌面版认证变量。
- P1：App Server 与 Creative Runtime 必须使用不同的数据目录，并由 Electron 负责生命周期。
- 条件：专用 CLI resolver、显式环境构造、应用 userData 目录和缺失 CLI 的本地降级均已纳入实现。

### 技术专家 B——实现与质量

- 结论：`APPROVE WITH CONDITIONS`
- P1：保留测试用命令注入，不破坏既有 JSONL 适配器测试。
- P1：覆盖打包路径、桌面 CLI 拒绝、凭据清理和路径不存在场景。
- 条件：增加 resolver 测试，并执行构建、Runtime 测试和 Agent 测试。

### 产品经理——目标与验收

- 结论：`APPROVE WITH CONDITIONS`
- P1：没有专用 CLI 时应用仍可编辑小说并使用本地 Runtime。
- P1：UI 必须明确当前能力来源，不能暗示复用了桌面版 Codex。
- 条件：首版只做运行时边界，独立登录和 CLI 更新器作为后续可见功能。

### UI/UX 设计师——流程与体验

- 结论：`APPROVE WITH CONDITIONS`
- UI 影响：复用现有 Agent 状态栏，增加“Novel Codex 专用 CLI”文案；本轮不新增设置页。
- P1：缺失 CLI、启动失败和本地降级必须有文字状态。
- P2：后续设置页展示 CLI 来源、版本、状态目录和登录状态。
- 条件：状态栏同时使用文字和状态颜色，保留现有 Codex 风格布局。

## 综合决策

四个角色均允许在条件落实后实施。本轮不复制桌面版 CLI 或认证文件；开发环境使用仓库构建产物，打包环境预留独立资源路径；无专用 CLI 时保持本地 Runtime 可用。

## 实施后复核

- `npm run build`：通过。
- `npm run test:runtime`：通过，3 项测试。
- `npm run test:agent`：通过，5 项测试，包含专用 CLI resolver、桌面 CLI 拒绝和凭据清理。
- Node 语法检查：`electron/main.mjs`、`electron/codex-adapter.mjs`、`electron/codex-runtime.mjs`、`electron/preload.cjs` 均通过。
- 当前机器 resolver 实测解析到 `/Users/hilbert/private/codex/codex-rs/target/debug/codex`，未解析到 ChatGPT.app。
- 待确认：用户桌面环境中由专用 CLI 完成真实 App Server 登录和首轮创作；打包时需要将版本固定的 CLI 放入 `resources/codex-runtime/bin`。

---

# Agent 窗口生命周期崩溃修复评审（第四轮）

## 实施后复核

- 根因：长期 Codex 事件回调引用已销毁窗口的 `event.sender`，`webContents.send()` 抛出 `Object has been destroyed`。
- 实施：新增 `webcontents-safety.mjs`；Agent 事件改为指向当前有效 WebContents；窗口销毁和应用退出时解绑回调；发送竞态由安全发送函数吸收。
- 验证：新增 live target、已销毁 target、销毁竞态 3 项测试；`npm run test:agent` 共 8 项通过。
- 兼容性：未改变 App Server JSONL 协议、审批决策或 renderer IPC API；UI 无结构和文案变化。
- 剩余风险：尚未在 macOS/Windows 的正式打包产物中执行窗口反复打开/关闭 smoke test，需在 Electron 运行时可用后补做。

---

# DeepSeek provider 试运行评审（第五轮）

## 变更目标

在不复用 ChatGPT 桌面版或普通 Codex CLI 环境的前提下，让 Novel Codex 可以通过环境变量选择 DeepSeek，并支持用户给出的 `DeepSeek-V4.1-Flash` 别名。

## 四角色实施后复核

### 技术专家 A——架构与安全

- 已解决：DeepSeek provider 写入 Novel Codex 专用 `CODEX_HOME/config.toml`，API key 不写文件。
- 已解决：仅当 provider 明确为 `deepseek` 时，才把 `DEEPSEEK_API_KEY` 注入专用 App Server 子进程；OpenAI 凭据和桌面版 Codex 凭据继续清除。
- 已解决：Dedicated config 使用 `https://api.deepseek.com`、Responses wire API 和 `DEEPSEEK_API_KEY` 环境变量。

### 技术专家 B——协议与测试

- 已解决：`DeepSeek-V4.1-Flash` 启动配置会规范化为当前模型标识 `deepseek-flash`，并通过 `thread/start` 传入。
- 已解决：增加 provider 解析、配置无密钥落盘和 provider 选择性注入测试。
- 验证：`npm run test:agent` 11 项通过，`npm run test:runtime` 3 项通过，`npm run build` 通过。

### 产品经理——范围与可用性

- 已解决：无需新增登录页；开发环境通过环境变量即可试运行，缺少 key 时仍保留本地 Runtime，并显示明确错误。
- 已解决：第一版状态栏区分 `DeepSeek · deepseek-flash`、专用 CLI 和本地 Runtime。
- 后续：provider/model 设置页和独立凭据管理仍属于后续版本。

### UI/UX 设计师——反馈与安全提示

- 已解决：状态栏展示 provider/model 和认证错误，不展示 API key。
- 已解决：沿用现有 Codex 风格三栏布局，不新增复杂配置流程。
- 后续：设置页需要补充密钥输入的系统安全存储、清除和切换反馈。

## 决策

允许当前试运行方案进入验证阶段。真实请求前必须轮换已暴露的 API key，并仅通过当前 shell 的环境变量传入；不得提交到仓库、配置文件或日志。

---

# Agent 执行轨迹 UI 评审（第六轮）

## 实施后复核

- 技术专家 A：已解决。新增 `src/agent-events.ts` 作为 Renderer 侧事件归一化边界；内容有长度限制，未知/缺失字段安全降级，不再全量渲染原始 JSON。
- 技术专家 B：已解决。delta 使用固定的 `agent-message` 记录累积，item 使用稳定 id upsert，`turn/completed.turn.items` 可补齐没有 delta 的最终 Agent 消息；`npm run build` 和 `npm run test:agent` 通过。
- 产品经理：已解决。用户可在同一面板看到指令、执行中、等待确认、已完成和失败；轨迹状态独立于左侧页面切换，发送期间阻止重复提交。
- UI/UX 设计师：已解决。新增 Codex 风格 Agent Activity 时间线、流式光标、工具/审批/错误区分、状态徽标、清除入口、`role="log"` 和 `aria-live="polite"`。
- 验证：`npm run build` 通过；`npm run test:agent` 11 项通过；`npm run test:runtime` 3 项通过；Electron 文件语法检查通过。
- 剩余风险：尚未在正式 macOS/Windows 打包产物中进行视觉和窗口生命周期 smoke test；取消当前 turn、完整 transcript 持久化和多线程仍留待后续版本。

---

# 产品与 UI/UX 体验优化评审（第七轮）

## 变更目标

让 Novel Codex 的 Agent 工作流更接近 Codex：执行过程可见、侧栏 Activity 可用、长任务可停止、失败或完成后可重试，并保持小说编辑工作区不被执行状态遮挡。

## 四角色实施后复核

- 技术专家 A：已解决。取消链路沿用 App Server `turn/interrupt`，仍由 Electron 主进程发送；Renderer 只通过窄化的 `agent:interrupt` IPC 调用，不接触子进程或 stdin。
- 技术专家 B：已解决。适配器增加 active-turn interrupt，fixture 增加中断场景；事件 reducer 增加 cancelled 状态、稳定步骤更新和安全空态。`npm run test:agent` 12 项通过。
- 产品经理：已解决。新增可验证的执行中/等待确认/已完成/已停止/失败反馈，支持重试和重复提交保护；“与 Codex 协作”按钮改为聚焦 Composer，减少无效点击。
- UI/UX 设计师：已解决。右侧 `Story Context`/`Activity` 标签可用；Activity 空态、运行中徽标、步骤摘要、完整轨迹跳转、停止/重试按钮和键盘可操作状态已补齐，并保持现有 Codex 风格深色三栏布局。
- 验证：当前 Electron 窗口已通过 AX 检查确认 Activity 标签切换与空态可见；`npm run build` 通过；`npm run test:agent` 12 项通过；`npm run test:runtime` 3 项通过。
- 剩余风险：当前轨迹仍只保留最近一轮，未做跨重启持久化；取消后的服务端最终状态依赖 App Server 事件回传；正式 macOS/Windows 打包 smoke test 仍待后续发布验证。

---

# 浅色主题与产品体验优化评审（第八轮）

## 变更目标

响应“黑色背景压抑”的反馈，采用浅色作为默认工作环境：用 Apple 风格的浅灰/白色层级承载长篇写作，用 Google 风格蓝色表达 Agent 操作状态，同时保留深色主题供夜间创作使用。

## 方案

- 不改变 Electron 主进程、Runtime、App Server 或 IPC 协议，仅调整 Renderer 的视觉层和主题状态。
- 默认主题为 `light`，通过顶栏主题按钮切换 `light` / `dark`；偏好仅保存在 Novel Codex 当前 WebContents 的 `localStorage`。
- 浅色层级使用 `#F5F5F7` 页面底、白色卡片和 `#FBFBFD` 上下文栏；正文采用接近黑色的高对比文本。
- Agent 主操作与运行中状态使用 Google Blue，创作品牌和流式光标保留紫色强调；等待确认、错误和成功状态使用独立语义色。

## 四角色实施后复核

### 技术专家 A——架构与安全

- 已解决：主题变更完全位于 React Renderer 与 CSS 主题选择器，不触碰 Codex 子进程、凭据隔离或 Creative Runtime。
- 已解决：主题偏好不写入项目业务数据、Codex 配置或日志，也不包含任何密钥。
- 已解决：显式 `data-theme="dark"` 才启用旧深色样式；没有偏好或存储不可用时稳定回退到浅色。

### 技术专家 B——可靠性与验证

- 已解决：主题初始值同步读取本地偏好，切换后同步更新根节点和 `localStorage`。
- 已解决：浅色规则使用主题作用域覆盖现有样式，避免引入新的组件状态分支；深色兼容路径保留。
- 验证：`npm run build` 通过；`npm run test:agent` 12 项通过；`npm run test:runtime` 3 项通过。

### 产品经理——范围与价值

- 已解决：默认打开即可进入低压的写作环境，不增加登录页或额外配置流程。
- 已解决：主题切换位于顶栏且不打断写作；Agent 执行轨迹、Activity、停止和重试能力继续保留。
- 后续：设置页可将主题、模型/provider、运行时状态和快捷键统一收纳，但不阻塞第一版小说创作。

### UI/UX 设计师——视觉与可用性

- 已解决：背景从纯黑切换为 Apple 风格的暖冷中性浅灰，卡片使用白色分层，避免大面积刺眼纯白。
- 已解决：蓝色仅用于 Agent、链接和主要操作，紫色用于 Novel Codex 创作品牌，状态色保持语义一致。
- 已解决：主题按钮提供 `title` 与 `aria-label`，支持键盘操作，并保留深色模式作为夜间创作选项。
- 后续：正式 macOS/Windows 打包后补充多窗口、系统级深色偏好和高对比度审查。

## 决策

四角色允许实施，本轮已落地并通过构建与自动化回归。视觉 smoke test 仍需在正确启动的 Electron 窗口中补做；当前 CUA 检查曾捕获 Electron 默认欢迎页而非 Novel Codex 页面，因此不将该次检查计为视觉通过。

---

# Agent 统一事件模型与 Codex 对齐评审（第九轮）

## 变更概览

- 背景/问题：当前 App Server 事件已经可以显示执行轨迹，但事件直接由 Renderer 解释；Local Runtime 只有一条合成完成消息，导致两种模式的工作流结构不一致。
- 目标：统一“阶段—步骤—产物—总结”的 Agent Run 展示模型，让用户能回答 Agent 当前在做什么、是否修改了内容、最终完成了什么。
- 非目标：不复制 Codex 全量历史/多线程 UI，不实现跨重启持久化，不改变 Codex 子进程、IPC、凭据隔离或 Creative Runtime 数据契约。
- 影响范围：`src/agent-events.ts`、`src/App.tsx`、`src/styles.css`，以及对应的验证记录；不新增 Electron 权限和外部依赖。

## 方案设计

### 候选方案

| 方案 | 优点 | 缺点/风险 | 结论 |
| --- | --- | --- | --- |
| A. 只调整 UI 文案和标签 | 改动小 | 无法统一 App Server 与 Local Runtime 的事件结构 | 不采用 |
| B. 统一 Renderer Agent 事件模型 | 保留现有架构，能同时适配两种 Runtime，风险可控 | 需要兼容现有 raw event 方法 | 采用 |
| C. 完整复制 Codex transcript/history | 视觉和功能上最接近 Codex | 范围扩大到历史任务、多线程、协议版本和持久化 | 不采用 |

### 推荐方案

- 架构与模块边界：`src/agent-events.ts` 负责将 App Server raw JSONL 和 Local Runtime 结果归一化为阶段、步骤、产物和总结；`src/App.tsx` 只负责状态展示与用户操作；Electron 主进程和 preload 边界不变。
- 数据流：App Server `JSONL -> safeMessage -> IPC -> reduceAgentEvent`；Local Runtime `HTTP command result -> local event sequence -> reduceAgentEvent`。两条链路共享同一份 UI 状态模型。
- 错误处理：未知事件安全忽略；事件内容继续保持有界；命令、路径、diff 仅作为文本渲染；审批仍必须由用户显式决定；失败、停止和重试沿用现有链路。
- UI：增加紧凑阶段轨道、可展开步骤、产物摘要和最终总结；右侧 Activity 只展示阶段摘要并支持跳转；覆盖运行中、等待确认、完成、失败、停止和空态。
- 安全与跨平台：不增加 Node 能力、文件能力或网络能力；仅使用现有 IPC 和 CSS，保持 macOS/Windows 兼容。
- 测试与验收：保留现有 App Server 正常/审批/中断/错误回归，新增统一模型的构建检查，并确认 Local Runtime fallback 不展示虚假的工具执行记录。

## 四角色评审

### 技术专家 A——架构与安全

- 结论：`APPROVE WITH CONDITIONS`
- P1：原始事件必须继续在主进程有界转发，Renderer 只接收归一化可展示内容；文件路径、命令输出只能作为文本展示，不能触发文件操作。
- P1：未知事件必须安全忽略或降级，不能破坏当前轮次状态。
- 建议与责任：复用现有 `safeMessage` 和长度限制，不扩展 Electron 权限，不改变审批语义；实现责任为 Agent 事件模块。

### 技术专家 B——实现与质量

- 结论：`APPROVE WITH CONDITIONS`
- P1：兼容现有 `turn/*`、`item/*`、审批和错误事件，不能因新增阶段模型破坏已有 reducer。
- P1：覆盖 App Server 正常完成、审批、停止、错误，以及 Local Runtime fallback 的构建和回归测试。
- 建议与责任：保持现有事件入口和测试夹具，新增的 UI 状态只通过现有 `reduceAgentEvent` 生成；实现责任为事件 reducer 和 UI。

### 产品经理——目标与验收

- 结论：`APPROVE WITH CONDITIONS`
- P1：用户必须能回答“Agent 当前在做什么、是否修改了内容、下一步是什么”。
- P1：Local Runtime 要和 Codex 展示同一套结构，但必须明确标记能力较弱，不能制造虚假的工具执行记录。
- 验收标准：两种模式都显示阶段、状态和完成结果；App Server 额外显示真实工具/审批/产物；失败可重试，停止可恢复。

### UI/UX 设计师——流程与体验

- 结论：`APPROVE WITH CONDITIONS`
- UI 影响：新增阶段轨道、产物折叠区和完成总结，但保留 Codex 风格三栏布局和底部 Composer。
- P1：阶段轨道要紧凑，不能挤压正文；详细命令、输出和 diff 默认折叠。
- P1：运行中、等待确认、完成、失败、停止必须同时有文字和颜色/图标表达，不能只依赖颜色。
- 状态与可访问性：保留 `role="log"`、`aria-live`、按钮键盘操作和显式 aria-label；空态、加载态、错误态和可恢复操作均需可读。

## 综合决策

四角色允许实施，以上 P1 转为实施项。本轮限定为 Agent 事件模型和工作流展示，不扩展到任务历史、多线程和跨重启持久化。

## 实施后复核

- 实际修改：`src/agent-events.ts` 增加 Agent 模式、四阶段状态、工具输出 delta、文件变更产物、完成总结和 Local Runtime 事件序列；`src/App.tsx` 增加阶段轨道、模式徽标、产物折叠区和完成总结；`src/styles.css` 增加深浅主题下的阶段/产物状态样式；fake App Server 增加命令输出和文件变更事件覆盖。
- 与方案的偏差：未修改主进程/preload/IPC 和 Runtime HTTP 契约；Local Runtime 仍不是真实规划 Agent，只生成诚实的结构化处理阶段和能力提示。
- 验证：`npm run build` 通过；`npm run test:agent` 12 项通过，包含命令输出和文件变更事件转发；`npm run test:runtime` 3 项通过；Electron 主进程、适配器、Runtime 和 preload 语法检查通过。
- 剩余风险：当前事件 reducer 仍是前端兼容层，尚未基于固定版本的 app-server schema 自动生成类型；当前 Agent Run 仍只保留最近一轮；正式 macOS/Windows 打包视觉 smoke test 仍待正确启动 Electron 窗口后执行。
- 后续工作：补充 schema 版本锁定、完整工具输出映射、任务历史/多线程和跨重启持久化；Local Runtime 若升级为真实 Agent，再将其事件源移动到 Runtime 服务端。

---

# Agent Timeline 可读性与低干扰优化评审（第十轮）

## 变更概览

- 背景/问题：统一事件模型已经落地，但步骤详情和产物详情默认展开，长命令、输出或 diff 可能挤压小说正文。
- 目标：让用户无需展开详情即可判断当前阶段、执行状态和是否产生内容变更；需要时再查看命令、输出和 diff。
- 非目标：不改变事件接收、审批、停止、重试、凭据隔离、任务历史、多线程和持久化范围。
- 影响范围：Renderer 的 Agent Timeline、Activity 摘要和 CSS；不新增依赖，不修改 Electron 主进程/preload/IPC 契约。

## 方案设计

### 候选方案

| 方案 | 优点 | 缺点/风险 | 结论 |
| --- | --- | --- | --- |
| A. 保持详情全部展开 | 实现零改动 | 长输出会干扰正文，信息密度过高 | 不采用 |
| B. 原生 details/summary 折叠详情 | 无依赖、键盘可用、兼容现有 DOM | 需要补充折叠状态样式 | 采用 |
| C. 新增独立日志窗口 | 可容纳更多内容 | 破坏当前工作区连续性，扩大窗口和生命周期范围 | 不采用 |

### 推荐方案

- 阶段轨道始终显示，当前阶段使用高亮；已完成、未使用、等待确认、失败有文字和图标状态。
- 步骤详情用原生 `details/summary` 默认折叠；步骤标题、状态和摘要保持可见。
- 产物变更使用默认折叠的 `details/summary`，只显示文件/章节名称、应用状态和数量。
- Activity 显示当前阶段、步骤计数和产物计数；完整轨迹继续通过定位按钮访问。
- 错误、停止、重试、审批入口和 `aria-live`/`role="log"` 保持不变。

## 四角色评审

### 技术专家 A——架构与安全

- 结论：`APPROVE WITH CONDITIONS`
- P1：折叠只改变展示，不能丢失事件或改变审批/取消链路。
- P1：详情内容继续使用现有有界文本，不能把原始 JSON 或秘密暴露到 UI。
- 建议与责任：只改 Renderer 组件与 CSS，复用现有数据边界；实现责任为前端模块。

### 技术专家 B——实现与质量

- 结论：`APPROVE WITH CONDITIONS`
- P1：兼容现有 AgentRun 数据结构和事件入口。
- P2：重新执行构建、Agent 回归和 Runtime 回归。
- 建议与责任：不引入新依赖，保持 fake App Server 测试；实现责任为前端模块与现有测试。

### 产品经理——目标与验收

- 结论：`APPROVE WITH CONDITIONS`
- P1：用户无需展开详情即可知道 Agent 当前状态和是否有产物。
- P1：停止、失败、重试和审批入口必须继续可见。
- 验收标准：运行中突出当前阶段；完成后显示总结和产物数量；Local Runtime 显示“本地处理/未调用工具”。

### UI/UX 设计师——流程与体验

- 结论：`APPROVE WITH CONDITIONS`
- UI 影响：Agent Timeline 和 Activity 侧栏增加低干扰信息层级，不改变三栏信息架构。
- P1：详细内容默认折叠，阶段轨道保持紧凑，正文区域优先。
- P1：状态同时使用文字、图标和颜色；折叠控件支持键盘和辅助技术。
- 状态与可访问性：使用原生 `details/summary`，保留 `aria-live`、`role="log"` 和现有按钮可操作性。

## 综合决策

四角色允许实施，所有 P1 已转化为本轮修改和验收项。本轮只优化展示层，不改变 Agent 数据流和权限边界。

## 实施后复核

- 实际修改：步骤命令/输出/diff 改为原生 `details/summary` 默认折叠；产物变更默认折叠；Activity 增加当前阶段、已完成步骤数和产物数；阶段轨道增加“未使用”状态。
- 与方案的偏差：未改变 Agent 事件入口、审批、停止、重试和权限边界；未新增独立窗口或第三方依赖。
- 验证：`npm run build` 通过；`npm run test:agent` 12 项通过；`npm run test:runtime` 3 项通过；Electron 主进程、适配器、Runtime 和 preload 语法检查通过。
- 剩余风险：视觉 smoke test 仍需在正确启动的 Novel Codex Electron 窗口中执行；当前 CUA 窗口曾停留在 Electron 默认欢迎页。

---

# Agent 会话内最近执行记录评审（第十一轮）

## 变更概览

- 背景/问题：当前 Agent Timeline 只展示最近一轮，用户连续创作时无法快速回看上一轮的结果。
- 目标：增加 Codex 风格的当前会话执行记录，帮助用户判断是否继续、重试或回到正文。
- 非目标：不做跨重启持久化、不保存完整 transcript、不做多线程和任务数据库。
- 影响范围：Renderer 的 Agent 状态、Timeline 和 Activity；不修改主进程、preload、IPC、Runtime 或 App Server 契约。

## 方案设计

### 候选方案

| 方案 | 优点 | 缺点/风险 | 结论 |
| --- | --- | --- | --- |
| A. 只保留当前轮次 | 状态简单 | 连续创作缺少上下文，无法回看结果 | 不采用 |
| B. 会话内有界摘要记录 | 能提供连续性，内存开销和隐私风险可控 | 应明确不是持久化历史 | 采用 |
| C. 完整持久化 transcript | 最接近 Codex 历史 | 需要安全存储、迁移和隐私策略，超出当前范围 | 不采用 |

### 推荐方案

- `AgentRunSummary` 只保存指令、模式、状态、总结、产物数和时间，最多 6 条，文本有长度上限。
- 新一轮发送、重试或清除前，将上一轮摘要放入当前会话内存；执行中的 turn 不允许重复归档。
- Timeline 底部增加默认折叠的“本次会话的其他执行”，显示完成/失败/停止/本地 Runtime 状态。
- 不写入 `localStorage`、Creative Runtime 数据库、Codex 配置或日志，不保存原始 JSON、命令输出和 API 凭据。
- 继续使用原生 `details/summary`、文字+图标+颜色状态和键盘可操作交互。

## 四角色评审

### 技术专家 A——架构与安全

- 结论：`APPROVE WITH CONDITIONS`
- P1：历史记录必须有数量和文本长度上限，不能存储 token、原始 JSON 或未裁剪输出。
- P1：会话历史只存在 Renderer 内存，不写入 localStorage 或业务数据库。
- 建议与责任：最多 6 条，字段全部摘要化；实现责任为 Agent 状态模块。

### 技术专家 B——实现与质量

- 结论：`APPROVE WITH CONDITIONS`
- P1：归档当前运行不能影响正在执行的 turn，也不能改变停止、审批和重试。
- P2：重新执行构建和现有 Agent/Runtime 回归。
- 建议与责任：只在开始下一轮时归档上一轮，运行中的重复提交仍被阻止；实现责任为 Renderer。

### 产品经理——目标与验收

- 结论：`APPROVE WITH CONDITIONS`
- P1：历史记录要能区分完成、失败、停止和 Local Runtime。
- P1：摘要必须能帮助用户决定是否重试或继续创作。
- 验收标准：连续执行两轮后可看到上一轮指令、状态、模式和结果摘要。

### UI/UX 设计师——流程与体验

- 结论：`APPROVE WITH CONDITIONS`
- UI 影响：Timeline 底部新增会话记录折叠区，不改变三栏信息架构和正文优先级。
- P1：历史记录默认折叠，不能抢占当前 Timeline 和正文注意力。
- P1：状态必须使用文字、图标和颜色，且支持键盘操作。
- 建议与责任：使用原生 `details/summary`，复用现有状态徽标和浅色主题；实现责任为 UI。

## 综合决策

四角色允许实施，所有 P1 已转化为本轮任务。本轮只实现有界会话摘要，不承诺跨重启历史。

## 实施后复核

- 实际修改：`AgentRun` 增加最多 6 条会话内 `AgentRunSummary`；新一轮发送或重试前归档上一轮终态；Timeline 增加默认折叠的“本次会话的其他执行”列表，区分完成、失败、停止和 Local Runtime。
- 隐私取舍：摘要只保留有界的指令、总结、模式、状态、时间和产物数量；不写入 localStorage、业务数据库、Codex 配置或日志，不保存完整步骤、命令输出、diff 或 API 凭据。
- UI 可用性：历史默认折叠，状态同时使用图标、文字和颜色；当前 Timeline 的停止、审批、失败和重试入口保持不变，原生 `details/summary` 支持键盘操作。
- 验证：`npm run build` 通过；`npm run test:agent` 12 项通过；`npm run test:runtime` 3 项通过。
- 与方案的偏差：清除按钮继续作为显式清空操作，不额外归档被清除的当前轮次；未新增主进程/preload/IPC 或持久化契约。
- 剩余风险：会话历史在应用重启后消失；正式 Electron 窗口视觉 smoke test 仍需在正确启动的 Novel Codex 窗口中执行。

---

# Codex App Server 协议对齐评审（第十二轮）

## 变更概览

- 背景/问题：当前 Agent Timeline 已能展示阶段、步骤、审批、产物和会话摘要，但仍缺少 Codex App Server 的计划流与统一 diff 流。
- 目标：补齐 `turn/plan/updated` 与 `turn/diff/updated` 的前端归约和可视化，使执行轨迹完整表达“准备做什么”和“实际改了什么”。
- 非目标：不扩展登录、会话持久化、权限策略、线程列表或 Runtime 隔离边界。
- 影响范围：Agent 事件归约、Timeline、Activity 摘要、测试夹具和架构文档；不改变 Electron 主进程/preload/IPC 契约。

## 四角色评审

### 两位技术专家

- 结论：`APPROVE WITH CONDITIONS`。
- 事件归约必须对计划步骤、diff 文本设置硬上限，并以更新替换旧状态，避免事件流导致内存或 DOM 无界增长。
- 需要记录 `thread/started` 与 `turn/started` 的标识用于事件关联，但不在 UI 中暴露原始 ID。
- 现有 `item/*` 和本地 Runtime 事件继续兼容；官方聚合 diff 作为独立变更摘要展示，不替换已有文件产物。

### 产品经理

- 结论：通过。
- 执行轨迹的最小闭环应为：用户指令 → Agent 计划 → 工具/回复流 → 统一变更 → 完成总结。
- 计划和变更都应低干扰呈现，默认不抢占实时步骤；用户需要时再展开查看。
- Local Runtime 必须继续明确标注“本地结构化操作”，不能伪装成 Codex 工具调用。

### UI/UX 设计师

- 结论：通过。
- 计划步骤使用与阶段轨道一致的完成/进行中/待处理语义，保持窄屏下的可读性。
- 统一 diff 采用原生 disclosure，沿用现有“执行详情”和“产物变更”的交互，不新增高密度弹窗。
- Activity 侧栏展示计划/变更计数，完整内容仍以主时间线为准，并保留键盘与辅助技术可读性。

## 综合决策与落地约束

四角色允许实施，所有条件纳入本轮验收：

1. `AgentRun` 增加有界的 `plan`、`diff` 以及 `threadId`、`turnId` 元数据。
2. `reduceAgentEvent` 支持官方计划与聚合 diff 通知，兼容 snake_case 状态写法。
3. 前端增加计划清单、统一变更折叠面板和 Activity 计数，不显示原始线程/轮次 ID。
4. Fixture、Adapter 测试、架构文档与实现后复核同步更新。

## 实施后复核

- 实际修改：`AgentRun` 增加有界的计划步骤、聚合 diff、thread/turn 关联元数据；`reduceAgentEvent` 接入 `thread/started`、`turn/plan/updated`、`turn/diff/updated`，并兼容 `in_progress` 状态写法。
- UI 可用性：Timeline 增加紧凑的执行计划和默认折叠的统一变更摘要；Activity 增加计划进度与变更计数；不在界面显示原始线程/轮次 ID。
- 兼容性：保留既有 `item/*`、approval、interrupt、error 和 Local Runtime fallback 事件路径；Local Runtime 只展示结构化本地计划，不伪装为 Codex 工具执行。
- 验证：`npm run build`、`npm run test:agent`、`npm run test:runtime` 通过。
- 剩余风险：事件 reducer 仍是前端兼容层，尚未基于固定版本 app-server schema 自动生成类型；正式 Electron 窗口视觉 smoke test 与完整任务历史仍待后续阶段。

---

# 活跃轮次交互与审批收敛评审（第十三轮）

## 变更概览

- 背景/问题：当前 Composer 在 Agent 执行中只能停止，无法像 Codex 一样向活跃 turn 追加指令；审批完成也缺少 `serverRequest/resolved` 的明确 UI 收敛。
- 目标：接入 `turn/steer`、让 `turn/interrupt` 关联当前 `turnId`、记录用户追加指令，并根据服务端可用决策展示审批按钮。
- 非目标：不改变 Local Runtime 能力、不自动批准任何请求、不引入通用 IPC、不实现跨线程切换或远程 WebSocket。
- 影响范围：Electron App Server adapter、preload 类型契约、Renderer Composer/Approval/Agent reducer、fixture 测试和文档。

## 四角色评审

### 技术专家 A——架构与安全

- 结论：`APPROVE WITH CONDITIONS`。
- P1：追加输入只能通过显式的 `agent:steer` IPC 进入当前 App Server turn；Renderer 不接触进程或原始凭据。
- P1：审批决策必须是显式白名单值；只有服务端声明支持的“本次会话允许”才可展示。
- 建议与责任：中断请求携带已知 `threadId`/`turnId`，继续由主进程维护权威状态；实现责任为 adapter、main、preload。

### 技术专家 B——实现与质量

- 结论：`APPROVE WITH CONDITIONS`。
- P1：保留既有 `sendTurn`，新增 `steerTurn`，不能破坏正常完成、审批、停止、错误和 fallback 测试。
- P1：追加指令只能在 running 的 App Server turn 发送；waiting、completed、error 和 local 状态必须安全拒绝。
- 建议与责任：fixture 覆盖 steer 请求参数、带 turnId 的 interrupt 和 `serverRequest/resolved`；实现责任为 adapter 与测试。

### 产品经理——目标与验收

- 结论：`APPROVE WITH CONDITIONS`。
- P1：执行中用户可以补充创作方向，且时间线可见“已追加指令”，不创建假的新轮次。
- P1：等待授权时 Composer 必须明确不可用，审批完成后状态自动恢复或结束。
- 验收标准：活跃 turn 中追加一句指令能被 app-server 接收并继续输出；审批响应后提示消失且不重复出现。

### UI/UX 设计师——流程与体验

- 结论：`APPROVE WITH CONDITIONS`。
- P1：执行中同时展示“追加”和“停止”两个动作，文案和图标不能只依赖颜色。
- P1：等待审批时显示“等待授权”，避免用户误以为追加输入会立即执行。
- P2：`acceptForSession` 仅在服务端提供该决策时显示，保持与 Codex 的能力协商一致。

## 综合决策与落地约束

四角色允许实施，所有 P1 纳入验收：

1. adapter 新增 `steerTurn`，`turn/start` 响应保存 `turnId`，interrupt 请求携带 `turnId`。
2. preload 与 `NovelCodexApi` 暴露窄接口 `steer`；Renderer 只在 Codex App Server running 状态调用。
3. reducer 处理内部 steer 记录与 `serverRequest/resolved`，不把原始审批响应当作普通文本展示。
4. 审批 UI 支持 accept/decline，并仅按 `availableDecisions` 条件展示 acceptForSession。

## 实施后复核

- 实际修改：`CodexAppServerAdapter` 新增 `steerTurn`，保存当前 turnId，并在 interrupt 请求中关联 turnId；main/preload/TypeScript API 增加窄接口 `agent:steer`。
- UI 可用性：活跃 Codex turn 的 Composer 同时提供追加和停止；追加成功后写入时间线；等待审批时禁用 Composer 并显示等待授权。
- 审批收敛：支持服务端声明的 `acceptForSession`；处理 `serverRequest/resolved` 时清除匹配提示，并在 Agent 时间线留下有界状态记录；网络审批显示 host/protocol，不把它伪装成 shell 命令。
- 兼容性：Local Runtime、正常完成、审批、停止、错误和既有 App Server 事件路径保持不变。
- 验证：`npm run build` 通过；`npm run test:agent` 13 项通过；`npm run test:runtime` 3 项通过；`git diff --check` 通过。
- 剩余风险：当前 adapter 仍只维护单个活动线程，尚未实现 `thread/resume`、`thread/list`、`turn/steer` 的多线程目标选择；正式 Electron 窗口视觉 smoke test 仍待正确启动 packaged/dev 窗口后执行。

---

# 线程历史与恢复评审（第十四轮）

## 变更概览

- 背景/问题：当前 App Server 连接只维护单个活动线程，用户无法查看隔离 Codex 环境中已保存的任务，也无法在应用重启后恢复任务。
- 目标：接入稳定的 `thread/list` 与 `thread/resume`，展示有界的任务历史，并允许在空闲状态恢复任务继续创作；为新线程设置简短标题，改善历史可读性。
- 非目标：不实现多线程并行、线程删除/归档、完整历史 transcript、`thread/turns/list` 实验性分页或跨环境导入。
- 影响范围：App Server adapter 生命周期、preload/API 类型、Renderer 任务历史 UI、fixture/适配器测试和架构文档。

## 四角色评审

### 技术专家 A——架构与安全

- 结论：`APPROVE WITH CONDITIONS`。
- P1：列表只返回标题、预览、时间、状态等有界摘要；线程 ID 只作为主进程恢复参数，不直接显示。
- P1：Renderer 不能直接发 JSON-RPC；恢复目标由主进程校验并调用隔离 App Server 的 `thread/resume`。
- 建议与责任：连接初始化与线程创建解耦，确保查看历史不会隐式创建空线程；实现责任为 adapter、main、preload。

### 技术专家 B——实现与质量

- 结论：`APPROVE WITH CONDITIONS`。
- P1：`thread/list` 失败只能降级任务历史，不得阻塞 Creative Runtime 和主编辑界面。
- P1：保留当前 `sendTurn`、`steerTurn`、approval、interrupt、local fallback 行为；恢复后下一轮仍复用已恢复线程。
- 建议与责任：fixture 覆盖 list、resume、thread/name/set 以及无历史/错误响应；实现责任为 adapter 与测试。

### 产品经理——目标与验收

- 结论：`APPROVE WITH CONDITIONS`。
- P1：用户能在“任务历史”中识别最近任务，并恢复后继续同一上下文，而不是新建空对话。
- P1：任务历史默认收起；恢复期间提供明确反馈，恢复失败不影响当前小说编辑能力。
- 验收标准：启动后能加载隔离 App Server 的线程摘要；选择一项后显示已恢复，随后发送内容进入该线程。

### UI/UX 设计师——流程与体验

- 结论：`APPROVE WITH CONDITIONS`。
- P1：任务列表使用标题、预览、时间和状态的紧凑层级；无标题时显示“未命名任务”，无历史时提供可理解空态。
- P1：加载、恢复中、恢复成功和失败均有文字反馈；列表按钮支持键盘操作和辅助技术标签。
- P2：当前任务和项目上下文保持分层，不把线程 ID、隔离路径或凭据状态放入用户界面。

## 综合决策与落地约束

四角色允许实施，所有 P1 纳入验收：

1. adapter 增加 `ensureInitialized`、`listThreads`、`resumeThread`，`thread/start` 只在真正发送新任务时执行。
2. 新增窄 IPC/API：`agent:list-threads` 和 `agent:resume-thread`，校验线程 ID、限制返回字段和数量。
3. Renderer 增加默认折叠的任务历史面板；恢复只在无活动 turn 时执行，成功后更新当前任务提示。
4. 新线程按首条指令设置有界标题；兼容旧 App Server 不支持标题接口的情况。

## 实施后复核

- 实际修改：adapter 拆分初始化与 `thread/start`，新增有界 `listThreads`/`resumeThread`，新线程按首条指令尝试设置标题；旧版本不支持标题时自动忽略该兼容性错误。
- IPC 与安全：main/preload 只暴露 `listThreads`、`resumeThread` 两个窄接口；线程 ID 在 adapter 校验，列表响应只保留标题、预览、时间、状态、provider 和 pin/ephemeral 标记，不向 UI 暴露隔离路径或凭据。
- UI 可用性：增加默认折叠的“CODEX TASKS”任务历史；支持加载中、空列表、恢复中、恢复成功和恢复失败文案；活动 turn 与等待审批时禁止切换线程。
- 兼容性：未实现多线程并行、归档/删除和实验性 turns/items 分页；当前选中线程仍由单个 adapter 会话维护，后续 turn 会继续进入已恢复线程。
- 验证：`npm run build` 通过；`npm run test:agent` 14 项通过；`npm run test:runtime` 3 项通过；Electron 主进程、adapter、preload 语法检查通过。
- 剩余风险：正式 Electron 窗口视觉 smoke test、真实 App Server 历史数据兼容和跨线程切换仍需后续验证；线程历史读取失败时仅降级该面板，不影响小说编辑与本地 Runtime。

---

# 评审机制升级：新增架构专家（第十五轮）

## 变更概览

- 背景/问题：既有评审包含两位技术专家、产品经理和 UI/UX 设计师，但长期架构、边界、数据契约和演进风险与实现层审查存在职责重叠，缺少单独的架构结论。
- 目标：新增独立的“架构专家”角色，形成五角色评审门禁；架构专家必须对系统分层、模块边界、领域模型、集成方式、可演进性和技术债务给出明确结论。
- 非目标：不改变应用代码、运行时行为、Codex App Server 协议、权限策略或既有历史评审结论；不把架构专家伪装成外部真人或独立代理。
- 影响范围：`design-review-gate/SKILL.md`、`design-review-gate/references/review-template.md` 以及后续所有代码变更的评审流程。

## 五角色评审

### 架构专家——边界与演进

- 结论：`APPROVE WITH CONDITIONS`。
- P1：架构审查必须单独记录，至少覆盖模块边界、数据/接口契约、依赖方向、扩展与迁移路径；不能用技术专家 A/B 的实现结论替代。
- 建议与责任：后续每个变更先在方案中列出架构影响和长期取舍；责任归属为设计方案作者与架构评审角色。

### 技术专家 A——可靠性与系统风险

- 结论：`APPROVE WITH CONDITIONS`。
- P1：职责调整为依赖、可靠性、安全、性能和可运维性，避免与架构专家重复；现有历史记录保持原样。
- 建议与责任：新评审记录按五角色模板填写；责任归属为评审记录维护者。

### 技术专家 B——实现与质量

- 结论：`APPROVE WITH CONDITIONS`。
- P1：继续要求兼容性、边界条件、测试覆盖和可执行迁移方案；新增角色不能降低原有实现门禁。
- 建议与责任：实现前确认五个角色的条件都已转成任务或验收项；责任归属为实施者。

### 产品经理——目标与范围

- 结论：`APPROVE`。
- P2：架构评审应服务于产品目标，不得借新增角色无故扩大需求范围。
- 验收标准：后续变更记录同时包含架构结论和产品验收标准。

### UI/UX 设计师——流程与体验

- 结论：`APPROVE`。
- UI 影响说明：本轮仅修改评审机制文档，无应用 UI 变化；后续 UI 变更仍需覆盖加载、空态、错误、成功、撤销、键盘和辅助技术场景。
- 建议与责任：按五角色模板继续记录 UI 结论；无 UI 影响时明确写 `N/A` 及理由。

## 综合决策

- 最终结论：`APPROVE WITH CONDITIONS`。
- 已解决的阻塞项：已在 skill 中将四角色门禁升级为五角色门禁，并区分架构专家与两位技术专家的职责。
- 带条件批准项及验收方式：后续每次代码、配置、接口、测试或 UI 修改前，必须存在五个独立标注的评审结论；缺少架构专家结论时不得开始实施。
- 需要用户决策的事项：无。

## 实施后复核

- 实际修改：更新 `design-review-gate/SKILL.md` 的角色定义、评审流程、通过条件和输出格式；更新 `review-template.md` 增加架构专家章节；在本文件记录机制升级。
- 与方案的偏差：无；历史轮次中的“四角色”表述保留为当时的真实记录，不回写历史结论。
- 验证命令与结果：skill-creator 自带校验脚本因环境缺少 Python `yaml` 模块未能运行；使用系统 YAML 解析器完成 frontmatter 校验，文本检查确认无 TODO 占位符和旧门禁表述。
- 剩余风险：当前已存在的历史评审记录仍使用旧角色结构，这是有意保留的审计记录；从第十五轮起按五角色执行。

---

# Codex 任务置顶与可逆归档评审（第十六轮）

## 变更概览

- 背景/问题：任务历史已经支持查看和恢复，但缺少 Codex 风格的置顶、活动/归档筛选和可逆归档，长期任务会难以维护。
- 目标：接入稳定的 `thread/list` 筛选、`thread/metadata/update` 置顶、`thread/archive` 归档和 `thread/unarchive` 恢复；在任务历史面板提供活动/已归档视图及可访问操作。
- 非目标：不实现永久删除、多线程并行、跨 provider 迁移、完整 transcript 分页或自动归档策略。
- 影响范围：App Server adapter、Electron main/preload IPC、TypeScript API、任务历史 UI/CSS、fixture、适配器测试和架构文档。

## 方案设计

- 架构与模块边界：adapter 作为唯一 JSON-RPC 边界；main 只暴露 `listThreads`、`updateThreadMetadata`、`archiveThread`、`unarchiveThread` 四个窄接口；Renderer 维护当前活动/归档筛选和有界摘要，不接触原始协议。
- 数据流/控制流：活动或已归档视图通过 `thread/list({ archived })` 加载；置顶调用 `thread/metadata/update({ threadId, isPinned })`；归档/恢复调用对应 RPC 后刷新当前列表；已归档任务打开时先取消归档再恢复线程。
- 错误处理与恢复：所有线程 ID 在 adapter 校验；App Server 错误只显示在任务面板；归档失败保留当前列表；运行中或等待审批时禁止切换和归档；不提供不可逆删除。
- UI 信息架构与交互状态：任务历史保持默认折叠；工具栏提供“活动/已归档”和刷新；每行提供恢复/打开、置顶/取消置顶、归档/恢复；按钮有文字化 `aria-label`、`title` 和 disabled 状态。
- 安全、性能与兼容性：列表固定最多 20 项，摘要字段继续有界；不返回路径、凭据或原始 JSON；只依赖已记录的 App Server v2 方法；保留旧的恢复、追加、审批、中断和 Local Runtime 路径。
- 测试与验收：fixture 区分活动与归档查询，并覆盖置顶、归档、恢复和恢复线程；构建、Agent/Runtime 回归、Node 语法检查和 diff 检查通过。

## 五角色评审

### 架构专家——边界与演进

- 结论：`APPROVE WITH CONDITIONS`。
- P1：任务状态操作必须继续集中在 adapter/main，不能在 Renderer 拼装 JSON-RPC；归档当前线程后要清除 adapter 的活动线程关联，避免下一轮误写入已归档任务。
- P2：未来若支持多线程并行，应把当前单线程 adapter 会话模型升级为显式线程上下文，而不是继续增加隐式全局状态。
- 建议与责任：本轮由 adapter 负责 RPC 与活动线程清理，架构专家复核状态转移。

### 技术专家 A——可靠性与系统风险

- 结论：`APPROVE WITH CONDITIONS`。
- P1：IPC 校验布尔参数，adapter 校验线程 ID；归档/恢复失败不能破坏编辑器和本地 Runtime；继续限制列表数量和文本长度。
- 建议与责任：main/preload 保持窄接口，adapter 统一错误边界；责任为 Electron 实现者。

### 技术专家 B——实现与质量

- 结论：`APPROVE WITH CONDITIONS`。
- P1：fixture 必须验证 `archived` 查询参数和四个任务管理 RPC；现有 send/resume/steer/approval/interrupt 回归不得退化。
- P2：真实 App Server 的归档后事件和不同版本返回字段需在后续兼容测试中补充。
- 建议与责任：新增 adapter 集成测试，并执行 build、Agent、Runtime 和语法检查；责任为实现者。

### 产品经理——目标与验收

- 结论：`APPROVE WITH CONDITIONS`。
- P1：用户能区分活动与已归档任务，置顶结果可见，归档后可以恢复；不得加入永久删除以扩大风险。
- 验收标准：活动任务归档后从活动列表消失并在归档列表出现；恢复后回到活动列表；置顶/取消置顶刷新后状态保持。
- 建议与责任：文案明确使用“归档/恢复”，避免与“删除”混淆；责任为产品与 UI 实现者。

### UI/UX 设计师——流程与体验

- 结论：`APPROVE WITH CONDITIONS`。
- P1：任务行不能嵌套按钮；打开、置顶和归档必须是独立且可键盘聚焦的控件，并使用状态文字、图标和辅助标签共同表达。
- P2：空态应随活动/归档视图变化，操作反馈应保持在任务面板内，不打断小说编辑。
- 状态与可访问性检查：加载、空列表、处理中、成功、失败、当前任务和运行中禁用状态均需有文字反馈。
- 建议与责任：使用现有折叠任务历史样式和浅色/深色主题变量；责任为 UI 实现者。

## 综合决策

- 最终结论：`APPROVE WITH CONDITIONS`。
- 已解决的 P1：新增四个窄 API，活动/归档列表筛选、置顶、可逆归档和恢复均由主进程 adapter 承担；运行中的任务不会被切换或归档。
- 实施范围：仅增加任务管理能力，不引入删除、多线程并行或 transcript 分页。

## 实施后复核

- 实际修改：adapter 新增归档筛选、置顶元数据、归档和恢复方法；main/preload/types 增加四个窄接口；Renderer 增加活动/归档切换、置顶、归档/恢复按钮和处理中状态；fixture 与 adapter 测试覆盖对应 RPC；架构文档同步更新。
- UI 可用性：任务行改为非嵌套按钮结构，打开、置顶、归档/恢复动作分别可键盘访问，并继续隐藏原始线程 ID。
- 兼容性：保留现有恢复、追加、审批、中断和 Local Runtime 路径；未实现永久删除和多线程并行。
- 验证：`npm run build` 通过；`npm run test:agent` 14 项通过；`npm run test:runtime` 3 项通过；`node --check electron/main.mjs`、`electron/codex-adapter.mjs`、`electron/preload.cjs` 通过；文件尾随空白检查通过。
- 剩余风险：真实 App Server 版本差异和正式 Electron 窗口视觉 smoke test 仍需在后续环境中验证。

---

# Codex 任务分页与只读详情评审（第十七轮）

## 变更概览

- 背景/问题：任务历史当前只展示有限摘要并直接恢复线程，无法浏览较早任务，也无法在不改变当前线程状态的情况下查看已保存内容。
- 目标：接入 `thread/list` 的 cursor 分页和稳定的 `thread/read(includeTurns)` 只读读取；在任务历史中提供“加载更多”和“查看详情”，让用户先浏览再决定是否继续。
- 非目标：不实现完整 transcript 编辑、线程 fork、多线程并行、永久删除、实验性 `thread/turns/list`/`thread/items/list` 或跨 provider 导入。
- 影响范围：App Server adapter、Electron main/preload IPC、TypeScript API、任务历史 UI/CSS、fixture、适配器测试、架构文档。

## 方案设计

- 架构与模块边界：adapter 是唯一 JSON-RPC 边界，新增有界 `readThread` 和可选 cursor 的 `listThreads`；main/preload 只转发窄接口；Renderer 只消费归一化摘要和只读详情。
- 数据流/控制流：首次列表请求省略 cursor；“加载更多”携带上次返回的 `nextCursor` 并追加去重；“查看详情”调用 `thread/read` 且 `includeTurns: true`，结果仅展示裁剪后的轮次/项目摘要，不恢复线程。
- 错误处理与恢复：cursor、线程 ID 和返回结构均校验；分页或详情失败只影响任务面板；详情加载中禁用重复读取；不改变当前活动线程、不自动产生新任务。
- UI 信息架构与交互状态：每行保留“继续/恢复”主操作，增加独立的“查看详情”操作；详情使用只读折叠区域，覆盖加载、空历史、失败、关闭和键盘访问；有下一页时显示“加载更多”。
- 安全、性能与兼容性：最多保留 20 项/页、最多 12 个轮次、每轮最多 8 个项目、每个文本最多 240 字；不展示原始线程 ID、工作目录、凭据或完整 JSON；兼容旧功能和本地 Runtime。
- 测试与验收：fixture 根据 cursor 返回不同页并提供 turns；测试断言分页、只读读取和既有任务管理回归；构建、Agent/Runtime 回归、Node 语法检查和差异检查通过。

## 五角色评审

### 架构专家——边界与演进

- 结论：`APPROVE WITH CONDITIONS`。
- P1：`thread/read` 必须保持只读，不能复用 `thread/resume` 或隐式改变 adapter 当前线程；详情归一化应留在 adapter 边界。
- P2：后续若引入完整 transcript，应单独设计分页/虚拟列表，不得取消本轮硬上限。
- 建议与责任：由 adapter 维护协议与裁剪边界，架构复核只读链路不改变会话状态。

### 技术专家 A——可靠性与系统风险

- 结论：`APPROVE WITH CONDITIONS`。
- P1：cursor、turn、item 和文本必须全部有界；服务端异常、缺失 turns 或未知 item 类型需安全降级。
- 建议与责任：main/preload 继续使用窄接口，Renderer 不渲染原始响应；责任为 Electron 实现者。

### 技术专家 B——实现与质量

- 结论：`APPROVE WITH CONDITIONS`。
- P1：fixture 必须验证首/后续页 cursor 和 `thread/read` 参数；现有置顶、归档、恢复、send/steer/approval/interrupt 测试不得退化。
- P2：真实服务端不同 items 形态的兼容映射在后续协议测试中扩展。
- 建议与责任：新增 adapter 集成覆盖并执行完整前端/Runtime 回归；责任为实现者。

### 产品经理——目标与验收

- 结论：`APPROVE WITH CONDITIONS`。
- P1：查看详情不能误触发恢复；用户能从历史详情识别任务后，再明确选择继续。
- 验收标准：存在下一页时可加载并追加；查看任务详情不改变当前任务；空详情、失败和关闭都有明确反馈。
- 建议与责任：将“查看详情”和“继续任务”使用不同文案和动作；责任为产品与 UI 实现者。

### UI/UX 设计师——流程与体验

- 结论：`APPROVE WITH CONDITIONS`。
- P1：详情、继续、置顶和归档必须是独立可聚焦控件，图标配合 `aria-label`/`title`；详情区域不显示原始 ID。
- P2：详情内容使用轮次、角色/工具类型和状态层级，较长文本默认裁剪，避免挤压正文工作区。
- 状态与可访问性检查：加载更多、详情加载、空历史、读取失败、空详情、关闭和键盘操作均需可感知。
- 建议与责任：复用当前任务历史浅色/深色主题样式；责任为 UI 实现者。

## 综合决策

- 最终结论：`APPROVE WITH CONDITIONS`。
- 已解决的 P1：新增只读 `thread/read` 和 cursor 分页窄链路；详情不恢复线程；所有返回内容有界；保留既有任务管理和 Agent 执行路径。
- 实施范围：仅增强任务历史浏览，不加入 transcript 编辑、fork、删除或多线程并行。

## 实施后复核

- 实际修改：adapter 新增有界 `thread/read` 和 cursor 分页；main/preload/types 增加只读详情与 cursor 参数；Renderer 增加“查看详情”、轮次/执行项摘要和“加载更多”；fixture、适配器测试和架构文档同步更新。
- UI 可用性：查看详情与继续任务为独立按钮，详情只读且默认裁剪；覆盖加载、空详情、错误、关闭、分页和运行中禁用切换状态。
- 与方案的偏差：无；未引入 transcript 编辑、fork、删除或多线程并行。
- 验证：`npm run build` 通过；`npm run test:agent` 14 项通过；`npm run test:runtime` 3 项通过；`node --check electron/main.mjs`、`electron/codex-adapter.mjs`、`electron/preload.cjs` 通过；文件尾随空白检查通过。
- 剩余风险：真实 App Server 的完整历史 item 类型和正式 Electron 窗口视觉 smoke test 仍需后续验证。

---

# Codex 任务实时状态同步评审（第十八轮）

## 变更概览

- 背景/问题：任务历史目前主要依赖发送完成后的刷新，运行中、等待授权、已完成或异常状态不能及时反映在任务列表。
- 目标：消费 App Server 的线程生命周期通知，将 `thread/started`、`thread/status/changed`、`thread/name/updated`、`thread/archived` 和 `thread/unarchived` 有界合并到当前任务列表。
- 非目标：不改变 adapter 当前线程选择、不实现多线程并行、不从 Renderer 发送 JSON-RPC、不新增自动轮询或永久删除。
- 影响范围：Renderer 任务状态合并、任务历史状态文案、fixture 事件、适配器回归测试、架构文档。

## 方案设计

- 架构与模块边界：adapter 继续转发 App Server 事件；Renderer 增加独立的任务摘要同步函数，只按线程 ID更新已加载列表或插入有界的 `thread/started` 摘要。
- 数据流/控制流：收到状态事件后，仅更新对应 `ThreadSummary.status`；归档事件从活动列表移除，归档视图不凭空插入缺少摘要的线程；取消归档事件从归档列表移除；名称和置顶事件只更新对应字段。
- 错误处理与恢复：未知事件、缺失线程 ID、未知状态或不完整线程对象安全忽略；列表刷新仍是最终一致性来源；状态同步失败不影响 Composer、编辑器或 Local Runtime。
- UI 信息架构与交互状态：任务状态显示“执行中/等待授权/可继续/空闲/异常”；当前任务仍保留当前标记，运行中继续禁用切换和归档；状态不依赖颜色，沿用文字与现有图标层级。
- 安全、性能与兼容性：线程摘要字段和文本继续有界，最多处理单个事件；不展示原始 ID、路径、凭据或 JSON；兼容服务端不发送这些通知的情况。
- 测试与验收：fixture 发送状态通知，适配器验证事件转发；构建、Agent/Runtime 回归、Node 语法检查和文件审计通过。

## 五角色评审

### 架构专家——边界与演进

- 结论：`APPROVE WITH CONDITIONS`。
- P1：事件同步只能更新 Renderer 的任务摘要，不能通过事件隐式切换 adapter 当前线程或改变会话控制流。
- P2：多线程并行阶段应抽取显式的线程状态存储，不继续扩展当前组件内的隐式单线程状态。
- 建议与责任：由 Renderer 维护显示层投影，adapter 继续作为协议边界；责任为架构与前端实现者。

### 技术专家 A——可靠性与系统风险

- 结论：`APPROVE WITH CONDITIONS`。
- P1：缺失或异常事件必须安全忽略；状态文本、线程字段和列表增长必须有界；列表刷新仍保留为恢复机制。
- 建议与责任：不引入轮询和额外持久化；责任为前端实现者。

### 技术专家 B——实现与质量

- 结论：`APPROVE WITH CONDITIONS`。
- P1：fixture 覆盖状态事件，现有 thread/read、分页、置顶、归档、恢复、send/steer/approval/interrupt 测试不得退化。
- P2：真实服务端 activeFlags 的更多枚举在后续协议兼容测试中扩展。
- 建议与责任：执行构建、Agent/Runtime 回归和语法检查；责任为实现者。

### 产品经理——目标与验收

- 结论：`APPROVE WITH CONDITIONS`。
- P1：用户在任务运行或等待授权期间能看到及时状态，状态变化不能打断正在编辑的小说内容。
- 验收标准：收到运行中/等待授权/完成/异常事件后，匹配任务行状态更新；归档和取消归档事件使对应视图及时移除；无事件时既有刷新逻辑继续可用。
- 建议与责任：状态文案保持与 Agent Activity 一致；责任为产品与前端实现者。

### UI/UX 设计师——流程与体验

- 结论：`APPROVE WITH CONDITIONS`。
- P1：状态必须同时使用文字和现有视觉标记；任务行不能因实时更新失去键盘焦点语义；运行中禁用行为要有明确原因。
- P2：异常状态不能使用“可继续”等误导性文案，应显示“异常”并保留刷新/恢复入口。
- 状态与可访问性检查：实时状态更新应位于已有任务面板语义内，避免额外弹窗或高频 aria 播报。
- 建议与责任：复用现有 `thread-history-status` 样式，不增加颜色体系；责任为 UI 实现者。

## 综合决策

- 最终结论：`APPROVE WITH CONDITIONS`。
- 已解决的 P1：事件仅作为任务列表显示层投影；未知事件安全忽略；现有刷新作为最终一致性兜底；不扩大线程控制和持久化范围。
- 实施范围：仅增加实时状态同步和状态文案，不引入多线程并行或轮询。

## 实施后复核

- 实际修改：Renderer 新增有界线程事件投影，实时合并启动、状态、名称、置顶、归档和取消归档事件；状态文案补充“等待授权”；fixture 增加状态与元数据事件，适配器测试验证状态事件转发；架构文档同步更新。
- UI 可用性：活动任务列表会及时反映执行中/等待授权/空闲/异常状态；归档事件会从对应视图移除；未识别或缺失线程 ID 的事件不会改变 UI。
- 与方案的偏差：无；未引入轮询、多线程并行或额外持久化。
- 验证：`npm run build` 通过；`npm run test:agent` 14 项通过；`npm run test:runtime` 3 项通过；`node --check electron/main.mjs`、`electron/codex-adapter.mjs`、`electron/preload.cjs` 通过；文件尾随空白检查通过。
- 剩余风险：真实 App Server 版本可能省略部分通知或提供新的 `activeFlags`，需以后续兼容测试确认。

## 第十九轮：区分权限审批与结构化用户回答

### 背景与目标

- 背景：当前 Renderer 将 `item/tool/requestUserInput` 与工具权限请求共用一张审批卡片，用户只能批准或拒绝，无法把问题答案回传给 App Server，Agent 可能因此停在等待状态。
- 目标：兼容 `item/tool/requestUserInput` 与 `tool/requestUserInput`，将“允许执行操作”和“回答 Agent 问题”分成两种明确交互；回答提交后沿用 `serverRequest/resolved` 收敛当前等待状态。
- 非目标：不引入通用表单设计器、MCP elicitation、远程浏览器授权或跨线程并行调度；不改变凭据、Runtime 和 Codex 配置隔离。

### 数据流与边界

```text
App Server requestUserInput
        │ JSONL
        ▼
Main / adapter：保存有界请求元数据并校验 response.answers
        │ narrow IPC
        ▼
Renderer：问题卡片 → 选项/自由输入 → { answers: {...} }
```

- 权限请求仍只回传 `{ decision: "accept" | "acceptForSession" | "decline" | "cancel" }`；用户问题只回传结构化 `answers`，Renderer 不拼接原始 JSON。
- 问题最多展示 3 个；问题、选项和答案均设长度上限；非法问题 ID、数组形状、答案类型和超长内容由 adapter 拒绝。
- 请求 ID 只在主进程和窄 IPC 中使用，不在卡片中展示；既有 `serverRequest/resolved` 仍负责清理对应等待提示。

### 五角色评审

#### 架构专家——协议边界与演进

- 结论：`APPROVE WITH CONDITIONS`。
- P1：请求类型识别、答案校验和 JSONL 回传必须集中在 adapter/main 边界；Renderer 只提交结构化答案，不直接接触子进程。
- P2：本轮不扩展多线程状态模型；用户回答不得隐式切换当前线程或改变 turn 控制流。

#### 技术专家 A——安全与可靠性

- 结论：`APPROVE WITH CONDITIONS`。
- P1：严格限制问题数量、文本长度、选项数量和答案键集合；不展示原始请求 JSON、凭据或内部 ID。
- P2：过期请求、空问题和非法 response 必须可理解地失败，不得吞掉既有审批异常。

#### 技术专家 B——实现与回归

- 结论：`APPROVE WITH CONDITIONS`。
- P1：fixture 必须覆盖用户问题请求、结构化回答、`serverRequest/resolved` 和完成事件；既有 approval、interrupt、steer、thread history 回归不得退化。
- 验收：构建、adapter 测试、Agent reducer 测试、Runtime 测试和 Node 语法检查通过。

#### 产品经理——任务完成闭环

- 结论：`APPROVE WITH CONDITIONS`。
- P1：用户必须一眼区分“需要批准”与“需要回答”；等待文案、提交反馈和失败反馈要与 Agent Activity 一致。
- 验收：选择选项或填写自由回答后，提交按钮发送答案并继续当前 turn；无有效问题时不允许提交空请求。

#### UI/UX 设计师——对齐 Codex 交互

- 结论：`APPROVE WITH CONDITIONS`。
- P1：采用紧凑、可折叠上下文中的任务卡片；每个问题使用 label/fieldset 语义，选项、自由输入、提交状态和错误状态均可键盘访问。
- P2：沿用现有浅色/深色主题与状态文字，不新增高饱和颜色体系；回答卡片不使用权限“批准”文案。

### 综合决策与实施后复核

- 综合结论：`APPROVE WITH CONDITIONS`；按上述 P1 落地。
- 实施：adapter 增加两种 requestUserInput 方法兼容识别与有界答案校验；Agent reducer 区分“等待你的回答”和“等待权限确认”；Renderer 增加选项/自由输入卡片；fixture 与 adapter 测试增加回答闭环。
- 保留：现有审批决策、实时线程状态、归档/恢复、steer、interrupt、Local Runtime fallback 和隔离环境均不变。

#### 实施后复核

- 实际修改：`ApprovalCard` 现在只处理授权决策；`UserInputCard` 负责最多 3 个问题的选项/自由输入和提交反馈；adapter 兼容两种 requestUserInput 方法并校验问题 ID、答案键、文本长度和可选项。
- 验证：`npm run build` 通过；`npm run test:agent` 15 项通过；`npm run test:runtime` 3 项通过；`node --check electron/main.mjs`、`electron/codex-adapter.mjs`、`electron/preload.cjs` 通过；文件尾随空白检查通过。
- 与方案的偏差：无；没有新增通用表单、MCP elicitation、远程授权或额外持久化。
- 剩余风险：不同 App Server 版本对 `answers` 的单选/多选编码可能存在差异；当前 adapter 接受字符串和字符串数组，后续以真实协议 fixture 继续扩展。

## 第二十轮：交互请求队列与状态收敛

### 背景、目标与范围

- 背景：adapter 已经保存多个待处理的 server request，但 Renderer 只有一个交互请求槽位；连续到达的权限审批或用户问题会互相覆盖，处理前一个请求后也可能错误恢复成“执行中”。
- 目标：在 Renderer 增加有界、去重、按到达顺序处理的交互请求队列；当前请求完成后自动显示下一个，并在队列非空时持续保持等待态。
- 非目标：不改变 App Server 的 request/response 协议，不引入多线程并行执行，不把队列持久化，不新增任意 IPC 或自动决策。

### 方案与取舍

- 选择 Renderer 队列而非 adapter 队列：adapter 继续保存协议层 pending request，Renderer 只保存显示层请求摘要，边界清晰且不影响主进程控制流。
- 选择 FIFO + requestId 去重：符合用户处理顺序，避免服务端重复通知造成重复卡片；单次最多显示 4 个请求，超出部分提示用户刷新/等待，不扩大内存。
- 保留同一张卡片组件：授权和用户回答继续使用不同的内容分支；切换 requestId 时重置表单状态，避免答案串到下一个问题。

### 数据流与错误处理

```text
server request → main adapter Map → narrow IPC event → Renderer FIFO queue
                                                     │
                          serverRequest/resolved ← resolve current request
```

- 请求缺少合法 ID 时不进入 UI 队列；相同 requestId 只保留一项。
- 解析/回传失败时当前请求仍保留，展示错误并允许重试；成功或服务端 `serverRequest/resolved` 后移除当前项。
- 队列仍有请求时，任务状态保持“等待交互”；队列清空后才允许显示运行中/完成态。

### 五角色评审

#### 架构专家——状态边界与一致性

- 结论：`APPROVE WITH CONDITIONS`。
- P1：队列只能作为 Renderer 显示投影，不能复制 adapter 的 turn 控制状态；`serverRequest/resolved` 是移除请求的协议事件。
- P2：队列实现保持在 App 组件附近；未来支持多线程时再抽取按 threadId 分区的状态存储。

#### 技术专家 A——可靠性与安全

- 结论：`APPROVE WITH CONDITIONS`。
- P1：限制队列长度、请求 ID 和请求摘要；禁止把原始 request JSON 或凭据写入队列、日志和持久化。
- P2：窗口关闭和 adapter 进程异常时清理 UI 队列，避免遗留“可操作”按钮。

#### 技术专家 B——兼容与测试

- 结论：`APPROVE WITH CONDITIONS`。
- P1：fixture 必须连续发送两个交互请求，测试验证 FIFO、去重、逐个 resolve 和既有流程回归。
- 验收：构建、Agent 测试、Runtime 测试、Node 语法检查和静态尾随空白检查通过。

#### 产品经理——任务闭环

- 结论：`APPROVE WITH CONDITIONS`。
- P1：当前请求和剩余排队数必须清楚可见；用户处理完一个请求后自动进入下一个，不需要重新发送创作指令。
- 验收：多个请求不会丢失或覆盖；队列清空后任务才恢复可继续/完成语义。

#### UI/UX 设计师——Codex 风格交互

- 结论：`APPROVE WITH CONDITIONS`。
- P1：队列提示使用紧凑辅助文案，不制造弹窗；当前卡片继续支持键盘访问、问题表单重置和错误重试。
- P2：等待文案统一使用“等待交互”，避免把用户问题误称为权限审批。

### 综合决策与实施后复核

- 综合结论：`APPROVE WITH CONDITIONS`；以上 P1 全部作为实施任务。
- 实施范围：新增 Renderer 有界交互队列、请求去重与队列状态派生；fixture 增加连续 user input 请求；adapter 测试验证 FIFO 处理和既有回归。

#### 实施后复核

- 实际修改：Renderer 使用最多 4 项的 requestId 去重 FIFO 队列，当前请求完成后自动切换下一项；权限审批与用户回答继续使用独立卡片分支；adapter 拒绝缺少合法 ID 的 server request。
- 验证：`npm run build` 通过；`npm run test:agent` 15 项通过；`npm run test:runtime` 3 项通过；`node --check electron/main.mjs`、`electron/codex-adapter.mjs`、`electron/preload.cjs` 通过；`git diff --check` 通过。
- 与方案的偏差：无；未新增 IPC、持久化、多线程调度或自动决策。
- 剩余风险：当前队列是单线程 Renderer 显示投影；真实服务端若同时推送不同 thread 的交互请求，后续需要按 threadId 分区，这是本轮明确保留的演进项。

## 第二十二轮：Codex 工作台完整理想态落地方案

### 目标与边界

- 目标：将当前 Novel Codex 从“可发送 App Server 请求的小说 UI”推进为完整的 Codex 风格本地工作台，覆盖任务生命周期、线程作用域、恢复、变更审查、事件投影、Review、诊断、后台终端、设置和键盘无障碍。
- 测试阶段策略：按工程根目录 `AGENTS.md` 执行，允许直接重塑当前接口和数据结构，不保留旧代码兼容层；每个阶段都保持当前目标状态可构建、可测试。
- 非目标：不加入登录页、不复用 ChatGPT 桌面环境、不改变 Codex Rust 核心、不把 API key 放入 Renderer、不实现云端协作。

### 分阶段实施

1. **任务与连接基础**：以 `threadId` 为状态边界，建立 `starting/running/waiting/reconnecting/completed/interrupted/failed` 状态机，支持重连、恢复、连接诊断和多任务切换。
2. **变更审查**：识别 command/fileChange/permissions/network 审批，显示操作对象和变更摘要，逐项接受/拒绝并准确呈现 `completed/failed/declined`。
3. **事件账本**：补充 `contextCompaction`、`thread/tokenUsage/updated`、`model/rerouted`、`hook/*`、`thread/closed`、Review 事件；事件按 itemId 有界合并。
4. **工作台布局**：左侧任务列表，中间 Transcript + Composer，右侧 Task Inspector（Plan/Changes/Usage/Diagnostics/Story Context）。
5. **交互质量**：命令面板、快捷键、焦点管理、状态播报、加载/空态/错误/成功/撤销状态和跨平台窗口体验。
6. **能力补齐**：`review/start`、后台终端查询/终止、Provider/Model 设置和诊断导出。

### 关键数据流

```text
App Server JSONL
      │
      ▼
Electron adapter：协议解析、线程/轮次校验、请求响应与进程恢复
      │ typed events
      ▼
Renderer TaskStore：按 threadId / turnId / itemId 建立显示投影
      ├── 左侧任务列表
      ├── 中间执行账本与 Composer
      └── 右侧任务 Inspector
```

- adapter 是唯一的协议事实源；Renderer 不控制线程、不拼接命令、不保存凭据。
- 事件和列表字段全部有界；未知事件安全显示为“未识别事件”或忽略，不阻塞任务。
- `thread/closed`、连接错误和进程退出进入可恢复状态；恢复只能显式调用 `thread/resume`，不能自动改变用户当前任务。
- 文件变更只有收到最终 `item/completed` 后才从“待审查”转为已应用、失败或拒绝；不以 `item/started` 推断成功。

### 五角色评审

#### 架构专家——任务模型与模块边界

- 结论：`APPROVE WITH CONDITIONS`。
- P1：必须先建立 TaskStore/adapter 状态边界，再扩展 Review、终端和 Inspector，避免继续把所有状态塞入 App 组件。
- P1：`threadId`、`turnId`、`itemId` 必须在 adapter 和 Renderer 投影中保持明确关联；不允许用单值状态覆盖多任务。

#### 技术专家 A——可靠性、安全和跨平台

- 结论：`APPROVE WITH CONDITIONS`。
- P1：断线、进程退出、错误响应、过期 request 和 WebContents 销毁必须可恢复且不泄漏密钥；所有新 IPC 继续白名单和输入校验。
- P1：后台终端和权限请求必须继续经过 App Server，禁止 Renderer 构造 shell 命令或自动批准。
- P2：macOS/Windows 路径、终端、快捷键和窗口尺寸使用平台抽象，不假设 POSIX。

#### 技术专家 B——落地与回归

- 结论：`APPROVE WITH CONDITIONS`。
- P1：每阶段同步更新 fixture、类型和集成测试；覆盖正常流、断线、恢复、审批、文件拒绝、Review、Token、Compaction 和未知事件。
- P2：测试阶段允许重写旧 fixture，不保留旧协议分支；禁止在未更新类型的情况下先加 UI 假数据。

#### 产品经理——Codex 对齐与小说价值

- 结论：`APPROVE WITH CONDITIONS`。
- P1：用户必须知道当前任务、使用的模型、运行状态、待处理变更和失败原因；所有恢复动作必须可解释。
- P1：Codex 通用能力不能吞掉小说核心体验；Story Canon、Scene/Beat、Chapter Diff 和 Story Tests 保持可见并与任务产物关联。
- P2：不做登录和云协作，先保证本地单用户闭环。

#### UI/UX 设计师——工作台和无障碍

- 结论：`APPROVE WITH CONDITIONS`。
- P1：采用左任务/中 Transcript/右 Inspector/底 Composer 的稳定布局；不让 Hero 内容挤压执行过程。
- P1：状态不只靠颜色；审批、变更、错误、重连、空态和加载态必须有文字、图标、焦点和可撤销路径。
- P2：流式区域只播报新增摘要，表单请求到达时可聚焦，快捷键和窄窗口布局必须可用。

### 实施顺序与验收

- 先落地阶段 1–3 的协议和状态基础，再落地阶段 4–6 的 UI 与能力入口；不在状态模型不稳定时堆叠视觉细节。
- 每阶段完成：构建、Agent/Runtime 测试、Node 语法检查、静态安全审计；Electron GUI 实机 smoke 受本地 Electron 二进制安装状态约束，不能用 renderer build 替代。
- 综合结论：`APPROVE WITH CONDITIONS`；所有 P1 已转为上述实施任务，允许在测试阶段直接采用理想态接口。

## 第二十一轮：测试阶段理想态开发规则

### 变更目标

- 当前工程处于测试阶段，后续实现以目标架构和理想产品闭环为优先，不为旧代码、旧 IPC、旧本地数据或旧 UI 行为保留兼容层。
- 允许直接调整接口、数据结构、事件模型和 UI；破坏性调整必须同步类型、测试、架构文档和评审记录，避免半迁移状态。
- 安全、隔离、IPC 校验、跨平台和测试要求不因测试阶段而降低。

### 五角色评审

- 架构专家：`APPROVE`。规则限定在 Novel Codex 子项目，减少临时兼容代码和技术债务。
- 技术专家 A：`APPROVE WITH CONDITIONS`。继续保留 Electron 安全边界、密钥隔离、IPC 校验和跨平台约束。
- 技术专家 B：`APPROVE WITH CONDITIONS`。破坏性调整必须同时更新类型、fixture、测试和文档，禁止半迁移。
- 产品经理：`APPROVE`。测试阶段优先验证理想工作流和产品闭环，减少历史行为对决策的干扰。
- UI/UX 设计师：`APPROVE`。允许重做信息架构和交互，但仍覆盖加载、空态、错误、成功、撤销和键盘操作。

### 综合决策

- 结论：`APPROVE`。
- 落地位置：新增工程根目录 `AGENTS.md`，作用域仅为 `eternallight/novel-codex`。
- 后续影响：后续设计和实现可直接采用理想态方案；不再为旧行为增加兼容分支，但每次有意破坏性变更仍需完成设计评审和回归验证。

## 第二十三轮：第二十二轮实施后复核

### 已落地

- adapter 初始化启用 experimental API；新增任务重连、Review、上下文压缩、后台终端查询/终止，以及 request 写入失败收敛。
- Renderer 新增连接状态、线程作用域过滤、文件变更审批摘要、任务检查器、Usage/Debug/Task/Changes/Story/Activity 面板、命令面板、快捷键和精简状态播报。
- Provider/Model 配置入口已接入主进程；配置写入隔离 Codex Home，Renderer 不接触 API Key。
- 任务列表进入左侧导航，中央区域保留 Transcript 与 Composer，右侧成为 Codex 风格 Task Inspector。
- fixture 覆盖 Review、Compaction、后台终端路径；新增完整 adapter 回归用例。

### 评审结论

- 架构专家：`APPROVE WITH CONDITIONS`。当前单 adapter 仍只允许一个活动 turn；任务切换和事件投影已按 threadId 收敛，真正并行 turn 留作下一阶段 TaskStore 拆分。
- 技术专家 A：`APPROVE`。新 IPC 通过白名单 handler，后台终端仍由 App Server 执行，密钥继续停留在主进程环境。
- 技术专家 B：`APPROVE`。构建、fixture、Runtime 和既有安全测试均通过；配置入口需要真实 App Server 环境再做一次端到端 smoke。
- 产品经理：`APPROVE WITH CONDITIONS`。当前任务、模型、连接、变更、用量和诊断均有可见入口；Review 结果会继续复用 Transcript 事件。
- UI/UX 设计师：`APPROVE`。左/中/右工作台、状态文字、命令面板和 `aria-live` 精简播报已落地；右侧窄宽度下通过横向 Inspector tabs 保持可用。

### 验证结果与剩余项

- `npm run build` 通过。
- `npm run test:agent`：16 项通过。
- `npm run test:runtime`：3 项通过。
- 剩余项：真实服务端不同版本的 Review/Compaction/Token 事件字段仍需在接入环境中校验；当前不伪造兼容层，按测试阶段理想态规则继续演进。
- Electron smoke：本次沙箱禁止监听 `127.0.0.1:5173`，本机启动权限申请又受当前 Codex 使用额度策略拒绝，因此未在本轮打开真实窗口；这不是代码构建失败，需在用户本机终端运行 `npm run dev` 完成窗口验收。

## 第二十四轮：对话优先展示重构方案

### 背景、目标与非目标

- 背景：当前中心区域虽然已经能展示 Prompt、阶段、计划、步骤、产物和总结，但这些内容处于近似同等视觉层级，实际体验更像执行日志，而不是连续的 Codex 对话；审批卡片还位于主内容顶部，容易脱离当前回合语境。
- 目标：将中心区域重构为“当前对话回合”——用户指令和 Agent 主回复优先，执行过程作为可折叠的辅助信息，审批、错误和内容变更紧贴当前回合；保留已落地的流式事件、状态、任务历史、变更和诊断能力。
- 非目标：不修改 App Server 协议、Electron IPC、Runtime 数据模型、凭据处理和任务调度；不在本轮实现真正的多线程并行状态仓库。

### 当前问题与受影响范围

1. `EnhancedAgentTranscript` 把 Agent 回复也渲染为普通步骤，用户需要从步骤列表中找主要答案。
2. 阶段和计划默认占用中心空间，完成后的执行细节仍然展开，造成信息噪音。
3. `EnhancedApprovalCard` 在中心内容顶部独立渲染，审批原因、当前任务和下一步之间缺少连续语义。
4. 右侧 Inspector 的 Activity 与中心步骤重复，造成两个“事实源”视觉入口。
5. `activity` 消息以多条条带出现在输入框下方，和对话内容重复；需要收敛为单条辅助状态。

### 推荐方案

中心区域采用以下固定层级：

```text
当前回合
├── 用户指令
├── Codex 主回复（流式/完成/无回复空态）
├── 等待交互（审批或用户问题，若存在）
├── 内容变更（若存在）
├── 查看执行过程（阶段、计划、工具、Hook、诊断，默认按状态折叠）
└── 回合结果/历史回合（可折叠）
```

- 以 `run.answer` 作为唯一主回复来源，从工作步骤中排除 `assistant` / `agent-message`，避免重复。
- 当前回合运行中、等待交互或失败时展开执行过程；完成/停止后默认收起，用户一次点击即可查看。
- 交互请求由 App 传入 Transcript，在当前回合中内联显示；不自动批准、不改变现有 resolve 流程。
- 变更卡片显示文件/内容、摘要和状态；存在待确认变更时展开，其余默认收起。
- 右侧移除重复 Activity 标签，仅保留 Story、Task、Changes、Usage、Debug、设置；命令面板不再导航到重复的 Activity。
- 输入区只显示最新一条非对话操作通知，避免多条活动消息压缩主内容。

### 数据流与边界

```text
AgentRun + 当前 approvalQueue[0]
              │
              ▼
      ConversationTurnView（纯展示投影）
       ├── UserMessage
       ├── AssistantMessage(run.answer)
       ├── InlineInteraction
       ├── ChangeSummary
       └── CollapsibleWorkLog(run.steps / phases / plan)
```

- 不新建后端契约；`AgentRun` 仍是当前活动回合的状态源，`history` 仍以有界摘要显示历史回合。
- `approvalQueue` 仍由 Renderer 显示投影管理，resolve 继续调用现有 `api.agent.resolveApproval`。
- 主回复、执行详情、审批和变更均通过现有有界字段输出；不在 UI 保存原始请求或凭据。

### 五角色评审

#### 架构专家——展示投影与边界

- 结论：`APPROVE WITH CONDITIONS`。
- P1：回复只能来自 `run.answer`，执行步骤不得再承担第二个回复渲染职责；右侧 Activity 不得成为第二事实源。
- P2：未来拆分多线程 TaskStore 时，当前 `ConversationTurnView` 应保持为纯投影组件。
- 建议：本轮只改 Renderer 组件结构，不把回合聚合逻辑扩散到 adapter。

#### 技术专家 A——可靠性、安全与性能

- 结论：`APPROVE WITH CONDITIONS`。
- P1：所有现有文本上限、审批白名单和 IPC 路径保持不变；折叠只改变渲染，不改变请求生命周期。
- P2：步骤列表继续使用有限高度和滚动容器，长事件不能撑爆主窗口；流式回复使用单一 `aria-live` 区域。
- 建议：错误和等待状态使用文字加语义属性，不只依赖颜色。

#### 技术专家 B——实现与回归

- 结论：`APPROVE WITH CONDITIONS`。
- P1：覆盖无回复空态、流式回复、完成默认折叠、等待交互展开、失败展示、变更展开和历史回合折叠。
- P2：旧的未使用 Activity/Transcript 组件不参与新渲染路径；测试阶段允许直接重写当前 UI 结构。
- 验收：`npm run build`、`npm run test:agent`、`npm run test:runtime`、Node 语法检查和 `git diff --check` 通过。

#### 产品经理——对话任务闭环

- 结论：`APPROVE WITH CONDITIONS`。
- P1：用户进入当前回合后，首屏先看到自己的指令和 Agent 回复；审批卡片要说明“为什么等待”和“处理后继续什么”。
- P1：执行过程默认降级为辅助信息，不能遮挡主回复；完成结果需要有清晰的总结和变更入口。
- P2：历史回合只展示有界摘要，并提供明确的展开文案，避免把任务历史和当前回合混在一起。
- 验收：首次阅读无需滚动即可识别当前回合状态；详情最多一次点击可见。

#### UI/UX 设计师——层级、状态和可访问性

- 结论：`APPROVE WITH CONDITIONS`。
- P1：使用用户/Agent 双消息层级，Agent 回复采用更大的阅读字号和更宽的内容列；工具、计划、阶段放进折叠工作日志。
- P1：运行中自动展开工作日志，完成态默认收起；审批、错误和待确认变更保持醒目并支持键盘操作。
- P2：窄窗口下消息气泡和执行日志单列；`details/summary` 提供原生键盘折叠能力，流式内容使用 `aria-live`。

### 实施任务、验收与风险

- 实施：更新中心渲染顺序、内联 approval、过滤重复 assistant step、重做对话样式、移除右侧重复 Activity、收敛输入区通知、补充状态和响应式规则。
- 验收：中心具备用户指令/Agent 回复/内联交互/变更/折叠过程/历史回合完整链路；状态文字清晰；构建与现有测试通过。
- 风险：当前仍只保留当前活动回合完整内容，历史回合为有界摘要；真实 App Server 的不同事件字段仍由现有 reducer 负责归一化。
- 回滚：本工程处于测试阶段，允许直接回退本轮 Renderer 文件到上一版；不涉及持久化迁移和外部协议。

### 综合决策与实施后复核

- 综合结论：`APPROVE WITH CONDITIONS`；所有 P1 条件已纳入本轮实施任务，允许修改 Renderer 展示层。
- 实施后复核：中心 Transcript 已改为当前回合视图，`run.answer` 是唯一主回复；执行步骤过滤掉 assistant item 并进入可折叠工作日志；审批/用户问题通过 `approvalQueue[0]` 内联；变更、错误、完成总结和历史回合均按回合分组；右侧 Inspector 移除重复 Activity 标签；输入区仅显示最新一条通知。
- 验证：`npm run build` 通过；`npm run test:agent` 16 项通过；`npm run test:runtime` 3 项通过；`node --check electron/main.mjs`、`electron/codex-adapter.mjs`、`electron/preload.cjs` 通过；`git diff --check` 通过。
- 与方案偏差：无协议、IPC、Runtime、凭据和持久化变更；没有引入兼容分支。
- 剩余风险：当前仓库没有现成的 React/Electron 截图测试，真实窗口 smoke 仍受本机 Electron 启动权限/安装状态约束；需要在用户本机运行 `npm run dev` 做最终视觉验收。

## 第二十五轮：Codex 风格对话与执行过程深化

### 背景、目标与非目标

- 背景：第二十四轮已经建立“对话优先”的回合结构，但工作日志展开后仍是平铺步骤；工具输出、当前动作、阶段和耗时没有形成 Codex 风格的紧凑进度层。回复仍按纯文本段落展示，代码/列表可读性不足。
- 目标：继续对齐 Codex 的信息层级——主回复优先、当前动作突出、工具项按需展开、阶段/计划/耗时集中摘要、回复支持安全的基础 Markdown 结构化展示，并提供复制回复操作。
- 非目标：不复制 Codex 内部实现，不增加协议字段、IPC、后端 Markdown 依赖、模型调用和新的持久化状态。

### 推荐方案

```text
Codex 当前回合
├── 用户指令
├── Agent 主回复
│   ├── 段落 / 标题 / 列表 / 代码块
│   └── 复制回复
├── 当前动作摘要（阶段、动作、耗时）
├── 工作日志
│   ├── 阶段与计划
│   └── 工具 / Hook / 系统步骤（当前、失败、等待展开；其余折叠）
└── 变更、错误、历史回合
```

- 使用 React 文本节点将有限格式的回复投影为段落、标题、无序列表和 fenced code block；未知 Markdown 保留为安全文本，不使用 `dangerouslySetInnerHTML`。
- 工作日志保留既有 `run.steps`、`run.phases`、`run.plan`，新增纯展示层的当前动作摘要；不改变事件 reducer。
- 有详情的工作项用原生 `details/summary`；`running`、`waiting`、`failed` 自动展开，已完成工具默认折叠；日志继续限制高度并滚动。
- 复制按钮调用浏览器能力并允许失败静默降级，不读取或存储凭据。

### 五角色评审

#### 架构专家

- 结论：`APPROVE WITH CONDITIONS`。
- P1：结构化回复和工作项折叠必须是纯 Renderer 投影，不能将 UI 状态反写协议层。
- P2：工作项组件应以 `AgentRun` 的有界数据为输入，未来可独立迁移到 TaskStore。

#### 技术专家 A

- 结论：`APPROVE WITH CONDITIONS`。
- P1：禁止使用 HTML 字符串注入；回复、工具输出和详情继续遵守 reducer 的长度边界。
- P2：复制能力必须在 Clipboard API 不可用时不阻断对话，不输出剪贴板内容。

#### 技术专家 B

- 结论：`APPROVE WITH CONDITIONS`。
- P1：工作日志的活动/等待/失败状态和无详情步骤必须都有可见文案；完成项默认收起。
- P2：构建和既有 Agent/Runtime 回归保持通过；新增逻辑不改变事件测试契约。

#### 产品经理

- 结论：`APPROVE WITH CONDITIONS`。
- P1：用户无需阅读工具输出即可理解 Agent 做了什么、当前进行到哪一步以及最终回复是什么。
- P2：复制回复属于低成本高频动作，放在回复底部，不抢占主信息。

#### UI/UX 设计师

- 结论：`APPROVE WITH CONDITIONS`。
- P1：当前动作摘要使用明确标题、阶段和耗时；详情折叠使用原生键盘语义；状态不能只靠颜色。
- P2：代码块、列表、长文本在暗色/浅色主题中都要保持阅读对比度和横向滚动边界。

### 实施与验收

- 实施文件：`src/App.tsx` 增加安全的回复块渲染、当前动作摘要、工作项折叠和复制操作；`src/styles.css` 增加 Codex 风格进度层、工作项和结构化文本样式。
- 验收：主回复仍为当前回合最醒目内容；当前动作一眼可见；已完成工具默认折叠、活动/失败项展开；代码/列表可读；复制不可用时不影响页面。
- 风险与回滚：仅改变 Renderer；可直接回退 `App.tsx` / `styles.css`，无协议或数据迁移风险。

### 综合决策

- 综合结论：`APPROVE WITH CONDITIONS`；所有 P1 条件已转为上述实施和验收项，允许进入代码修改。
- 实施后复核：工作日志新增当前动作、阶段、计划和耗时摘要；有内容的工具/Hook/系统步骤使用原生 `details/summary`，运行中/等待/失败自动展开，完成项默认折叠；`run.answer` 使用安全的段落、标题、无序/有序列表和 fenced code block 文本投影，并提供 Clipboard API 复制入口。
- 验证：`npm run build` 通过；`npm run test:agent` 16 项通过；`npm run test:runtime` 3 项通过；`node --check electron/main.mjs`、`electron/codex-adapter.mjs`、`electron/preload.cjs` 通过；`git diff --check` 通过。
- 与方案偏差：无协议、IPC、Runtime、依赖、凭据和持久化变更；未知格式保留为文本，没有 HTML 注入。
- 剩余风险：当前仍没有 React/Electron 截图回归测试；真实窗口视觉验收需在用户本机运行 `npm run dev`。

## 第二十六轮：按 Codex 截图重做对话视觉与过程链路

### 视觉基准与问题判断

- 用户提供的 Codex 截图显示：对话是连续白底阅读流，用户指令右对齐为黑色圆角胶囊；中间只有一条“用时 + 折叠箭头”的紧凑过程入口；Agent 回复是无头像、无重卡片的正文；文件变更紧贴回复，以独立变更卡片展示文件数、增删统计、审查入口和文件列表；底部是大尺寸圆角 Composer。
- 当前 Novel Codex 虽已将内容分层，但仍使用深色边框卡片、双方头像和左对齐用户消息，工作日志处于回复之后，视觉和阅读顺序与截图存在根本差异。

### 目标与非目标

- 目标：对齐截图的对话节奏和层级：用户消息 → 用时/执行过程折叠行 → Agent 正文回复 → 变更卡片 → 输入区；保留运行中、等待、失败、审查和历史回合能力。
- 目标：变更卡片展示文件数和可解析的增删统计，提供真实的“审查”动作；没有回滚 IPC 时不伪造可执行的“撤销”。
- 非目标：不复制 Codex 品牌资源、不调用 Codex 内部 UI、不修改 App Server/IPC/Runtime 协议和事件模型。

### 推荐信息架构

```text
Conversation stream
├── 用户指令（右对齐黑色胶囊）
├── 用时 / 当前动作（原生折叠的过程入口）
│   └── 阶段、计划、工具、Hook、错误详情
├── Agent 回复（阅读优先的正文）
├── 文件变更卡片（文件数、+/-、审查、文件清单）
├── 失败/等待交互（紧贴当前回合）
└── Composer（大圆角、底部固定阅读路径）
```

- 执行日志移动到 Agent 回复之前，完成态只露出“用时”；运行中/等待/失败自动展开。
- 用户和 Agent 不使用头像作为主识别，使用左右位置、胶囊/正文层级和文字语义区分。
- 变更统计从现有 diff/文件详情中安全解析；解析不到时展示文件数，不编造数字。
- “撤销”在当前没有回滚能力时不展示为可操作按钮，审查继续复用现有 `onReview`。
- Composer 视觉升级为大圆角编辑器，发送/停止/追加语义沿用现有事件和 IPC。

### 五角色评审

#### 架构专家

- 结论：`APPROVE WITH CONDITIONS`。
- P1：执行过程只是 `AgentRun` 的展示投影，必须放在回复前但不能改变 reducer 或 turn 生命周期。
- P1：变更统计只能从已有 `diff` / `artifact.detail` 派生；不得新增不可信的变更事实源。

#### 技术专家 A

- 结论：`APPROVE WITH CONDITIONS`。
- P1：结构化正文继续使用 React 文本节点；diff 统计按行解析并限制输入规模，不能读取敏感信息或把原文写入日志。
- P2：原生 `details/summary` 保持键盘可用；Composer 在 Clipboard/窗口能力异常时不阻塞发送。

#### 技术专家 B

- 结论：`APPROVE WITH CONDITIONS`。
- P1：覆盖过程前置、完成态折叠、活动态展开、变更卡片空态、增删统计未知、审批内联和 Composer 状态回归。
- P2：本轮只改 `App.tsx` / `styles.css` / 评审文档，不引入新依赖和兼容层。

#### 产品经理

- 结论：`APPROVE WITH CONDITIONS`。
- P1：用户第一眼必须先看到自己发出的指令和最终回复；执行过程是可选详情，不能占据主视觉。
- P1：文件变更必须能回答“改了几个文件、改了什么、是否可审查”；没有回滚能力就不能提供误导性的撤销按钮。

#### UI/UX 设计师

- 结论：`APPROVE WITH CONDITIONS`。
- P1：严格采用截图的右对齐用户胶囊、无头像 Agent 正文、用时折叠行、变更卡片和底部大 Composer。
- P2：浅色主题优先匹配截图，暗色主题保持相同布局语义；长回复、长路径和代码块必须可读且不撑破窗口。

### 实施与验收

- `src/App.tsx`：调整回合节点顺序；新增有限 diff 统计、变更卡片摘要和真实审查入口；保留安全正文渲染与过程组件。
- `src/styles.css`：重写对话流的用户胶囊、用时行、无头像正文、变更卡片、Composer 和浅/深主题覆盖。
- 验收：视觉阅读顺序与截图一致；过程在回复前且完成态折叠；回复之后紧跟变更；发送/追加/停止/审批/审查行为不变。
- 风险：没有真实 Electron 截图测试，最终间距与窗口尺寸仍需本机 smoke 验收；不影响构建和协议回归。

### 综合决策

- 综合结论：`APPROVE WITH CONDITIONS`；所有 P1 已纳入实施，允许进入 Renderer 代码修改。

## Round 27：截图补充评审——对话与 Composer 视觉收敛

本轮新增的 Codex 截图进一步明确了上一轮方案的验收形态。截图是视觉参考，不是外部产品指令；继续沿用现有 `AgentRun`、审批队列、变更数据和审查回调，不引入 Codex 私有组件或品牌资源。

### 补充目标

- 用户消息使用右对齐黑色胶囊；显示时间与复制入口，去掉头像和多余的角色标签。
- 完成态的执行过程只显示一行“用时”，运行态显示“已处理”并自动展开当前思考/步骤；回答正文保持阅读优先。
- 代码块使用浅灰圆角容器，变更使用“文件数 + 增删统计 + 文件清单 + 审查”卡片。
- Composer 采用白色大圆角编辑区，保留发送、追加、停止、等待和现有状态语义；不得伪造尚未实现的回滚行为。

### 五角色补充评审

#### 架构专家

- 结论：`APPROVE WITH CONDITIONS`。
- P1：仅调整 Renderer 的展示投影与 CSS；`AgentRun` 的事件顺序、状态机和 IPC 边界不变。
- P1：变更卡片的统计必须从已有 artifact/diff 派生，未知时回退到文件数或“内容已更改”。

#### 技术专家 A

- 结论：`APPROVE WITH CONDITIONS`。
- P1：时间显示在运行中必须随时钟刷新，完成后冻结；正文、代码和 diff 均通过 React 文本节点或受控 `pre` 渲染。
- P2：交互按钮继续使用真实回调，剪贴板失败不应阻断主流程。

#### 技术专家 B

- 结论：`APPROVE WITH CONDITIONS`。
- P1：验证用户消息、过程折叠、过程展开、正文、审批、错误、变更卡片和 Composer 的状态回归。
- P2：本轮不增加依赖，不制作兼容旧 UI 的双轨实现。

#### 产品经理

- 结论：`APPROVE WITH CONDITIONS`。
- P1：首屏阅读顺序必须回答“我说了什么、Agent 做到哪、Agent 回复什么、改了哪些文件”。
- P1：完成态默认隐藏细节，只有运行中、等待和失败状态主动暴露过程；审查入口要能触发现有审查流程。

#### UI/UX 设计师

- 结论：`APPROVE WITH CONDITIONS`。
- P1：浅色主题对齐截图的留白、细分隔线、黑色用户胶囊、无头像正文、轻量用时行和变更卡片。
- P2：深色主题只改变色板，不改变信息层级；窄窗口下胶囊、路径、代码和按钮必须可换行或滚动。

### 补充实施与验收

- `src/App.tsx`：补充安全的 diff 行统计、变更卡片摘要、复制/审查入口和 Composer 模型状态展示。
- `src/styles.css`：覆盖旧的深色卡片规则，使对话流成为连续的浅色阅读路径，并为深色主题保留对应层级。
- 验收：截图中的四个关键状态（完成、运行、正文代码块、Composer 运行中）均有对应 DOM/CSS 状态；构建、Agent 测试、Runtime 测试通过。

### 补充综合决策

- 综合结论：`APPROVE WITH CONDITIONS`；条件均为实现与验证项，允许继续进入 Renderer 落地。

### 实施后五角色复核

- 架构专家：`APPROVE`。未修改 AgentRun 状态机、App Server、Runtime 或 IPC；执行详情仍来自既有事件投影。
- 技术专家 A：`APPROVE`。正文与代码使用 React 文本节点，diff 统计按行派生，剪贴板失败不会阻断界面。
- 技术专家 B：`APPROVE`。`npm run build`、`npm run test:agent`、`npm run test:runtime` 和 `git diff --check` 均通过；无新增依赖。
- 产品经理：`APPROVE`。首屏顺序已收敛为用户指令、处理状态、Agent 回复、变更与 Composer；审查复用真实回调，撤销保持禁用提示。
- UI/UX 设计师：`APPROVE WITH FOLLOW-UP`。浅色 Electron 窗口已完成运行态与完成态视觉核对；后续仍可增加多分辨率截图回归，但不阻塞本轮交付。

### 本轮交付记录

- `src/App.tsx`：用户消息元信息、回复代码块工具栏、过程二级折叠、变更统计/文件卡片/审查动作和模型 Composer 状态。
- `src/styles.css`：Codex 风格阅读流、黑色用户胶囊、用时分隔行、浅灰代码/变更卡片、圆形发送/停止按钮及暗色主题覆盖。
- 窗口级检查：在本地 Electron 窗口验证了运行中、等待授权和完成态的阅读顺序；测试期间产生的布局测试授权已拒绝，未批准临时写入。

## 第二十八轮：Cursor 类中央文章区与右侧对话栏

### 背景、目标与非目标

- 背景：当前对话已经完成 Codex 风格的回合视觉，但对话和文章仍处于同一中央纵向流，用户在编辑正文时需要在文档与 Agent 之间上下滚动。
- 目标：采用 Cursor 类工作台结构：左侧项目导航、中间文章/章节文档、右侧固定对话栏；对话栏承载当前回合、Composer 和任务 Inspector，上下文不会再挤占文章主阅读宽度。
- 非目标：不改变 AgentRun、App Server、Runtime、IPC、审批协议、章节数据和现有编辑行为；不新增依赖；不复制 Cursor 私有实现或资源。

### 方案与取舍

候选方案：

1. 四列布局：文章、对话、Inspector 分别占列。优点是边界清晰，缺点是小窗口下文章宽度不足，右侧信息重复。
2. 右侧工作栏：文章作为中央主列，对话和现有 Inspector 合并为同一右侧 rail，Inspector 作为下方上下文区。优点是保持三栏工作台、对话紧邻文章、保留既有 Story/Task/Changes/Usage/Debug/设置能力；缺点是右侧内容需要独立滚动。

选择方案 2。右侧 rail 采用固定视口高度和内部滚动：上部对话记录，中部 Composer，下部 Inspector；中央文档继续使用主页面滚动，形成编辑与协作并行的阅读路径。

### 推荐结构与数据流

```text
Workspace
├── Sidebar（项目 / 章节 / 任务）
├── MainDocument（Hero / 状态 / 章节正文 / 章节编辑）
└── CollaborationRail
    ├── ConversationScroll（当前回合与执行过程）
    ├── Composer（发送 / 追加 / 停止 / 审批等待）
    └── Inspector（Story / Task / Changes / Usage / Debug / 设置）
```

- `EnhancedAgentTranscript` 只改变挂载位置，不改变回合投影、事件顺序和回调。
- `Composer` 只改变布局容器和宽度，不改变发送、追加、停止、等待逻辑。
- `AgentInspector` 保持原有 tab 与操作，放入 rail 下方并独立滚动；不创建第二份状态。
- 中央文章区域保留 `Overview`、`ChapterView`、`DiffView` 和其他 Story 视图，作为唯一文档主工作区。

### 五角色评审

#### 架构专家

- 结论：`APPROVE WITH CONDITIONS`。
- P1：只能调整 React 挂载层级和 CSS 布局，不能让 rail 为 AgentRun 建立第二个状态源。
- P1：Inspector 必须复用现有组件和回调，避免为了布局复制 Story/Task 数据。

#### 技术专家 A

- 结论：`APPROVE WITH CONDITIONS`。
- P1：rail 内部必须有明确的 `min-height: 0` 和滚动边界，长回复、审批详情和 Inspector 不能撑破窗口。
- P2：宽度策略需覆盖 macOS/Windows 常见窗口，禁止依赖平台专属滚动 API。

#### 技术专家 B

- 结论：`APPROVE WITH CONDITIONS`。
- P1：回归验证必须覆盖 Overview、章节编辑、运行中、等待授权、完成态、错误态、停止态和窄窗口布局。
- P2：不保留旧的“中央对话”兼容双轨，测试阶段直接切换结构并保持构建通过。

#### 产品经理

- 结论：`APPROVE WITH CONDITIONS`。
- P1：用户必须能同时看到文章正文和对话入口，且发送指令后上下文仍在右侧，不打断编辑路径。
- P1：Story Canon 和任务诊断不能丢失，应该作为右侧 Inspector 的可切换上下文层保留。

#### UI/UX 设计师

- 结论：`APPROVE WITH CONDITIONS`。
- P1：中央文档是视觉主区域，右侧对话栏使用独立边界、固定宽度和连续滚动；Composer 固定在对话栏底部。
- P2：浅色主题维持 Codex/Cursor 式白色表面和细分隔线，深色主题仅替换色板；键盘焦点和辅助技术顺序必须与视觉顺序一致。

### 实施与验收

- `src/App.tsx`：将 `EnhancedAgentTranscript`、`Composer` 和 `AgentInspector` 挂载到 `conversation-rail`；中央只保留文章相关视图和项目状态。
- `src/styles.css`：将工作区调整为左导航/中央文档/右协作栏三列，设置 rail 内滚动、Composer 底部固定和 Inspector 下方面板。
- 验收：文章正文在中央持续可见；对话栏在右侧独立滚动；发送、追加、停止、审批、审查、重试和 Inspector tabs 行为不变；窄窗口不发生横向溢出或内容重叠。

### 综合决策

- 综合结论：`APPROVE WITH CONDITIONS`；全部 P1 已转为实施与验收项，允许进入 Renderer 结构调整。

### 实施后五角色复核

- 架构专家：`APPROVE`。`EnhancedAgentTranscript`、`Composer` 和 `AgentInspector` 仅调整挂载层级，继续复用同一份 `agentRun`、`approvalQueue`、`contextTab` 及既有回调；没有引入第二套状态。
- 技术专家 A：`APPROVE`。右侧 rail 使用固定视口高度、`min-height: 0` 和独立 `overflow-y: auto`，长对话不会撑破窗口；布局只依赖标准 CSS Grid/Flex，兼容 macOS 与 Windows。
- 技术专家 B：`APPROVE WITH FOLLOW-UP`。构建、Agent 16 项测试、Runtime 3 项测试、Electron 主进程语法检查和 `git diff --check` 均通过；运行中、等待、完成、审批等行为仍由原组件驱动，后续可补充多分辨率截图回归。
- 产品经理：`APPROVE`。正文成为中央主任务，对话、发送入口和 Story/Task/Changes/Usage/Debug/设置仍集中在右侧，用户可以边读边协作，不需要离开文章上下寻找输入框。
- UI/UX 设计师：`APPROVE`。实际 Electron 窗口已核对中央文章、右侧对话、底部 Composer 和下方 Inspector 的视觉层级；浅色表面与细分隔线延续 Codex/Cursor 类工作台，深色主题保留独立色板。

### 本轮验证结果

- `npm run build`：通过。
- `npm run test:agent`：16/16 通过。
- `npm run test:runtime`：3/3 通过。
- `node --check electron/main.mjs electron/codex-adapter.mjs electron/codex-runtime.mjs`：通过。
- `git diff --check`：通过。

## 第二十九轮：创建与切换小说作品

### 背景、目标与非目标

- 背景：当前应用只有固定的《最后一盏灯》种子项目，项目切换器没有真实行为，用户不能在 UI 中创建自己的作品。
- 目标：提供首版可用的作品创建与切换闭环：新建作品、保存标题/简介、创建成功后自动进入新作品、重新打开应用后可从作品列表切换。
- 非目标：本轮不实现作品删除、导入导出、团队协作、封面/类型元数据、章节批量生成，也不复制默认作品的角色/场景/规则。

### 产品经理与 UI/UX 讨论结论

#### 产品经理

- 用户价值：先让用户拥有一个独立的创作容器，再通过右侧 Composer 开始写作；创建流程必须在一次操作内完成并自动进入新作品。
- MVP 范围：标题为必填字段，简介为可选字段；新作品只创建项目元数据和空内容集合，避免产生虚假正文。
- 验收标准：创建成功后顶部显示新标题；左侧项目卡更新；刷新后作品仍存在；可从顶部选择器切换回已有作品；失败时保留表单并显示原因。
- 失败场景：空标题、标题过长、Runtime 不可用、重复提交；均不能产生半成品记录或丢失表单输入。

#### UI/UX 设计师

- 入口：左侧 PROJECT 标题旁的“＋”和顶部作品选择器中的“新建作品”均打开同一对话框，不使用原生 `window.prompt`。
- 对话框：标题输入框自动聚焦；简介输入框可选；显示取消/创建按钮、提交中状态、错误提示；支持 Escape 关闭、Enter 提交标题。
- 选择器：展示作品标题与简介摘要，当前作品有选中态；Agent 执行中或等待授权时禁用切换，避免切换造成上下文误解。
- 空态：新作品无章节时，中央显示“作品已创建，开始第一章”的说明，右侧 Composer 仍保持可用；章节页显示明确的无章节状态。
- 无障碍：对话框使用 `role="dialog"`、`aria-modal`、标注输入、错误文本关联；操作顺序为标题 → 简介 → 取消 → 创建。

### 方案与取舍

候选方案：

1. 仅在前端创建临时项目。实现快，但刷新丢失，不满足作品资产持久化。
2. 新增 Runtime 项目列表/创建接口，并由 Renderer 维护当前项目 ID。持久化完整，边界清晰，选择此方案。
3. 复用默认项目并清空内容。会破坏示例数据，也无法支持多个作品，拒绝此方案。

### 推荐架构、数据流与错误处理

```text
ProjectCreateDialog
        │ IPC
        ▼
Electron runtime:create-project
        │ HTTP loopback
        ▼
Creative Runtime POST /api/projects
        │ SQLite INSERT projects
        ▼
project_state(newProjectId) ──► Renderer 更新 projects / activeProjectId / state
```

- Runtime 新增 `list_projects` 与 `create_project`，复用已有 `projects` 表，不改变既有数据表结构，不复制 `seed()` 子实体。
- Electron 只暴露窄 IPC：`runtime.listProjects()`、`runtime.createProject(title, logline)`；输入在主进程和 Runtime 两侧均校验长度与空值。
- Renderer 使用 `activeProjectId` 驱动状态加载，切换作品时清空当前 Agent 回合、任务历史详情、diff 和 Story Test 结果；执行中的 Agent 禁止切换。
- 创建失败时 Runtime 事务不提交，Dialog 保留用户输入并展示错误；成功后自动切换且更新本地 active project 偏好。

### 受影响文件、测试与验收

- `runtime/storage.py`、`runtime/server.py`：项目查询和创建能力。
- `electron/main.mjs`、`electron/preload.cjs`：隔离 IPC 能力。
- `src/types.ts`、`src/App.tsx`、`src/styles.css`：项目列表、创建对话框、切换器、空态和视觉状态。
- `runtime/tests/test_storage.py`：创建、列表、独立性和重复/空输入边界测试。
- 验收：创建后新项目只包含元数据；默认项目内容不变；切换后文章、Inspector 和 Composer 上下文对应当前项目；构建、Runtime 测试、Agent 测试和 Electron 语法检查通过。

### 五角色评审

#### 架构专家

- 结论：`APPROVE WITH CONDITIONS`。
- P1：项目 ID 必须由 Runtime 生成，Renderer 不能拼接或覆盖；切换只通过 `activeProjectId` 加载 `project_state`。
- P1：创建接口必须复用现有 `projects` 表并保持项目内容隔离，不能在 Renderer 维护第二份持久化数据。

#### 技术专家 A

- 结论：`APPROVE WITH CONDITIONS`。
- P1：标题/简介必须在 IPC 和 Runtime 双重校验；SQL 必须使用参数绑定；创建失败不得留下半条记录。
- P2：项目列表只返回必要摘要字段，避免把完整小说内容放进选择器响应。

#### 技术专家 B

- 结论：`APPROVE WITH CONDITIONS`。
- P1：必须覆盖新建、列表、切换、空标题、重复提交和新旧项目数据隔离测试。
- P2：不保留旧的固定项目双轨；测试阶段直接改为 active project 驱动。

#### 产品经理

- 结论：`APPROVE WITH CONDITIONS`。
- P1：创建成功后必须自动进入新作品，并给出明确成功反馈；失败不能关闭对话框或清空输入。
- P1：首版明确空作品边界，不把“创建项目”误报成“已经创建章节”。

#### UI/UX 设计师

- 结论：`APPROVE WITH CONDITIONS`。
- P1：创建入口统一使用同一 Dialog，覆盖首次使用、提交中、校验错误、Runtime 错误、成功和取消状态。
- P2：选择器与 Dialog 必须支持键盘操作、语义标签和当前项目选中态。

### 综合决策

- 综合结论：`APPROVE WITH CONDITIONS`；全部 P1 转为实施与验收项后进入代码落地。

### 实施后五角色复核

- 架构专家：`APPROVE`。项目 ID 由 Runtime 使用 UUID 生成，Renderer 只持有 active project 引用；新作只写入 `projects` 表，默认种子项目及其实体保持隔离。
- 技术专家 A：`APPROVE`。创建请求经过 Renderer 表单、Electron IPC 和 Runtime 三层校验，SQLite 使用参数绑定，创建失败不会提交半成品记录；选择器只读取项目摘要。
- 技术专家 B：`APPROVE`。已覆盖创建、列表、独立性、空标题、超长标题和超长简介测试；没有引入依赖，也没有保留固定项目的第二套切换逻辑。
- 产品经理：`APPROVE`。用户可以从左侧或顶部进入同一创建流程，成功后自动进入新作品；新作品明确显示空内容状态，不误报已有章节。
- UI/UX 设计师：`APPROVE WITH FOLLOW-UP`。实际 Electron 无障碍树已确认“新建作品”入口、作品选择器、对话输入与空态文案存在；后续可补多分辨率截图回归，但不阻塞首版交付。

### 本轮实际验证

- `npm run build`：通过。
- `npm run test:runtime`：5/5 通过。
- `npm run test:agent`：16/16 通过。
- `python3 -m py_compile runtime/server.py runtime/storage.py`：通过。
- Electron `main.mjs`、`codex-adapter.mjs`、`codex-runtime.mjs`、`preload.cjs` 语法检查：通过。
- `git diff --check`：通过。
- Electron 窗口检查：左侧和顶部均显示“新建作品”入口；中央保留文章区，右侧保留 Composer 与 Inspector；当前运行窗口未产生额外测试作品。

## 第三十轮：Composer Enter 发送交互

### 背景、目标与非目标

- 背景：当前右侧 Composer 只有 `⌘/Ctrl + Enter` 才会提交，普通 Enter 只换行，与用户期望的聊天输入习惯不一致。
- 目标：普通 Enter 发送当前对话；Shift+Enter 插入换行；中文输入法组合态 Enter 不触发发送；运行中继续调用既有 steer，等待交互时保持禁用。
- 非目标：不改变 Agent 请求协议、Runtime、IPC、消息状态机、历史数据和发送按钮；不新增快捷键设置系统。

### 产品经理与 UI/UX 讨论结论

#### 产品经理

- 用户价值：减少发送动作，符合常见聊天产品心智；多段创作指令仍可通过 Shift+Enter 保持在同一条消息中。
- 验收标准：空输入 Enter 不产生请求；有内容 Enter 只提交一次；Shift+Enter 不提交；运行中 Enter 追加指令；等待状态不能提交。
- 失败场景：输入法候选确认、重复 keydown、发送后焦点保留、发送按钮和键盘行为必须一致。

#### UI/UX 设计师

- 输入提示改为“Enter 发送 · Shift+Enter 换行”；运行中改为“Enter 追加 · Shift+Enter 换行”，让行为可发现。
- Enter 发送前调用 `preventDefault()`，避免发送后额外插入换行；Shift+Enter 保留浏览器默认换行。
- 使用 `nativeEvent.isComposing` 保护中文/日文/韩文输入法组合态；发送按钮、快捷键和辅助技术名称保持同一语义。
- 保持文本域焦点和现有禁用/等待态，不引入平台专属键位判断。

### 方案与取舍

1. 仅把 Meta/Ctrl+Enter 改成 Enter：实现最小，但会误触发输入法确认，也无法保留多行输入。
2. 使用 Enter 发送、Shift+Enter 换行，并跳过 IME composing 状态：覆盖桌面聊天常用交互，选择此方案。
3. 增加设置项允许用户切换快捷键：可配置但超出测试阶段首版范围，暂不采用。

### 实施边界与测试

- `src/App.tsx`：仅修改 Composer 的键盘事件和提示文案；`onSubmit` 继续根据 Agent 状态决定新回合或 steer。
- `DESIGN_REVIEW.md`：记录产品、UX、架构和技术复核结论。
- 验收：发送按钮与 Enter 共用 `onSubmit`；Shift+Enter、IME composing、空输入、运行中、等待态均有明确行为；构建和既有 Agent/Runtime 测试通过。

### 五角色评审

#### 架构专家

- 结论：`APPROVE WITH CONDITIONS`。
- P1：快捷键只能位于 Composer UI 层，必须复用现有 `onSubmit`，不能绕过 Agent 状态机或新增发送通道。

#### 技术专家 A

- 结论：`APPROVE WITH CONDITIONS`。
- P1：必须识别 `nativeEvent.isComposing`，并使用 `preventDefault()` 防止 Enter 产生重复换行或重复提交。
- P2：不引入第三方快捷键依赖，保持跨平台浏览器事件模型。

#### 技术专家 B

- 结论：`APPROVE WITH CONDITIONS`。
- P1：验证普通发送、Shift 换行、IME 组合态、运行中追加、等待禁用和按钮发送行为；不得破坏既有测试。

#### 产品经理

- 结论：`APPROVE`。交互目标明确、范围小、可验证，且保留多行输入和原有发送按钮。

#### UI/UX 设计师

- 结论：`APPROVE WITH CONDITIONS`。
- P1：Composer 必须显示新的快捷键提示，并保持文本域焦点、禁用状态、可访问名称和按钮语义一致。

### 综合决策

- 综合结论：`APPROVE WITH CONDITIONS`；所有 P1 已转为实现与验证项，允许进入 Renderer 修改。

### 实施后五角色复核

- 架构专家：`APPROVE`。键盘事件仍只调用 Composer 既有 `onSubmit`，没有新增 Agent 请求通道，也没有改变状态机。
- 技术专家 A：`APPROVE`。Enter 分支调用 `preventDefault()`；Shift+Enter 保留默认换行；`nativeEvent.isComposing` 避免输入法候选确认误发。
- 技术专家 B：`APPROVE WITH FOLLOW-UP`。TypeScript 构建、Runtime 5/5、Agent 16/16、Electron 语法检查和差异检查均通过；后续可增加真实组件级键盘自动化测试，但不阻塞本轮。
- 产品经理：`APPROVE`。发送路径更短，保留多行创作指令和鼠标发送按钮；运行中追加、等待禁用和空输入行为未被改变。
- UI/UX 设计师：`APPROVE`。Electron 实际窗口已展示“Enter 发送 · Shift+Enter 换行”提示，文案与可访问输入区域保持一致；IME 保护满足中文创作场景。

### 本轮实际验证

- `npm run build`：通过。
- `npm run test:runtime`：5/5 通过。
- `npm run test:agent`：16/16 通过。
- Electron 主进程、Runtime、适配器和 preload 语法检查：通过。
- `git diff --check`：通过。
- Electron 窗口检查：Composer 无执行时显示“Enter 发送 · Shift+Enter 换行”；现有右侧对话布局和发送按钮正常。

## 第三十一轮：移除 Composer 快捷键提示

### 方案与评审结论

- 产品经理：`APPROVE`。移除冗余提示，保留 Enter 发送、Shift+Enter 换行和发送按钮的实际行为；不改变用户任务流。
- UI/UX 设计师：`APPROVE`。Composer 底部只保留模型选择、发送/追加/停止控制，减少视觉噪音；通过 `justify-content: flex-end` 保持控件右对齐。
- 架构专家：`APPROVE`。仅移除 Renderer 文案节点和对应布局规则，不触碰 Agent 状态机及请求契约。
- 技术专家 A：`APPROVE`。键盘事件处理不变，IME 保护和 `preventDefault()` 不变。
- 技术专家 B：`APPROVE WITH CONDITIONS`。需要重新执行前端构建、既有 Runtime/Agent 测试和窗口检查。

### 实施与验收

- `src/App.tsx`：删除 Composer 快捷键提示节点。
- `src/styles.css`：在提示节点移除后将底部控制组右对齐。
- 验收：界面不再显示“Enter 发送 · Shift+Enter 换行”，Enter/Shift+Enter 行为、模型显示、发送按钮和运行中控制均保持正常。

## 第三十二轮：作品创建错误信息与 Runtime 响应规范化

### 背景、目标与非目标

- 背景：新建作品已经进入 Electron IPC handler，但 Runtime 返回非字符串错误载荷时，Electron 的 `new Error(object)` 会把真实原因压扁为 `Error: [object Object]`；弹窗还会把 Electron IPC 包装前缀直接展示给用户。
- 目标：统一把 Runtime 的 `error`/`detail` 转成稳定、可读的错误消息；创建弹窗只展示业务错误主体，并保留服务端返回的真实细节；不改变作品创建请求、数据库结构和成功后的项目切换流程。
- 非目标：不引入新的错误码体系、不兼容旧 Runtime 协议、不把测试密钥或运行时凭据写入工程、不修改 Agent 状态机。

### 方案

1. 在 Electron `runtimeRequest` 的 HTTP 非成功分支读取 `error` 或 `detail`，字符串原样保留，JSON 对象序列化后再构造 `Error`。
2. 在 `ProjectCreateDialog` 的 catch 分支移除 Electron IPC 的通用包装前缀；非 `Error` 值使用稳定兜底文案，避免把内部对象直接渲染到界面。
3. 保留 Runtime 现有三层校验、SQLite 参数绑定、创建后列表刷新和 active project 切换；通过既有 Runtime 创建测试与构建验证。

### 五角色评审

#### 架构专家

- 结论：`APPROVE WITH CONDITIONS`。
- P1：错误规范化必须位于 Electron Runtime 边界，不能让 Renderer 猜测服务端 payload；不得改变成功响应契约。

#### 技术专家 A

- 结论：`APPROVE WITH CONDITIONS`。
- P1：同时覆盖字符串、对象、缺失字段和 IPC 包装错误；JSON 序列化失败时必须有固定兜底消息。

#### 技术专家 B

- 结论：`APPROVE`。
- P2：沿用现有 Runtime 测试和 `node --check`，不新增依赖；真实 HTTP 创建验证使用临时数据库，避免污染用户作品。

#### 产品经理

- 结论：`APPROVE`。
- 创建失败应让用户知道“为什么失败”，而不是看到 Electron 内部实现文本；成功流程和输入内容保持不变。

#### UI/UX 设计师

- 结论：`APPROVE WITH CONDITIONS`。
- P1：错误区域继续使用 `role="alert"`，文案简洁可读；保留表单内容，方便用户修正后重试。

### 综合决策

- 综合结论：`APPROVE WITH CONDITIONS`；所有 P1 转为实现与验收项后允许进入代码修改。

### 实施后五角色复核

- 架构专家：`APPROVE`。错误已在 Electron Runtime 边界归一化，Renderer 不再承担协议推断；成功响应和作品创建状态流未改变。
- 技术专家 A：`APPROVE`。覆盖字符串、嵌套 `message`、JSON 对象、缺失错误字段和异常序列化兜底；`Error: [object Object]` 不再由主进程生成。
- 技术专家 B：`APPROVE`。构建、Runtime 5/5、Agent 16/16、Python 与 Electron 语法检查、差异检查均通过；临时数据库 POST 创建验证通过。
- 产品经理：`APPROVE`。失败时用户看到可读原因并可保留输入重试，成功后仍自动切换到新作品。
- UI/UX 设计师：`APPROVE`。弹窗继续使用 `role="alert"`，移除 Electron 内部调用前缀，错误区域不改变表单布局。

### 本轮实际验证

- `npm run build`：通过。
- `npm run test:runtime`：5/5 通过。
- `npm run test:agent`：16/16 通过。
- `python3 -m py_compile runtime/server.py runtime/storage.py`：通过。
- Electron `main.mjs`、`codex-adapter.mjs`、`codex-runtime.mjs`、`preload.cjs` 语法检查：通过。
- `git diff --check`：通过。
- 临时 SQLite 调用 `response_payload("/api/projects", "POST", ...)`：通过；中文作品名和空简介返回 201，创建作品无章节且默认作品保持存在。
- 当前已打开的 Electron 窗口：主进程不会被 Vite 热更新，需重新执行 `npm run dev` 后才能加载本轮 `main.mjs` 错误处理修复。

## 第三十三轮：FastAPI 作品创建请求体解析

### 背景、目标与非目标

- 背景：截图中的错误为 FastAPI 校验错误，`request` 被识别为必填 query 参数，导致合法的 `{title, logline}` JSON 请求在进入 Runtime 前失败。
- 目标：让 `/api/projects` 的 POST 路由稳定识别 `ProjectCreate` 为 JSON request body，并保持 HTTP fallback、Electron IPC 和数据库创建契约一致。
- 非目标：不修改作品字段、不引入兼容旧 schema 的迁移、不改变前端表单和成功后的项目切换。

### 方案

1. 移除 Runtime 模块中不必要的 `from __future__ import annotations`，让工厂函数内的 Pydantic 模型注解在路由注册时保持为真实类型。
2. 使用项目虚拟环境中的 FastAPI 路由依赖检查和临时 SQLite POST 验证，确认 `request` 从 query 参数变为 body 参数，并确认实际创建成功。
3. 继续保留 HTTP fallback 的直接 `response_payload` 路径与现有校验，避免仅修复一种 Runtime 启动模式。

### 五角色评审

#### 架构专家

- 结论：`APPROVE`。修复发生在 FastAPI 适配层，不改变 Runtime 领域函数和 Renderer/IPC 契约。

#### 技术专家 A

- 结论：`APPROVE WITH CONDITIONS`。
- P1：必须通过路由元数据确认 request 位于 body 参数，不能只依赖手工调用；需要覆盖 FastAPI 和 fallback 两种路径。

#### 技术专家 B

- 结论：`APPROVE`。
- 这是 Python 注解运行时行为修复，不需要新增依赖；应执行 py_compile、Runtime 单测和临时数据库创建验证。

#### 产品经理

- 结论：`APPROVE`。用户填写标题和可选简介后应直接创建，不应看到内部框架的 query/body 校验错误。

#### UI/UX 设计师

- 结论：`APPROVE`。前端错误区域保留用于真正业务错误；请求体解析修复后不改变弹窗视觉和输入保留策略。

### 综合决策

- 综合结论：`APPROVE WITH CONDITIONS`；满足 FastAPI body 元数据、真实创建和既有回归测试后进入实施。

### 实施后五角色复核

- 架构专家：`APPROVE`。仅调整 FastAPI 适配层的注解解析，不改变 `create_project`、HTTP fallback、IPC 或 Renderer 契约。
- 技术专家 A：`APPROVE`。项目虚拟环境中的 FastAPI 路由元数据显示 `request` 位于 body 参数，query 参数为空；不再产生截图中的 missing query request 错误。
- 技术专家 B：`APPROVE WITH FOLLOW-UP`。Fallback 临时 SQLite POST、Runtime 5/5、Agent 16/16、构建和语法检查通过；Uvicorn 端口级验证受当前沙箱禁止绑定回环端口限制，需在本机重启应用后验证一次真实提交。
- 产品经理：`APPROVE`。标题和可选简介可以按表单意图提交，用户不再被框架内部 query/body 错误阻断。
- UI/UX 设计师：`APPROVE`。现有错误区域和输入保留策略不变，修复后仅在真正失败时展示业务错误。

### 本轮实际验证

- `.venv/bin/python` FastAPI 路由依赖检查：通过，POST `/api/projects` 的 `request` 为 body 参数且无 query 参数。
- 临时 SQLite HTTP fallback `POST /api/projects`：通过，中文标题和空简介返回 201。
- `npm run build`：通过。
- `npm run test:runtime`：5/5 通过。
- `npm run test:agent`：16/16 通过。
- `.venv/bin/python -m py_compile runtime/server.py runtime/storage.py`：通过。
- Electron 主进程、适配器、Runtime 和 preload 语法检查：通过。
- `git diff --check`：通过。

## 第三十四轮：Novel Codex 产品级 UI 重构

### 当前问题、目标与非目标

- 当前问题：现有界面功能齐全，但顶部、侧栏、中央内容和右侧协作区都使用相近的面板语气；文章阅读、Agent 回复、执行过程、Composer 和上下文检查器的主次不够明确。浅色主题仍残留较多早期深色面板的颜色、间距和按钮层级。
- 目标：建立“作品导航 → 文章创作 → Codex 协作”的稳定层级；中央文章成为第一视觉主体，右侧对话成为持续工作区，执行过程作为可折叠证据；统一浅色默认主题、蓝色协作色、状态色和焦点环；保留 macOS/Windows 跨平台行为。
- 非目标：不修改 Runtime/Agent 数据流、IPC、数据库、模型配置、作品协议或功能入口；不新增 UI 框架、图标包或网络依赖；不移除深色主题。

### 方案与影响范围

- `src/styles.css`：增加集中式设计令牌和末尾覆盖层，统一三栏网格、导航、中央文章卡、对话、执行时间线、Composer、Inspector、空态和弹窗。
- `src/App.tsx`：增强 Agent Markdown 分块，让单换行标题/列表也能正确分组；不改变原始文本、事件 reducer、请求方法或状态机。
- 候选方案：只调颜色收益不足；完全重写 JSX 风险和范围过大；选择“设计令牌 + CSS 层级重构 + 局部 Markdown 渲染修复”。

### 产品与设计评审（七个独立角色）

#### 产品经理 A：创作工作流

- 结论：`APPROVE WITH CONDITIONS`。
- P1：中央文章必须始终可定位当前章节，右侧协作不能覆盖正文路径，新作品空态必须有唯一下一步。
- 实施项：中央内容使用阅读宽度、章节标题和主操作；Composer 固定在协作栏底部。

#### 产品经理 B：Agent 反馈

- 结论：`APPROVE WITH CONDITIONS`。
- P1：用户回复、Agent 回复、执行过程、变更和错误必须分层，不能全部做成相同卡片。
- 实施项：Agent 主回复使用连续正文，执行/变更使用低对比度折叠容器，等待和错误使用明确状态色。

#### UI/UX 设计师 A：视觉系统

- 结论：`APPROVE WITH CONDITIONS`。
- P1：默认浅色使用灰色工作台 + 白色内容面，主色只用于操作和进行中状态；通过令牌统一圆角、边框、间距和焦点环。

#### UI/UX 设计师 B：跨平台交互

- 结论：`APPROVE`。
- macOS 与 Windows 使用相同布局；保留 1120px 最小宽度和独立滚动边界；图标按钮继续提供 title/aria-label。

#### 架构专家

- 结论：`APPROVE`。
- P1：视觉重构必须停留在 Renderer；Markdown 分块只改变展示，不改变原始 Agent 文本和事件数据。

#### 技术专家 A：可靠性与性能

- 结论：`APPROVE WITH CONDITIONS`。
- P1：不得引入新依赖或高频布局脚本；动画仅用于进行中提示；保持所有执行内容有界。

#### 技术专家 B：实现与测试

- 结论：`APPROVE WITH CONDITIONS`。
- P1：覆盖空态、已有章节、执行中、等待、失败、变更、搜索和创建弹窗，并执行构建、回归、语法和 Electron 窗口检查。

### 综合决策与验收

- 综合结论：`APPROVE WITH CONDITIONS`；全部 P1 转为实现与验收项后落地。
- 风险：末尾覆盖层较集中，未来应逐步拆分为组件样式模块；当前通过无新增依赖和单一 Renderer 状态源控制风险。
- 验收：中央文章是第一视觉主体；对话主回复、执行过程、变更、错误和空态可区分；作品/章节/Story Tests/任务历史入口不回退；深色主题、最小窗口和焦点不回退。

### 实施后七角色复核

- 产品经理 A：`APPROVE`。中央文章、章节导航和空态形成清晰主路径；创建、编辑、导出入口未改变。
- 产品经理 B：`APPROVE`。Agent 回复成为主要内容，执行过程、变更和错误按需展开；Composer 保持在协作上下文中。
- UI/UX 设计师 A：`APPROVE`。浅色主题改为灰色工作台 + 白色内容面，蓝色承担行动/状态强调，视觉令牌已统一。
- UI/UX 设计师 B：`APPROVE WITH FOLLOW-UP`。当前 Electron 无障碍树确认三栏、Composer、任务历史和 Inspector 仍存在；Windows 多分辨率截图回归留作发布前任务。
- 架构专家：`APPROVE`。修改集中在 Renderer CSS 和回复渲染，不穿透 Runtime、Agent、IPC 或数据层。
- 技术专家 A：`APPROVE`。未新增依赖和高频脚本，已有有界执行列表与状态动画保持不变。
- 技术专家 B：`APPROVE`。构建、Runtime/Agent 回归、Electron/Python 语法及差异检查通过；Markdown 分块不改变原始数据。

### 本轮实际验证

- 当前运行 Electron 窗口热更新后，无障碍树确认品牌、作品切换、左侧导航、中央文章、右侧 Codex 对话、Composer、任务历史和 Context Inspector 均可访问。
- `npm run build`：通过。
- `npm run test:runtime`：5/5 通过。
- `npm run test:agent`：16/16 通过。
- Electron 主进程、适配器、Runtime 和 preload 语法检查：通过。
- `.venv/bin/python -m py_compile runtime/server.py runtime/storage.py`：通过。
- `git diff --check`：通过。
- 剩余风险：当前沙箱无法绑定额外 Uvicorn 回环端口，尚未进行 Windows 实机截图回归；不影响本轮 Renderer 重构，发布前补齐即可。

## 第三十五轮：CreativeProject → Story Canon → Narrative IR 基础链路

### 产品目标与当前审计

- 产品目标：证明“对话可以代替写作操作”，但 Agent 的每次创作都必须建立在可查询、可校验、可回溯的故事结构上；本轮优先完成数据事实层和叙事中间层，不以小说生成字数作为验收指标。
- 当前审计：SQLite 已有 `relationships`、`events`、`locations`、`scenes`、`beats` 和 `passages` 表，但 `project_state` 只返回角色、情节线、规则、场景和章节；检索只对少数标题字段做 `LIKE`；Story Tests 只有场景 Beat、章节锚点和规则存在三条检查；Codex 专用运行时还没有应用技能包。
- 本轮边界：不新增小说自动生成器，不将 Codex 对话历史写入 Canon，不接入向量数据库；将当前本地运行时的结构化链路做成清晰、可审查、可供 Agent 使用的目标状态。

### 目标契约

```text
CreativeProject
├── Story Canon
│   ├── Character / Relationship / Event / Location
│   ├── WorldRule / PlotThread
│   └── notes
├── Narrative IR
│   └── Scene → Beat + participants + state changes + canon constraints
└── Novel Edition
    └── Chapter → Passage → Scene / Beat anchors
```

- Runtime 状态以 `canon`、`narrativeIr`、`novelEdition` 三个命名空间作为权威契约，Renderer 同步迁移到该契约，不再把顶层散落数组作为长期数据模型。
- `Narrative IR` 只表达媒介无关的事件与结构；章节正文通过引用关联 Scene / Beat，不反向成为 Canon 的事实来源。
- 搜索建立项目级文档索引，覆盖所有 Canon 实体、Scene/Beat 和章节正文；结果必须带实体类型、来源和有界摘录，便于 Agent 做长篇检索与用户核对。
- Story Tests 变成稳定的规则结果集合，覆盖 Canon 完整性、Beat 顺序、参与者/约束引用、章节锚点和 Passage Beat 引用。
- Chapter Diff 继续保存前后快照，并在读取时提供新增/删除字符统计，支持“对话修改后可审查”的产品闭环。
- Codex Skills 作为应用资源放在 `codex-runtime/skills`，由 Electron 主进程以有界静态指令注入 Novel Codex 专用 App Server；不读取桌面版或普通 CLI 的技能目录。

### 五角色独立评审

#### 架构专家

- 结论：`APPROVE WITH CONDITIONS`。
- P1：必须只有一个权威状态契约，`canon`、`narrativeIr`、`novelEdition` 由 Runtime 组装；不能为了暂时兼容继续复制一套长期顶层模型。
- P1：检索索引必须以 `project_id` 隔离，且 FTS 不可用时仍能通过同一份规范化文档表回退，不能让 SQLite 编译选项决定功能是否可用。
- 条件：Renderer、类型、fixture 和测试同步更新；应用技能只能通过专用主进程注入，不允许 Renderer 直接读取文件系统。

#### 技术专家 A

- 结论：`APPROVE WITH CONDITIONS`。
- P1：索引必须覆盖正文和结构字段，搜索结果必须有最大数量与最大摘录长度；中文查询需要保留 `LIKE` 回退，避免单纯依赖 unicode tokenizer 的分词行为。
- P1：写入章节、Canon 规则和创建作品后刷新当前项目索引；Story Tests 不得因空集合产生错误的“全部通过”假象。
- 条件：为默认种子、新作品、中文搜索、章节 Diff 和不完整结构增加运行时测试。

#### 技术专家 B

- 结论：`APPROVE WITH CONDITIONS`。
- P1：不能改变 Electron 安全边界；技能提示需要有界并只对 App Server 生效，Local Runtime fallback 不应伪装成模型技能执行。
- P1：FastAPI 与 loopback fallback 使用同一套 storage 函数和搜索参数；IPC 继续只暴露窄接口。
- 条件：通过 Python 编译、Runtime 回归、Agent 运行时测试、TypeScript 构建和 Node 语法检查。

#### 产品经理

- 结论：`APPROVE WITH CONDITIONS`。
- P1：用户输入“修改人物关系”“查找灯塔规则”“重写某段”时，系统都应能落到可定位的 Canon、Scene/Beat、章节和 Diff 证据上。
- P1：Story Tests 的失败必须解释“哪里不完整、影响什么”，而不是只显示一个红色结果；新建空作品允许创建，但应明确显示尚未满足故事结构。
- 条件：搜索结果显示来源与摘录，Diff 显示变更规模，技能驱动 Agent 优先查询并引用结构化事实。

#### UI/UX 设计师

- 结论：`APPROVE WITH CONDITIONS`。
- P1：本轮不推翻已完成的三栏布局，但搜索结果和 Story Tests 要为后续中央文章/右侧协作展示保留 `source`、`snippet`、`severity`、`details` 等字段。
- P2：Canon、Narrative IR 和 Novel Edition 应能在 Inspector 中分别呈现；本轮先完成数据契约，避免为了赶 UI 继续使用散落字段。
- 条件：失败态、空作品态、长篇检索证据态要有稳定数据，不使用原始异常对象或无上下文的 `[object Object]`。

### 综合决策、实施顺序与验收

- 综合结论：`APPROVE WITH CONDITIONS`；所有 P1 转为本轮实现项。
- 实施顺序：先重组 Runtime 状态契约，再建立可回退的项目检索索引；随后扩充 Story Tests 与 Chapter Diff 读取模型；最后接入隔离 Codex Skills，并同步类型、UI 投影和测试。
- 验收标准：默认项目状态可完整读取 Canon/Narrative IR/Novel Edition；新作品与默认项目检索互不串数据；搜索能返回正文/规则/角色等实体的来源与摘录；结构破坏能被 Story Tests 指出；章节保存后 Diff 有变更统计；App Server 请求携带 Novel Codex 技能约束且不泄露其他环境。

### 实施后五角色复核

- 架构专家：`APPROVE WITH FOLLOW-UP`。Runtime 已以 `canon`、`narrativeIr`、`novelEdition` 组装权威状态，关系/事件/地点不再被状态接口遗漏；扁平字段明确标记为 Renderer 派生投影。后续应继续迁移 UI 读取，最终移除投影。
- 技术专家 A：`APPROVE`。项目级规范化文档索引覆盖 Canon、Scene/Beat 和章节正文；FTS5 可用时按相关度查询，不可用时同一份文档表执行 `LIKE` 回退；中文结果、类型过滤、数量上限、Story Tests 详情和 Chapter Diff 字符统计均有测试。
- 技术专家 B：`APPROVE`。技能只由主进程从 `codex-runtime/skills` 读取并注入 App Server；Local Runtime fallback、窄 IPC、Electron 主进程和 Python 语法检查通过，未读取其他 Codex 环境。
- 产品经理：`APPROVE WITH FOLLOW-UP`。默认作品的结构事实、长篇正文证据、失败规则和章节变更已可被 Runtime 返回；空作品会明确提示 Canon 基础未建立。下一步需把结构化 Runtime 写操作接入 Agent 工具，让“对话代替写作操作”从约束与检索进入真正写入闭环。
- UI/UX 设计师：`APPROVE WITH FOLLOW-UP`。新增的 `source`、`snippet`、`severity`、`details` 字段足以支持右侧协作和中央文章证据展示；当前 UI 仍主要消费派生投影，搜索摘录的可视化和 Canon/IR 分栏留作下一轮。

### 本轮实际验证

- `npm run test:runtime`：8/8 通过。
- `npm run test:agent`：17/17 通过，包含 Novel Codex 三技能加载与有界 prompt 组合测试。
- `npm run build`：通过，TypeScript 类型检查和 Vite 生产构建通过。

## 第三十七轮：Composer 控件图标视觉收敛

### 需求判断

- 截图显示模型选择器仍使用字体字符下拉符号，发送按钮使用字体箭头；在不同平台和缩放比例下，两者的笔画、对齐和视觉重量不稳定。
- 目标：使用无依赖的内联 SVG 统一下拉、发送、追加和停止图标；将发送/停止按钮从 38px 收敛为 32px，降低 Composer 底部控件的视觉压迫感；保留右对齐布局、键盘发送和所有状态行为。

### 五角色评审

- 架构专家：`APPROVE`。修改限于 Composer Renderer 和 CSS，不改变 Agent 请求、模型值或状态机。
- 技术专家 A：`APPROVE`。内联 SVG 不引入图标依赖，使用 `currentColor` 和 `aria-hidden`，可随主题继承颜色；按钮仍保留 aria-label。
- 技术专家 B：`APPROVE`。需要验证 TS 构建、Runtime/Agent 回归及 CSS 中旧字体图标未残留。
- 产品经理：`APPROVE`。模型标识继续显示真实值，图标只承担识别和操作提示，不增加无行为入口。
- UI/UX 设计师：`APPROVE`。32px 圆形按钮、16px 箭头和 14px chevron 更接近 Codex/系统级 Composer 的轻量控件层级；焦点环由全局可访问性规则保留。

### 实施后复核

- 五角色一致：`APPROVE`。Composer 使用统一 SVG 图标，按钮尺寸和图标笔画已收敛；发送、追加、停止和模型选择行为不变。

### 本轮验证

- `npm run build`：通过，TypeScript 类型检查和 Vite 生产构建通过。
- `npm run test:runtime`：8/8 通过。
- `npm run test:agent`：17/17 通过。
- `git diff --check`：通过。
- `python3 -m py_compile runtime/server.py runtime/storage.py`：通过。
- Electron 主进程、Runtime、Adapter、preload `node --check`：通过。
- loopback fallback 搜索路由：通过，`limit` 与 `kind=worldRule` 能正确返回过滤结果。
- FTS5 在当前 SQLite 环境可用；即使不可用，规范化 `search_documents` 表仍提供 LIKE 回退路径。
- `quick_validate.py` 未能运行：当前 Python 环境缺少 `yaml`（PyYAML），已由 Node 技能加载测试和人工 frontmatter 检查覆盖；不影响应用构建。

## 第三十六轮：移除无行为的 Composer 加号

### 需求判断

- 截图中的加号位于 Composer 输入区左侧，但当前没有附件、上下文选择或其他点击行为；它形成了错误的可操作暗示。
- 目标：移除无行为的图标和对应 CSS 占位，不改变输入、发送、停止、模型选择和 Agent 事件链路。

### 五角色评审

- 架构专家：`APPROVE`。仅调整 Renderer JSX/CSS，不触碰 IPC、Runtime、Agent 状态或数据契约。
- 技术专家 A：`APPROVE`。删除装饰节点及其伪元素规则即可，避免保留不可达交互；需要确认构建后不存在 `.composer-leading`。
- 技术专家 B：`APPROVE`。保留 `textarea`、发送按钮和键盘事件；执行 TypeScript/Vite 构建及差异检查。
- 产品经理：`APPROVE`。删除无效入口，Composer 视觉重点回到“输入 → 发送”；未来若提供添加上下文，应以真实功能和明确语义重新设计。
- UI/UX 设计师：`APPROVE`。移除左侧空白装饰，输入区域获得更直接的起始位置，避免用户寻找不存在的附件能力。

### 实施后复核

- 五角色一致：`APPROVE`。`composer-leading` JSX、CSS 规则和“＋”伪元素均已移除；Enter/Shift+Enter、发送、停止和等待态保持不变。

### 本轮验证

- `rg` 检查：未发现 `composer-leading` 或“＋”伪元素残留。
- `git diff --check`：通过。
- `npm run build`：通过，TypeScript 类型检查和 Vite 生产构建通过。

## 第三十八轮：小说创作闭环——Context Builder、Story Plan 与 ChangeSet

### 变更概览

- 背景/问题：当前 Runtime 已能读取 Canon、Narrative IR、Novel Edition 并执行基础检索与 Story Tests，但自然语言创作指令还不能形成有证据的工作上下文、结构化计划和可审阅变更；这不足以实现“对话代替写作操作”。
- 目标：增加一个可独立测试的小说工作流模块，让指令先生成有界 `Working Context` 和 `Story Plan`，再生成带影响范围、测试结果和审核状态的 `ChangeSet`；补充角色知识事件和剧情依赖记录，为后续真实写入提供稳定契约。
- 非目标：本轮不接入复杂语义向量检索、不实现 LLM 质量评分、不引入 PostgreSQL/Neo4j/Temporal，不让自然语言直接绕过 ChangeSet 修改任意数据库字段。
- 假设与约束：测试阶段允许按新契约演进；保持 SQLite、本地 Runtime 和现有窄 IPC；所有输出有数量/长度上限，并按 `project_id` 隔离。

### 当前状态与影响范围

- 现有行为：`/commands` 只记录 prompt；Runtime 没有上下文包、规划结果、变更集和依赖分析接口；Canon 中只有角色静态 `knowledge_boundary` 字符串。
- 相关模块：`runtime/storage.py`、`runtime/server.py`、`runtime/tests/test_storage.py`、`src/types.ts`、`electron/preload.cjs`、`electron/main.mjs`、`ARCHITECTURE.md`。
- 数据契约：继续以 `canon`、`narrativeIr`、`novelEdition` 为权威状态；新增工作流结果是项目级审阅记录，不替代三大命名空间。

### 方案设计

| 方案 | 优点 | 缺点/风险 | 结论 |
| --- | --- | --- | --- |
| A：直接在 `storage.py` 扩展所有工作流逻辑 | 改动入口少 | 继续膨胀存储模块，查询、规划、变更耦合，难以独立测试 | 不选 |
| B：新增 `runtime/novel_workflow.py`，由存储提供快照和持久化表 | 模块边界清晰；可先做确定性规则，未来替换为 Agent/LLM；接口可供 FastAPI 和 fallback 共用 | 首轮需要增加少量表和路由 | 采用 |

### 推荐方案

- 架构与模块边界：`storage.py` 负责 SQLite schema、项目状态和基础索引；`novel_workflow.py` 负责 Context Builder、Plan、依赖分析与 ChangeSet；`server.py` 只做请求校验和 API 映射；Renderer 只消费结果，不直接拼装领域事实。
- 数据流：`prompt + 可选 chapterId` → 查询当前项目状态/检索结果 → 生成有界 `workingContext` → 生成 `storyPlan` → 从计划生成 `ChangeSet(status=proposed)` → 返回关联的 Story Tests 与 affected entities；本轮 ChangeSet 先提供提议和审阅，不自动伪造正文修改。
- 新数据：`character_knowledge` 保存角色对事实的 `unknown / suspected / known / false` 状态及章节位置；`story_dependencies` 保存源实体到受影响实体的有向关系；`workflow_runs` 保存有限的 prompt、上下文摘要、计划和 ChangeSet JSON，便于恢复和审查。
- API：新增 `POST /api/projects/{id}/workflow/context`、`POST /api/projects/{id}/workflow/plan`、`POST /api/projects/{id}/workflow/change-sets`；三条接口都走同一 Python 领域函数，FastAPI 和 loopback fallback 行为一致。
- 错误处理：项目不存在返回 404；空 prompt、无效章节或超过长度上限返回 400；查询结果不足时返回空证据并在计划中标注“需要调查”，不捏造事实。
- 安全、性能、跨平台：只读查询按项目过滤；prompt、snippet、计划步骤和变更明细设置固定上限；SQLite 事务写入；不新增平台特定依赖，Windows/macOS 共用同一 Python/HTTP 契约。
- UI 信息架构：本轮不改主布局；后续 UI 可在右侧 Task/Changes 面板展示 `workingContext`、计划、影响实体和审阅状态。本轮先保证数据可渲染，空态、错误态和 proposed 状态均有稳定字段。
- 测试与回滚：增加 Runtime 单元/接口级测试，覆盖中文上下文、章节过滤、知识状态、依赖影响、ChangeSet 项目隔离、空 prompt 和错误状态；schema 使用 `CREATE TABLE IF NOT EXISTS`，回滚仅需删除本轮新增记录/数据库文件，不修改既有章节正文。

### 五角色评审

#### 架构专家——边界与演进

- 结论：`APPROVE WITH CONDITIONS`。
- P0/P1：P1：工作流模块不能成为第二套 Canon；必须以 `project_state()` 的三大命名空间快照为输入，并把提议结果作为审阅记录保存。
- P2/P3：P2：后续可将 `story_dependencies` 抽象为统一实体引用类型，避免未来为漫画再建一套图结构。
- 架构决策与证据：选择独立 `novel_workflow.py`，使 Context/Plan/ChangeSet 不侵入当前存储读模型；所有 Runtime 入口可共享同一实现。
- 建议与责任：实施时保持领域函数无 HTTP 依赖；由技术专家 B 补齐契约测试。

#### 技术专家 A——可靠性与系统风险

- 结论：`APPROVE WITH CONDITIONS`。
- P0/P1：P1：必须限制 prompt、证据、步骤和 JSON 摘要大小，并对查询、写入使用项目 ID 过滤；P1：不能把未验证的自然语言推断写入 Canon。
- P2/P3：P2：未来增加真正的语义检索时再引入 embedding，不在本轮增加网络依赖。
- 建议与责任：使用 SQLite 事务和 JSON 序列化；workflow API 与 loopback fallback 共用函数；由实施者增加边界测试。

#### 技术专家 B——实现与质量

- 结论：`APPROVE WITH CONDITIONS`。
- P0/P1：P1：计划和 ChangeSet 必须是确定性、可测试的 JSON 结构；P1：项目不存在、章节不存在、空指令和旧数据库都不能产生 `[object Object]` 或 500 未解释错误。
- P2/P3：P2：后续为 ChangeSet 增加 apply/undo 前，先补完整状态迁移和快照测试。
- 建议与责任：新增模块保持小而纯；FastAPI/fallback 路由对齐；执行 Python、Runtime、Agent、TypeScript 构建回归。

#### 产品经理——目标与验收

- 结论：`APPROVE WITH CONDITIONS`。
- P0/P1：P1：用户输入“让林夕开始怀疑张衡”时，系统必须返回当前证据、计划步骤、影响对象和待审核变更，而不是直接声称完成；P1：无证据时必须明确显示需要调查。
- P2/P3：P2：质量评分、伏笔回收和自动重写进入下一阶段，不能混入本轮造成验收不可控。
- 验收标准：上下文能按章节过滤；计划至少包含调查、规划、验证步骤；ChangeSet 状态为 `proposed` 并带 affected entities；Story Tests 结果可关联；不同项目不能互读。
- 建议与责任：先打通可审阅链路，再接 Agent 工具调用和真实结构化写入。

#### UI/UX 设计师——流程与体验

- 结论：`APPROVE WITH CONDITIONS`。
- UI 影响说明：本轮以契约为主，不改变三栏布局；数据为右侧 Task/Changes 卡片预留 `status`、`evidence`、`steps`、`affected`、`tests` 字段。
- P0/P1：P1：计划中必须区分“已知事实”和“需要调查”，避免用户误以为模型推断就是 Canon；P1：ChangeSet 必须显示 proposed/reviewed/applied/declined 状态。
- P2/P3：P2：后续增加详情折叠、键盘导航和空态文案。
- 状态与可访问性检查：错误返回可读文本；空上下文有明确空态；后续 UI 使用 `aria-live` 展示工作流状态，不暴露原始 JSON。
- 建议与责任：本轮先由类型和 API 固定数据形状，下一轮在右侧 Changes/Task 落地视觉层。

### 综合决策

- 最终结论：`APPROVE WITH CONDITIONS`，所有 P1 转为本轮实施与验收项。
- 已解决的阻塞项：不将工作流逻辑塞入 Renderer；不自动修改正文；统一 FastAPI/fallback 领域函数；新增数据按项目隔离且有界。
- 带条件批准项及验收方式：通过 Runtime 测试验证 Context、Plan、ChangeSet、knowledge/dependency 和错误路径；通过构建验证前端契约。
- 需要用户决策的事项：无。本轮采用提议式 ChangeSet，真实 apply/undo 留待下一轮结构化写入方案。
- 批准后实施范围：`novel_workflow.py`、`storage.py` schema/查询、`server.py` 路由、preload/main/TypeScript 契约、Runtime 测试、架构文档。

### 实施后复核

- 实际修改：新增 `runtime/novel_workflow.py`，实现有界 Working Context、中文短语检索、角色知识状态读取、剧情依赖分析、Story Plan、提议式 ChangeSet、结构化操作校验与显式 Apply；`storage.py` 增加 `character_knowledge`、`story_dependencies`、`workflow_runs` 表及默认作品迁移 fixture；`server.py`、Electron preload/main 和 `src/types.ts` 接入统一契约；App Server 发送前生成工作包并向对话时间线发出工作流摘要，本地 Runtime 也展示实际计划步骤；新增 `novel-workflow` 应用技能并同步 `ARCHITECTURE.md`、`codex-runtime/README.md`。
- 与方案的偏差：没有偏离核心方案；本轮 ChangeSet 仍按批准范围默认为 `proposed`，只有显式调用 Apply 才会写入。右侧 Task/Changes 的视觉展示和 Undo 尚未实现，保留为下一轮 UI/结构化写入工作。
- 验证命令与结果：`npm run test:runtime`（14/14）；`npm run test:agent`（17/17）；`npm run build`（通过）；`python3 -m py_compile ...`（通过）；`node --check electron/main.mjs`、`node --check electron/preload.cjs`（通过）；`git diff --check`（通过）。
- 架构专家：`APPROVE`。工作流逻辑位于独立领域模块，仍以三大命名空间状态为事实输入，没有新增第二套 Canon。
- 技术专家 A：`APPROVE`。上下文、证据、章节正文和操作均有边界；所有数据库写入按项目隔离并通过事务执行，FastAPI 与 loopback 共用实现。
- 技术专家 B：`APPROVE`。14 个 Runtime 测试覆盖 Context、Plan、ChangeSet、知识状态、章节 Diff、项目隔离、路由和错误操作；Agent 17 项回归保持通过。
- 产品经理：`APPROVE WITH FOLLOW-UP`。用户指令现在能形成证据、计划、影响范围和待审阅 ChangeSet；下一步必须把右侧 UI 的 Review/Apply/Undo 与真实 Agent Tool 调用接上，才能完成完整的“对话代替写作操作”。
- UI/UX 设计师：`APPROVE WITH FOLLOW-UP`。数据契约已为 Task/Changes 面板预留稳定字段；本轮未改变现有布局，下一轮需要实现 Context/Plan/Changes 的折叠展示、空态、失败态、键盘操作和审阅状态反馈。
- 剩余风险：当前 Agent App Server 通过主进程注入工作包，尚未拥有独立的 Runtime Tool schema；因此 Agent 能看到证据和计划，但仍不能在对话中直接提交结构化操作。质量评估、伏笔检测和语义向量检索仍未纳入本轮。
