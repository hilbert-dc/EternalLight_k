# Renderer 职责边界重构（2026-10-12）

## 变更与责任边界（前审）

Renderer 负责 UI 状态、对话及创作编辑投影（B01/B03/B06/B08）。本次按布局、对话呈现、执行控制、自动创作及发布审阅等实际职责拆分 App 与 CreativeWorkspaceView；保留已存在的项目、编辑会话、执行代次与回执归属检查。作品事实仍唯一归 Native，公开接口仍为 NovelCodexApi，未增加字段、IPC、授权或预算政策。不创建重复领域状态，不以抽 hook 宣称减少渲染。

保留现有用户输入、保存/放弃/取消、错误文案与可访问性入口。区域本地交互应由对应子组件持有；跨区协调通过显式参数传递。新请求或异步回执必须保留原身份检查，旧项目结果不得覆盖当前 UI。实现缺陷之外不改变创作流程。零兼容：不新增旧字段 fallback、双轨、迁移或删除用户资料。

## 状态判定

| 状态 | 结果 | 验证 |
| --- | --- | --- |
| 当前项目、当前编辑/执行身份 | 保持现有读取/保存/停止调用与真实回执 | 相关 UI 测试 |
| 切换项目/对话后旧请求到达 | 保留 scope/ref/取消检查，不投影到新上下文 | 对话/执行回归 |
| 取消、失败、未知写入 | 保留原回执与错误区分，不因重构重发写请求 | 停止、恢复和草稿回归 |

## 五视角前审（同一代理五视角自审）

| 视角 | 前审 | 后审 |
| --- | --- | --- |
| 架构 | APPROVE：仅 Renderer 内部职责拆分，事实权威和公共接口不变 | 见后审与证据 |
| 技术 A | APPROVE WITH CONDITIONS：保留异步 scope 与清理；类型检查与行为回归 | 见后审与证据 |
| 技术 B | APPROVE WITH CONDITIONS：一次切换调用方，清除原实现，核对闭包依赖 | 见后审与证据 |
| 产品 | APPROVE：保持现有创作、保存、采用与停止流程 | 见后审与证据 |
| UI/UX | APPROVE WITH CONDITIONS：无新增入口，保持键盘、焦点和状态提示 | 见后审与证据 |

## 证据计划

E1：tsc --noEmit；E2：architecture:check；E3：现有对话、执行、草稿、创作 UI 相关测试。按实际结果补充后审，不将计划作为通过证据。没有性能 profiling，不能声称渲染加速。

## 实施与五视角后审（同一代理五视角自审）

- App 的布局、导航壳、对话输入、记录与检查器分别归 AppShell、WorkspaceLayout、ConversationComposer、AgentTranscript、AgentInspector；布局交互由 useWorkspaceLayout 持有，执行恢复与暂停/重试分别由 useExecutionRecovery、useConversationTaskActions 持有。保留原请求 scope、项目身份与重复请求保护。
- CreativeWorkspaceView 的保存回执协调归 useCreativeSave，生成进度归 useGenerationProgress，发布审阅归 PublicationReview；GenerationPermitDialog 持有确认中/错误状态并校验项目及 review 身份。useReviewChoices 用单一快照集中六项审阅状态，保留函数式更新；review-ui-storage 是持久化读写单点。
- 本次不是拆包或渲染性能优化。主组件仍负责跨区域生命周期、对话受理、编辑来源与项目切换；仅移动一个巨大组件或引入数十参数的发送工厂不能改善这些边界，所以没有采用。App 从本次起点 2542 行降至约 1914 行（同期集成变更会影响计数），CreativeWorkspaceView 从 2010 行降至 1590 行；useState 调用分别由 79/56 降至 71/39。尚未做性能 profiling。

