# 对话任务控制实施记录

依据：[目标方案](CONVERSATION_TASK_CONTROL_DESIGN.md)、[架构规则](../../../ARCHITECTURE_RULES.md)。开始日期：2026-10-09。状态：实施中；未完成项不视为交付。

## 实施前评审

本轮按 P0–P4 顺序实施，先完成准确停止及可恢复的原 Work，再接消息和并发执行。Native 是控制、身份、来源和保存权威；Engine 拥有请求及取消；Host 只定位和派发；Renderer 显示原目标的真实回执。当前工作区包含大量已有改动，保留其内容，不将整份工作区差异归于本轮。

| 视角 | 结论 | 实施约束 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 复用 Work、执行租约与持久事件；不增加第二领域规划器 |
| 技术 A | APPROVE WITH CONDITIONS | Native 持久停止先于中断；旧 Attempt 不复活；保存和费用证据保留 |
| 技术 B | APPROVE WITH CONDITIONS | preload/Host/Runtime/Native/Renderer 合同一次性同步；先验证真实边界再开放入口 |
| 产品 | APPROVE WITH CONDITIONS | 停止保留目标，取消独立；新功能只有实际可用才展示 |
| UI/UX | APPROVE WITH CONDITIONS | 不串对话、消息和审批；保留草稿；等待停止与已停止分开 |

## 状态判定与验证

| 输入事实 | 合法动作与副作用 | 验证 |
| --- | --- | --- |
| 停止指向旧作品/任务/Turn | 拒绝，零控制其他执行 | Host 目标隔离测试 |
| Work 持久停止先于写入 | 原能力失效，拒绝新提交；已有费用不清零 | Native 事务与恢复测试 |
| 保存先于停止 | 保存成果保留，禁止后继 | Native 真实仓储测试 |
| 暂停解除后旧 Attempt 回调 | 原终态不重开，不再获得写资格 | Native 旧能力回归 |
| 空对话创建后名称保存失败 | Host 保留已返回的 Core Thread 身份，只重试名称保存和 Native 登记，不重复创建 | 对话创建生产合同测试 |
| 运行结束与停止竞争 | 采用真实终态，不伪造取消失败 | Host/UI 回执测试 |
| 控制/取消失败 | 保持真实错误与待核对，不宣称成功 | Host/UI 故障测试 |

## 阶段记录

| 阶段 | 状态 | 证据 |
| --- | --- | --- |
| P0 控制与消息合同 | 部分完成 | 精确启停、原 Work 恢复、独立控制修订、持久控制命令和本步结束后暂停已接通；持久消息合同尚未完成 |
| P1 执行隔离与多对话 | 待实施 | 无 |
| P2 运行中调整与临时插问 | 待实施 | 无 |
| P3 持久队列与恢复体验 | 待实施 | 无 |
| P4 产品验收与资源收敛 | 待实施 | 无 |

## 已接通的切片：可靠启停

- [execution-requests.mjs](../../electron/execution-requests.mjs) 在 Host 中维护执行请求身份。preload 为启动调用生成独立 `requestId`，向界面报告该身份，然后通过 `{ requestId, args }` 调用当前 IPC。停止必须携带 `{ projectId, requestId }`，同时验证原 WebContents。不存在、已结束或其他请求不会回退控制全局任务。相同在途停止合并，异步准备阶段检查停止标记；后台消费者显式脱离作者请求上下文。
- [main.mjs](../../electron/main.mjs) 先对原 Work 执行 Native 暂停，再中断绑定的执行。尚在创建中的 Work 返回后先暂停，不能继续领取或发送。停止与终态取消分离；已有明确“取消任务”入口保持取消语义。Native 暂停失败仍中断精确的本机执行，并保留未确认错误，不伪造成功回执。
- [work_tasks.rs](../../native/story-core/src/work/work_tasks.rs) 在同一暂停事务中撤销原 Work 的全部活动 Attempt 能力，记录 `author_paused` 和终态，再把对应步骤保留为可核对的阻塞状态。恢复仍经过原来源、保存结果和费用核对；创建新 Attempt，旧 Attempt 不复活。没有清除产物、阅读检查点或费用。Generation 原有暂停事务沿用其领域实现。
- [App.tsx](../../src/App.tsx)、[preload.cjs](../../electron/preload.cjs)、[types.ts](../../src/types.ts) 同步当前合同。界面按本轮 scope 取请求身份；后台任务或重载后的控制按实际 Work/Attempt 定位。恢复期间的停止改为暂停原 Work。媒体对话同步精确目标，准备阶段停止保留输入和图片，且不启动模型。
- [codex-adapter.mjs](../../electron/codex-adapter.mjs)、[conversation-handlers.mjs](../../electron/conversation-handlers.mjs) 在空对话创建返回身份后、名称持久化前保存待登记身份。名称或历史核对失败时保留精确线程及管理进程，重试不再次 `thread/start`。完整历史核对成功前，不发布线程就绪事件；Native 对话登记成功后才切换选择。

这些是已使用的生产路径，未增加无调用方的并发或消息框架。`ExecutionRequests` 是存活 Host 内的定位与取消上下文，不是持久 AuthorCommand 账本，也不提供崩溃后的 exactly-once 保证。

## 验证证据

以下测试均使用隔离夹具、内存或临时数据库，没有运行真实付费模型，也没有修改真实作者资料。

| 验证 | 结果与边界 |
| --- | --- |
| `npm run architecture:check` | 通过，零架构诊断 |
| `npm run build` | TypeScript 与 Vite 构建通过；存在 bundle 体积提示，不作为性能验收 |
| `execution-requests`、`execution-control-handlers`、`active-turn-recovery`、`chapter-structure-engine` | 25 项通过：目标隔离、异步准备、停止失败、迟到结果、后台任务、未知创建和预算保留 |
| `codex-adapter`、`conversation-handlers` | 44 项通过：线程持久化、名称失败重试、真实目标恢复与管理/任务能力隔离 |
| `agent-configuration` | 40 项通过：创作接续、停止阻止后继、不同 Work 控制、Generation 暂停、模型配置门禁 |
| `revision-repair-handlers` | 18 项通过：原修复来源、当前请求封套、暂停恢复沿用预算 |
| Native `work_pause_control_tests` | 2 项通过：原 Work 暂停、其他 Work 不受影响、旧 Attempt 不复活、过期 CAS 与迟到失败拒绝 |
| Native `reading_checkpoint_tests` | 8 项通过：阅读进度和预算随新 Attempt 恢复、旧凭证和过期来源拒绝 |
| Native `work_modeling_recovery_tests` | 15 项通过：已保存成果核对、未知费用/写入阻断、作者暂停不被自动解除 |
| Electron `actual App reconciles…` | 真实 App 受控测试通过：8 次发送、7 次停止，覆盖重复点击、回执先后、旧回执、新轮次与真实供应商错误 |
| Electron `media-application-ui` | 真实媒体界面测试通过：草稿、图片、导航、迟到历史及准备阶段停止零新增模型请求 |

验证时同步了不再符合当前合同的夹具：阅读测试不再把已暂停的 Attempt 改写为 failed；修复测试检查独立 additionalContext 内的应用约束，并提供真实 rootThreadId；恢复测试区分未发布结果导致的 executionBlocked 与未知写入导致的 pendingRecovery。长文本测试保留原文与预算断言，采用能容纳当前完整约束的长文本样本；没有放宽 9000 文本预算或截断作者输入。

本机最初的沙箱 Native 新二进制未进入测试入口。只终止了本轮已核对的挂起进程，使用获准的普通本机环境重跑隔离测试后通过；没有修改系统安全设置。没有把启动等待计为通过。

## 后审

| 视角 | 本切片结论 | 剩余边界 |
| --- | --- | --- |
| 架构 | APPROVE | Native 保留 Work/能力权威，Host 只定位并派发；全局运行时仍待 P1 拆分 |
| 技术 A | APPROVE | 暂停、旧凭证、阅读恢复与未知结果阻断已有真实仓储验证；尚无完整持久命令日志 |
| 技术 B | APPROVE | 当前 IPC、preload、界面与测试同步；不接受无目标停止旧合同 |
| 产品 | APPROVE WITH CONDITIONS | 可靠启停切片可用，不能称整套 Cursor/Codex 体验已完成 |
| UI/UX | APPROVE WITH CONDITIONS | 精确回执和准备阶段停止已验证；多对话并发、临时插问返回尚无交付 |

零兼容检查：未添加历史 schema 迁移、双读双写、旧 IPC fallback；没有删除或重建真实作者数据。后台 Work 控制与在途 request 控制是不同生命周期目标，不是旧协议兼容入口。

## 第一段结束时的未交付范围（历史记录）

以下描述第一段当时的状态；后续实现和当前剩余项以第八至十二段记录为准。

1. P0 剩余：“完成当前步再暂停”。持久 Work 控制命令、原命令回放、未知投递核对和 Host 待确认发送日志见第三段；尚未扩展为消息、插问等完整 AuthorCommand 体系。
2. P1：按 Run/对话隔离 Adapter、Bridge、环境、模型配置、恢复状态；取消全局执行单例限制，按资源控制并发；Renderer 按对话订阅。
3. P2：统一消息交付、运行中调整的确定回执、同 Work 临时插问及精确返回关系。
4. P3：Native 持久待发送队列、afterWork/afterTurn 条件、刷新和重启核对。现有 localStorage outbox 尚未替换。
5. P4：C01–C18 完整旅程、真实供应商取消/模型理解、跨平台及资源上限验收。当前只覆盖 C02/C03/C09 等场景的部分边界，不将局部测试视为整体验收。


## 第二段 PRE：独立控制修订

范围：Work 控制使用 Native 持久化 `controlRevision`，普通进度仍使用 `version`。不把本段宣称为持久命令幂等或并发会话完成。

| 视角 | 责任与验证 |
| --- | --- |
| 产品 | 进度刷新不使暂停按钮失效；已执行的作者控制使旧按钮失效。 |
| 领域 | Story Core 在原写事务中核对并递增控制修订；生成任务保持 Run 的独立写版本校验。 |
| 执行 | Host 传递 Native 修订，停止仍定位原 request/Work；无重试扩大权限。 |
| 界面 | Renderer 传递独立字段，回执验证控制修订；不从普通进度推断控制成功。 |
| 验证 | 真实 SQLite 进度/暂停/恢复/旧请求组合，HTTP 字段拒绝、Host 与实际界面回归，构建与架构门禁。 |

事实权威：Native Work 表；公开合同：Work DTO、Work control HTTP、Host IPC 与 Renderer 类型。控制 CAS 不放在 Host/UI，因为它们不能与领域写入原子提交。采用当前测试阶段零兼容合同：更新新库 schema，不为旧库添加迁移或字段回退，不操作作者实际数据库。

| 输入事实 | 判定 | 副作用与后续 |
| --- | --- | --- |
| 普通进度增加 version、控制修订未变 | 允许合法控制 | 原事务暂停/撤销原 Attempt，控制修订递增 |
| 先前作者控制已改变控制修订 | 拒绝旧请求 | 不改变任务、预算、输出 |
| 修订相同但 Work 已自然终结 | 仍拒绝非法状态转换 | 不重开 Work |
| 生成任务进度更新 | Work 控制修订不因进度递增 | Run 写入仍核对自身版本 |
| 生成任务暂停/恢复/取消（含原 Run 入口） | 控制修订递增 | 使旧 Work 控制请求失效 |


## 第二段实现与验收

已接通 Native `WorkTask.controlRevision` 与 Work control HTTP 的 `expectedControlRevision`。普通进度、步骤、费用等仍使用原 `version`；暂停、恢复、取消、核对保存结果在各自原事务中递增控制修订。生成任务的作者控制也递增所属 Work 的控制修订，包括直接 Run 入口；Run 自身 `expectedVersion` 保持独立。HTTP 拒绝旧 Work 控制字段，不提供字段回退。

Host 的停止、暂停菜单、恢复、保存结果核对、失败后暂停等入口同步传递控制修订。界面暂停菜单不再比较普通进度版本；Native 返回的控制修订必须递增才显示成功。用不同的普通版本与控制修订作为实际 App 夹具，验证不是两个字段碰巧相等才通过。

应用入口回归还复现了“恢复期间暂停成功，旧恢复请求的中断错误覆盖暂停提示”。控制器现在仅抑制已确认暂停对应的 `TASK_CANCELLED`；其他失败继续显示。Host 使用专用 executionRequestFailure 回执传递中断，preload 以可序列化拒绝记录跨 contextBridge 传递，Renderer 按明确 code 识别；不依赖 Electron 保留异常自定义字段，也不把中断冒充 Native 领域失败。同步上一段尚未更新的夹具：停止期待暂停；Native 旧 Attempt 保持 author_paused，迟到失败 PATCH 被拒绝；停止须携带原 request 身份。保存结果核对保留当前无模型/无新 Work 的合同。

第二段已取得的证据：

- Host 控制、生成配置、规划恢复、结构任务：75/75；启动失败恢复与保存结果恢复：21/21；Host/preload 序列化、真实 contextBridge 及 Native 错误边界：4/4；请求身份隔离：3/3。
- 实际 Electron App：2/2，覆盖跨页面控制、草稿保留、暂停期间恢复中断、进度变化、旧控制修订、错误项目/对话、错误回执及离线暂停。
- SQLite 原 Work 暂停/恢复/进度/旧控制：3/3；任务集成：11/11。
- 生成 Work HTTP：1/1，覆盖旧字段拒绝、控制修订跨读取持久化、进度不递增控制修订、旧控制拒绝、直接 Run 恢复与 Work 取消。
- 构建通过，架构门禁 339 文件/7 crates/0 diagnostics；保留既有 bundle 体积告警。

Native 相邻 HTTP 控制/撤权/待结算/恢复：1/1；保存结果恢复：15/15。这些检查保留未知写入与费用阻挡，不把暂停前已保存成果当作未发生。macOS 新测试二进制发生过 `_dyld_start` 启动停滞，使用只读采样确认后重启同一隔离用例；未修改作者数据库、未调用付费模型。

### 第二段 POST

| 视角 | 结论与边界 |
| --- | --- |
| 架构 | Native 唯一持久控制修订；无 Host/UI 第二份领域版本。 |
| 技术 A | 普通 Work 在原事务中 CAS；生成 Work 保留 Run CAS 和事务内控制修订。拒绝旧控制不会重开 Work。 |
| 技术 B | DTO、HTTP、IPC、UI 与直接调用方同步；无迁移、旧字段回退或双轨合同。 |
| 产品 | 进度变化不会使暂停按钮失效；确认暂停后的正常中断不会误报新失败。 |
| UI/UX | 真正错误仍可见，原目标、草稿、产物和费用保留；会话并发与临时插问仍未开放。 |

本段不提供 commandId 幂等账本、未知投递自动核对、完成当前步再暂停、持久队列或并发运行。重复旧控制目前明确拒绝；后续持久 AuthorCommand 才负责同一命令的回执重放。

聚焦回归入口：`npm run test:task-control`（包含真实 Electron 窗口与隔离 Native 测试）。


真实桥接缺陷证据：初版在 preload 抛出带 code 的 Error，隔离窗口收到的仅有 message，`code` 丢失；改为专用可序列化中断记录后，真实 Electron 用例与两项 App 用例全部通过。非中断错误继续拒绝，不以字符串内容识别停止。

第二段验收完成；完整产品方案仍为实施中。下一顺序为持久 AuthorCommand/回执核对与“完成当前步再暂停”，之后推进按对话隔离、同 Work 临时插问及持久消息队列。

## 第三段 PRE：持久 Work 控制命令

| 视角 | 责任与检查 |
| --- | --- |
| 架构 | Native 保存唯一命令与领域回执；Host 仅保存尚未确认的发送信封。当前 Runtime 是单作者授权角色，命令键为库内受信 local-author 主体与 commandId，不接受客户端自报作者。 |
| 技术 A | 命令、Work 控制、Attempt 撤权同一最外层写事务；生成控制的中间推进也在该事务内。嵌套领域写入使用 savepoint，失败不能留下半个控制。 |
| 技术 B | HTTP 命令带 commandId、threadId、expectedControlRevision；重放请求逐字段匹配；当前快照与历史应用回执分开。Host 先落盘后发送，只核对原命令。 |
| 产品 | 网络丢回执不再等价于操作失败；不会重复恢复或中断一个后来启动的执行。 |
| UI/UX | 原 Work 状态保持真实，未知投递明确显示待核对；持久核对不调用模型或清空草稿。 |

| 输入事实 | 判定与副作用 |
| --- | --- |
| 同作者同 commandId 同信封且已应用 | 返回原回执与当前快照；零重复控制 |
| 同 commandId 不同目标、动作、对话或修订 | 拒绝；零控制 |
| 旧控制修订、错误对话或非法当前状态 | 拒绝且不写领域状态 |
| 控制已提交但 HTTP 响应丢失 | 按原命令查询；不生成新命令替代 |
| 回执已应用但当前 Work 已有更新控制 | 展示当前状态，不按旧暂停回执中断新执行 |
| 重启只有未确认发送记录 | 只查询 Native；不由发送日志启动模型 |

验证使用隔离 SQLite、真实 HTTP、受控网络丢包及真实 Electron 入口，不改真实作者数据库。当前 schema 直接调整，无迁移和旧字段 fallback。


## 第三段实现：持久控制与未知回执

Native 新增 `story_work_control_commands_v1`，以受信单作者主体和 UUID commandId 记录原始信封及不可变回执。暂停、恢复、取消和保存结果核对均通过同一个 Work 控制 HTTP 合同；Work、生成 Run 中间推进、Attempt 撤权和回执同事务提交。失败回滚领域效果；明确拒绝保存 rejected 回执，存储失败不冒充拒绝。命令重放不执行控制回调，返回原历史回执及最新 Work 快照。

当前 HTTP 请求为 `{commandId, threadId, expectedControlRevision, command}`；成功响应为 `{receipt, replayed, work}`。领域拒绝返回 409 `work_control_rejected` 和完整 outcome；GET `work-tasks/:id/control-commands/:commandId` 返回 `{receipt}`，未知命令为 null。查询和写入均为 Author 路由，Candidate 不可调用。同 ID 更换信封或目标明确拒绝。直接领域 Run 控制仍使用它自身版本，属于现有领域接口；桌面 Work 控制不再分流成另一条 Run 暂停路径。

Host 新增待确认发送日志：先原子落盘并同步，再提交 Native；同信封重试沿用 commandId，重复点击合并。响应丢失只查询原命令；已知回执才删除日志。重启后的任务列表/详情读取只核对，不重发控制或启动模型。旧回执遇到更新控制不能中断后来的执行；重放的恢复回执不会再派发执行。该日志只保存未确认信封，不能决定 Work 状态、费用或权限。

Renderer 从 Host 投影显示“任务控制待核对”，提供“核对控制结果”。核对触发只读刷新，未知时保留提示；不把本地请求失败当作任务失败，不清除原目标或输入。明确拒绝按原因显示可读提示。当前没有自动定时重发承诺，未观察到的命令可沿原信封重试。

与本段相关的测试夹具同步当前合同：不再返回裸 Work 控制响应；真实 Native HTTP 夹具透传真实协议，模拟 Runtime 夹具显式构造并校验回执。恢复失败后的生成任务暂停也统一走持久 Work 控制。真实 Electron 测试新增重载后的待核对展示与只读核对场景。

### 第三段 POST

| 视角 | 结论与边界 |
| --- | --- |
| 架构 | Native 唯一命令、回执及领域状态权威；Host 文件仅为传输日志。当前为库内单作者主体，不声称多账号命令隔离已实现。 |
| 技术 A | 命令与效果事务化；重复命令零执行；失败与 panic 回滚；数据库重开保留回执。嵌套读取共用外层快照，生成控制使用 savepoint。 |
| 技术 B | HTTP、Host、测试夹具和 UI 类型已同步；回执核对目标/对话/动作/修订；无旧协议 fallback、迁移或真实资料重建。 |
| 产品 | 丢回执与拒绝可区分；恢复回放不重复启动；旧暂停不作用于后来的执行。完成当前步再暂停、并发对话、临时插问尚未交付。 |
| UI/UX | 原 Work 可见待核对提示，核对只读；保留草稿和原任务。不显示无证据的“已暂停”或“将自动重试”。 |