| 视角 | 后审 |
| --- | --- |
| 架构 | APPROVE：公共 IPC/HTTP 未新增，领域事实仍由 Native 保管，Renderer 仅拆状态与呈现责任；共享类型由合同任务负责。 |
| 技术 A | APPROVE WITH CONDITIONS：tsc 与架构门禁通过；scope/CAS/取消保护随原实现保留；全 UI 套件存在下列基线失败，不能将其宣称全绿。 |
| 技术 B | APPROVE：旧内联实现已由明确模块取代，无双轨；新增测试覆盖函数式审阅更新、重复确认以及跨项目晚到失败隔离。 |
| 产品 | APPROVE WITH CONDITIONS：停止、恢复、发布、输入保留、多对话定向测试通过；没有为过时夹具恢复旧对话 UI 或虚构回执。 |
| UI/UX | APPROVE WITH CONDITIONS：viewport 验证跨主题/尺寸、输入光标/滚动、IME、停止按钮可达与详情空间恢复；既有窄栏检查及历史对话夹具失败单独登记。 |

## 实测证据

- `npx tsc --noEmit`：通过。共享生成类型出现后再次验证，未编造缺失时间字段。
- `npm run architecture:check`：通过，714 文件、7 crate，diagnostics 为空。
- 定向 `chapter-publication-ui`、`conversation-generation-handoff-ui`、`creative-workspace-ui`、`app-task-controls-ui`、`multi-conversation-ui`：13/13 通过。
- 新 `frontend-boundaries-ui`：1/1 通过；原生 React DOM 测试，不仅检查实现文字。
- `npm run test:ui`：完整完成，115 检查、96 通过、19 失败（包括父测试及子测试）；该结果发生于下面四项夹具补齐之前，不以之后的定向通过替代一份不存在的全绿报告。
- 之后 `conversation-reply-actions-ui`、`property-rules-ui`、`language-first-conversation-ui`、`search-experience-ui` 均通过；后者2个测试均过。相关改动仅是当前公开接口、现行控件名称和已审核的展示快照。
- `app-recovery-ui` 的四项、`workspace-viewport-ui`、`app-author-input-ui`、`author-session-ui`、`chapter-title-sync-ui` 均已定向通过；最终联合批次结果见下。

夹具调整保留原有保护断言：输入消费通过 `agent/message-updated: queued`；对话夹具登记并读取实际线程；项目标题读取现行切换器；原始错误须展开详情；保存完成以真实保存状态等待；删除待定期间以 native modal 的背景焦点不可达验证保护，未要求每个背景按钮都设置 disabled。reply-actions 快照仅将旧步骤计数换为已有的耗时标题，复制原文与异步隔离断言均不变。

## 第一轮未通过项与基线边界（历史记录，后续结果见末尾）

以下失败没有通过放宽断言或改变产品来掩盖：

1. `agent-events`、`conversation-process-ui`、`unfinished-output-ui`：旧 provider 错误可读说明断言，与本次提取无关的错误呈现合同。
2. `analysis-conversation-entry-ui`：独立规划审阅组件窄栏溢出；未导入本次两个主组件。
3. `app-agent-availability-ui`：精确讨论来源后续提交未抵达 Host；补齐受理事件后，用重构前 App/Creative 精确重现同一失败。
4. `chapter-deletion-ui`：空对话中的待处理连续性检查未显示；原组件精确重现，早先删除确认、读回与未知回执保护已通过。
5. `conversation-title-ui`、`thread-history-view-ui`：夹具仍依赖已移除的 sidebar-task/thread-history 入口；原组件同样失败，需要按现行多对话界面重新设计这些完整旅程。
6. `discussion-source-ui`、`planning-submission-ui`、`selection-discussion-ui`：讨论模板、规划准备后精确来源与重试消费输入的旧合同；原组件同样复现。
7. `project-entry-ui`、`revision-repair-ui`：保留其失败，使用原测试 harness 和原组件对照结果见后续证据。
8. `work-directory-ui`：独立目录的 scroll 关闭菜单断言，未导入本次两个主组件。