验证结果（第三段）：Host 聚焦回归 103/103；独立 SQLite 命令/暂停事务 6/6；实际 Electron App 1/1（包含新增待核对与只读核对）；构建通过，架构门禁 340 文件/7 crates/0 diagnostics。Native HTTP 回放覆盖原回执、更新后重放零控制、同 ID 改参数拒绝和 Candidate 查询拒绝；相邻恢复回归结果见下方分组记录。没有调用付费模型。

整体仍在实施中：下一段是 Native “完成当前步再暂停”屏障，之后为按对话运行隔离、同 Work 临时插问和 Native 消息队列。发送日志不替代这些能力。


迟到控制的相邻修正：桌面 Work 控制在 HTTP 发送前捕获原 flow/execution/task；回执晚到且同 Work 已换成本地新执行时，不再从全局变量选中并中断新执行。生产处理器回归直接模拟同 Work 的执行替换，断言新执行零取消、原回执提示核对。另补缺少 replayed 字段的恢复回执拒绝测试，避免把协议不完整当作首次执行许可。

Native 相邻回归已通过：Author/Candidate 与上下文边界 12/12；修订成果 43 项先通过，剩余旧暂停夹具改为验证 TaskNotWritable 与 author_paused 后单项复验通过；生成 Work HTTP 命令回放 1/1。规划评审旧夹具的拒绝断言同步当前更具体的 work_usage_unsettled，继续保留零派发、零授权、费用账本和 Work 完全不变断言。


规划评审 HTTP 6/6 已通过。预算回归的快照合同显式检查新命令账本：明确拒绝只能新增一条与 HTTP 回执逐字段一致的 rejected 记录，所有既有回执、Native 领域表及 Engine 费用表保持原样；鉴权或绑定前拒绝仍要求全库不变。未删除失败样本，也未把命令表整体从快照中忽略。


第三段最终验收：预算恢复组先通过 23 项，两个保存报告场景在上述夹具修正后单独复验 2/2 通过，合计覆盖该组全部 25 项；没有重复运行已通过且源码不受修正影响的用例。当前验证分组为 Host 103、Native 原子控制 6、上下文/权限 12、修订成果 43+1、生成控制 HTTP 1、规划评审 6、预算恢复 23+2，以及真实 Electron App 1。各分组只证明对应合同，不证明 P1–P4 已实现。构建及架构门禁通过；保留现有 bundle 体积和无关 dead-code 告警。

环境记录：部分 macOS 新测试二进制曾停留在 `_dyld_start`，采样后最终进入用例并完成；只使用隔离 SQLite、临时发送日志与受控模型夹具。未更改实际作者库，未创建提交或 PR。

## 第四段 PRE：运行中暂停入口的并发一致性

暂停屏障调研发现现有入口仍有两个 P0 问题，先修复这一可独立验收的控制链：菜单及点击初筛仍使用进度 version；App 把正在等待恢复执行结束误当作不可发送暂停。Native 控制修订和持久命令合同保持唯一权威，Renderer 仅负责正确选择和请求生命周期（B01/B03/B06/B08），不新增权限、账本或旧协议兼容。

| 视角 | 实施前结论 |
| --- | --- |
| 架构 | APPROVE：选择绑定使用项目、对话、Work、controlRevision、状态；普通进度不能改变作者控制身份。 |
| 技术 A | APPROVE：恢复请求和控制请求保持两个并发槽；暂停可打断恢复，但控制请求之间仍单飞；读取晚到后再次核对当前归属。 |
| 技术 B | APPROVE：真实 App 验证菜单打开后的列表刷新、控制修订变化、旧按钮及恢复执行中的暂停；保留现有停止入口回归。 |
| 产品 | APPROVE：作者在持续输出和恢复执行期间都能明确暂停原任务；重复点击不派发第二条命令。 |
| UI/UX | APPROVE：进度更新保持菜单可操作；控制或归属变化关闭旧菜单；仅确认回执显示暂停成功。 |

| 事实组合 | 行为 |
| --- | --- |
| 同 Work、同控制修订，仅普通进度更新 | 菜单继续可用，读取最新快照后发送原 Work 暂停 |
| 控制修订、状态、对话或项目变化 | 旧选择失效，零控制请求 |
| 恢复请求仍在等待执行结束，无控制请求 | 允许发送原任务暂停 |
| 暂停读取或控制已经在途 | 单飞，重复点击零额外请求 |
| 原暂停回执晚到 | 仅原请求有效作用域显示；不污染新对话 |

验证只用临时 Electron 页面和受控 API，不调用模型、不操作作者数据库。完成本步再暂停仍须独立 Native 屏障验收；本段不把普通暂停改名冒充该能力。

### 第四段 POST

实际修改：`ConversationTools.tsx` 选择指纹和回调校验统一绑定项目、对话、Work、controlRevision 与状态；`App.tsx` 的点击初筛同步控制修订。`useWorkTaskControls.ts` 显式暴露在途控制检查，App 暂停入口只等待已有控制，不等待长时间运行的恢复请求结束。原来的读取单飞、当前作用域检查、Native CAS、持久命令及回执校验全部保留。

| 视角 | 实施后结论 |
| --- | --- |
| 架构 | PASS：仅调整 Renderer 选择与请求生命周期；Native 仍决定暂停，未增加另一份任务状态。 |
| 技术 A | PASS：真实 App 在菜单打开后接收轮询进度，菜单 DOM 和键盘焦点保持，控制准确派发；控制修订改变后旧菜单失效。 |
| 技术 B | PASS：原恢复请求未完成时，菜单暂停成功且重复点击仅一次；原中断结果被正确归入已确认暂停；原停止按钮用例继续通过。 |
| 产品 | PASS：持续进度和正在恢复都不再妨碍暂停；离线、外来任务、迟到控制及刷新失败仍有对应回归。 |
| UI/UX | PASS：菜单稳定、焦点保留，状态变化才撤销选择；没有新增确认步骤或技术参数。 |

验证（2026-10-09，本段源码）：真实 Electron App 2/2，包含打开菜单后的进度刷新、控制修订失效及恢复中的菜单暂停；Host 控制/恢复合同 15/15；`npm run build` 通过；`architecture:check` 342 文件、7 crates、0 diagnostics。构建保留现有 bundle 大小告警。本段没有修改 Native，未重复运行上一段已通过的 Native 测试。所有 UI 用例使用临时目录和受控 API，不是付费模型或真实供应商端到端证据。

验证日志：`/tmp/control-menu-regression.log`、`/tmp/control-menu-contracts.log`、`/tmp/control-menu-build.log`、`/tmp/control-menu-architecture.log`。没有创建提交、修改真实作者数据库或增加兼容层。

### 第四段结束时核对的暂停屏障接入点（第五段已实施）

当前不能只在界面增加一个“完成当前步后暂停”按钮或把 Work 提前设为 paused：后者会立即撤销正在收尾的写权限。Native 屏障实施必须同时满足以下接入点，完成相应用例后才能暴露入口。

- `task_attempts::attach_attempt` 是当前统一 Attempt 挂接点：新领取、替代尝试与自动重试需要在此拒绝，且同事务回滚任务创建、授权和预算预留。
- `work_tasks::finish_attempt` 是普通 Attempt 收尾点：冻结集合全部终态后收束暂停；失败、未知写入与未知用量保留原事实，不能被包装成成功完成。
- `refresh_work_task` 对 generation 有单独分支；只更新普通 Work 无法覆盖生成任务。生成 Run 的 `state_json`、version 与 Work 状态必须原子一致。
- `authoring_runs::update_generation_run_with_guard` 会投影 Run 状态到 Work；必须防止晚到进度覆盖已经生效的屏障，同时允许原请求的用量核对。
- `work_decision_generation`、`work_decision_actions`、`task_actions` 与执行事件领取均会安排后续步骤：既要禁止新 Attempt，也要阻止屏障期间创建或领取新行动。不能只靠 Host 内存退出循环。
- 生成恢复依赖原 `activeTaskId`、完成回执和计费记录；屏障不能清空这些身份来实现假恢复。必须覆盖完成、失败、保存待核对、数据库重开、立即停止覆盖待暂停等场景。

第四段结束时，暂停屏障仍待实施；第五段完成情况见下文。完整目标中的对话执行隔离、临时插问与返回、Native 持久消息队列及跨平台完整旅程仍待实施。

## 第五段 PRE：完成当前步后暂停

责任：Native 持久化暂停屏障及冻结 Attempt 集合，统一禁止后继调度；Host 传送控制并保持当前执行，Renderer 投影状态。公开 Work 控制新增 `pauseAfterStep`，Work 投影新增 `pauseAfterStep`（控制修订、未结束尝试数）；沿用持久命令、CAS 和回执。无需新增作者确认。

| 视角 | 实施前评审 |
| --- | --- |
| 架构 | APPROVE：屏障、Run/Work 状态与权限由 Native 单一事务维护，Host 不决定收尾完成。 |
| 技术 A | APPROVE WITH CONDITIONS：冻结身份不可扩大；新 Step、Attempt 和事件领取均需阻断；立即停止/取消/显式恢复清除屏障。 |
| 技术 B | APPROVE WITH CONDITIONS：验证空集合、正常收尾、失败、重开、重复命令、原身份及生成 Run 同步；同步 HTTP、Host、UI 类型和夹具。 |
| 产品 | APPROVE：当前获准步骤可保存，后继不会自动启动；无法完成时保留失败事实与产物，不承诺一定成功。 |
| UI/UX | APPROVE：等待收尾明确可见，仍可立即暂停；不显示无证据的“已完成”。 |

| 事实 | 行为 |
| --- | --- |
| running 且存在活动 Attempt | 冻结集合，Work 保持 running；零新领取 |
| running 且集合为空 | 当次事务直接 paused |
| 冻结集合全部结束 | Work/生成 Run 收束 paused；保留原失败、产物和用量 |
| 旧控制修订或已经终态 | 拒绝，零新增授权 |
| 显式立即暂停/取消 | 清除屏障并沿原撤权流程停止 |
| 显式恢复 | 核对原事实，清除屏障后按既有恢复准入继续 |

当前 schema 直接建设，不增加历史兼容或重建真实数据库；测试仅用隔离数据。


### 第五段 POST

实际变更：Native 新增持久暂停屏障，冻结命令生效时已获准的 Attempt 集合；屏障与控制命令回执、控制修订在同一事务登记。当前集合继续使用原授权收尾，后继 Step、Attempt、执行事件领取及生成交接被阻断；集合全部终态后，Work 与生成 Run 同步暂停。立即暂停、取消和经过准入检查的显式恢复清除屏障，重复命令沿用原持久账本。

Host 新控制只登记屏障，不中断当前请求；Renderer 菜单提供“本步结束后暂停”，任务卡显示等待收尾并保留立即暂停入口。恢复请求仍在途时也能登记屏障；自然收束的暂停不再误报为执行失败。模型自行暂停不能覆盖作者登记的屏障。

失败与费用分别保留：原 Attempt 失败但服务商请求已完成并计费时，显式恢复核对原用量、未确认写入、来源和额度后创建后续尝试；不会要求篡改计费状态、清零费用或复活原失败 Attempt。用量未结算或写入待核对时仍拒绝恢复。

| 视角 | 实施后结论 |
| --- | --- |
| 架构 | PASS：Native 是屏障、授权及 Work/Run 的唯一权威；Host 仅传送控制，Renderer 投影状态。 |
| 技术 A | PASS：覆盖多 Attempt 冻结、空集合、正常/失败收尾、重开、幂等重放、拒绝新领取及立即暂停覆盖；原产物和账本保留。 |
| 技术 B | PASS：HTTP、Host、Renderer 与类型同步；受控 Electron App 验证恢复期间登记及自然暂停，不以 mock 状态替代 Native 事务验证。 |
| 产品 | PASS：可选择立即暂停或等当前已获准步骤结束；失败仍保留为失败，显式恢复经过原准入。 |
| UI/UX | PASS：等待收尾与已暂停明确区分，移除重复登记入口，保留立即暂停，无新增确认步骤。 |

验证（2026-10-09，本段源码）：Native 核心单元测试 280/280、暂停控制专项 12/12；Host 控制与配置合同 48/48；真实 Electron App 两项通过，随后针对自然收束补充的 App 用例单独重跑通过；`npm run build` 通过（保留现有 bundle 大小告警）；`architecture:check` 扫描 344 文件、7 crates，0 diagnostics。运行时生成流程整组 14/14，包含持续三章创作、失败计费后显式恢复与原 Work 控制修订。

扩展回归同步了当前合同的测试夹具：未结算预算断言改为 `WorkUsageUnsettled`，分页夹具初始化真实预算账本，两份共享计费回执夹具补齐空的失败观察和用量核对数组；未为通过测试放宽生产检查。原失败记录保留在 `/tmp/pause-after-core-lib.log`、`/tmp/pause-after-runs-authorized.log`、`/tmp/pause-after-runs-final.log`。

通过证据：`/tmp/pause-after-runs-accepted.log`、`/tmp/pause-after-core-lib-final.log`、`/tmp/pause-after-controls-final.log`、`/tmp/pause-after-host.log`、`/tmp/pause-after-ui.log`、`/tmp/pause-after-ui-natural.log`、`/tmp/pause-after-build-final.log`、`/tmp/pause-after-architecture-final.log`。全部验收使用临时数据库、受控 API 或本机测试服务，没有调用付费模型或修改真实作者资料。零兼容门禁 PASS；没有增加历史数据适配或重建真实数据库。

剩余范围：P0 持久消息合同及 P1–P4 仍需继续实施；本段不代表完整 Cursor/Codex 产品体验已经交付。真实供应商长时间运行、进程崩溃后的完整 UI 恢复旅程及 Windows 验收尚未进行。

## 第六段 PRE：持久消息与队列

Native 保存已提交输入、修订、领取与交付身份；Host 只执行一次领取并记录实际派发，Renderer 保存草稿及读取投影。消息的接收不等于模型已收到；领取后崩溃保留未知，不自动重发。队列默认绑定整个 Work 正常完成，显式 afterTurn 只绑定精确 Turn。Host 重启后的未派发项须作者继续。

| 视角 | 前审结论 |
| --- | --- |
| 架构 | APPROVE：Native 单一消息队列，移除 Renderer 自动派发；复用原任务执行及预算合同。 |
| 技术 A | APPROVE WITH CONDITIONS：接收幂等、编辑/取消与领取 CAS、同线程单派发，派发未知不可重试。 |
| 技术 B | APPROVE WITH CONDITIONS：同步 Runtime/Host/preload/类型/UI，真实 SQLite 与受控 Host 验证崩溃窗口和归属。 |
| 产品 | APPROVE：持久接收后清除原草稿版本；停止、失败与重启保留队列且明确待处理原因。 |
| UI/UX | APPROVE：显示排队条件、交付未知和可用操作；切换不丢失已提交消息。 |

当前合同直接替换前端已提交队列，不导入旧 localStorage 队列、不增加兼容层；测试仅使用隔离数据。后续执行隔离与临时返回各自实施前补充前审。

## 第七段 PRE：按对话隔离执行

Host 引入执行域注册表，每个根对话执行持有独立 adapter、Bridge、恢复 journal、取消状态与冻结配置；Core SQLite 目录共用，运行配置和进程能力隔离。入口显式绑定域，事件回调捕获原域，控制由项目/对话/Work 查询原域。为避免在现有百余处调用链手工转传并漏传，使用 Node AsyncLocalStorage 传递已经绑定的执行域；它不读取当前选中对话，也不决定任何领域准入。默认域只用于配置和只读管理，不能用作未绑定执行的兜底。

| 视角 | 前审结论 |
| --- | --- |
| 架构 | APPROVE WITH CONDITIONS：注册表只拥有进程句柄，Native Work/消息仍拥有事实与领取权；按身份入口建立上下文。 |
| 技术 A | APPROVE WITH CONDITIONS：相同线程单执行；Native 租约还需拒绝同根线程另一 Work；旧域回调不能污染新域。 |
| 技术 B | APPROVE WITH CONDITIONS：完整覆盖配置、恢复、媒体执行、后台唤醒、销毁和控制 IPC；受控 A/B 并行和停止测试。 |
| 产品 | APPROVE：切换与元数据管理解锁，A 停止只影响 A，B 配置变更不改 A 的冻结配置。 |
| UI/UX | APPROVE WITH CONDITIONS：按对话保存执行、请求和问题；后台事件只更新原对话，当前草稿保持。 |

实例执行结束即关闭进程，历史目录与真实费用保留；不为每条消息保留空闲常驻进程。并发资源测量、真实模型及跨平台验收仍在 P4，不以本段单元测试宣称已完成。

### 第六段当前验收记录

已接通 Native 消息接收/幂等、afterWork/afterTurn 触发合同、修订与领取 CAS、任务创建同事务绑定、派发前记录、已交付核对和明确未派发的显式重评；Host 在原事件消费者中优先检查作者队列。Renderer 已移除 localStorage 已提交队列和自动发送，保留未确认提交的传输编号及草稿，收到持久回执才清除原草稿版本。界面提供继续等待、核对交付、排序和移除。

验证：Native 仓储 6/6（`/tmp/messages-native-accepted.log`），Runtime HTTP 作者权限与消息回执 1/1（`/tmp/messages-http.log`），Host 消息/执行请求/后台事件 30/30（`/tmp/messages-host.log`），真实 Electron App 队列旅程 1/1（`/tmp/messages-ui.log`）；构建和架构检查通过。仅受控测试，不含真实模型。现有 App 测试夹具同步新增消息列表 API。

五视角阶段后审：架构 PASS（Native 单一已提交队列）；技术 A PASS（创建和绑定原子、未知不重发）；技术 B 部分通过（当前消息切片合同与 UI 已验，跨对话隔离接续第七段）；产品/UI PASS（真实等待、失败保留、重启后明确继续）。消息合同的剩余收敛包括 steer 与 Work 意图关系、媒体提交及完整 P2/P3 恢复旅程；本记录不把整个 P0 标记完成。

第七段 Renderer 细化前审：每个对话保存运行投影、原请求、执行管线和作者问题；Host 事件附受信执行域身份，后台事件按原域更新投影。切换只更换显示投影，异步发送收尾按原 scope 清理，不能清理另一对话。模型默认变更不改活动运行展示。五视角同意沿用本段边界，条件为 A/B 并行、问题隔离、后台完成及草稿验证通过后才后审。

第七段交互复核补充：后台输出自动滚动会触发全局 scroll，原菜单将其视为锚点移动而关闭，导致运行结束附近的操作无法选择。菜单改为仅在真实锚点位置改变时关闭；菜单内部滚动与不移动锚点的内容更新均保留操作。此为原身份/控制选择稳定性的修复，不改变动作准入。

## 第八段 PRE：精确 Run 补充入口

Engine 公开 `steerRun`，核对 Native Run 快照、Session、租约代次和预期 Turn；Runtime 再检查当前已派发且未撤销的执行。产品 Host 不再直接越过 Engine 向 adapter 追加输入。作者来源和权限仍由 Native 原意图合同管理，未知交付继续拒绝自动重发。

| 视角 | 前审结论 |
| --- | --- |
| 架构 | APPROVE：Engine 仅提供通用定向输入；不解释作者创作意图。 |
| 技术 A | APPROVE WITH CONDITIONS：旧 Run/Session/lease/epoch/Turn 必须拒绝，无等待间隙自动选择新 Turn。 |
| 技术 B | APPROVE WITH CONDITIONS：生产 App Server adapter 与 Engine Runtime 同步，运行中撤销及晚回执验证。 |
| 产品 | APPROVE：补充只能交付到作者操作时的原回合，失败保留原输入与核对信息。 |
| UI/UX | APPROVE：继续沿用原对话补充反馈，不把补充失败显示为成功或新任务。 |

### 第六、七段补充验收：消息回执、编辑与执行恢复

本轮补齐以下实际链路：

- `afterTurn` 同样携带并核对原 Work 控制修订；作者控制变化后显示明确阻挡。“继续等待”仅重新确认等待条件，不恢复原 Work、不替换原 Turn。
- 公开只读 `readMessage(projectId, threadId, messageId)`，用于接收回执丢失而消息已经交付的恢复。Renderer 按原编号核对，移除本地未确认提交标记，零模型重放；查询失败保留原未确认记录。
- 队列支持编辑；编辑期间修订变化时保留草稿、阻止覆盖。每个对话单独限制消息操作在途数量，A 的请求不阻塞 B。
- 每个 Host 执行域持有独立进程、Bridge、冻结配置、问题与取消句柄。多字段查找必须全部一致，不能因其中一个字段匹配就误选其他执行；新请求编号不能绕过同线程互斥。
- Renderer 按原项目/对话路由后台事件和问题。只读执行快照恢复活动 Turn、问题及停止入口；晚到的快照不能重新激活已结束请求。恢复不重新派发，草稿仍按原对话保留。
- 对话标题、置顶及作品切换不再被无关执行全局锁定。Native 拒绝归档仍有运行 Work 或待交付消息的对话；同一根对话的两个 Work 不能取得并发执行所有权。

前审补充：以上仍分别由 Native 拥有消息/领取/控制事实、Host 拥有执行句柄、Renderer 拥有草稿和投影；五视角沿用第六、七段准入，无新增领域权限。编辑冲突和回执恢复只核对原身份，不创建新的作者要求或费用账户。

| 视角 | 本轮后审 |
| --- | --- |
| 架构 | PASS：消息权威仍在 Native，执行域仅管理资源；只读快照与回执查询不派发模型。 |
| 技术 A | PASS（本轮范围）：原身份、多个选择条件冲突、同线程互斥、过期控制、编辑修订和晚回执均保留拒绝边界。 |
| 技术 B | PASS（下列验证范围）：生产 Host 入口、Engine、Native 消息事务、真实 Electron App 旅程均有验证；完整 Host 崩溃和跨平台矩阵仍待验。 |
| 产品 | PASS（本轮范围）：A/B 可独立执行；停止 A 不影响 B；接收与模型交付有区别；重载不重放。 |
| UI/UX | PASS（本轮范围）：运行状态与停止入口可恢复，原草稿不串对话，队列编辑冲突保留输入，菜单不因无关输出滚动而消失。 |

验证证据（2026-10-09）：

| 验证 | 结果与日志 |
| --- | --- |
| Native 消息及控制等待 | 8/8，`/tmp/messages-native29.log`；修正单条读取/修改回执遗漏等待投影后通过，前次失败保留于 `/tmp/messages-native23.log` |
| Host 配置、对话、消息、执行域 | 59/59，`/tmp/domains-host24.log`；追加精确多字段身份测试后 5/5，`/tmp/domains-exact25.log` |
| 精确控制、启动前停止、持久控制命令 | 20/20，`/tmp/control-regression28.log` |
| 媒体执行隔离相关服务 | 9/9，`/tmp/domains-media21.log`；使用本机受控服务 |
| Engine Host / Runtime / App Server | 45/45，`/tmp/engine-regression28.log` |
| App 多对话及暂停控制 | 2/2，`/tmp/domains-ui25.log`；包括活动执行期间重载、原草稿恢复、B 并行完成、A 后台完成 |
| App 队列编辑、版本冲突与丢失回执 | 1/1，`/tmp/queue-ui30.log`；零额外发送 |
| Host 只读消息回执 | 6/6，`/tmp/messages-host26.log` |
| 构建 | `npm run build` 通过，`/tmp/messages-build30.log`；保留现有 bundle 大小告警 |
| 架构门禁 | 348 文件、7 crates、0 diagnostics，`/tmp/domains-architecture29.log` |

测试夹具已同步当前契约：原来钩住全局 adapter 的断言改为观察实际执行域；元数据写入不再要求全局空闲；暂停结束依据实际状态而非作品切换按钮。未放宽生产准入，不增加旧接口兼容。

### 第八段 POST：精确 Run 补充入口

Engine `steerRun` 贯通 Run、Session、thread、Turn、leaseId、epoch 核对；Native Session Runtime 要求当前代次已经派发且未撤销。产品 Host 捕获原执行身份后才登记作者补充并调用 Engine，adapter 不再用当时“最新 Turn”替代请求所指 Turn。错误 Session、线程、Turn、租约、代次和结束后的 Run 均拒绝；冻结审阅继续拒绝修改输入。

五视角后审：架构 PASS（通用 Engine 定向输入，Native 作者来源）；技术 A/B PASS（真实 adapter/Runtime 合同及 45 项回归）；产品/UI PASS（补充失败不冒充成功、不投递到另一回合）。此后审只覆盖精确传输入口，**不代表 TaskIntent 与持久消息的完整统一已完成**。

### 整体退出状态（不得作为“全部完成”）

P0/P1/P3 的本轮链路已实现并通过上述范围验证，但设计中的完整退出条件尚未全部满足。仍需完成：

1. TaskIntent 与统一消息交付的收敛；纯提问和实质要求变更的模型解释/领域准入；精确 Turn 已结束时同一消息的确定未交付接续。
2. P2 同 Work 插问及父链返回已在第九段实现；后续仍需完整真实供应商旅程验证。
3. 第十/十一段补齐资源准入、队列公平轮转与媒体持久提交；完整 Host 崩溃窗口、快照游标竞态、同作品对象级编辑保护仍需验收。
4. C01–C18 全矩阵、真实供应商长时间旅程、资源测量及 Windows 验收。本轮没有调用付费模型，也没有执行 Windows 验收。

以上是实际未完成项，不以已有测试数、设计 APPROVE 或同层局部 PASS 替代完整交付。

## 第九段 PRE：同 Work 临时插问与返回

Native 持久化 interlude 关系、原消息、显式只读 Step、父层、控制版本与自动返回授权。当前已准入步骤安全收尾，关系立即冻结普通后继；没有结算或存在未知写入时不派发插问。Host 仅按 Native 准入领取原 Step，沿原 Work 预算执行；Renderer 投影暂挂、回答和返回状态。开始与返回使用稳定命令身份及 CAS；作者停止/取消优先并使自动返回失效。

| 视角 | 前审 |
| --- | --- |
| 架构 | APPROVE WITH CONDITIONS：Native 是关系、来源、授权和预算唯一权威；不以 Renderer mode 扩权。 |
| 技术 A | APPROVE WITH CONDITIONS：所有普通调度入口检查专属阻挡，既有步骤收尾不授予新步骤；精确顶层、父链、控制修订及来源核对。 |
| 技术 B | APPROVE WITH CONDITIONS：事务/重放/未知写入和费用/同预算/嵌套/停止竞争需要 Native 合同测试，公开接口至 App 同步。 |
| 产品 | APPROVE：普通插问默认手动返回；“问完继续”单独记录，失败与费用保留。 |
| UI/UX | APPROVE：区分“等待当前步骤结束”“回答中”“等待返回”和作者暂停；不承诺未知执行成功。 |

自然步骤边界是本段明确的安全让出方式；不会为追求即时切换丢弃在途写入或费用。真实供应商与 Windows 的验证边界单独记录，不冒充本机受控测试。

第九段合同复核：真实 Host/Native 集成发现，生成 Work 的未完成交付义务被错误应用到只读临时问题，导致回答完成被拒为 `TaskOutputMissing`。按 B10 调整完成主体：持久 interlude Attempt 只完成本问题，不提交原 Work 交付审阅，也不清除原义务；原 Work 返回后继续适用全部交付条件。五视角同意此主体区分，条件为普通/生成两条真实 Runtime 路径均验证、原目标与预算保持、插问不能修改原 Work 计划或创建写动作。

第六/九段恢复复核：消息派发领取属于一次 Attempt，不能成为可恢复 Step 的冻结目标。新建根 Step 只保存领域目标，Task 仍保留完整领取身份；恢复比较排除派发元数据，消息与原 Attempt 的持久绑定不改变。否则原消息已交付后重试 Step 会误尝试再次领取同一消息。该修复不建立新作者要求或新预算。

## 第十段 PRE：后台准入与资源边界

Host 的执行资源上限与 Native 的领域准入分开：每个 Host 最多保留四个活动执行域，准备与清理阶段也占用名额；达到上限的持久消息保持待发送，直接发送保留草稿并返回可读原因。未保存草稿只阻挡所属作品的后台写执行；其他作品及明确只读问题可继续。事件消费者在领取前及实际恢复前复查相关作品的编辑状态，不把全局 UI 草稿状态当作所有作品的领域事实。

五视角前审：架构 APPROVE（Host 资源/编辑生命周期，Native 身份/写入最终准入不变）；技术 A/B APPROVE WITH CONDITIONS（并发准备、失败清理、容量回收、相关编辑复查）；产品/UI APPROVE WITH CONDITIONS（满额不丢输入、不暗中扩额度；只读问题可解释未保存选区）。同一作品内更细的对象级编辑保护和后台公平派发继续单独验证，不用本段局部变更宣称完成全部 P3/P4。

## 第十一段 PRE：媒体提交进入持久消息合同

媒体消息只保存版本、选区及不可变图片素材引用，不保存图片字节或凭据。Native 核对作品/版本/对话范围和素材归属；创建 Attempt 时核对同一消息的媒体引用，接收不授予创作权限。Host 保留现有媒体预检与执行适配器，复用原消息领取、派发、交付和失败核对，不创建另一套媒体队列。Renderer 在收到原消息持久接收回执后才确认原草稿版本，并保留重开后的只读核对与显式继续入口。

五视角前审：架构 APPROVE（Native 统一消息权威，媒体素材仍由原素材库管理）；技术 A/B APPROVE WITH CONDITIONS（图片独立消息、错误范围/素材、版本变化、绑定不一致、停止与确认丢失）；产品/UI APPROVE WITH CONDITIONS（不重复生成，不因持久接收误显示模型回答完成，未确认输入可恢复）。媒体暂挂返回与自由文本意图解释不由此提交合同自动授予。

第六/十一段核对语义复核：确定未派发后的“核对交付”只消除未知，不授予自动重发。Native 持久化待确认标记，恢复 queued 时必须再由作者选择“继续等待”；Host 和 Native 领取都检查此标记。旧领取修订继续失效。这补齐当前进程内核对后的窗口，避免只有重启后才要求明确继续。


### 第九段 POST：同 Work 插问与返回

已实现 Native 原 Work 只读 Step/Attempt、持久父链、默认手动返回、显式自动返回、原预算计量及停止整链失效。Host 在自然步骤边界派发，原步骤未结算时保持等待；普通后继被 Native 阻挡。返回只解除对应顶层关系，不能重新授权被暂停的 Work。自动返回失败保留已经交付的回答并显示可手动核对状态。

五视角后审：架构 PASS（关系、权限和预算在 Native）；技术 A/B PASS（普通/生成两条真实 Host+Runtime 2/2，`/tmp/interlude-native-host17.log`；重开及旧请求不复活 5/5，`/tmp/interlude-reopen19.log`）；产品/UI PASS（实际 App 插问旅程，`/tmp/interlude-app-ui12.log`）。只读问答不承担原生成 Work 的交付义务，原义务保持原状。最大嵌套 8 层保护有界父链；达到时先返回已有问题，不自动创建新 Work 绕开限制。并未把此通过范围写成全产品验收完成。

### 第十段 POST：资源边界与队列轮转

执行域准备、执行和清理都占用同一资源槽；失败完成清理后回收。相关作品的未保存编辑在领取前及恢复前复查；明确只读插问允许继续。消息消费者按作品和对话轮转，持续输入不会固定占据首项。验证 `/tmp/interlude-scheduler15.log` 36/36，涵盖容量、清理、领取后编辑变化及两级轮转。

五视角后审：架构/技术 A/技术 B/产品/UI PASS，范围限上述 Host 生命周期与公平选择。四个执行域是本机进程资源上限，不是作者请求、步骤或预算额度；满额消息保留待发送。后台消费者目前按完整执行串行消费，不能据此声称四路后台队列并行或已测得最优容量；同作品对象级编辑隔离仍需后续实施。

## 第十二段 PRE：追加指令的统一消息投影

TaskIntent 与原作者消息仍由 Native 现有来源账本管理，不复制作者要求。会话消息的列表、单条读取和只读核对增加该账本投影，未确认交付的追加消息可从原对话找回；不能进入普通待发送消费者或通过“继续等待”重复 steer。Host 发布原作者消息身份的接收/交付通知，仍严格定位原 Run/Turn。此段解决可查询的交付状态，不把未知投递误判为确定未投递，也不更改模型的要求解释合同。

| 输入事实 | 判定与副作用 | 下一步 |
| --- | --- | --- |
| Native 已登记，Engine 回执未知 | 待核对；保留原要求，不重发 | 只读原 TaskIntent 回执 |
| 原 TaskIntent 已交付 | 返回原交付事实；零新指令 | 正常读取原回答 |
| 别的对话请求同一消息 | 拒绝；零副作用 | 回到原对话 |
| 对追加消息请求编辑/移动/继续/取消 | 拒绝；不撤销已接收要求 | 原 Work 控制仍是唯一停止入口 |

五视角前审：架构 APPROVE（统一查询合同，原账本唯一权威）；技术 A/B APPROVE WITH CONDITIONS（来源核对、未知不重派、跨对话拒绝、交付后的读取）；产品/UI APPROVE WITH CONDITIONS（明确标记追加而非普通待发，不显示不可执行的队列操作）。精确 Turn 确定未收到时的接续与自由文本要求解释仍另行实现。


### 第十一段 POST：媒体消息的持久接收和恢复

媒体文字、版本、选区及图片引用已进入 Native 原消息合同；图片独立消息保持空作者文本。素材归属、版本和 Task 冻结目标逐项核对。界面按持久接收回执确认原草稿，较新输入不被清除；重开查询原消息及活动执行快照，不重新发送。后台队列开始/结束、真实 Host 执行结束和停止均按原请求绑定；纯图片活动消息也能恢复预览。

确定未发送的领取核对回 queued 后带持久 `requiresContinue`；同一 Host 与重启 Host 都不能绕过明确继续，旧领取仍被修订号拒绝。没有将未知模型投递当作确定未发送。

五视角后审：架构 PASS（媒体引用在原素材库、消息在 Native、生命周期在 Host）；技术 A/B PASS（Native 消息 8/8、媒体 14/14、插问 5/5，`/tmp/media-native26.log`；真实 Host+Runtime 媒体绑定 1/1，`/tmp/media-native-host30.log`；Host 41/41，`/tmp/media-host31.log`）；产品/UI PASS（实际媒体组件覆盖接收后清草稿、队列恢复、精确停止、外来事件拒绝、重开纯图片执行，`/tmp/media-ui36.log`）。构建 `/tmp/media-finalbuild36.log` 通过，架构 350 文件/7 crates/0 diagnostics，`/tmp/media-finalarchitecture36.log`。既有 bundle 大小和未使用字段告警未伪装成新增失败或已解决问题。

验证边界：真实 Native 集成为文字媒体轮次，图片归属与空文本由 Native 实际领域测试证明，图片预览和恢复由实际 Renderer 受控夹具证明；尚未调用真实识图供应商。媒体界面并发多个活动对话的完整体验和快照游标竞争仍须进一步收敛，不能以单一活动恢复代替完整验收。

## 第十三段 PRE：接入持续验证

责任模块为产品构建与 CI；只编排已有真实测试，不更改生产准入。增加可独立运行的任务控制测试脚本，并接入 `verify` 和现有 Linux 产品质量任务；包含 Host 生命周期、消息队列、真实 Native 集成及实际 Electron 组件。Native Rust 回归仍走现有 `test:runtime` 全 workspace 测试。五视角前审均 APPROVE：避免只能手动运行的新测试，无供应商调用、无凭据、无兼容路径；远端 CI 尚未运行必须明确标记。

### 第十二段 POST：追加指令回执可查询

Native 由原 TaskIntent/AuthorMessage 账本生成会话消息投影，未复制作者要求或另建派发队列。原对话可读取 pending 与 delivered；列表保留未确认追加，核对只读原记录，队列 claim/continue/delivered 等变更全部拒绝。跨对话身份和与普通提交的消息编号冲突拒绝。Host 通知使用原作者消息编号；Renderer 对追加消息仅显示核对入口。

五视角后审均 PASS，范围限统一查询与禁止重复派发：Native 消息 9/9 与作者要求恢复 3/3（`/tmp/intent-native33.log`），真实 HTTP 作者/模型权限及回执 1/1（`/tmp/intent-http34.log`），Host 54/54（`/tmp/intent-host33.log`），实际 App 队列/追加投影 1/1（`/tmp/intent-ui35.log`）。TaskIntent 的自由文本生效解释及确定未交付接续仍不在此段完成范围内；接收重放的修订身份合同见第十五段。

### 第十三段 POST：统一门禁

新增 `npm run test:task-controls`，接入本地 `verify` 和 Linux CI 的 Xvfb 环境。当前已构建 Engine/Runtime 上执行 `npm run --ignore-scripts test:task-controls`，66/66 通过（`/tmp/task-control-gate37.log`），覆盖真实 Host/Runtime 与 Electron。构建及架构证据见第十一段；远端 CI 和 Windows 尚未运行，不把脚本接入视为远端绿色。

五视角后审均 PASS：生产职责不变、原测试真实执行、没有削弱失败断言或使用付费模型。此次门禁涉及的 16 个源码/配置文件指纹集合 SHA-256：`17c34c823201907a05a0708de45abe5ee0ea4de42722d0a11edf5f4dce1ca103`，逐文件清单在 `/tmp/task-control-source37.json`。该指纹只证明本次本地验证来源，不是提交号或发布身份。

## 第十四段 PRE：媒体对话独立执行投影

Host 已按原请求隔离执行；Renderer 将媒体单一活动状态改为按请求保存、按当前对话选择的执行投影。切换对话只改变视图，新的发送在另一对话建立独立 UI 归属。进度、停止、图片和提问按原 request/thread/task 核对，晚到结果不能清除另一对话草稿或释放另一执行。未保存作品编辑仍走现有 Native/CAS 与 Host 准入。

五视角前审：架构 APPROVE（只保存可重建 UI 投影，不新增领域账本）；技术 A/B APPROVE WITH CONDITIONS（双对话并发、准备/结束竞争、快照恢复及来源核对）；产品/UI APPROVE WITH CONDITIONS（A 运行时 B 可发送，停止当前对话只作用于原请求，后台完成不抢焦点）。同一作品正式保存冲突继续由 Native 拒绝，不用多对话 UI 绕过版本校验。

### 第十四段 POST：媒体多对话执行隔离

媒体执行投影提取到 `useMediaExecutions`，每次发送拥有独立记录；后台消息、图片、提问、停止和完成不再读当前界面对应的单一引用。准备阶段与原请求控制保持绑定，A 完成不释放 B。共享导航器接收完整运行对话集合，运行中仍可新建独立对话。

复验发现并修复两处实际竞争：切回 A 与 A 完成同时发生时历史没有刷新（`/tmp/media-parallel-ui39.log`）；同一时机已发送草稿被旧状态重新显示（`/tmp/media-parallel-ui41.log`）。现改为完成后刷新当前历史，草稿确认用最新状态比较并保留新输入；图片从运行中展示移交持久历史时保持单份可读。

五视角后审均 PASS，覆盖 A/B 并行发送、相同编号问题隔离、A 完成后 B 继续、切回 B 精确停止、新对话入口、独立草稿及纯图片恢复。实际媒体和小说 App 三组 UI 3/3（`/tmp/media-parallel-ui44.log`）；构建通过（`/tmp/media-parallel-build44.log`），架构 351 文件/7 crates/0 diagnostics（`/tmp/media-parallel-architecture44.log`）。Host/Native 沿用第十三段已验证合同，没有据此声称跨供应商或 Windows 完整验收。