基线采用实施前保存的两个原组件，通过 esbuild onLoad 仅替换 App/Creative 文件；其余当前工作区与测试 harness 不变。没有把 Git HEAD 当作用户当前工作区基线。日志保存在本次执行环境 `/tmp/frontend-*`，不会加入产品。

最终联合批次 `frontend-boundaries-ui`、`app-recovery-ui`、`workspace-viewport-ui`、`app-author-input-ui`、`author-session-ui`、`chapter-title-sync-ui`、`property-rules-ui`、`language-first-conversation-ui`、`search-experience-ui`、`conversation-reply-actions-ui`：14/14 通过。与前述13项定向结果有各自明确覆盖，不合并声称全量套件通过。新增边界测试已由工程整理任务加入 `test:ui` 及受影响前端验证链。

原 harness 对照 `project-entry-ui` / `revision-repair-ui` / `thread-history-view-ui`：3项在原组件同样失败，分别为 `Narrow volume navigation overflow light`、`App failed to load`、`History UI did not settle`，与本次输出一致。最终 tsc 与 architecture 再次通过。上述未绿项是仍需独立解决的工作区测试/产品问题，不是这次已经解决的成果。

## 独立章节创建生命周期补充

沿用上面的前审责任边界，将章节创建作为完整应用生命周期抽到 `useChapterCreation`：单一 reducer 管理请求打开、目标卷、提交与已创建回执的读回恢复。对外只提供 create/recover/close 与只读投影，不暴露状态 setter；标题输入仍由 `StandaloneChapterForm` 局部持有。CreativeWorkspaceView 仅提供共享写入锁的 acquire/release、一次性的读回应用和章节选择回调，保留 dirty/reviewDirty/执行中/补充编辑锁，不改变作者导航确认。

已创建章节的恢复只使用原 chapterId 重新读取，不再次 materialize。异步 owner 绑定 API、项目和视图代次，离开再返回同一项目的旧请求也不得选中新章节。读回必须同时匹配项目与章节；其他项目的目标卷、打开状态和恢复提示不显示。原 materializeStandalone 函数、四项分散状态及其准备 effect 已从主组件删除，无旧实现并存。

五视角补充后审：架构 APPROVE（事务与生成事实仍归 Native）；技术 A/B APPROVE（同步重复提交保护、回执恢复及 owner 防 ABA 已测试，tsc通过）；产品 APPROVE（保留未保存保护、卷身份、手动创建与后续写作）；UI/UX APPROVE（保留表单位置、标题输入、自动聚焦、取消和重读入口，无新增作者操作）。这是状态及生命周期边界改造，不声明渲染性能收益。

验证：`frontend-boundaries-ui` 新增章节重复提交、dirty阻止写入、保存后读失败只读恢复、A→B→A晚到回执隔离用例并通过。`chapter-first-write-ui` 通过；真实 `work-directory-ui` 的 source-bound volume chapter 用例通过，仍断言两次实际作者创建、零模型调用和读回恢复不重复写。`creative-workspace-ui` 的8个相关用例通过，空工作区夹具发现缺少公开 multimedia read API，补齐既有 novelFixtureMedia 后独立复验（见最终结果）。tsc与715文件/7crate架构检查通过。

空工作区夹具补齐后独立复验1/1通过（包含无task API与重启子场景）；整个本次相关批次11项均有通过证据，另有真实卷内创建1项通过。最终没有为本次生命周期新增失败，也未修改 Rust。

生产构建闭环：章节创建新增断言位于已注册的 `electron/tests/frontend-boundaries-ui.test.mjs`，已再次读取 package.json 确认 `test:ui` 包含该项，无新增测试文件漏挂。最后执行 `npm run build` exit 0：preload API 校验、Renderer tsc、Host 111文件类型覆盖与类型检查、Vite 546模块生产构建均通过。Vite仍报告既有大chunk提示（JS约4.70MB），本次没有进行bundle切分或承诺加载性能改善。

### 已知输出上限错误说明补充前审