## 第十五段 PRE：作者追加要求的幂等回执

Native 已有 `(taskId, expectedIntentRevision + 1)` 是追加要求的明确并发身份。相同身份和相同原文重放返回原记录；身份相同但原文不同拒绝。回执查询先于当前任务可写性检查，允许已停止后读取原接收结果，但不重新生效、不重复失效旧动作、不创建新的作者消息。Host 仍不得把未知传输回执当成可以再次注入模型。

五视角前审：架构 APPROVE（复用现有作者账本和修订身份）；技术 A/B APPROVE WITH CONDITIONS（重放、不同正文、任务停止及修订递进）；产品/UI APPROVE（接收回执可重放，不把作者要求接收误称为模型已收到）。不新增兼容入口，不更改现有字段含义或授权范围。

## 当前验收矩阵（2026-10-10）

本表区分已验证边界和完整退出条件，不把局部 PASS 合并为全产品完成。

| 场景 | 本地已验证边界 | 剩余验收或实现 |
| --- | --- | --- |
| C01 | 消息幂等、版本冲突、重复提交、新草稿保护、追加接收重放及 Electron 组合键/输入法事件保护 | 各系统真实输入法手工矩阵 |
| C02 | 准备阶段停止、晚到创建屏障、精确请求停止 | 全部真实供应商连接/Turn 建立窗口 |
| C03 | 原 Work 暂停、停止先后关系与后继派发屏障 | 全链路保存/完成竞争矩阵 |
| C04 | 精确 Run/Turn steer、未知不重发、可查询追加回执；发送前拒绝证据、原子暂停、同 Work/对话/预算显式接续 | 模型交付与要求生效进一步拆分、真实供应商语义旅程 |
| C05 | 普通/生成/媒体 Work 同预算只读插问、嵌套、手动及授权自动返回；媒体冻结范围与写入拒绝 | 自由文本插问意图解释、真实供应商完整旅程 |
| C06 | 插问中暂停、整链失效、重开旧命令不复活 | 与完整 Host 崩溃恢复组合验收 |
| C07 | Host A/B 隔离；小说及媒体界面 A/B 独立发送、切换、停止 | 实际供应商并发与模型切换长程旅程 |
| C08 | 显式只读插问、未保存引用只读、原权限不扩张 | 纯提问和实质要求变更的模型解释合同 |
| C09 | 原预算继承、未知消息不重派、费用身份保持；首次账本未创建但已撤权的未派发消息可结算，发送后丢账本拒绝 | 真实供应商未知费用与各恢复窗口组合 |
| C10 | 不同作品及独立媒体 Edition 的编辑准入、领取前后复查、Native 保存 CAS；实际 App 精确上报媒体草稿范围；媒体不同对象修改三方合并、显式冲突选择、重开历史基线回读、保存 CAS；同 Edition 普通消息与草稿并存，未保存选区拒绝；小说独立场景及单侧结构三方合并、明确冲突选边、历史基线回读、来源/保护/保存中拒绝、按作者章节持久草稿恢复；仅章节正文dirty时普通聚焦章节消息及失败重试可执行 | 规划/审阅表单的对象级并发及全媒介双候选整合旅程 |
| C11 | 原对话进度/提问/错误隔离；同编号问题隔离；同步快照游标、缺口检测及重复合并 | 多 Host 与长程供应商事件组合 |
| C12 | 实际 Host 强杀、Native 同库重开、租约到期前拒绝抢占、到期后接管及旧租约拒绝 | 双 Host 与全部插问/媒体接续组合 |
| C13 | 插问完成不解除原交付义务；媒体引用与 Task 原子绑定 | 全媒介必需产物/非必需分析失败组合 |
| C14 | 原作者要求继承，pending 阻止恢复，冻结来源不改写 | 新旧 writer 输出与实质要求变更全旅程 |
| C15 | Renderer 重开只读恢复、纯图片预览；快照读取竞争、容量裁剪与重复事件；真实 Host 强杀与 Native 同库重开 | 真实供应商进程故障组合 |
| C16 | 真实 Host 提交/领取/绑定/sending/模型接收/Turn 绑定/交付七个窗口强杀；首次及已有账本原 Work 接续；发送后丢账本拒绝 | 与插问、媒体、双 Host 租约过期的组合故障验收 |
| C17 | 队列 CAS、条件触发、重开不派发；后台消息与原 Work 接续均不阻塞扫描，容量/领取/关闭屏障及项目轮转；24轮双对话进程与监听释放、预热后堆稳定 | 长时间真实负载及全部过期引用组合 |
| C18 | 本机实际 Electron 路径及统一 123 项门禁，已接入 CI | Windows、远端 CI、资源测量与真实模型完整作者旅程 |

### 第十五段 POST：追加接收重放保持原来源

Native 使用原任务及下一意图修订作为接收身份，相同原文返回同一作者消息和交付状态；不同原文拒绝。重放在停止后仍可查询，但不再次登记作者要求、不失效其他动作、不增加预算、不开放新追加或交付。暂停与取消的 HTTP 夹具一次性同步当前 `commandId/threadId/expectedControlRevision` 合同，没有恢复旧字段支持。

五视角后审均 PASS：Native 消息 9/9、作者要求 4/4（`/tmp/intent-replay-native46.log`）；HTTP 来源、权限、暂停/取消、损坏记录、事务回滚及最终答复竞争 9/9（`/tmp/intent-replay-http46.log`）。最后架构检查 351 文件/7 crates/0 diagnostics（`/tmp/task-control-architecture45.log`）。纯要求解释与确定未投递接续仍按上表列为未完成，接收幂等不等同于模型注入幂等或全部产品退出条件达成。

## 第十六段 PRE：活动执行快照与事件游标

Host 为每个原执行保留有界、已脱敏的展示事件及单调游标，同步读取快照，替换异步 Core 历史读取拼接活动状态。Native 仍拥有领域事实，Core 历史仍拥有完整对话；缓存只服务 Renderer 重开。Renderer 在读取期间收集同执行事件，按游标补入快照之后的事件，问题新增/解决也使用同一序列。结束执行不再返回活动快照。接口、类型与全部快照 fixture 一次切换，无旧合同 fallback。

B10：每执行最多 512 条、2 MiB JSON 展示缓存，保护 Electron 内存与 IPC；连续同条文字增量合并，超过容量只裁剪展示并明确提示完整内容见对话历史，不停止任务、不改变预算。读取期间缓存发生缺口则显示读取失败、允许重新切换读取，绝不把部分事件冒充完整恢复。此限制不限制作者任务、输出或历史存储。

五视角前审：架构 APPROVE（Host 生命周期投影、共享无副作用事件容器）；技术 A/B APPROVE WITH CONDITIONS（游标边界、重复、洪峰、结束竞争和问题恢复）；产品/UI APPROVE WITH CONDITIONS（重开不重发、晚到进度不丢失，裁剪和缺口可见）。验证包含真实 Host handler、游标边界及实际小说/媒体 App 延迟快照竞争；不调用付费模型。

### 第十六段 POST：同步活动快照与有界事件合并

Host 活动快照不再等待 Core 历史；每个执行事件携带单调游标，结束清理中的执行不再投影为活动。Renderer 先缓存读取期间事件，再只补入快照之后的事件，恢复游标同时阻止重复文字追加。问题请求/解决共用序列；展示缓存裁剪明确提示，读取期间出现缺口明确失败，不重发作者消息。

五视角后审 PASS。Host 原执行隔离、活动读取不访问 Core 历史、结束过滤及 App 控制/媒体恢复 7/7（`/tmp/snapshot-integration49.log`）；补充游标重复、缺口、容量以及实际小说和媒体快照返回前文字到达 6/6（`/tmp/snapshot-ui50.log`）。最终构建通过（`/tmp/snapshot-build51.log`），架构 354 文件/7 crates/0 diagnostics（`/tmp/snapshot-architecture51.log`）。新事件测试与原多对话 UI 测试加入统一任务控制门禁。没有把显示缓存当作 Native 恢复凭证；真实 Host 崩溃和供应商恢复仍单独待验。

## 第十七段 PRE：后台消息派发不等待另一对话模型结束

当前 `drain` 等待完整 Turn，导致队列扫描被一个长请求占住。Host 将领取与执行生命周期分开：原消息领取确认后释放扫描，执行 Promise 按原消息追踪，其他对话可使用剩余 ExecutionDomain 容量。Native 的 claim/CAS、原 Host session、只读编辑保护和失败不重发仍为准入依据。关闭时先停止领取，再清理执行域并等待已领取消息落回原回执。

五视角前审：架构 APPROVE（Host 调度，不新增 Native 事实）；技术 A/B APPROVE WITH CONDITIONS（同时 drain、防重领取、容量、关闭期间领取回执、后台失败不成为 unhandled rejection）；产品/UI APPROVE（后台长任务不阻塞其他对话，消息原归属保留）。最多后台活动数沿用执行域 4 槽，包含领取阶段；不新增模型步骤限制。验证真实 Host 两个队列同时活动及受控领取/关闭竞争；不以 mock 并发代替应用域验证。


### 第十七段 POST：后台并行派发与关闭屏障

队列领取确认后，扫描不再等待整个模型回答。Host 按原对话追踪执行 Promise，领取阶段即占用有界槽位；重复扫描不会重复领取，同一对话仍串行，其他对话可使用剩余容量。领取后再次检查可用性与关闭状态，条件改变保留原消息待核对，不推断已发送。执行后的失败保留在原 Native 回执及消息结束事件，没有未处理的 Promise 拒绝。

后台扫描显式脱离发起回调的 request/domain 上下文。退出先禁止新领取，等待准备阶段结束并清理其创建的资源，再等待已启动执行和原消息回执；准备期间关闭不调用模型，也不会留下关闭后才创建的资源。

五视角后审 PASS。真实 Host 双队列在任一回答完成前同时活动、来源域隔离、关闭/容量/重复扫描等通过（`/tmp/parallel-context58.log`）。首次统一门禁的插问测试仍以扫描返回作为完成，出现 2 项失败（保留 `/tmp/task-control-gate55.log`）；夹具同步等待原消息生命周期，并在失败清理时关闭全部域。真实普通/生成 Work 插问 2/2（`/tmp/parallel-interlude56.log`），含后续关闭屏障的相关集成 10/10（`/tmp/parallel-shutdown57.log`）。最终统一任务控制门禁 76/76（`/tmp/task-control-gate59.log`）、构建通过（`/tmp/task-control-build59.log`）、架构 354 文件/7 crates/0 diagnostics（`/tmp/task-control-architecture59.log`）。本地受控并发证明实际独立进程和原消息交付，尚不代表 Windows/远程 CI 或长程 RSS/真实供应商性能验收。

## 第十八段 PRE：输入法确认与快捷发送

Renderer 统一判断发送快捷键的组合状态。输入法 composition、229 确认事件、按键重复和 Shift/Alt 组合不触发发送；小说保留 Enter 发送，媒体保留 Ctrl/⌘+Enter 发送，并标明快捷键。仅影响输入交互，不修改消息准入、模型或控制合同。

五视角前审均 APPROVE WITH CONDITIONS：复用一个 UI 纯判断，不引入平台旧接口兼容；实际两类 App 验证组合输入不调用发送、正常快捷键仍可发送、草稿和附件保留。验证模拟浏览器 composition 事件，不冒充各操作系统真实输入法全矩阵。

### 第十八段 POST：快捷发送不消耗输入法确认

小说和媒体输入框复用当前浏览器事件判断，composition/229、重复 Enter、Shift/Alt 组合均不调用发送；正常 Enter 与媒体 Ctrl/⌘+Enter 仍走原持久发送路径。媒体输入框增加关联的快捷键说明。实际 App 验证两类输入框的原草稿、媒体附件保留和正常发送，2/2（`/tmp/composer-keyboard60.log`），构建通过（`/tmp/composer-build60.log`），架构 355 文件/7 crates/0 diagnostics（`/tmp/composer-architecture60.log`）。五视角后审 PASS；此证据是 Electron 合成键盘事件，不是各系统真实输入法手工验收。

最后静态复核同时补齐仅通过快照恢复时的 Renderer 缓存容量回收，避免长时间跨项目切换而不收到实时事件时积累旧缓存；事件单元 4/4（`/tmp/snapshot-bounds61.log`）。原领域状态、消息、预算和任务均未因展示缓存回收改变。

最终当前源码构建再次通过（`/tmp/task-control-final-build61.log`），架构 355 文件/7 crates/0 diagnostics（`/tmp/task-control-final-architecture61.log`）。本轮 13 个生产源码/合同/配置文件指纹见 `/tmp/task-control-source61.json`，集合 SHA-256 `d6c896de4eace42744e9f2b347868bef811b3b3dfdefab035ad7b4bd8fee6669`；这是列明范围的工作区指纹，非全库提交或发布身份。

## 第十九段 PRE：媒体同 Work 只读插问

媒体插问沿用现有 Work/interlude/message 身份，将冻结的 Edition/branch/selection/images 放入原消息；Native 由媒体消息生成只读 Step，不借用小说页面来源。媒体读取沿原素材与版本权限，保存结构、媒体生成等写入在 Native 拒绝。Host 使用原 Step 和预算运行媒体提示，不创建另一根任务或新的媒体生成额度。Renderer 复用原插问/返回面板，并按所选媒体对话读取原 Work。

五视角前审 APPROVE WITH CONDITIONS：同 Work 与同媒介范围、消息与 Task 原子绑定、保存/生成负例、暂停和返回、过期版本不自动替换；真实 Native + Host 与媒体 UI 受控旅程证明入口闭环。接口和 fixture 一次性同步，不提供旧接口兼容。源版本变化保持待核对，不用重新读取悄悄改变已提交选区。

### 第十九段 POST：媒体插问闭环

Native 将媒体插问绑定原 Work、冻结媒体范围和原预算；只读任务可读原作品，不能保存文档、准备媒体生成、提交候选、改写计划或转交生成任务。Host 保留原 Step target，Renderer 提供原任务选择、临时提问及返回，返回不消耗新草稿。

五视角后审 PASS。Native 消息 9/9、媒体 15/15、插问 5/5（`/tmp/media-interlude-native66.log`）；最终 Runtime 构建70后的完整任务控制门禁 77/77（`/tmp/media-interlude-gate71.log`），含实际媒体 UI 和真实 Host/Native 媒体、普通及生成 Work 插问。构建通过（`/tmp/media-interlude-build70.log`），架构 356 文件/7 crates/0 diagnostics（`/tmp/media-interlude-architecture70.log`）。首次 Native 夹具预算 ID 不符合当前合同、首次 Host 控制文本不匹配精确 fixture，失败日志64/68保留；修改夹具后分别验证通过，没有放宽生产准入。真实付费供应商与跨平台仍未验收。

## 第二十段 PRE：媒体排队步骤恢复原上下文

源码复核发现普通 advisory 续跑只传 resumeStep，媒体上下文仅由单独 mediaInput 构造，可能错误进入小说 workspace；队列入口也未传原媒体对话。Host 从 Native 原 Step 的 edition target 重建输入，统一校验冻结版本、选区和图片，保留原 Step 权限及预算；续跑显式使用原对话。Native 仍是步骤准入和额度权威，版本变化拒绝，不替换来源。五视角前审 APPROVE WITH CONDITIONS：合同覆盖同范围恢复、原对话、版本变化、无新预算，实际已有媒体插问链路再验；本项不声称媒体自主决策全流程完成。

### 第二十段 POST：原媒体步骤恢复上下文

Host 统一从冻结 edition Step 构建媒体上下文；显式传入的来源仍需与原 Step 全字段一致。续跑保留原 target、resumeStepId、媒体额度和只读权限，不创建新媒体额度。队列入口绑定原媒体对话。媒体插问也走此统一恢复入口，真实集成证明不依赖 Renderer 再提供媒体输入。

五视角后审 PASS。恢复合同与步骤驱动 17/17（`/tmp/media-resume-contract72.log`），真实普通/生成/媒体插问 3/3（`/tmp/media-resume-integration72.log`），最终冻结步骤媒体入口 1/1（`/tmp/media-resume-frozen73.log`）。构建通过（`/tmp/media-resume-build72.log`），架构 356 文件/7 crates/0 diagnostics（`/tmp/media-resume-architecture72.log`）。版本变化负例拒绝，媒体额度缺失的只读步骤不被补发额度。该项证明媒体排队步骤派发与插问恢复，不宣称媒体全部自主决策及故障恢复旅程完成。

截至第二十段尚未完成：确定未投递追加要求的同 Work 接续、自由文本补充与实质修订解释的完整旅程、Host 崩溃逐窗口注入、更细对象并发编辑隔离，以及真实供应商/Windows/远端 CI/长程资源验收。既有本地受控证据不替代这些退出条件；整体目标保持未完成。

## 第二十一段 PRE：确定未投递追加要求的原任务接续

Engine/适配器只在调用底层发送前的身份或能力检查失败时返回 `steered:false, delivery:notSent`，底层调用后异常继续保持未知。Host 根据明确结果调用受信 Native 接口；Native 原子记录 notSent、暂停原 Work 并撤销原 Attempt，保留作者消息、步骤、预算及累计用量。作者继续原任务时冻结原要求与这条追加要求，不能将原 Turn 标记为已收到。pending 未知仍阻止恢复。重复回执幂等，已 delivered 不允许改为 notSent；取消 Work 不被重新打开。

五视角前审 APPROVE WITH CONDITIONS：架构由执行层提供未调用证据、Native 管持久事实；技术 A/B 要求事务、原身份、拒绝边界、重复及同预算恢复测试；产品/UI 要求明确“未发送，继续原任务”与“交付未知”区别，重开后保留。当前 schema/接口/fixture 同步，不提供历史迁移。Host 崩溃在证据落库前仍保持未知，不冒充安全重发。

## 第二十二段 PRE：同步工作区不重复读取正式状态

实际 App 提交回归在保存本地规划后读到两次 getState（`/tmp/unsent-ui82.log`），草稿和内容保留但违反一次同步合同。根因是 refreshFormalState 已读取并应用，再更新 creativeRefreshToken 触发正式状态 effect 重读。Renderer 分开子面板刷新与正式状态失效计数；普通刷新同时失效两者，已应用正式结果的刷新只通知子面板。身份/项目和草稿门禁不变。五视角前审 APPROVE WITH CONDITIONS：只修显示读取生命周期，不改领域事实；用原失败的实际 App 用例验证一次读取、草稿保留、晚到结果隔离及不增加模型调用。


### 第二十一段 POST：发送前拒绝与未知交付分离

Engine/产品适配器在发送前拒绝原执行身份或不支持的操作时返回明确 notSent，运行时方法已经调用后的异常不被归类为未发送。Host 只凭前者写 Native not-sent 回执；Native 原子保留原消息、暂停 Work、撤销旧 Attempt。重复回执不增加控制修订，已 delivered 不可改写。原任务恢复仍经过当前权限、来源、未结用量和原额度校验，将原要求与追加要求冻结到同 Work 的后继 Attempt；队列 advisory 续跑明确绑定原对话。原 Turn 的记录仍是 notSent，不伪造已收到；消息展示在要求纳入后继 Attempt 后退出待处理列表。

Renderer 将已保留与已投递区分，确认保留可清除同一草稿，等待期间新写的草稿不被清除；追加消息仍不可作为普通队列重派。五视角后审 PASS，零兼容门禁 PASS。Native 作者来源/重开恢复 6/6、会话消息 9/9（`/tmp/unsent-native74.log`）；执行层与适配器 67/67（`/tmp/unsent-engine74.log`），补充当前合同 72/72（`/tmp/unsent-contract76.log`），已移除执行拒绝 14/14（`/tmp/unsent-engine84.log`）。真实 Host/Native 两种结果 2/2（`/tmp/unsent-integration82.log`）：notSent 同 Work/对话接续且只发一个根预算；实际 steer 返回后丢回执保持 pending，无第二次模型执行；模型身份不能写 not-sent 接口。统一门禁 79/79（`/tmp/unsent-gate83.log`），当前构建/架构见83日志。

首次集成80/81日志保留：夹具把401鉴权拒绝写成403，把恢复失败投影误当 Promise 拒绝，并在完成后重新请求已结算任务预算；修正为当前合同与真实派发期间预算回执，没有修改生产准入来迎合夹具。证据在 Native 落库前丢失仍保持未知，不声称覆盖全部 Host 崩溃窗口。

### 第二十二段 POST：正式状态与子面板刷新分开

显式工作区同步应用 getState 回执后，仅刷新下游面板；其他正式数据失效仍刷新两者。原失败的实际 App 用例通过（`/tmp/unsent-ui83.log`，1/1），包含一次同步、保存和放弃编辑、晚到结果、草稿保护以及保留追加要求提示。早期77/79 UI夹具缺少当前 project/execution 问题归属、把问题身份对象当作旧 requestId；同步当前合同后81/82暴露生产重复读取，修复后原断言与快照全部通过。五视角后审 PASS，保留全部失败记录，未放宽一次读取断言。构建通过（`/tmp/unsent-build83.log`），架构356文件/7 crates/0 diagnostics（`/tmp/unsent-architecture83.log`）。

当前剩余：自由文本补充与实质修订解释的完整模型旅程、Host 崩溃逐窗口注入、更细对象并发编辑隔离，以及真实供应商、Windows、远端 CI 和长程资源验收。第二十一段已完成明确发送前拒绝的接续；未将“所有未知都可重发”列为目标。整体退出条件尚未全部满足。

第二十一段补充取消边界：Native 作者要求测试最终7/7（`/tmp/unsent-native84.log`），确认作者取消后的 notSent 回执不改变 Work 状态/控制修订、无法恢复已取消任务。最终架构复核仍为356文件/7 crates/0 diagnostics（`/tmp/unsent-architecture84.log`）。本轮16个生产文件/配置源码指纹保存于 `/tmp/unsent-source84.json`，集合 SHA-256 为 `7e58ff9e69e49828a29e670c9a5a29bb3af56081e2b6cb75f2d86443c4d567cd`；这是所列范围的工作区身份，不是提交或发布身份。

## 第二十三段 PRE：真实 Host 进程崩溃窗口

使用隔离 Native Runtime、真实生产 Host handler/ConversationMessages 和受控 App Server，分别在消息提交、领取、Task 原子绑定、sending 提交、模型已接收、Turn 绑定与 delivered 回执落库后强制结束测试 Host 进程；同时重开同一测试数据库，再由新 Host 读取和核对。生产代码不加入故障开关；测试 transport 只在真实 HTTP 提交后阻挡回执。只杀本次创建的进程组，清理隔离目录。

五视角前审 APPROVE WITH CONDITIONS：架构 Native 保有消息/Task 事实，Host 只重建生命周期；技术 A/B 验证无清理 finally 的崩溃、原消息身份、无自动重发、原 Task/Work 与预算不丢失、明确继续的旧领取失效；产品/UI 验证重开后可查询、确定无 Task 的领取可核对但必须明确继续，有 Task/未知发送不变成新根请求。受控 App Server 不代表真实供应商计费或所有操作系统进程语义。该段首先补齐实际故障证据，发现生产缺口再做对应修复。

## 第二十四段 PRE：崩溃前未派发的原任务恢复

真实失败89：Task 绑定后、sending 之前崩溃，作者暂停并继续原 Work 时，首次 Engine 账本文件尚未建立，费用核对报不可读。失败90：已有有效账本时原 Work 能续跑，但原消息仍 claimed，继续占住对话。修复职责：Native 仅凭仍为 claimed、精确 Task 绑定、终态撤权、无 Turn/输出/工具操作证明尚未派发；Runtime 仅在配置的 Engine 文件确实不存在时使用这份更严格证明，损坏/不可读/已 sending 不推断零用量。有效 Engine 回执始终优先。Native 消息核对允许追踪同一 Step、同一项目/对话的后继 Attempt 的真实 Turn，并原子更新消息交付关联与修订，旧领取不能覆盖。

五视角前审 APPROVE WITH CONDITIONS：Engine 真实用量优先，Native 管未派发状态/原任务血缘；不创建新根预算或复制要求；只在已核准的原 Step 续跑后确认交付，未知投递不触发自动重发。验证原89/90入口、首次/已有账本、坏账本/未知写入负例、后继 Turn 之前不确认、另一任务 Turn 不串入、旧领取 CAS 拒绝。没有引入历史格式兼容。


### 第二十三段 POST：真实进程逐窗口崩溃

新增隔离 Native Runtime fixture 与独立 Host 子进程，使用生产 handler、消息队列、Adapter 和 Engine；故障只由测试 transport 在实际 HTTP 已提交后阻挡回执，再强杀测试进程组，不执行正常清理。七个基础窗口覆盖提交、领取、Task 绑定、sending、模型已接收、Turn 绑定和交付落库；重开同一 Native 数据库并启动新 Host。明确继续前无自动写入或 Task 派发；无 Task 的旧领取被修订屏障废止，已有 Task 不变成新根请求，真实 Turn 才证明交付。

五视角后审 PASS：架构保留 Native 事实权威；技术 A 验证真实强杀与同库重启，技术 B 核对无自动重派及精确身份；产品/UI 保留可查询消息、明确继续和未知投递，不承诺不可证明的 exactly-once。首次 fixture 失败85/86为重复鉴权/内容类型请求头，87为成功退出与测试清理竞争；修正 fixture 后88基础7/7通过，92包含后续恢复的10/10通过（`/tmp/conversation-crash92.log`）。未把受控 App Server 当作真实供应商，也未宣称 Windows 进程语义已验收。已加入统一 `test:task-controls` 门禁。

### 第二十四段 POST：未派发的原 Work 可恢复，后继回执解除消息占用

Native 使用精确消息/Task/Step 绑定、仍为 claimed、终态撤权、无 Turn/输出/工具操作及冻结预算校验证明未派发。Runtime 优先读取 Engine 实际回执；仅当配置的账本文件确实不存在时使用更严格的未派发证明。损坏、不可读或已 sending 都不能被当作零费用。继续原 Work 后，Native 核对同 Step 后继 Attempt 的真实 Turn，原子更新原消息交付关联及修订；别的任务和无 Turn 的新 Attempt 都不能解除占用。

真实故障89（首次账本不存在）与90（续跑后消息永远 claimed）已由相同实际 Host 入口重验：92中 `taskBoundFirstResume`、`taskBoundResume` 均通过，复用原 Step、无新根预算、原 Work 恰有两个 Attempt。`sendingLostLedger` 保持消息 sending、费用待核对，零新 Attempt。消息仓储10/10通过（`/tmp/crash-native91.log`），含无回执、无关任务回执和旧领取 CAS 负例。构建通过（`/tmp/crash-build92.log`），架构356文件/7 crates/0 diagnostics（`/tmp/crash-architecture92.log`）。五视角后审在上述恢复范围 PASS；预算相邻负例通过（`/tmp/crash-budget91.log`，1项测试内8种状态）；当时统一门禁89/89通过（`/tmp/crash-gate93.log`），之后媒体扩展的最终门禁另记。未引入历史兼容或迁移，未修改真实作者资料。


## 第二十五段 PRE：媒体崩溃恢复沿冻结输入接续

扩展同一真实进程故障夹具到漫画 Edition：在 Task 原子绑定及 Turn 已绑定两个窗口强杀，Native 同库重开后分别明确继续原 Work、核对已有回执。Host 通过生产 MultimediaHost 读取公开媒体合同；冻结原 Edition/修订/选区/图片，恢复不由 Renderer 补传，也不建立新预算。五视角前审 APPROVE WITH CONDITIONS：不改领域权限；断言同对话、同 Step、同冻结媒体 target、无自动重发与无新根预算；这是媒体生命周期验收，不代替真实生成服务、动画产物或模型语义验收。

第二十五段故障与方案补充：94/95为测试场景映射和 Scope DTO 夹具错误，修正当前公开合同后96复现生产故障：媒体原 Step 已成功续跑，Host 下一轮无排队步骤仍请求小说 creative-workspace，返回 novel_scope_required。责任是 Host 跨媒介生命周期派发；Native 的 completed + discussion_ended/ready_for_review、无待接力与无待结算状态才允许媒体返回。媒体无可派发步骤但仍未满足上述事实时保留未完成，不借小说决策或创建新 Work。五视角补充前审 APPROVE WITH CONDITIONS：仅媒体 Step 分支，小说决策不变；真实原失败入口重验，合同覆盖未完成/未知用量/待接力拒绝。


### 第二十五段 POST：媒体原步骤完成后停止跨入小说决策

Host 在没有排队步骤时识别 Native 的 Edition 步骤，只在 Native 报 completed、discussion_ended/ready_for_review、无待接力且无待结算 Attempt 时返回原 Work；其余状态保留 CONVERSATION_INCOMPLETE。小说的原决策路径不变，也没有把模型回答当作作品完成。故障96由实际漫画 Host 入口98重验通过（`/tmp/media-crash98.log`，2/2）：Task 绑定后强杀能继续原 Step 和预算，已有 Turn 的强杀只核对交付，冻结媒体 Scope、revisionId 和对话保持。

五视角后审 PASS：架构按 Native 已有完成事实消费，不新增领域权威；技术 A 覆盖未完成、未知用量、待接力、待结算负例；技术 B 验证原失败及相邻恢复入口；产品/UI 解除已经完成却报小说范围错误的恢复失败，不隐藏真实待核对事项。29项步骤/恢复合同通过（`/tmp/media-driver98.log`）。97相邻恢复 fixture 仍在旧全局 Adapter 注入失败，导致真实每对话 Adapter 未受控；改为现有 onPrepareDomain 后原断言全部通过，未修改生产恢复保护。94/95/97媒体 fixture 错误分别为场景变量、Edition 元数据混入 Scope、将消息 expectedRevisionId 当作 Task target 字段；按现行四字段 Scope 和 target.revisionId 修正，失败记录保留。媒体本次使用空版本与无附件来源；已有非空版本/附件的合法性与过期拒绝沿用第十九、二十段证据，本次不冒充全部媒体素材组合。


第二十三至二十五段最终验收：统一任务控制门禁91/91通过（`/tmp/final-controls99.log`，含12项真实 Host 强杀/同库重开场景、实际 Electron 控制及普通/生成/媒体插问），恢复驱动合同29/29通过（98），Native 消息10/10及预算相邻负例1/1通过（91）。最终构建通过（`/tmp/final-build99.log`），架构356文件/7 crates/0 diagnostics（`/tmp/final-architecture99.log`）。构建的 bundle 体积提示仍存在，不作为性能通过证据。11个本轮生产/测试/配置文件指纹保存于 `/tmp/task-control-source99.json`，集合 SHA-256 为 `5e344cf83bf3aafe15b742a1782c05a6686d42ab30edd24ea7aad9f2b8bcc1dd`；这不是提交或发布身份。

当前整体仍未完成：模型区分纯提问/补充材料/实质要求修订的完整旅程、同作品更细对象级编辑隔离、双 Host/租约与插问组合故障，以及真实供应商、Windows、远端 CI、真实输入法和长程资源验收。上述已交付崩溃修复不替代这些退出条件；当前未创建提交、未推送或发布，保留既有工作区改动。

## 第二十六段 PRE：未保存媒体编辑不阻塞只读插问

检查同作品编辑限制发现 Host 消息准入只认小说 focus.readOnly，Native 已绑定为只读的媒体 interlude 因 focus=null 被同项目 dirty 一并拦住。职责：Renderer 继续保存编辑草稿状态；Host 依据 Native 持久消息的 interlude 合同允许只读派发；Native 仍校验同 Work、冻结媒体范围及禁止写入。普通媒体消息和后继写动作仍等待编辑结束，不凭自由文本猜只读。五视角前审 APPROVE WITH CONDITIONS：架构不扩权限，技术 A/B 使用真实 Host/Native 复现 dirty 时插问阻塞并验证普通消息仍等待，产品/UI 保持未保存草稿且可提问；同对象细粒度写隔离仍是后续独立范围。无 schema 或兼容调整。

### 第二十六段 POST：只读媒体插问与编辑草稿并存

真实故障100：同项目 dirty 时持久 interlude 始终 waiting，零插问 Attempt。Host 根据 Native interlude 合同放行只读派发后，原失败入口101通过：问题 answered、同 Work/媒体范围/预算、Task readOnly=true；保持 dirty 再提交普通媒体消息仍 queued、没有第三次 Attempt。未扩大写权限，未清除或自动保存编辑草稿。五视角后审及零兼容门禁 PASS；代码与受控 Native/Host 行为已验，真实界面草稿组合另验。

## 第二十七段 PRE：强杀后的真实租约过期接管

扩展已有真实进程崩溃夹具：原 Host 在模型已接收后强杀，第二个生产 WorkExecutionOwners 在原租约有效期间被 Native 拒绝；等待 Native 回执的 expiresAt，再接管同 Work，旧 Host renew/release 不得影响新租约。消息仍是未知投递，不因接管创建新模型执行或预算。五视角前审 APPROVE WITH CONDITIONS：Native 依旧是租约权威，Host 不改时钟、不改数据库、不伪造过期；验证实际 HTTP 与独立 Host 进程、旧租约隔离和零重发；不声称这已覆盖所有多 Host 媒体/插问组合。只处理隔离测试进程和作品。

第二十六段界面补充前审：实际 MediaApplication 的 MediaInterludePanel 也把 dirty 加入 blocked。同步移除仅只读入口的 dirty 阻塞；普通发送/保存行为保持，明确插问基于已保存版本。实际 Electron 测试先新增未保存页面，执行插问与返回，检查页面草稿、新输入和普通发送禁用均保留，再明确放弃测试草稿。五视角同意此入口同步，不把未保存内容冒充已保存引用；新建选区不适用已保存来源时仍由当前 Native 合同拒绝。

### 第二十七段 POST：原租约有效期保护与过期接管

实际强杀与 Native 重开后，第二个生产 WorkExecutionOwners 在原租约到期前被拒绝；等待回执 expiresAt（实际约30秒）后取得不同 leaseId，旧 Host 续期失败、释放返回 false，新 Host 正常续期。原消息、原 Task 和 Attempt 数保持，无模型重发或预算重建。102测试通过（`/tmp/owner-crash102.log`，1/1），五视角后审 PASS。这是一个真实 Host 崩溃/租约过期场景，仍不能替代全部双 Host 接续、插问和媒体故障组合。

## 第二十八段 PRE：重复双对话执行的资源释放验收

使用生产 Host handlers、ExecutionDomains、Adapter、Engine 预算持久化与真实受控 App Server 子进程，连续24轮双对话，交替停止其中一条、另一条追加后正常完成。每轮验证原线程可再次进入、关闭进程实际退出、执行域/请求等待器/Renderer销毁监听恢复基线，记录进程峰值和内存观测。Runtime领域接口使用现有受控合同夹具，因此本项证明 Host 生命周期，不能声称长篇真实供应商或 Native历史增长性能合格。五视角前审 APPROVE WITH CONDITIONS：只观测并清理测试自建资源，不引入产品阈值、不清理历史或作者数据；失败保留后修复责任模块。


第二十六段界面闭环：103在实际 Electron 复现只读按钮被 dirty 禁用。104原入口通过：新增未保存漫画页面后可临时提问，返回后页面、新输入和 dirty 都保留，普通发送仍禁用；测试最后明确放弃自己的隔离页面。提示明确回答基于已保存版本，未把新建未保存对象冒充合法保存来源。五视角后审 PASS，未改变普通发送或 Native 写权限。

### 第二十八段 POST：48回合的进程与监听释放

105、107各完成24轮/48回合：每轮两条对话交替停止/追加完成，进程峰值2、48个子进程全部实际退出，执行域归零、容量恢复、等待器清除、Renderer destroyed 监听恢复基线。此生命周期范围五视角后审 PASS。未主动GC时首轮Host RSS从约97MiB增至425MiB；107显式GC诊断中heapUsed从约17MiB到246MiB，GC后约112MiB（`/tmp/resource-soak-gc107.log`）。内存观测并未证明稳定或无泄漏，须继续区分夹具保留数据、执行缓存与长期增长；未添加生产强制GC、内存上限或掩盖增长。该15秒受控压测不能称为长程真实供应商验收。仅进程/监听生命周期得到闭环，内存与更长持续负载仍保持未完成。


第二十八段内存诊断闭环：109本地堆快照显示2个 `_Tiktoken` 实例、约30万组词表 Uint8Array/ArrayBuffer；源码 `shared/author-input-budget.mjs` 首次初始化 o200k_base 与 cl100k_base。110在第一轮完成后记录预热基线，再完成剩余23轮，显式GC后heapUsed从116247104降至115984064字节（约110.9→110.6MiB），arrayBuffers两次均228297字节，48个子进程全部退出。支持本次增长来自词表预热而非按回合持续残留；未修改产品代码强制GC。证据 `/tmp/resource-warm110.log`，无预热、GC与堆分析实验分别保留105/107/109日志。短程受控循环及内存观测得到闭环；仍不把15秒负载升级为长篇、真实供应商和跨平台性能认证。

第二十六至二十八段最终验证：统一任务控制回归93/93通过（`/tmp/control-gate108.log`，包括13个真实崩溃/租约窗口、实际媒体未保存编辑插问、48回合进程释放）。之后仅增加资源测试的预热观测字段，110原测试再次通过，生产源码未变。构建通过（`/tmp/control-build106.log`），架构356文件/7 crates/0 diagnostics（`/tmp/control-architecture106.log`）。未执行提交、推送或发布。7个本轮生产/测试文件指纹保存于 `/tmp/task-control-source110.json`，集合 SHA-256 为 `f087dca3d7550c899625dae245ee8f167230205c5d81ba32f09c4a1e8cd8d21a`。

当前剩余范围仍是：自由文本意图解释与要求变更的完整模型旅程、同作品对象级写入/草稿隔离、更多双Host与插问/媒体接续故障组合，以及真实供应商、Windows、远端CI、真实输入法和长时间负载验收。未保存编辑与只读媒体插问的并存已完成，但不能把它写成对象级写并发全部完成。

## 第二十九段 PRE：独立媒体版本的消息准入隔离

Renderer 当前仅上报 projectId/dirty，导致同作品独立 Edition B 的持久消息被 Edition A 的未保存编辑阻断。新增当前 IPC 必填 mediaScope（明确 EditionScope 或 null）；Renderer 只对正在编辑的媒体版本报告该范围，小说或作品目录/删除动作报告 null。Host 以 Native 已保存消息的冻结媒体范围比较：项目不同或独立 editionId+scopeId 可派发，同 Edition（包括不同 branch）、同 Scope、未知范围保持阻塞；Native interlude/明确只读范围沿用只读准入。此范围只收窄消息队列的编辑保护，不能推断自由文本未来写范围，Work 后继和小说多对象依赖仍不放宽。

事实权威：Renderer 拥有未保存草稿范围，Native 拥有消息/Edition 身份与权限，Host 只做派发准入，Native CAS/写权限继续执行。生产 IPC、类型、Renderer、当前 fixture 一次性同步，无旧字段 fallback。五视角前审 APPROVE WITH CONDITIONS：架构不按文字猜目标、不扩写权限；技术 A 验证未知/同范围/错误字段拒绝、领取前后两次复查；技术 B 用真实 Native 两个 Edition 的消息派发及实际 App 上报再验；产品/UI 保留本地编辑和同版本写阻塞，不自动保存或丢弃草稿。模型仍自主决定各版本内部的创作操作。整体对象级写并发未在本段宣称完成。


第二十九段状态判定（本地草稿由 Renderer 提供，消息范围由 Native 冻结）：