全量验收发现真实 Host 输出上限错误为 `provider_output_item_limit; outputItem=<JSON>`，Renderer 仅识别裸 code，导致实际失败无对应中文说明。五视角前审：架构/技术A/技术B/产品/UI均 APPROVE WITH CONDITIONS；仅修现有错误说明边界，接受精确机器code、明确结构化code或已校验输出诊断格式，可剥离精确Electron Error包装；不从任意正文substring推断，不改变错误、重试或预算语义，详情仍显示原始诊断。旧测试的虚构colon尾缀替换为生产适配器规定的outputItem载荷，保留原本的本地化、原始证据和可见性断言，并补负例防任意正文伪造。

### 对话名称通知补充前审

现行可见导航测试发现 `applyThreadEvent` 不处理 thread/name/updated，尽管调用方宣称支持。已核对本地 app-server-protocol 的 ThreadNameUpdatedNotification：正式字段是 `threadName`，旧夹具的 `name` 也不正确。五视角前审均 APPROVE WITH CONDITIONS：仅更新当前项目已知线程名称，未知线程/明确跨项目事件不得变更；使用正式字段而不保留旧字段fallback；名称是展示事实，不触发模型、resume或改写preview。当前多对话导航替代已删除旧详情入口，以同等隐私、执行归属与项目隔离断言验证。


## 当前完整 UI 复跑与补充后审

在共享类型最新消费和章节 reducer 纳入后，单进程重新执行 `npm run test:ui`：**116 tests，102 pass，14 fail**，exit 1，耗时235748.854791ms。此为实际完整运行，未用前一轮96/115加减推算；日志 `/tmp/frontend-final-full-current.txt`。14个失败包括 agent-events 父/子2项，以及 app-agent-availability、chapter-deletion、conversation-process、conversation-title、discussion-source、app-turn-recovery、planning-submission、revision-repair、selection-discussion、thread-history-view、unfinished-output、work-directory 各1项。新章节边界用例通过。原先窄栏规划/项目入口本轮通过，故不可当作稳定基线失败；app-turn-recovery 本轮失败没有实施前对照证据，也不得归因基线。其余原组件对照仅证明当时相同失败可复现，不代表之后所有失败都是旧问题。

输出上限说明补充后审：五视角均 APPROVE。已从生产 bounded-output-correction 核对真实格式，3个场景仍传带 outputItem 的完整诊断，保留本地化、原文详情、非重试和事件时间顺序断言；新增精确结构化code、Electron包装及任意正文/坏JSON/未超限负例。`agent-error-origin`、`agent-events`、`conversation-process-ui`、`unfinished-output-ui` 联合 **33/33通过**，日志 `/tmp/frontend-error-final.txt`。修复是 Renderer 分类边界，不改变 Host 预算、重试或事件事实。

对话名称通知补充后审：五视角均 APPROVE。现行导航旅程保留作者输入不泄漏、原始preview不改写、运行会话与选中会话独立、零resume、跨项目隔离；官方 threadName 更新已知当前项目线程，未知和明确跨项目通知负例均通过。快照按现行“新对话”持久名称及导航界面人工审阅，不把作者指令临时变成标题。`conversation-title-ui` 单独通过后，再联合 `frontend-boundaries-ui`、`multi-conversation-ui` **3/3通过**，日志 `/tmp/frontend-last-boundaries.txt`。

修复后的 `npx tsc --noEmit`、`architecture:check`（719文件/7crate，零diagnostics）和 `npm run build` 均exit0。最新构建日志 `/tmp/frontend-corrections-production-build.txt`，仍有既有约4.70MB JS chunk提示。其余9项失败由并行任务继续收口，最终集成报告应引用其实际验证；本文不将上述定向通过等同于一轮尚未执行的全套全绿。

### 集成夹具复核：恢复与目录