| 输入事实 | Host 准入 | 副作用与后续 |
| --- | --- | --- |
| dirty 且同 Edition、同 Scope、仅 branch 不同，或任一范围未知 | 等待 | 不领取或执行；保留消息与草稿，保存/放弃编辑后重新检查 |
| dirty 且 editionId、scopeId 均不同，同一项目身份有效 | 可派发独立版本消息 | Native 仍校验该 Edition 身份、修订与写权限；不自动保存当前草稿 |
| dirty 且 Native interlude 或明确只读 focus | 可派发只读问题 | 读取保存版本，沿用原 Work/预算/只读限制 |
| 小说、目录/删除动作 | 整个项目写入保护 | 不推断章节独立性；后继 Work 的保护未放宽 |
| 上报合同缺字段、越项目或范围多字段 | IPC 拒绝 | 不替换原有效上下文；同步当前调用方，无历史 fallback |

### 第二十九段 POST：独立媒体版本不再被其他版本草稿阻塞

新增 Host 编辑上下文纯函数，当前 IPC 必填四字段 mediaScope 或 null；Renderer 对加载完成的媒体版本上报实际范围。Native 消息范围不同 editionId 且不同 scopeId 时允许队列派发，同版本、同 Scope、未知范围仍等待。保留领取前后复查、Native 权限/CAS、原 Work 与预算；没有放宽后继工作或小说对象保护，没有自动保存/丢弃草稿。

五视角后审 PASS（限独立媒体 Edition 队列准入）：架构依据 Native 冻结目标消费本地编辑事实，无新增领域权威；技术 A 校验精确合同、跨项目拒绝、同范围/分支/未知范围负例；技术 B 通过真实 Native 两个 Edition 的派发及实际 Electron App 上报；产品允许后台独立版本继续工作；UI 保留当前页面草稿，退出清理范围，小说继续报告 null。零兼容门禁 PASS，类型、生产者、消费者、fixture 与当前文档同步。

证据：111合同2/2、真实 Native 媒体派发1/1通过（`/tmp/edit-scope111.log`、`/tmp/edit-native111.log`），实际 App 113通过（`/tmp/edit-ui113.log`），统一任务控制114为96/96通过（`/tmp/edit-gate114.log`，包含新增合同和实际 App 检查）。构建111通过（`/tmp/edit-build111.log`），架构112扫描357文件/7 crates，0 diagnostics（`/tmp/edit-architecture112.log`）。该真实派发证明独立消息完成，不冒充真实生成服务或跨对象写入整合验收。

相邻 fixture 修正：112章节测试仍拦截旧全局 Adapter，等待未触达；115改到执行域时字段误写 adapter，改为现行 codexAdapter 后116章节13/13通过。116真实 Native 工作事件3个场景因同样的旧全局 Adapter 注入未触达而失败；同步每对话执行域后117四个场景全部通过，包括完成、失败、生成可恢复、生成阻塞（`/tmp/edit-events117.log`）。原拒绝/暂停/预算断言保留，未通过放宽生产校验使测试变绿；原失败日志保留。只终止本次自建的挂起测试进程。

整体仍有未完成项：同 Edition 内及小说多对象依赖的编辑隔离、自由文本意图解释与要求修订的完整模型旅程、更多双 Host 与插问/媒体接续组合，以及真实供应商、Windows、远端 CI、真实输入法和长时间负载验收。本段没有将独立版本准入扩大表述为所有对象写并发完成。未提交、推送或发布。

第二十九段源码身份：11个生产/测试/配置文件指纹保存在 `/tmp/task-control-source117.json`，集合 SHA-256 `c7437dda613c1a53d7eb546e0d91ce1184b5fc0a6fa89c427166cdbca811db14`；不是提交或发布身份。

## 第三十段 PRE：后台 Work 接续不占住事件扫描器

当前消费者在扫描循环 await 完整 resume，长回合阻止后续项目和持久消息被扫描，即使 ExecutionDomains 仍有容量。职责为 Host 生命周期：扫描只等待有界读取/派发准备，原 Work 接续独立跟踪；以项目+Work 及项目+对话占位防重复，准备与清理也计入消费者容量（沿用 ExecutionDomains.maxActive），最终模型并发仍由 ExecutionDomains 原子准入。Native 继续负责 claim/lease/暂停取消/ack，未知或中断仅释放原领取，不制造消费回执。增加 settled 供验收等待实际接续完成，不让 tick 再承诺模型完成；同步当前测试调用方。

| 输入事实 | 结果 | 副作用与后续 |
| --- | --- | --- |
| A 接续未结束，B 对话可用 | 后续扫描和 B 派发可进行 | 各自原 Work/租约/预算，不复制作者请求 |
| 同 Work/同对话重复事件或扫描 | 本 Host 只保留一个在途接续 | 其他事件保留 Native 原状态 |
| 容量满（含准备/清理） | 等待下一次扫描 | 不创建额外模型回合 |
| stop 在领取中或执行中 | 旧扫描不得新增派发或 ack | 返回后释放原 claim，未知结果保留 |
| dirty/暂停/租约失效 | 继续原准入拒绝 | 不借并发绕过 Native 控制 |

五视角前审 APPROVE WITH CONDITIONS：架构只拆分 Host 扫描与生命周期等待，无模型策略变更；技术 A 覆盖容量、重复、stop 与清理；技术 B 复现旧串行阻塞并通过真实 Native/Host 相邻恢复入口；产品让其他对话在容量可用时获得执行机会；UI 继续消费原事件与健康快照。零兼容：同步当前 tick/settled 调用方，无旧行为双轨。无需更改 Engine 或 Native schema，不放宽对象写范围。


### 第三十段 POST：长回合接续期间继续扫描其他对话

118复现原故障：resume 尚未结束时 tick 不返回，其他队列扫描无法推进（`/tmp/wake-failure118.log`）。消费者现在把接续保存在独立的在途集合，tick 只等待扫描；项目+Work 和项目+对话在第一次异步准备前占位，直到领取/租约清理结束才释放容量。原 ExecutionDomains 仍控制真实模型并发；Native 仍负责原 claim、暂停取消、预算和消费证明。新增 settled 用于验收和关闭收尾，Runtime 在执行域退出及领取清理后才停止。当前调用方全部同步，没有旧行为兼容分支。

容量在页面中途耗尽时，下轮扫描从后面的项目继续，避免持续繁忙项目重复占据起点。stop 改变扫描代次，旧领取返回后不能派发/ack，仅释放原领取。原 Work、对话、预算和作者输入都保持，未扩大媒体/小说的写范围。

五视角后审 PASS：架构仅调整 Host 生命周期；技术 A 验证并行容量、同 Work/对话互斥、领取中停止、扫描轮转；技术 B 在实际 Native/生产 Host/Adapter 回合挂起时证明再次扫描发生且零重复领取/Attempt；产品与 UI 保持原进度/错误归属，其他对话不再等待该回合结束才被扫描。现有后台健康状态和失败退避保持。零兼容与 B01–B10 检查在本段范围通过。

验证：合同120为28/28，补轮转后123为29/29；真实 Native121四类恢复4/4；统一122为104/104（`/tmp/wake-gate122.log`，已加入真实 Native 工作事件测试，覆盖本轮新合同、媒体/插问、崩溃/租约及实际 Electron）。构建121通过；最终架构123扫描357文件/7 crates，0 diagnostics。123单独 Native 复验出现测试只读 SQLite 观察连接与 Runtime 事务竞争的 database is locked（原日志 `/tmp/wake-native123.log`）；只对该测试观察连接设置 busy_timeout=5000，未改产品事务或重写领域数据，原断言继续验证。

124原 Native 测试4/4通过（`/tmp/wake-native124.log`）。7个本轮源码/测试/配置文件指纹 `/tmp/task-control-source124.json`，集合 SHA-256 `d7092945f3522a330b8f4430fb8e504b52db597576d5d9e74161da0fc162843c`。未提交、推送或发布。整体剩余仍包括对象内编辑隔离、模型意图/要求修订完整旅程、多 Host 插问/媒体组合以及真实供应商、Windows、远端 CI、真实输入法、长程负载；本段后台调度完成不替代这些退出条件。

## 第三十一段 PRE：媒体草稿与独立后台修改三方合并

当前媒体编辑器刷新后保留本地 document 与旧 expectedRevisionId，同时替换 workspace；后台改动另一对象也会让本地保存必然 CAS 冲突。Renderer 管本地草稿合并，Native 保存/版本/权限继续是唯一权威。以实际加载的 base、当前 local、最新 remote 做结构合并：一方未改则取另一方；对象字段及稳定唯一 id 数组内的不同对象可合并；两方改变同一值、删除与修改相遇、双向结构/排序变化无法证明独立时保留冲突。不依据文学含义推断，不自动保存、采用或发布。

仅在已知 base 与当前草稿 expectedRevisionId 精确对应且 Edition/Scope/branch 身份一致时合并，并随成功合并一起更新预期修订；同源 sources/共同设定发生变化时保守保留冲突，不拼接不同来源。本地持久草稿格式不变，重开缺少原 base 时保留原草稿和旧修订，不构造历史迁移。真实作者资料不删除。保存进行中的读取不得覆盖保存回执，刷新结果必须检查当前操作状态。

| 输入事实 | 结果 | 副作用/后续 |
| --- | --- | --- |
| 不同页/镜头/字段改变，原 base 可证明 | 合并草稿，预期修订更新到最新 | 保持 dirty，作者保存仍走 Native CAS |
| 同字段不同改动、删除与修改冲突、双方改排序 | 保留原草稿及预期修订 | 显示需核对，不覆盖远端 |
| base 缺失/身份或来源不一致 | 不合并 | 原草稿保留，保存不得绕过 CAS |
| 保存与刷新回执交错 | 保存回执优先，旧读取失效 | 不倒退修订，不丢本地编辑 |

五视角前审 APPROVE WITH CONDITIONS：架构只处理 Renderer 本地草稿；技术 A 验证删除、排序、重复 id、冲突不产生部分覆盖及 CAS 身份；技术 B 通过实际 Electron 编辑/刷新/保存入口验证；产品保留独立编辑并解释后台更新；UI 不展示内部对象路径或自动丢弃冲突。零兼容与 B01–B10 保持，整文档写合同未改变，本段不直接解除同 Edition 消息写准入。


### 第三十一段 POST：媒体独立修改可合并，冲突仍受 CAS 保护

新增 Renderer 纯结构三方合并函数：按字段与唯一稳定 id 合并不同页、画格、镜头、片段的独立修改；一方的新增/删除/排序可与另一方现存对象修改合并。同字段不同改动、删除与修改冲突、双方结构/排序变化及重复 id 的歧义会整体拒绝，输入不被修改。编辑器仅在原 base、预期修订、Scope/branch 与来源/共同设定可核对时应用合并，更新的是未保存草稿和下一次 CAS 预期值，没有自动保存、采用或发布。重开缺少 base 时保留本地草稿，未新增持久字段、旧格式兼容或迁移。

保存、素材导入、采用设定开始时废止之前的读取回执；进行中的操作不发起新读取，避免晚到旧工作区倒退修订或覆盖新素材。原失败保存的幂等键仍按原请求复用；合并产生新请求时才重建键。冲突草稿保持原预期修订，不能通过界面覆盖当前已保存版本。

五视角后审 PASS（限本段范围）：架构保持 Renderer 草稿/Native 保存事实边界；技术 A 验证重叠、删除、排序、别名、重复 id、修订竞争；技术 B 通过实际 Electron 编辑→刷新→保存与真实 Host/Native 保存合同；产品减少独立对象修改导致的整版冲突；UI 明确“合并后仍未保存”，冲突保留原输入。零兼容检查通过，不删除真实作者数据。

证据：125合并合同4/4、实际 Electron1/1、构建通过；127扩展界面仍通过，覆盖独立画格合并、冲突零覆盖、共同设定变化不合并、重开缺失 base 不猜测、保存成功后的旧读取不倒退。真实 Native126为1/1（`/tmp/draft-native126.log`），经生产 MultimediaHost 保存双方修改、回读历史修订不变，两次旧 CAS 拒绝且最新已保存内容保持。界面测试中的保存服务是受控合同，Native126单独验证真实领域持久化，两者不能冒充真实模型文学质量。

最终生产构建128通过（`/tmp/draft-build128.log`），架构128扫描359文件/7 crates，0 diagnostics（`/tmp/draft-architecture128.log`）。本段并未解除同 Edition 普通消息在 dirty 时的写保护，也未完成小说多对象合并；允许的既有后台执行产生更新时，独立媒体草稿现在可无覆盖地整合。统一门禁结果另记。

第三十一段最终验收：统一128为110/110通过（`/tmp/draft-gate128.log`）。随后完善双方修改恰好相同的状态提示：按真实保存内容显示“已与最新保存版本一致”，不误报未保存；129实际编辑器完整流程再次通过（`/tmp/draft-ui129.log`）。最终构建129通过，架构129为359文件/7 crates/0 diagnostics。6个本轮生产/测试/配置文件指纹 `/tmp/task-control-source129.json`，集合 SHA-256 `27ba2ed809d6110e301f988bb550173767497ea4acd85fed010d272f0da02c10`，不是提交或发布身份。未提交、推送、发布。

整体仍未完成：同 Edition 普通消息 dirty 写准入与冲突人工整合、小说对象级编辑隔离、自由文本意图与实质要求修订完整模型旅程、更多多 Host/插问/媒体组合、真实供应商、Windows、远端 CI、真实输入法及长程负载。第三十一段已完成有原版本证据的媒体独立草稿合并，不将其写成全部对象并发编辑已经完成。

## 工作区本地合入记录

作者要求将工作区代码全部合入。当前仅有 main 分支及一个 checkout，按当前工作区完整提交源码、测试、配置与文档。新增忽略范围仅为现存本地验收运行目录、启动采样及三个安装包目录；数据库、凭据及安装包留在磁盘，不删除或提交。五视角前后审：此项不改产品行为/领域合同，忽略规则保持代码完整与本地数据隔离，PASS。修正一处文件尾空行。架构检查359文件/7 crates/0 diagnostics；此前统一任务控制110/110、最终媒体界面及构建已通过。整仓 repo:check 尚未通过：既有 codex-rs/codex-api/src/api_bridge.rs、api_bridge_tests.rs 与 codex-rs/core/src/mcp_tool_call/conversation_history.rs 未被上游路径登记覆盖；不为合入扩大白名单，不将本次提交称为发布验收通过。未请求远端推送。

## 第三十二段 PRE：媒体冲突核对与显式选择

Renderer 在确知原 base、当前草稿、最新保存版本及相同来源时提供冲突核对：复用只读媒体编辑器展示最新保存内容，作者可选择冲突处保留草稿或使用保存内容，独立修改继续合并。结构冲突明确采用所选一侧完整列表；重复 id、类型变化仍拒绝。选择只生成未保存草稿并绑定所核对修订，仍须点击保存，Native CAS/权限/历史保持。缺少 base、来源或共同设定变化时不开放该选择，原草稿保留；不能让 UI 选择变成来源授权。

| 状态 | 行为 | 副作用 |
| --- | --- | --- |
| 同字段冲突，base/来源可核对 | 展示最新保存内容，作者明确选择 | 仅更新草稿，独立修改保留 |
| 删除/排序等结构冲突 | 采用选择侧完整冲突列表 | 明确提示，保存仍经 Native |
| 核对后又出现新修订 | 重新核对，旧选择不覆盖新内容 | CAS 拒绝，不自动重试写入 |
| base 缺失或来源变更 | 不允许猜测合并 | 保留草稿与旧修订 |

五视角前审 APPROVE WITH CONDITIONS：架构属于 Renderer 草稿决策；技术 A 验证两种选择、删除/排序/重复 id 及旧修订拒绝；技术 B 真实 Electron 与 Native 原保存入口验收；产品提供可操作冲突恢复；UI 不自动选边、不自动保存，最新内容可按页/镜头浏览。无兼容或 schema 变更，不解除 dirty 消息准入。


### 第三十二段 POST：媒体冲突可核对、选边并经原 CAS 保存

已提供“查看最新保存版本”只读浏览，以及“冲突处保留我的修改”“冲突处使用已保存内容”。两个选择只重建未保存草稿，独立修改继续保留；页面/镜头/片段列表的结构冲突明确采用所选侧完整列表。重复 id、类型歧义仍拒绝。来源/共同设定变化或 base 不可核对时不开放选边，原草稿保持。复用原漫画/动画编辑器可按页、画格、镜头和片段浏览，预览不写入作品。快速重复操作按 base 修订复查，不能用已变更的基线再次套用旧选择。

补齐保存错误后的“核对最新保存版本”入口，读取仍保留草稿并复用三方合并；核对后远端再次保存，旧 CAS 仍拒绝，作者可以读取新版本、重新明确选择再保存。没有自动写入重试、扩大权限或采用/发布。新增面板沿用现有主题与窄屏布局。

五视角后审 PASS：架构严格区分草稿选择和 Native 保存；技术 A 覆盖冲突双方选择、独立修改、结构列表完整性、重复 id 与别名不变；技术 B 经实际 Electron 和真实 Native 保存验证；产品从只能看到冲突提示变为可完成核对与保存；UI 不暗中选边，明确列表范围和仍需保存，失败可重新读取。零兼容与 B01–B10 在本段范围通过。

证据：130合并合同6/6；真实 Native 保存1/1，明确选择保存后回读历史修订不变。130界面夹具首次未选择预览中的画格即查找属性，失败记录 `/tmp/conflict-integration130.log` 保留；131按实际浏览操作选择画格后通过，未放宽断言。132新增“核对后发生新保存→拒绝→重新核对→保存”通过；134扩展动画镜头、只读浏览及380px窄屏后完整界面通过（`/tmp/conflict-ui134.log`）。最终构建133通过；架构133为359文件/7 crates/0 diagnostics。领域测试使用实际本地 Native 与 Host，模型质量/外部生成服务不在该证据范围。

整体尚未全部完成：重开缺少 base 时的历史原版本回读、同 Edition 普通消息 dirty 写准入、小说对象级编辑隔离、模型意图与要求变更完整旅程，以及更多多 Host 组合与外部环境验收。本段不把这些范围写成已完成；未自动提交或推送本轮改动。

第三十二段最终门禁133为112/112通过（`/tmp/conflict-gate133.log`）；之后仅补动画界面验收，134通过，生产代码不变。6个生产/测试文件指纹 `/tmp/task-control-source134.json`，集合 SHA-256 `1f14f4acf4feba4a72e387f3dcfbb580c7ab57e7d4b38c711f4c54246567b173`。

## 第三十三段 PRE：重开媒体草稿回读真实原修订

Native 与 MultimediaHost 已支持按固定 revisionId 读取历史工作区，但 Renderer 的公开类型/IPC 只传 branchId。贯通当前四参数只读合同；恢复草稿的 baseRevisionId 非空且与当前保存修订不同、内存无对应 base 时，读取该精确历史修订。核对 project/Edition/Scope/branch/revision/kind 后才作为合并基线；读取失败或错身份保留草稿，不猜测或覆盖。读取期间复查会话读取代次，切换作品或保存后晚到结果不能应用。原稿读取不授予写权限，保存仍基于最新 CAS。

五视角前审 APPROVE WITH CONDITIONS：架构复用 Native 既有历史事实，无新增持久状态；技术 A 验证固定修订与错误身份、迟到响应；技术 B 用当前 IPC 与真实 Native 回读及重开编辑器验证；产品让已持久草稿能继续核对，UI 明确恢复与无法核对，不要求作者填写内部修订号。四参数类型/IPC/夹具一次性同步，无旧字段 fallback 或历史格式迁移。


### 第三十三段 POST：重开草稿按真实原修订恢复

已贯通 Renderer 类型、Electron IPC 与既有 MultimediaHost/Native 的精确历史工作区读取。草稿重开或重新核对时，按原修订读取并检查项目、Edition、Scope、分支、修订和媒介类型；独立修改可继续三方合并，冲突继续交作者核对。历史读取失败或归属不一致保留草稿，不使用当前版本假冒原基线。新增“重新核对版本”入口，无需作者填写技术标识。

重开路径同时保留未改变保存请求的原幂等标识；仅实际重基到新修订才清除旧保存尝试。真实 Electron 用例验证“保存失败→切换作品→重开→重试”仍使用原键，独立修改与选中对象不丢失。另覆盖历史响应挂起后切到另一作品，晚到响应不改变当前作品；错误 Scope 不开放合并，重新读取正确历史后可恢复。读取代次与保存操作屏障继续有效，保存仍由 Native CAS/权限控制。

五视角后审 PASS：架构复用 Native 历史事实且仅恢复 Renderer 草稿；技术 A 核对身份、读取竞争及保存幂等；技术 B 验证实际注册 IPC 回读真实 Native 历史，并从实际编辑器执行重开恢复；产品解除原基线仅存内存造成的恢复限制；UI 提供可操作重查及明确未保存状态，无自动选边或自动保存。零兼容与 B01–B10 在本段范围通过，未引入持久格式变化或第二份历史权威。

证据：135真实 Native/界面2/2（`/tmp/history-integration135.log`）；136保存失败重开幂等回归通过（`/tmp/history-ui136.log`）；138增加历史响应跨作品晚到隔离后界面通过（`/tmp/history-ui138.log`）。136最终生产构建及 preload 合同检查通过，架构359文件/7 crates/0 diagnostics；仅后续增加测试场景，生产代码未改变。统一门禁137结果另记。真实模型、跨平台和长时负载不在本段证据范围。

## 第三十四段 PRE：媒体草稿与同版本普通对话并存

前两段已具备后台版本合并、冲突选择与重开恢复。现在解除同一精确媒体 Edition/Scope/branch 的普通消息全局 dirty 阻塞；仍从 Native 冻结已保存工作区，草稿不作为已保存输入。未知编辑范围、小说编辑及同 Edition 的异分支仍保持原保护；独立 Edition 准入不变。未保存选区不能绑定到已保存版本，发送入口明确要求取消该引用或先保存，不暗中删除作者选择。Native 原权限、CAS、消息回执和 Work 预算均不变。

五视角前审 APPROVE WITH CONDITIONS：架构仅调整 Host 调度准入与 Renderer 发送条件，不授予新领域权限；技术 A 验证精确范围及无效/交叉身份拒绝；技术 B 从真实 Native 消息派发和实际媒体 App 验证；产品允许一边编辑一边围绕保存内容对话，背景更新仍经草稿合并/冲突核对；UI 明示只使用已保存版本，保留未保存选区阻塞原因。无 schema 或兼容分支。已有真实媒体保存/冲突测试继续约束写入一致性。


### 第三十四段 POST：同版本媒体草稿不再阻断普通对话

Host 对精确一致的媒体 Edition/Scope/branch 放行消息调度，继续冻结 Native 已保存版本；独立版本沿用原准入，异分支、交叉身份、未知编辑范围和小说草稿保持保护。Renderer 普通发送不再仅因媒体 dirty 被禁用，明示“对话使用已保存版本，当前编辑会保留”；未保存选区仍拒绝并提示先保存或取消引用，未静默去掉作者选区。原消息队列、权限、版本保存和幂等合同不变。

实际 App 验证新增未保存画格不能被误引用、取消引用后普通对话可发送、消息明确携带保存修订且不带草稿选区，完成后草稿仍存在。真实 Native 验证同版本两条普通消息与另一独立版本均正常派发；编辑范围未知时普通消息继续排队。独立修改合并及冲突保存由第三十一至三十三段真实编辑器/Native 用例验证。

139/140/141失败证据保留：夹具新增页面未选择画格，不会生成选区；队列 drain 每次仅派发一条，首次断言错误地要求所有消息同时结束；UI 外层总调用计数未更新。按实际对象选择和多次队列扫描修正夹具后142四项回归全部通过（`/tmp/media-concurrent142.log`），没有改变调度规则或放宽身份/版本检查。139生产构建及 preload 检查通过，架构359文件/7 crates/0 diagnostics。第三十三段统一137为112/112；新增第三十四段后最终统一143另记。

五视角后审 PASS：架构只变更草稿与消息调度边界，Native 仍为写入权威；技术 A 精确范围与选区保护通过；技术 B 实际 App、Host 和 Native 三边界通过；产品实现边编辑边对话且保留草稿；UI 区分已保存输入和未保存选择，不自动保存。零兼容检查通过。工作区同时出现其他任务的 MCP 及相关测试改动，本轮未改写或归入本段实现。

剩余范围仍包括小说对象级编辑隔离、自由文本意图与实质要求修订完整模型旅程、更多多 Host/插问/媒体组合，以及真实供应商、Windows、远端 CI、真实输入法及长程负载；不将本段完成写成整体全部完成。

## 第三十五段 PRE：小说冲突保存的编辑会话归属

检查发现 `saveLocalAsLatestCandidate` 在读取最新工作区后未复查原编辑会话。编辑器卸载或身份切换后，旧回调仍可能重设基线并调用保存。Renderer 应使用已有 editorOwner 令牌，在异步读取后及错误/清理分支复核归属；不同项目回执不得用作基线。Native CAS 与写权限不变，不增加领域状态。

五视角前审 APPROVE WITH CONDITIONS：架构归属 Renderer 操作生命周期；技术 A 先实际复现卸载后晚到读取导致保存，再验证零写入；技术 B 使用现有实际 CreativeWorkspaceView 夹具，不模拟被测函数；产品避免离开编辑器后触发未完成保存；UI 旧回执不得改变新页面忙碌/错误状态。无兼容或存储格式变化。本段仅封闭该竞争，不能称为小说对象级编辑隔离全部完成。

### 第三十五段 POST：离开小说编辑器后不再触发晚到保存

146实际 CreativeWorkspaceView 复现了“点击冲突保存→读取基线挂起→卸载→读取返回→仍调用 saveDraftCheckpoint”，原失败日志 `/tmp/conflict-lifetime-before146.log` 保留。现读取返回时同时核对原 editorOwner 与原 EditorSession，拒绝异项目回执；旧会话的错误和 finally 不再改变当前页面。复用已有编辑会话令牌，未新增持久账本、权限或兼容路径。

147原故障场景通过；148补齐“错误项目回执→零保存→仍可重试→卸载后晚到→零保存”，实际编辑器全部断言通过（`/tmp/conflict-lifetime-final148.log`），既有快照未改。147生产构建与 preload 校验通过，架构359文件/7 crates/0 diagnostics。五视角后审 PASS：Renderer 只决定仍归属当前会话的操作；错误项目不能成为保存基线；真实编辑器复现与原条件回归闭环；用户离开后的旧读取不再开启保存；本地稿不因拒绝被清空。此证据不替代小说对象级并发编辑的其余工作。

统一143实际为110/112，两项 Electron 超时（媒体对话约966秒、多对话约184秒），未出现业务断言结果，不认定具体环境根因。144按原断言复跑两个场景2/2通过（`/tmp/task-ui-recheck144.log`），未提高超时或跳过用例；统一145结果另记。第32–34段14个源码/测试文件指纹 `/tmp/task-control-source143.json`，集合 SHA-256 `376a90b94f27d22de27039876f2a1da26e5759ffa1a8243b74c777c65bd4099e`。

最终统一145为112/112通过（`/tmp/task-final145.log`，约125秒）；新增小说冲突生命周期回归148单独通过，覆盖其实际变更路径，未以统一门禁代替该新故障验证。148两文件源码指纹 `/tmp/task-control-source148.json`，集合 SHA-256 `cc6f171c17f793638c86e3339495dc5819582539ab6d0b6e2eb61be16db3c46b`。本轮未提交或推送；并行 MCP 改动保留，不冒充本轮任务控制修复。整体剩余以第三十四段及验收矩阵为准。

## 第三十六段 PRE：小说章节独立场景修改三方合并

Renderer 保留当前已加载候选的原基线，后台候选变化时只在同章节、相同章纲来源、无未保存审阅/附属编辑且场景稳定身份与顺序完全一致时，合并彼此独立的标题/场景正文修改。正文按完整场景比较，不猜测句子语义；双方同字段不同修改、插入/删除/排序、重复身份、来源变化和保护冲突保留原草稿及原 CAS。合并更新未保存草稿，采用最新场景组 id 与候选修订；不自动保存或发布。

五视角前审 APPROVE WITH CONDITIONS：架构属于 Renderer 草稿投影，Native 继续保存权威；技术 A 校验稳定 sceneId、版本内 id 更新、同字段冲突与保护状态；技术 B 纯合同和实际编辑器验证，保存保持现有 CAS；产品允许后台修改其他场景时继续本地编辑；UI 提示合并仍未保存，冲突继续现有核对入口。无 schema、持久格式或兼容改动；不解除小说普通消息全局 dirty 保护。

### 第三十六段 POST：小说独立场景合并落地

新增纯函数 `mergeChapterDraft`，以稳定 sceneId（无 sceneId 时为当前场景 id）核对三侧同序唯一身份，合并彼此独立的标题、完整场景正文和保护变化，采用最新候选的场景组 id。双方同字段不同改动、场景增删重排、重复身份和正文/保护冲突整体拒绝；返回值不与输入共享可变对象。Renderer 只在当前章节来源一致、无审阅/附属编辑或保存进行中时应用，保留最新候选与项目 CAS，明确仍未保存。来源变化时保留原稿与原基线，不自动保存或发布，也未解除小说普通消息 dirty 准入。

实际界面153验证两场独立修改、最新场景组 id 和候选/项目保存参数、章纲来源变化拒绝。154新增“保存进行中后台候选变化→草稿保持→保存失败→重新读取→独立修改合并”通过，同时原卸载后晚到保存及异项目回执拒绝继续通过。纯合同3/3；真实 Native152验证原修订旧写入409、合并后两个场景均保留、旧候选不能覆盖、正式稿仍为空。150/151夹具项目创建缺少当前必填 logline/medium，失败日志保留；152按当前完整 DTO 修正通过，没有变更生产接口。新合同和 Native 用例已加入统一任务控制门禁。

五视角后审 PASS：架构保持草稿/Native事实分离；技术 A 覆盖场景稳定身份、版本内 id、来源和保护拒绝、并发保存屏障；技术 B 纯合同、真实 Native 保存和实际编辑器三层通过；产品可保留不同场景的作者与后台修改；UI 明确未保存与冲突，没有暗中选边。154生产构建（含 preload）通过，架构360文件/7 crates/0 diagnostics；统一153结果另记。本段为本地已知基线的同序场景合并，场景结构冲突及重开草稿持久恢复仍不计为完成。

第三十六段最终统一153为116/116通过（`/tmp/chapter-gate153.log`）；保存中合并屏障新增后154实际编辑器和构建/架构通过。6文件指纹 `/tmp/task-control-source154.json`，集合 SHA-256 `bab33344d7b283ec29cbb6e57b5fbfc0f3fc205d5720948011ca5b46251122cc`。

## 第三十七段 PRE：按作者和章节持久草稿

Renderer 按 author/project/chapter 保存未保存正文、场景身份、保护状态与原候选/项目修订，仅用于本地恢复。重开时当前候选相同可继续编辑；候选已变则保留原草稿和旧 CAS，不用本地缓存冒充 Native 历史基线，不自动重基。保存内容已与 Native 当前候选相同则无需重复保存。明确放弃和成功保存清理草稿；删号沿现有作者清理路径清理自己的草稿。解析失败保留原存储并提示，不静默覆盖。

五视角前审 APPROVE WITH CONDITIONS：架构仅 Renderer 恢复副本，不新增领域事实；技术 A 作者/项目/章节隔离、重开、保存/放弃和损坏存储保护；技术 B 实际编辑器卸载重挂验证，不依赖只测试存储函数；产品关闭重开不丢正文；UI 区分恢复草稿、已保存内容与待核对修订。当前新格式无迁移或兼容入口，不恢复旧格式。作者 key 在 App 与测试调用方一次性同步。

### 第三十七段 POST：章节草稿重开恢复及作者隔离

新增 `chapter-draft-storage` 当前格式，按作者/项目/章节持久保存未保存标题、场景正文/身份/保护及原编辑版本。App 显式传入作者身份，作者切换重建编辑会话；媒体之外的章节草稿亦纳入删号时的原作者清理入口。相同候选重开恢复编辑，不制造冲突；候选已变则保留原草稿、原 CAS 并显示核对入口，不凭缓存补造历史基线。恢复零保存，成功保存及明确放弃清除记录，解析失败保持原记录并提示。

157存储合同2/2验证作者/项目/章节隔离、作者清理不触及他人、损坏记录不被改写。实际编辑器156/158验证卸载重开、保存候选变化后冲突保留、原候选重开不伪造冲突、切换作者隔离及返回恢复、成功保存清理、两种明确放弃入口、损坏存储仍保留，全部通过（`/tmp/chapter-recovery-ui158.log`）。158最终构建含 preload 检查通过，架构361文件/7 crates/0 diagnostics。新增存储合同纳入统一门禁158，结果另记。无付费模型或真实作者资料写入，夹具存储与 Native 测试库均隔离。

五视角后审 PASS：Renderer 仅本地恢复副本；Native 当前候选仍是保存权威；技术 A 编辑会话归属防止旧状态写入新 key，保存中不更新基线；技术 B 实际卸载重挂与作者切换验证；产品重开不丢未保存正文；UI 区分恢复草稿与保存成功，错误不静默删除。无旧格式兼容、迁移或第二套作品历史。小说重开后的自动历史基线读取、结构冲突细分整合及普通消息对象级准入仍未完成。

第三十七段补充：159实际编辑器加入跨项目旧响应拒绝与返回原项目恢复，通过；loadEditor 拒绝异项目工作区，项目切换重置恢复读取身份，避免旧视图写入新项目 key。最终构建159及架构通过（361文件/7 crates/0 diagnostics）。统一158尚在 Native 编译阶段，没有以等待中状态宣称通过；该编译同时包含工作区其他任务正在修改的 Native 工具契约。

第三十七段最终证据：160实际 App 任务控制、多对话、登出上下文及存储隔离5/5通过（`/tmp/chapter-app160.log`）。159八文件指纹 `/tmp/task-control-source159.json`，集合 SHA-256 `23470abf973fee1566f4c684f37feac00654d8ead1ea3c7b61ddf999cd2b6911`。统一158仍未完成：本任务 Cargo 子进程 rustc 长时间无编译进展；只读采样 `/tmp/task-native-build158.sample` 显示宏动态库加载经过 dyld Loader::mapSegments 后停在 __fcntl（采样869次同栈），主线程等待该编译线程。该证据定位等待位置，不宣称确定系统根因，未关闭系统安全机制、未杀死其他任务进程或跳过门禁。未将153旧门禁冒充恢复功能158的全量验收。编译进程保留，当前未提交或推送。本段已验证路径之外仍按矩阵保留未完成项。

## 第三十八段 PRE：章节结构变化与独立正文合并

统一158已完成118/118通过（130秒），先前动态库等待不再阻塞。Renderer 扩展稳定场景身份三方合并：单侧增删重排可与另一侧独立正文修改并存；两侧相同结构可收敛。删除/正文修改、受保护场景删除、同身份不同新增内容、两侧不一致结构继续冲突，不能静默丢稿。Native 仍拥有候选、CAS与保护权限，合并只产生未保存草稿；不改模型决策与工作预算。

五视角前审 APPROVE WITH CONDITIONS：架构为 Renderer 纯草稿运算；技术 A 覆盖增删排序、身份碰撞、保护及不变性；技术 B 实际编辑器与 Native 保存回读验证；产品保留独立结构/正文修改；UI 延续未保存提示，歧义进入原核对界面。无 schema 兼容或双轨。结构双方冲突的细分选择及历史基线回读另行完成。

### 第三十八段 POST

单侧结构变化与独立正文合并已实现，采用远端现存场景组身份，新场景保留其身份，并复核结果无重复 id。5项纯合同、162实际编辑器（插入→重排→删除仍保留本地正文）及真实 Native CAS 保存均通过；161构建与架构361文件/7 crates/0通过。161首次UI/Native在沙箱中被系统拒绝，原日志保留；162以隔离测试所需权限重跑通过，没有放宽业务断言。五视角后审 PASS，证据 `/tmp/chapter-structure-{ui,native}162.log`。双方结构不一致仍保留为冲突，不声称所有冲突自动解决。

## 第三十九段 PRE：章节历史修订读取与恢复核对

Story Core 按 project/chapter/revision 三重归属读取不可变 draft；Runtime公开只读API，Host/Preload/Renderer类型同步。Renderer 恢复时只用 Native 历史候选作为三方基线，来源一致才合并，错误与晚到保持草稿。失败可明确重新核对；不新增领域事实、写授权或自动保存。

| 状态 | 结果 | 副作用/下一步 |
| --- | --- | --- |
| 历史候选归属正确且当前章纲来源相同 | 尝试纯草稿合并 | 更新草稿和CAS，无保存 |
| 外项目/外章节/非draft/不存在修订 | 拒绝读取 | 草稿保留，可重试 |
| 历史返回前编辑、保存或切换会话 | 丢弃过期结果 | 当前草稿不变 |
| 来源变化或内容冲突 | 保留待核对 | 不静默重基 |

五视角前审 APPROVE WITH CONDITIONS：架构 Native 权威、Host传输、Renderer草稿；技术 A 三重身份和异步竞态；技术 B 实际HTTP/IPC与编辑器；产品重开可核对原稿；UI失败可重试、无内部参数输入。当前格式直接同步，无兼容路径。

第三十九段实际编辑器163通过：错章节历史拒绝后重试可合并、重开独立修改保留、卸载后晚到读取零写入；构建163通过。Native 当前构建中，HTTP三重身份及历史不变性用例已补，未先行宣称通过。

## 第四十段 PRE：章节冲突明确选边

Renderer 在可核对的原修订与相同章纲来源下提供两种冲突选择，独立标题/正文继续保留；结构冲突明确以所选侧完整场景顺序为准。保护约束不因选边绕过，无法合法整合保持冲突。选择前读最新workspace并复核编辑会话、版本与草稿身份，版本再次变化先显示最新冲突；结果仅未保存草稿，原Native CAS保存入口不变。

五视角前审 APPROVE WITH CONDITIONS：架构草稿决策无领域写入；技术 A 重复点击、读后离开、再次变更及保护边界；技术 B 纯合同/实际UI/Native保存；产品冲突可完成处理；UI明确结构选择范围与仍需保存。失败保留输入，无兼容层。

### 第三十九/四十段 POST

163 Native完整构建通过；165实际注册Host IPC→HTTP→Native验证精确历史读取、错项目/章节拒绝、后续保存不改变历史通过。164构建/preload及架构通过。165实际编辑器验证本地/服务器两种冲突选择、版本再次变化先拒绝再重新核对、历史恢复与原保存生命周期全部通过。纯合同追加了新保护场景删除拒绝，共7项通过。选边不自动保存，也不绕过保护或来源检查。五视角后审在本段范围PASS，无兼容分支。统一166仍在运行，不冒称通过。

## 第四十一段 PRE：仅章节正文草稿的普通对话准入

Renderer明确上报仅章节正文未保存的project/chapter范围；审阅、附属表单、保存中、章节结构操作仍上报未知范围并保持阻塞。Host消息准入仅放行同作品且明确聚焦章节的普通消息，冻结Native已保存输入，未保存引用仍只读；不改变Native写授权或Work恢复准入。章节原稿由本地持久恢复、历史三方合并及CAS保护，源变化/同字段冲突保留草稿。规划及未知范围保持原保护。同步当前IPC类型和全部fixture，无旧字段兼容。

五视角前审 APPROVE WITH CONDITIONS：架构Renderer仅报告草稿范围、Host仅调度、Native保存权威；技术 A 精确上下文校验及宽泛focus拒绝；技术 B 真Native消息和实际App编辑发送；产品允许围绕已保存章节继续对话；UI明示已保存输入、草稿保留。读写权限不由编辑范围授予。

### 第四十一段 POST

当前IPC统一增加可空chapterScope，严格校验项目、章节、字段及与媒体范围互斥。编辑器仅在正文dirty且无审阅/附属编辑/保存时报告范围；App允许聚焦章节的普通消息和该消息的失败重试，提示仅使用已保存内容。Host仍阻塞宽泛/未知范围，原Work恢复和章节结构操作准入未放宽，Native权限与CAS不变。