`app-turn-recovery-ui` 在初始化按钮已出现但尚不可发送时点击，未进入提交；现在等待按钮 enabled 后提交，保留 6 次生成 / 5 次只读恢复及旧回执隔离断言。`work-directory-ui` 过去派发没有位置变化的 scroll；现先断言未移动锚点不丢菜单，再通过真实滚动移动锚点并要求菜单关闭，保留编辑器/身份/键盘全部断言。没有修改生产交互。五视角后审：架构、技术 A/B、产品、UI/UX APPROVE（模拟真实已定义状态而非放松保护）。Electron 真实运行：model-connection 2/2、work-directory 3/3 通过。

进一步核查当前 Engine 已取消独立单项token窗口，真实新诊断为 utf8Bytes/262144/null tokens。因此上述三场景及分类正负例改用当前生产字节诊断（262145 bytes），不再用历史token诊断作当前证据；对应四文件联合再次 **33/33通过**，日志 `/tmp/frontend-error-bytes-final.txt`。说明边界仍能解释Host已认可的历史token诊断，未修改Engine预算或重试行为。

## 补充收口：只读历史入口及旧 fixture 合同

五视角前审（架构、技术A、技术B、产品、UI/UX）APPROVE WITH CONDITIONS：恢复导航重构遗漏的既有只读阅读能力，复用现存 [useThreadHistoryDetail](../src/useThreadHistoryDetail.ts) 与 [ConversationReader](../src/ConversationReader.tsx)，不得恢复原任务、取消归档、切换当前运行或删除未发送输入。主代理确认保持该能力属于本轮范围。当前“管理对话”菜单提供“只读查看”，独立阅读层保持原 live scroller 与 composer 节点，沿用现有样式和项目/请求代次隔离，没有第二套载荷来源。

fixture 更正均按现行源码合同：
- [planning-submission fixture](../electron/tests/fixtures/planning-submission-ui.tsx)：App.prepareConversation 附加来源时不会生成作者输入；测试先验证空输入，再明确输入问题，并保留原精确来源/CAS/禁止正式写入断言。App 的 `agent/message-updated` 仅在匹配请求收到 `queued` 后清理提交草稿；本 fixture 未发受理事件，故两项快照明确保留原输入，不伪造受理回执。
- [revision-repair fixture](../electron/tests/fixtures/revision-repair-ui.tsx)：补齐 [work-projection-refresh](../src/useWorkProjectionRefresh.ts) 消费的 WorkStep.outputRefs；[章节草稿存储](../src/chapter-draft-storage.ts) 在重挂载恢复本地正文，测试验证恢复后由作者还原保存文本；ProjectSwitcher 的 busy 来自 projectLoading，不是执行状态，测试改为导航后拒绝旧 pending 请求并保留跨项目不污染断言。
- [thread-history fixture](../electron/tests/fixtures/thread-history-view-ui.tsx)：只替换已不存在的 sidebar-task 入口为当前可见管理菜单；原运行/停止、composer 同节点与选区、只读权限、未配置模型可读、加载失败重试、项目与选择 ABA、焦点返回和窄栏主题断言全部保留。

五视角后审均 APPROVE：实际 Electron 三项回归均通过（planning 1/1、revision-repair/history 2/2），`tsc --noEmit` 通过。未删隐私/权限/只读保护断言，未扩展合同之外的模型或存储权限。

## 最终合并后验证

全部并行修改完成后执行 `npm run test:ui`：116/116，通过、0 跳过、0 失败（211.36 秒，`/tmp/eternallight-ui-final-integrated.log`）。`npm run build` 同时通过 preload 合同、前端 TypeScript、111 个 Host 文件类型覆盖及 TypeScript、Vite 构建。现有约 4.7 MB 主 chunk 提示仍保留，本次不宣称完成打包性能优化。`architecture:check`：720 文件/7 crate，无诊断；`test:verification`：12/12。五视角后审最终技术条件关闭；全部原先失败的 UI 场景已经在此完整套件重新执行，无用定向通过代替全套的声明。