167实际App、上下文与合同5/5通过；168真实Native普通消息“未知范围排队→明确章节范围派发”及原插问/结构保护15/15通过；169实际App覆盖重开后普通发送与失败重试，未保存正文不变、写入调用为零，通过。169构建/preload与架构通过。五视角后审PASS：跨层形状同步、无兼容路径，草稿范围不授予写权限。原始日志 `/tmp/chapter-context-{ui167,native168,retry169}.log`。166统一122/122通过，但不覆盖随后第四十一段全部变化；170最终统一运行中。

第四十一段最终统一170为123/123通过（130秒），169最终构建与架构通过。该门禁包含新增章节范围派发及原媒体/崩溃/插问/消息生命周期回归；真实供应商与Windows不在该证据范围。

## 第四十二段：真实模型验收前核验开发CLI构建身份

只读检查确认当前macOS有开发供应商配置，但CLI因固定支持清单指纹不匹配不可选。按现有build:codex-runtime生成源码构建与BuildIdentity，再运行现有refresh-model-policy核验源码、二进制和受支持版本后更新开发构建引用；不关闭校验、不改供应商认证结论、不选桌面自带引擎。五视角方案结论APPROVE WITH CONDITIONS：构建脚本负责来源身份，运行时保持拒绝不匹配；验证实际resolver与现有运行时合同；产品真实模型入口需可启动；UI仍真实报告可用性。模型质量与任务旅程须另测，构建本身不证明。本段评审追加首次使用错误相对路径失败，脚本已启动，现补记该过程，不伪造先写成功的记录。


第四十二段验收调整：现有live-story探针项目创建DTO缺少当前必填logline/medium/idempotencyKey，且通过拼接推断Work身份。按当前合同一次性修正，Native回读Work真实id用于预算绑定与结算；不调整模型任务目标、不降低来源不变断言。先合同/构建身份验证，再按探针既有6请求/40000token上限运行真实只读样本。该样本只能证明只读问答及真实调用链，不能冒充全部停止/插问/要求变更验收。

### 第四十二段结果与真实验收阻断

171通过现有脚本构建CLI并生成BuildIdentity；172现有refresh-model-policy验证后更新开发指纹，resolver实际返回available=true。未关闭版本/来源/指纹校验。174本地运行时35/35通过；173沙箱首次因loopback权限失败，原日志保留，174隔离测试权限下通过。

真实模型调用首次被自动审批拒绝（173尚未发起）。用户随后明确授权“这次真实模型验收”，174实际运行返回task_token_budget_exhausted，失败日志 `/tmp/task-live-story174.log`。未扩大6请求/40000tokens作品问答预算，未重新调用真实供应商，不能记为模型验收成功。原探针没有输出阶段/用量，且finally移除临时库，故不能由该错误推断失败发生于连接资格还是作品问答，也不能推断实际费用为零。已补探针阶段、失败/成功预算快照和精确凭据脱敏；该诊断修正仅语法检查，未再次请求真实模型，不能反向补造174费用证据。

五视角后审：开发构建身份和当前DTO/Work身份修正PASS，真实模型旅程BLOCK（预算失败，待诊断完整的再次验收另获授权）。静态检查不代替该旅程。剩余范围：自由文本补充/提问/实质要求变更的模型完整旅程，更多双Host/插问组合、真实供应商取消和故障窗口，规划/审阅的对象级并发，Windows/真实IME/长时间负载及远端CI。当前仍不能称为全部完成。所有本轮代码仍未提交或推送；保留工作区其他任务修改。

174本次19个受影响代码/契约文件指纹 `/tmp/task-control-source174.json`，集合SHA-256 `263aefdd4aeb82279eb705cc2361ea4cbc7be51caace02cc04872dd4ca7e026a`。包含共享工作区文件的完整当前内容，不将他人修改归为本次实现。

175探针本地合同首次16/18，两个真实HTTP夹具仍用缺字段的项目创建DTO，返回非JSON验证错误。按第四十二段当前DTO调整范围，同步两处隔离fixture的logline/medium/idempotencyKey，不更改生产校验或断言；复验另记。用户已另行授权一次相同上限诊断验收，176启动，尚未记成功。

176真实诊断确定失败阶段为modelAnswer；作品问答账本2请求、32505tokens、reservedTokens=0、unknownTokens=0、overrun=false，maxTokens=40000。后续预留被拒绝，未完成问答；连接资格检查费用不属于该账本，仍不推断总费用。未第三次调用。178本地探针17/18，提取夹具已按当前分页source合同完整读取；剩余改稿探针缺少当前强制chapterAcceptance。补齐与该探针既有明确虚构作者要求对应的标准（藏信、主动证词、保护其他场景，无篇幅要求），不绕过Native校验、不新增真实作者需求。该探针本就脚本化测试作者授权，不作为模型自发理解证据。

探针本地合同190最终18/18通过。修正点是当前必填DTO、分页原文读取、章节标准与局部目标分开、同Work决策及原对话绑定、Native实际批次、原要求完整回读、结构化证据错误。测试不再以直接创建后继任务替代模型选择，也不从错误自然语言解析分类；候选报告仍通过实际HTTP保存并核验锚点/幂等/来源不变。总结只投影两个局部场景目标，明确partial；章级标准由Native报告完整校验，未声称全章交付。

诊断显示40000tokens预算在两次请求后不足以预留后续请求。准备显式 `--max-tokens=80000` 参数供下一次授权，默认仍40000且仍须--confirm-cost；正安全整数、重复/未知参数和授权标记分离已由191合同2/2验证。没有自动增加现有账本或自动调用供应商。该调整属于探针输入与预算传输，Native仍为预算权威，模型/作品权限不变。已向用户请求独立授权；答复前不执行该付费命令。

### 192：已授权 80000 tokens 验收及探针合同修复前审

用户明确授权一次最多6请求/80000tokens验收，实际已执行且未重试。`/tmp/task-live-story192.log`：modelAnswer 阶段 invalid_tool_arguments，read_context /additionalProperties；实际作品问答账本 requests=1、tokens=14167、reservedTokens=0、unknownTokens=0、overrun=false。连接资格费用不包含在此账本内。未到只读结果验证阶段，不记录为成功。

定位：探针提示仍向模型提供 projectId/taskId/threadId 和 purpose；当前 MCP model_tool 移除三个可信身份字段，purpose 由 operation 推导。修复归 scripts 验收调用方，Native 仍是身份/权限权威；只传 expectedRevision、budget、byteBudget，删除旧身份提示，无兼容分支。模型继续选择需要阅读的来源与回答。五视角前审 APPROVE WITH CONDITIONS：架构保持 B04/B05 可信绑定；技术A保留严格schema和预算；技术B补提示合同回归并运行现有探针测试；产品保留失败证据不冒充完成；UI无变化，结果如实报告。真实再验需新的单次费用授权，本次不会自动重试。

193后审：已删除探针对模型身份和purpose的旧参数要求，保留当前expectedRevision、读取额度及只读约束，并明确目录返回后按需读取来源。探针合同3/3通过（`/tmp/task-probe-options193.log`），架构检查361文件/7crates/0问题（`/tmp/task-architecture193.log`）。五视角对该调用方修复通过；零兼容PASS，权限/schema未放宽。回归证明提示参数不再注入可信身份，不证明真实供应商问答成功；192失败证据原样保留，尚未再次付费运行，完整产品验收仍未完成。本次修改未提交或推送。

### 194：继续验证及结束报告修复前审

用户“继续验证”授权后，仅执行一次原6请求/80000tokens隔离验收。`/tmp/task-live-story194.log` 返回 verifyReadOnlyResult / persistent budget is closed / accounting=null。检查顺序证明此前作品 sourceDigest/headSnapshotId 不变、存在MCP读取、回答含虚构作品关键词的断言均已通过；但回答原文和费用未输出，不能补造。根因是探针在 budget.complete() 关闭持久预算后调用 budget.snapshot()，报告失败；临时库已清理，无法恢复该次用量。

修复前审：scripts负责验收输出，Engine仍是费用事实权威。终态用公开PersistentTaskBudget.receipt读取，不从已关闭实例读取；细分任务完成、预算完成、用量结算阶段并保留已核验结果。五视角 APPROVE WITH CONDITIONS：架构B03/B05保持事实权威；技术A以真实本地持久预算测试终态及未知用量；技术B测试相同结束路径，不发模型请求；产品不把报告故障误报模型失败或零费用；UI无变更。当前单次授权已用，不自动重跑。无兼容或权限变更。

195后审：完成预算后改读公开持久回执；错误保留 verifiedResult，并细分 completeTask/completeBudget/settleUsage/reportResult。真实本地PersistentTaskBudget回归覆盖已知17tokens以及未结算50tokens转unknown两种终态，证明关闭实例后可读真实回执且未知用量不会被伪装零消耗。探针4/4通过（`/tmp/task-probe-options195.log`）；架构361文件/7crates/0诊断（`/tmp/task-architecture195.log`）。五视角修复后审PASS、零兼容PASS；194已通过的只读行为检查与报告失败分别记录，没有将旧失败改写成功。未重复付费调用、未提交或推送。完整真实验收报告仍未取得。

### 196：修复报告后的真实复验

用户再次“继续验证”后，按相同范围只执行一次6请求/80000tokens隔离DeepSeek验收。原始日志 `/tmp/task-live-story196.log`，进程exit=1。结果为 modelAnswer / task_token_budget_exhausted；作品问答账本 requests=4、tokens=72657、reservedTokens=0、unknownTokens=0、overrun=false，剩余7343tokens不足以继续请求预留。连接资格费用仍不计入该账本。verifiedResult=null，尚未进入最终只读行为检查；没有发生上次的closed-budget报告错误，但未走到成功终态，不能据此宣称成功报告路径已完成真实验证。

本次没有扩大额度或自动重跑。194的基本只读检查曾通过与196预算失败均保留：前者不证明当前预算下稳定完成，后者不证明作品发生变更。现有证据说明此只读问答尚不能在6请求/80000tokens内稳定验收；后续应核对实际每轮上下文、工具读取及请求用量再优化，不用继续扩大预算替代定位。未提交或推送。

### 197前审：真实逐轮诊断

用户明确要求直接用模型真实验证。保持6请求/80000tokens，仅一次；探针接入Engine现有requestObserver/responseObserver，记录请求关联、工具数量、输入项数量、工具名称及各观察时点的累计预算快照，不记录凭据/正文，不修改Engine协议/预算/模型选择。观察时点快照不冒充单次请求结算回执。五视角APPROVE：架构遵循B04/B05公开观察接口；技术A有界元数据；技术B不改变调用语义；产品保留失败诊断；UI不受影响。结果另记，失败不自动重跑。

### 197后审：真实只读问答验收通过

本次按用户明确要求仅执行一次真实DeepSeek隔离验收，进程exit=0、status=passed。证据：`design-reviews/evidence/2026-10-10-task-readonly-live197.json`（原日志`/tmp/task-live-story197.log`）。账本completed，4请求/71665tokens，maxRequests=6/maxTokens=80000、unknownTokens=0、overrun=false；连接资格费用不包含在作品问答账本内。5次MCP读取，sourceDigest/headSnapshotId不变，实际回答明确仅有候选梗概、无已成正文。终态回执和报告均实际通过。

逐轮观察：每轮16工具声明，输入项5→8→16→20；工具调用依次为read_context、3次read_source_content、1次read_source_content、最终回答。相邻已结算累计快照差值为14045、18302、19323、19995tokens。说明总用量是多轮请求累计，不是最终470字符回答的长度；没有逐成分tokens证据，不能将精确占比归给工具或系统提示。196未完成与197完成同时保留，单次通过不证明稳定成功率。

探针合同4/4、架构361文件/7crates/0诊断（`/tmp/task-probe197.log`、`/tmp/task-architecture197.log`）。五视角后审：公开观察接口无正文/凭据记录，预算、模型自主选择、Native权限不变；报告与费用事实分开，零兼容PASS。本次仅证明隔离只读问答，不证明完整任务控制、多平台、取消/故障或所有语义旅程已完成。未提交或推送。

### 198：完整任务体验理想验收

按用户明确选择执行本地全链路并汇总真实模型缺口；本轮无付费模型请求。报告 `design-reviews/2026-10-10-task-experience-acceptance.md`，结果索引 `design-reviews/evidence/2026-10-10-task-ideal198-results.json`。当前Native构建成功后任务控制123/123通过；独立无Native重编译子集99/99（与全集重复，不相加）。创作旅程73/91、权限预算47/53、生产桌面0/5；启动单独复验5/11。新Native补验修订链通过，候选交付goalStatus空值失败仍复现。前端构建及架构通过，Fork门禁3个上游路径未登记。

结论BLOCK：离线已保存结果恢复被模型可用性提前阻断、启动持久化/恢复夹具及实际桌面初始化未闭环、交付与规划审阅归属路径失败，详见报告。真实供应商目前仅197只读问答通过，不足以证明完整任务体验；Windows/IME/持续负载/多Host组合等仍缺证据。本次未更改产品实现、断言或权限，未提交推送。

### 199—214：198本地验收阻断修复完成

详见 `design-reviews/2026-10-10-task-experience-repair.md`，最终证据 `design-reviews/evidence/2026-10-10-task-repair214-results.json`。Host恢复入口不再为本地已保存结果准备模型域；启动持久化RPC期限及晚到进程屏障补齐；Renderer成功加载后不再重复加载。当前执行域、项目DTO、可信MCP参数和抽取后的IPC模块夹具一次性同步；桌面通过隔离安装OS启动包装器验证延迟/失败/重试。Fork按实际代码及Core测试核对登记，没有豁免。

最终任务控制123/123、创作旅程93/93、权限预算53/53、生产桌面5/5；Core bridge1/1、tools110/110、构建和repo:check通过。原始失败均保留；五视角后审及零兼容PASS。只关闭198本地阻断，不宣称真实供应商完整旅程、Windows/IME/长时负载等全部完成。无付费模型、提交或推送。

### 2026-10-10：真实验收后局部修复 218–220

修复已完成 Work 的插问关闭误受未来额度阻断；未完成父任务保持预算门禁与事务回滚。真实探针补正式 generation handoff、场景剩余额度、后台重试/费用终态观察、明确暂停收尾和隔离目录标记。Native/Host/脚本回归通过，无新增付费调用；真实供应商完整旅程仍待复测。详见 [修复评审与证据](../../design-reviews/2026-10-10-task-live-followup.md)。

### 221–222：真实任务复测（2026-10-10）

用户授权三个场景各 30 请求、token 不限。实际使用 66 请求，已确认 2,492,261 tokens，未知预留 85,145，资格检查另计。新候选 95 字已保存但独立核验未完成；插问 returned 但父改写目标丢失；改要求 delivered 后后台已自动续接，但工具纠错未产出，150 秒探针观察超时主动暂停。完整体验仍未通过。修复探针跨项目章节 ID 和后台等待窗口（600 秒），未新增付费复跑、未提交推送。证据、原始失败、范围和下一步责任见 [复测报告](../../design-reviews/2026-10-10-task-live221.md)。

### 223–225：插问期间父任务动作保留（2026-10-10）

修复 221 真实验收暴露的登记/派发混淆：Native 允许原父执行保存后续动作，插问关闭前仍禁止新 attempt；临时问题不作为业务动作依赖，跳过也不丢原目标。续接任务从真实绑定查 Work，保留原预算和暂停约束。MCP 短说明补齐章节改写的章节号要求。Native 插问 9、暂停 12、绑定 14、交付 5 项，JS 42 项、生产 Host/Native 2 项通过，构建与架构检查通过；未付费复验、未提交推送。见[评审和证据](../../design-reviews/2026-10-10-interlude-action-continuation.md)。完整真实创作体验仍未宣告通过。

### 226–235：持续真实复验与根因修复（2026-10-10，进行中）

用户授权每轮每项最多30请求、持续多轮、token总量不限。修复同Host后台claim借用并释放前台租约、发送前失败缺Engine结算、插问期间候选被提前派发、失败普通问答缺同Work恢复及恢复提示阶段混淆。Host核验方式说明与Native已存在的direct/independent合同一致，未降低验收条件。

231插问场景真实通过：27请求，1,042,364已知tokens、未知0；自动回答、返回原Work、执行改写、保存96字候选、领域fulfilled且无待结算，无人工额外重试/修稿/采用。仅这一场景通过，原始报告保留，人工判定单独记录。233生成及改要求各耗尽30请求仍未交付：分别1,186,941及1,229,134已知tokens，改要求另有39,550未知预留，不能当真实费用或零用量。

后续修复：Engine无token总额上限允许新的请求但保留旧未知用量，旧未知ID不能重放（18测试通过）；Native识别当前完整relevantText与workspace来源的等价全文读取证据，避免重复读正文，正在验证。详见[持续复验评审](../../design-reviews/2026-10-10-task-live-loop.md)及其evidence目录。尚未宣告整体完成，未提交推送。

236新增真实通过：改要求恢复21请求、743,325已知tokens，36,551未知预留保留。追加“必须保留密函”已送达；同Work自动完成94字新候选、直接核验和fulfilled，无人工重试/修稿/额外批准，未采用发布；原始失败观察和后台旧claim冲突保留。生成项当前全文证据已修复并核验通过，最终整体自检仍把可选后续建议填为阻塞作者决定而失败；237只澄清schema语义，尚待复验。见 `design-reviews/evidence/2026-10-10-task-live236-verdict.json`。

238补充真实复验：插问21请求/808074已知tokens/未知0，改要求28请求/937015已知tokens/未知预留36289，均再次Native fulfilled。插问最终回复实际回答主人公林夕，自动回到改写并保存99字雨夜钟声候选；改要求保存107字明确不烧信候选。未采用、未发布。生成项240/242/244仍未通过：分别原要求漏读、冗余分页、最终自检参数层级错误耗尽30请求，失败记录完整保留。当前修复已覆盖当前任务全文来源、完成阶段指南和紧凑分页，245继续修正核验指南与schema的一致性及最终review字段位置，未宣告全部真实验收完成。

249—252：候选CAS从模型必填改为Runtime可信任务绑定（本任务冻结值/自身最新输出，外部头不跟随）；原模型请求哈希保持不变，省略机械字段的重启恢复只承认精确ledger，不从现状推断历史未知。MCP116、绑定HTTP2、恢复8、Core1、真实重启2项通过。250真实插问再次完整通过（20请求、749445已知tokens、未知0），生成首次省略CAS成功保存但部分稿回执错误引导完成，改要求误把明确替换视为冲突，均不计通过。251修复保存完整度回执和明确完成错误，252明确同作者修改优先级；本地验证及下一轮真实复验进行中。原始失败与人工判定分开保留，未提交推送。

### 253：根因修复后真实目标旅程闭环（2026-10-10）

249把候选版本机械参数交给Runtime可信任务绑定，保留显式断言、外部并发拒绝、原始调用哈希与未知恢复；251分开“已保存”和“可完成”，从回执对应的真实版本传递完整度，部分稿不再被提示完成任务，也不会丢失；252明确作者后续修改替换对应旧要求，其余原目标继续，不扩大范围或抹去历史原话。Native并发/重放/恢复、MCP117项、Core/HTTP语义、Host7项、生产提示链4项、构建与架构检查通过。

最新真实结果：生成30请求、99字，改要求24请求、110字，均实际保存、核验、Native fulfilled和最终回复完成；插问250为20请求、97字，已回答并自动回到原任务完成。未采用发布，无额外补额度。改要求保留38,432未知token预留，不伪造为零费用。失败原始记录、启动延迟、源码/二进制哈希和独立判定全部保留。详见 [持续修复后审](../../design-reviews/2026-10-10-task-live-loop.md) 与 `design-reviews/evidence/2026-10-10-task-live253-verdict.json`。本次完成三条已约定真实短篇任务旅程及相关根因合同修复，不等于所有平台、视觉交互与长时稳定性均已验收。未提交、推送。
