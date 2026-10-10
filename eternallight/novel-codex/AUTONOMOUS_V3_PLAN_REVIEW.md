# v3 作者约束与可变工作计划实施评审

2026-10-07。对应 SIMPLE_REQUEST_AUTONOMOUS_CREATION_DESIGN.md v3.0；本文件由计划实现者独占维护。

## 当前默认 edition 外键下 outbox fixture 同步的实施前评审

统一验收实际 Core 结果为 234 通过、1 失败：`modeling_outbox::tests::deletion_and_recreation_cannot_restore_an_old_cursor_watermark` 的测试末尾直接删除 project；当前 `create_project` 已创建默认 canon/novel scope、edition 与 scope branch，复合外键默认禁止删除被引用的 scope。未发现生产 project-delete 入口；不能为此调整活跃多媒体 schema 或新增删除能力。

最小范围仅当前 fixture：在同一事务内精确清理该 project 的 `story_scope_branches_v1`、`story_editions_v1`，再执行原 project 删除，外键继续启用。保留 source NULL→source 后旧 cursor 失效的原验证；不存在项目时明确断言 `ProjectNotFound`，不把任意存储错误当通过。仅定向该 unit，完整 Core 由主代理下一统一入口覆盖。

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 测试遵守当前领域子记录归属；无新生产删除、schema 或兼容路径 |
| 技术 A | APPROVE WITH CONDITIONS | 同 project 精确清理、事务提交、foreign_keys 保持启用；不访问实际作品库 |
| 技术 B | APPROVE WITH CONDITIONS | 旧 cursor 失效断言保留；明确不存在项目错误；定向单测记录真实结果 |
| 产品 | APPROVE | 不因修 fixture 放宽作品/媒体/任务保护，不误报全 Core 已通过 |
| UI/UX | APPROVE | 无新 UI、确认或绕过路径；不把测试清理当用户删除功能 |

以上为实施者五视角自审。零兼容门禁 PASS；只能同步当前测试及本评审记录，不修改另一活跃线程的媒体 schema。

### outbox 当前 fixture 同步的实施后复核

五视角实施者复核 APPROVE：架构/产品上仅当前内存 fixture 的 teardown，不创建生产项目删除能力；技术 A 在事务中按该 project 的 scope branch→edition→project 清理，未关闭外键；技术 B 保留 source 删除/重建后的旧 cursor 拒绝，并收紧末项为明确 `ProjectNotFound`，没有删除或放宽原安全断言；UI/UX 无流程变更，单测通过不宣称完整 Core 或真实创作验收通过。

- 精确单测 **1/1** 通过，234 项 filtered：`/tmp/autonomous-v3-outbox-current-schema-final.log`；真实内存 SQLite、当前 schema、`require_escalated` 与统一 Native 验收相同环境。编译 28.22 秒，实际测试体 0.03 秒；新 binary 首启期间的只读采样保留于 `/tmp/autonomous-v3-outbox-unit-startup-sample.txt`，显示系统 dyld 入口等待，未伪称测试体在执行。
- 修改仅 `native/story-core/src/modeling_outbox_tests.rs` 与本评审；scoped rustfmt、`git diff --check` 通过。未修改多媒体 schema、生产代码、实际作品 DB、模型或网络。
- 当前修复已冻结；完整 Core/Native 验收交主代理下一统一入口覆盖，不将这项单测写成 235/235。

零兼容复核 PASS；无 fallback、外键绕过、升级或历史数据适配。

## 统一验收阅读恢复失败的实施前评审

当前完整 Native 验收在 `reading_checkpoint_tests` 发现真实失败：同 Step 的只读阅读恢复使用新 Task/thread，已成功重绑游标与当前能力，但结束讨论时 `root_review_required` 无条件借用 `model_work_plan::work_for` 的原根对话检查。Work 没有登记计划、执行或最终交付义务，仍因线程不同报 `TaskBindingInvalid`。

最小修复只调整“是否存在作者交付义务”的读取：按实际当前 Attempt→Step→同工程 Work 查 ledger，不以计划工具的根对话权限替代领域归属。真正需要最终交付时继续走原 `work_for`、完整原作者来源读取、独立报告和真实保存证据门禁。不会开放跨对话计划操作、写入或执行；仅阅读已实际批准接回的 Step 可以结束讨论。保留所有原阅读恢复、有限预算、來源、幂等、拒绝旧能力和无小说产物断言；补有真实已登记交付义务时新线程不得完成的负例。

| 视角 | 结论 | 验收条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 只读义务查询与原根对话方法权限分开；不得绕过实际交付门禁 |
| 技术 A | APPROVE WITH CONDITIONS | 实际同工程、当前 Attempt/Step 绑定；旧能力及非当前 Attempt 仍拒绝 |
| 技术 B | APPROVE WITH CONDITIONS | 当前契约回归保留重启恢复、累计阅读预算、来源变更、CAS/事务；真实计划义务负例 |
| 产品 | APPROVE WITH CONDITIONS | 单纯讨论无需登记计划或伪造产物才可结束；真实创作义务不能因此消失 |
| UI/UX | APPROVE WITH CONDITIONS | 无新增 UI/确认流程；阅读完成仍不宣称理解或创作已验收 |

以上为实施者五视角自审。零兼容门禁 PASS：无 schema 改动、兼容分支或历史 fallback；仅当前 schema 临时数据库及对应原生测试。

### 阅读恢复修复的实施后复核

五视角实施者复核 APPROVE：架构上只调整真实义务的只读查询，计划及最终交付写权限不变；技术 A 保留当前 Attempt/同工程绑定和旧能力拒绝，真实计划义务的新线程完成拒绝且状态不变；技术 B 保留原七项恢复、来源、预算、事务、CAS 和无小说输出检查，并加一项权限负例；产品上讨论恢复无需造计划/产物，真实作者义务不会消失；UI/UX 无新增流程，`discussion_ended` 与验收仍分层。

- 修改仅 `work_decision_status::root_review_required` 的归属查询及当前阅读 fixture；不改 schema、不放宽 `model_work_plan::work_for` 或最终 review/原作者来源/成果门禁。
- `reading_checkpoint_tests` **8/8** 通过：`/tmp/autonomous-v3-reading-resume-escalated-final.log`，原 7 项全部保留并通过；新增 `new_thread_reading_resume_does_not_inherit_root_plan_or_final_delivery_authority` 验证实际计划登记的义务不能跨线程结清，拒绝无状态/产物变更。
- 默认 sandbox 首轮编译成功但新测试程序在系统 `_dyld_start+0` 停留，尚未执行测试体；按批准只结束本代理精确 PID Cargo/test，再以统一验收相同 `require_escalated` 环境运行同一二进制，fresh 构建 0.07 秒、实际测试体 0.73 秒。原启动证据保留于 `/tmp/autonomous-v3-reading-resume-final.log` 与 `/tmp/autonomous-v3-reading-resume-startup-sample.txt`，不将该环境等待算测试失败或通过。
- scoped rustfmt 与 `git diff --check` 通过。全 Native 统一验收由主代理重跑；这些临时数据库测试不证明真实 DeepSeek 语义或前100章创作完成。

零兼容复核 PASS；无生产应用、实际作品数据库、网络或模型请求操作。

## 变更概览

- 目标：把模型计划改为可纠错的近期方法窗口；简单行动无需创建计划；撤销、合并、替换保留不可变版本历史。
- 权威边界：原作者消息及 Work 归属、执行范围、保护、版本、资源预算、已登记行动和必要独立核验仍由现有领域记录校验。计划中的 acceptance 是模型对子步骤的解释，不是作者合同；改计划不得修改这些记录或关闭已登记执行。
- 实施：允许 0–12 项近期目标整体替换，允许依赖和目标类型调整；保留环检测、输出/引文真实性和幂等/CAS。计划没有 activeGoal 或尚有方法待办不阻止已授权简单行动；方法待办不决定 canFinish。已声明完成的证据仍须真实有效。
- 历史：保存当前计划新版本；新增按精确版本回读方法，历史内容仅作参考。当前计划窗口有界，连续滚动不累计一千项目标。
- 非目标：计划层不能保证作者自然语言语义被完整理解，不允许它创建或扩大授权；不改真实小说、真实数据库或模型连接。作者合同及解释更正由对应领域模块负责。
- 影响：model_work_plan、work_decision_status、model_work_plan_tool 及相关测试；Runtime GET 的 planVersion 接线由主代理负责。无新增数据库 schema。

## 零兼容门禁

- PASS：当前接口一次同步；没有旧格式 fallback、迁移、回填、双写或版本探测。
- 保留不可变作品/计划版本是当前审计和一致性，不是旧接口兼容。
- 测试使用内存及临时目录中的当前 schema；不访问作品数据库。

## 五视角实施前评审

以下是实现者按五个视角自审，不代表五名独立专家。

| 视角 | 结论 | 验收条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 计划为方法记录，作者事实只有领域存储一个权威；计划编辑不删除已登记要求或改变范围/预算 |
| 技术 A | APPROVE WITH CONDITIONS | CAS、幂等、事务回滚、证据来源、只读身份、未核验成果及未知写入仍有回归 |
| 技术 B | APPROVE WITH CONDITIONS | 当前 MCP/Runtime 接口同步；删除旧冻结断言、改用滚动/撤销/直接行动/历史回读的真实仓储测试 |
| 产品 | APPROVE WITH CONDITIONS | 可取消无价值方法，不为简单请求填计划；未来计划与执行分开表达，不把改模式当真实暂停或完成 |
| UI/UX | APPROVE WITH CONDITIONS | 提供 modelMethodsOnly 权威标识、历史参考状态和清晰工具说明，不增加必填 UI 或作者确认 |

## 综合决策

可实施。P0：移除计划门禁时不得移除 Native scope、预算、作者来源、必要评估及成果校验；保存 planningOnly 不得把已登记行动标为结束。P1：版本回读和 MCP/Runtime 接线必须同步，千章只能滚动展开当前方法窗口。不存在授权兼容例外。

## 实施后复核

计划实现的五视角复核为 APPROVE WITH CONDITIONS：方法窗口可纠错和历史回读已经实现；CAS、幂等、无效引文及旧计划证据仍会拒绝。删除方法或改为 planningOnly 不会删除已登记的 requestedOutput、当前稿 revisionOutcome 或续写交接义务；1000 次小窗口更新没有累计 1000 个待办目标。简单只读答复、候选行动及范围提案无需重计划。

- 当前 Repository 回归 15/15 通过：`/tmp/autonomous-v3-plan-repository.log`。
- 完整 Core lib 首轮 220 通过、5 个 work_delivery_review 旧 fixture 因新作者原话全文读取 gate 返回 TaskBindingInvalid；已交来源实现者同步 fixture：`/tmp/autonomous-v3-plan-core-unit.log`。
- 完整 MCP 首轮 36 通过、34 失败。其中计划工具 schema 测试通过；1 个 registry 数量断言 40/44 由主代理同步，其余本地 HTTP fixture 因 sandbox 无法监听 TCP 而 PermissionDenied：`/tmp/autonomous-v3-plan-mcp.log`。这些不是已通过的全套测试。
- 原作者语义是否完整理解、独立评估质量及真实应用创作闭环仍须对应领域测试和实机验收。本模块不提供自然语言语义保证。

## 对话投影补充实施前评审

审计现有界面已具备：按实际 attempt/turn 关联，只显示一次原作者要求；公开模型说明保留顺序，内部操作按需折叠；正式保存只认 Native checkpoint/artifact 回执；成果保存、部分交付、需要作者决定和已采用状态在各领域入口区分。缺口是新隔离候选工具尚未投影，子任务完成文案容易混淆成果交付。

最小改动：在既有轻量操作行中根据 `state=isolated`、`writeState=committed`、`manuscriptChanged=false` 和 candidateId 展示“候选稿已保存，待整合”；正式保存仍只认正常 checkpoint 回执。候选目录只显示目录，内容读取核对请求 candidateId 和实际分页响应。协作任务完成显示“子任务已返回”，空摘要明确仅表示子任务结束。补作者要求来源标签，不添加新卡片、流程或模型文本语义判断。

截图复核发现已投影协作状态的行同时重复显示“委派/等待子 Agent”操作摘要。由现有 ConversationTimeline 局部排除带 collaborators 的成功路由行；未知子身份和失败仍可见。终态只有协作结果时仍保留协作列表，诊断入口保留原始操作。

额外 App fixture 发现旧断言把历史失败的 `.transcript-error` 当成当前失败。审阅实际生产转场：submitPrompt 创建新的 run/turn，原失败只进不可变 history；成功答复后的工作区读取 catch 只设置同步提示，付费重试入口须当前 run.state=error。回归应绑定 `.agent-transcript` 的当前 turn 及精确 `ui-thread/turn-2/turn-3`，同时证明历史失败仍可查、当前成功/等待状态及调用数保持正确，不能简单删除错误断言。

| 视角 | 结论 | 验收条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 只投影领域回执，候选分支不进入正式成果或父任务交付判定 |
| 技术 A | APPROVE WITH CONDITIONS | 隔离回执、错误/缺失回执、候选目录与内容分页均有负例；正常 checkpoint 保存不回退 |
| 技术 B | APPROVE WITH CONDITIONS | 同步 operation outcome 类型、摘要和展开行状态、持久快照及 Electron fixture |
| 产品 | APPROVE WITH CONDITIONS | 子任务已返回不声称小说目标完成；作者要求仍只展示一次 |
| UI/UX | APPROVE WITH CONDITIONS | 沿用现有简洁折叠行，300px 可读；摘要与展开一致，不用技术字段解释用户状态 |

零兼容门禁 PASS：仅当前 Native 回执与当前 UI 类型，无旧格式 fallback、迁移或双写。

## 对话投影实施后复核

以下五视角实施者自审，结论 APPROVE WITH CONDITIONS；条件是本次自动化界面验证不替代真实模型语义评估或实际应用创作验收。

| 视角 | 结论与证据 |
| --- | --- |
| 架构 | PASS：隔离候选与正式 checkpoint 为两个不同 outcome；路由去重只改变显示，未改模型决策、授权、领域成果或交付 ledger |
| 技术 A | PASS：缺失/错误/运行中/非隔离回执不确认候选保存；候选目录不算读取，分页仅绑定本 candidateId；正式父 checkpoint 仍确认正文保存 |
| 技术 B | PASS：事件增量完成与持久快照保留 isolatedCandidate，不包含候选正文；来源身份与 cursor 区分重试恢复；类型检查通过 |
| 产品 | PASS：原作者要求一次展示；子报告、候选保存待整合、正文保存、部分交付/作者决定、规划采用分层；历史失败不是当前模型失败 |
| UI/UX | PASS：既有轻量操作行与协作列表，无新卡片；成功路由不重复，active/失败/未知身份可查看；300px、键盘展开、折叠偏好及阅读位置验证保持通过 |

验证记录：

- Node 34/34：`/tmp/autonomous-v3-ui-value.log`（新隔离候选回执、原作者來源、事件/快照，以及既有语义操作和 Work 身份）。
- `tsc --noEmit` 成功：`/tmp/autonomous-v3-ui-types.log`。
- 最终协作/过程 Electron 2/2：`/tmp/autonomous-v3-ui-final-render.log`。子任务报告不丢；成功 routing 去重，active/error/未知身份保留；候选与正式稿在300px下区分。
- 对话身份与阅读位置 Electron 已通过：`/tmp/autonomous-v3-ui-render.log` 中 conversation-presentation。该初次组合中的协作失败已在 final-render 纠正并复测；不把初次失败记录当整体通过。
- Work 的 partial/needsAuthorDecision/fulfilled、原作者去重、持久回复与项目对话 fixture 已通过：`/tmp/autonomous-v3-ui-delivery.log`。其中 submission 旧全局错误断言最初失败，现已由精确当前 turn 与历史 identity 的完整 fixture 修正并通过：`/tmp/autonomous-v3-ui-submission.log`。没有修改 App 的模型重试或工作区读取生产逻辑，也未改 snapshot 基准。
- 规划页保存源/采用决定的既有 Electron 1/1：`/tmp/autonomous-v3-ui-planning.log`。
- 截图复核：`/tmp/autonomous-v3-ui-snapshots/collaboration-progress.png`、`conversation-codex-flow-narrow.png`；轻量层级及窄栏无横向溢出。

零兼容复核 PASS：未新增 fallback、迁移、双写或历史格式适配。测试均用临时 HTML、内存 mock 或当前 schema 临时数据库；没有实际模型请求，也未操作真实应用或作品数据。

## 方法复用与效果记录补充实施前评审

当前方法草案只有创建它的 Task 能读取；同一 Work 的后续决策和写作 Task 无法接回。观察关联仅接受 goal/strength，不能保存方法试用的实际效果或失败。最小实现沿 Native Work→Step→Attempt 关系提供方法及其观察的按需分页，不接受模型传入的 Work 身份，不改变 scope=task 或 proposed 状态。普通任务偏好仍只限原 Task，其他 Work/作品不共享；模型不能确认、发布或把方法效果提升为作品偏好。

observation.relatesTo 可引用当前任务有权读取且未撤回的方法；效果继续要求真实、唯一逐字正文引文及 current 锚点，supported/unmet/insufficientEvidence 表达有限判断，不形成质量认证。已有 CAS、幂等、记录/分页上限、来源变化标识和作者决定边界不变。UI 沿用原关联选择和详情，不增加流程。

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 只使用 Native 同 Work 关系；不新增方法库、调度器、全局启用或权限 |
| 技术 A | APPROVE WITH CONDITIONS | 同 Work 续接/工作者可读，跨 Work/作品及伪造 target 不可读；效果需真实来源，失败无写入 |
| 技术 B | APPROVE WITH CONDITIONS | Native、MCP、模型说明和 UI 一次同步；分页恢复及幂等/CAS有实际回归 |
| 产品 | APPROVE WITH CONDITIONS | 方法可复用，成功与无效都可记录；记录不表示方法普遍有效或作者接受 |
| UI/UX | APPROVE WITH CONDITIONS | 关联方法直接展示可读名称；方法详情不误称试写，保留草案和证据状态 |

零兼容门禁 PASS：不改 schema，不新增旧格式或行为 fallback。上述为实施者五视角自审，不代表五名独立专家。验收仅当前接口与临时 fixture，不替代真实模型方法收益评测。

## 方法复用与效果记录实施后复核

五视角实施者复核为 APPROVE WITH CONDITIONS：条件是当前自动化回归不替代真实模型对方法的选择、效果或成本评测。

| 视角 | 结论与证据 |
| --- | --- |
| 架构 | PASS：复用由 Native Work→Step→Attempt 关系判断，方法及方法观察保留原 scope、authorKind 和 proposed 状态；其他任务/作品不共享；没有方法库、自动启用或新权限 |
| 技术 A | PASS：真实续接决策与写作者读取原方法；相同 thread、伪造 target、其他 Work 和作品不能获得模型方法；普通 task 偏好不随方法扩散 |
| 技术 B | PASS：唯一正文引文、current/reference、真实关联、CAS、幂等和拒绝无写入保持；失败与证据不足观察在同 Work 的后续 Task 可分页回读；MCP/Host 说明一次同步 |
| 产品 | PASS：方法可以零确认按需参考；成功与失败是局部证据判断，不自动确认、采用或发布；撤回仍保留真实历史 |
| UI/UX | PASS：沿原控件关联方法，已保存观察显示方法名称和引文；详情明确“查看方法”及适用问题，模型草案显示“效果未验证”，不误示必须先作者确认 |

验证：

- Native quality_notebook 单元回归 **16/16**：`/tmp/autonomous-v3-method-core.log`。新增真实 Work 续接与 writer 来源测试、分页重复读取、ordinary task 偏好隔离、局部失败/不足证据保留、跨 Work/伪造关系/空证据/虚构引文/自动提升/撤回负例。测试只用内存数据库。
- MCP quality_tools **4/4**：`/tmp/autonomous-v3-method-mcp.log`。首轮被并行角色目录的 String/&str 编译错误阻断，由该模块实现者修正后续跑通过；本地 test binary 启动曾停滞，最终正常完成。直接诊断进程的30秒超时不计为通过。
- Node quality notebook **5/5**：`/tmp/autonomous-v3-method-node.log`。说明覆盖同 Work 按需参考及方法失败关联，既有隔离阅读边界保持。
- Electron quality notebook **1/1**：`/tmp/autonomous-v3-method-ui.log`。新增方法草案状态/适用条件/可读关联名称、实际保存带来源的未达成观察、原有确认/撤回/试写/匿名反馈行为均验证。
- `tsc --noEmit` 成功：`/tmp/autonomous-v3-method-types.log`；所改文件 `git diff --check` 成功。
- 视觉复核：`/tmp/autonomous-v3-method-quality-ui.png`。仅独立临时窗口与内存 mock，沿用原布局，无横向溢出；未操作实际小说应用。

零兼容复核 PASS：未改 schema，未新增旧格式 fallback、迁移或双写。方法正文及效果记录是按需资料，不承诺模型必然正确复用；当前测试不表示 DeepSeek 方法收益通过、前100章完成或长篇质量保证。

## 同 Work 规划接力实施前评审

实质断点：保存第一项简报后，已批准但尚未执行的大纲/近章规划仍绑定旧工程版本；旧 queued Step 阻止模型决策。即使接回决策，artifact proposal 又要求之前已有同 operation Task，所以无法生成已授权的新类型产物。

最小修复沿既有 Work/Step/Attempt 和 request_work_action：Native 只将已批准、从未创建 Attempt、来源已旧的普通规划 Step 标为 superseded，保留原 action、作者许可、完整要求和 pending 交付义务；模型按需读取当前资料、选择旧授权目标。选中后复用原 Step 身份、冻结当前真正版本和精确原 target，再执行正常保存。未批准、已执行、正文、未知写入不自动替换；仅允许简报的 Work 不能派生大纲。没有新范围、预算或规划流水线，也不把工程版本变化当作作者新授权。

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 复用原批准 action/Step 及同 Work 模型决策；作者许可保持独立；当前源重冻结不改变原授权目标 |
| 技术 A | APPROVE WITH CONDITIONS | 仅 never-attempted planning；已执行/未知结果/越权/锁定/预算/CAS 拒绝仍有回归，失败不改 Step 或保存产物 |
| 技术 B | APPROVE WITH CONDITIONS | Native status、proposal、接回 Step 与 Runtime HTTP fixture 一次同步；原 action 由真实新 Attempt 和保存回执完成 |
| 产品 | APPROVE WITH CONDITIONS | 一次明确允许简报/大纲/近章规划后可以同任务逐步生成；简单仅简报不扩大；未完成规划义务不因 supersede 消失 |
| UI/UX | APPROVE WITH CONDITIONS | 使用既有对话和轻量执行行；模型说明下一行动，不新增固定阶段、确认卡片或技术字段 |

上述为实施者五视角自审。零兼容门禁 PASS：当前 schema 不变，没有旧格式/行为 fallback、迁移或新并行执行通路。临时 HTTP/仓储 fixture 验证可执行性和边界，不能替代真实模型判断或长篇实际验收。主代理已批准 work_tasks.rs 的最小接线。

## 同 Work 规划接力实施后复核

以下为实施者五视角复核，结论 APPROVE WITH CONDITIONS；条件是实际模型的规划选择、完整作者语义和长篇收益仍须实机验收。

| 视角 | 结论与证据 |
| --- | --- |
| 架构 | PASS：只派生同 Work 的已批准原规划 target；不依赖此前同 operation Task，不扩大操作/范围；保留原 action 和许可，复用 never-attempted Step，真实新 Attempt 保存完成其原 pending 义务 |
| 技术 A | PASS：依据实际 captured sources 是否变化，而非仅版本号；未知 accepted 写入不自动替换，真实 Attempt/partial 输出不视为未执行；live scope、原预算/attempt allowance、对象锁、CAS/幂等及已登记义务保持 |
| 技术 B | PASS：refresh_work_task 和 prepare_work_decision 都可重新审视安全的旧 Step；status 给出精确 originSource/actionKey；同源有多个目标时需选精确 key；重选方法说明优先投影，保存依旧走原正常接口 |
| 产品 | PASS：明确批准简报/大纲/近章规划后能在同一对话任务逐步完成；仅简报许可不能变为大纲/Plan；方法撤销不能免除失败作者输出，诚实 partial 不再要求额外方法阻塞计划 |
| UI/UX | PASS：复用原对话和执行行，没有新流程/UI；当前模型说明取代旧预登记方法文案，原作者要求和有效授权不被重写或重复为新作者输入 |

当前自动化证据：

- Runtime `work_decision_http_tests` **10/10**：`/tmp/autonomous-v3-planning-http-complete.log`。新 fixture 分别使用 finiteScope 和三个 exact author permits，实际保存 Brief→Outline→Plan，核对完整原作者、原 Step/action 完成回执、单 Work/单预算授权/原三个许可、无正文产物；无此前 Outline/Plan Task。负例覆盖原范围外类型、伪造来源/正文请求、unknown write 保留且 trusted 结清前不入新决策、真实 partial Attempt 不自动替换、相同请求幂等。既有对象锁、过期 CAS、Host 自造额度和失败额度回归仍通过。
- 该完整 HTTP 回归还证明：连续真实失败的作者输出仍 `canFinish=false`，改计划为 `planningOnly` 并删除全部方法不能让无稿 fulfilled 通过。此前旧断言差异是 `canReportPartial=true` 和方法 pending 不再决定 canFinish，未发现失败 mandatory 输出义务丢失；现保留实际无稿和遗漏回执的拒绝断言。
- Core `model_work_plan_tests` 仓储回归 **15/15**：`/tmp/autonomous-v3-planning-plan-regression.log`。
- Core `work_action_proposal` 单元回归 **5/5**：`/tmp/autonomous-v3-planning-proposal-unit.log`。章节范围收窄、保护正文可读不可写、跨作品拒绝、artifact 锁/commitment current revision、Host-owned 字段拒绝。
- MCP 当前 description/schema 定向回归 **1/1**：`/tmp/autonomous-v3-planning-mcp-metadata.log`。更新后的说明公开已有批准目标接力；恢复契约保持 strict bounded。没有声称该定向检查是完整 MCP 全套测试。
- 所改 Rust 文件定向格式化；所改文件 `git diff --check` 通过。使用 `/tmp/closed-loop-acceptance-freeze-target`，没有重启或操作真实应用、真实作品 DB、网络或模型。

零兼容复核 PASS：未改 schema、未新增 fallback/迁移/双轨。范围仍限已批准的规划目标；没有把作者自然语言推断为更大的 Native 授权，没有新增规划总权限 envelope。这是可运行的领域接力与边界验证，不代表 DeepSeek 语义评估、前100章创作或完整文学质量闭环已完成。
## 2026-10-07：9.3 稳定问题引用，实施前评审

本节为实施者逐项五视角检查，不冒充外部独立评审。当前缺口是标准使用文本/位置序号、独立报告、章节自检与最终未解决清单彼此不引用同一实际问题。方案把标准身份和问题身份分开：模型提出稳定标准 ID，以及选择已有问题 ID 或提出新问题；Native 在已有真实报告保存事务中登记所属 Work、稿版本、评价者、证据和观点引用，不按文本哈希推断语义相同。章节交付引用当前独立报告，不复制文学结论。原作者来源、权限、有限范围、3500 UTF-8 acceptance 上限、6500 UTF-8 bytes status 上限和当前最小独立核验保持。

| 视角 | 实施前检查与结论 |
| --- | --- |
| 方案/架构 | 使用一个报告事实索引，复用现有报告保存与完成入口；不建另一调度流程。标准 `{id,text}` 与 `issueId` 不混用；问题无条件自动消失、删计划降标和序号身份均禁止。通过。 |
| 技术一：数据/权限 | issue 与观点在实际报告事务登记；同 Work、真实报告任务/版本、当前稿与精确引用验证。幂等重放不另建问题。模型提供有界 ASCII ID；首次需 newIssue，已有 ID 不可重定义。现 schema 直接同步，无旧字符串兼容。通过。 |
| 技术二：质量/恢复 | 负评是合法成果；新稿使旧观点 stale，实际重评显式引用 resolutionOf。相同版本冲突保留 disputed；不多数票、不把模型语义判断伪装成机械证明。当前适用问题不能靠删最终清单绕过。通过。 |
| 产品 | 无问题路径 issue:null / []，不新增登记工具调用。非正文未完成要求可在已有交付审查中登记，原稿仍保存可读；问题资料按需分页，1000 章不复制全部历史进状态。通过。 |
| UI/UX | Native 输出统一 issue 投影及真实观点引用，UI 只展示问题、版本、待处理/待复核/争议/已复核结果，不自行按措辞合并或推断解决。章节自检减少重复结论。通过。 |

实施分工：本代理负责 story-core 标准/问题索引、revisionOutcome/chapterReview/deliveryReview、状态投影、story-runtime 读取与原生 fixtures；delivery_audit 同步 MCP strict schemas/描述/注册及两个合成 Engine fixtures；ui_autonomous_controls 同步前端类型、引用展示和 UI fixtures；Root 同步业务 prompt、seed 以及统一编译/实机验收。回归验证：同问题跨稿重评、冲突与明确解决、跨 Work/伪造观点/旧稿拒绝、无问题零登记、负评正常保存、最终遗漏问题拒绝、分页与真实幂等。脚本通过不代表 DeepSeek 或长篇目标完成。

## 2026-10-07：9.3 稳定问题引用，实施后复核

下列是实施者的五视角后审，不冒充五位外部独立评审。结论 APPROVE WITH CONDITIONS：机械保证仅覆盖真实权限、来源、当前版本、报告引用和事务一致性；同一文学问题的识别、标准解释、解决理由及正文质量仍由模型判断，并需要主代理的真实模型验收。

| 视角 | 当前实现与复核 |
| --- | --- |
| 方案/架构 | PASS：`work_issues.rs` 只索引实际已保存报告，不新增调度器或第二套文学结论。模型选择稳定 criterion/issue ID，Native 不按文字/序号/哈希自动归并。章节完成只引用当前独立报告与标准 ID；最终任务复用该问题事实。 |
| 技术一：数据/权限 | PASS：同 Work、固定章节范围、真实报告/观点引用、精确当前来源与事务登记。一次实际独立报告可让多个标准关联同一新问题；重复定义需完全同摘要，定义前后的 null 引用均可，批内预登记一次，各观点保留。跨报告重新首次声明、不同定义、foreign Work、伪造引用均拒绝；同 receipt 重放保持实际 ID。源限 Writer 的列表与详情整体拒绝，实际 whole Writer/critic 可按章读取，摘要不作为红删后的伪全文。 |
| 技术二：质量/恢复/窗口 | PASS：旧负评不会因改稿或没有 resolutionOf 的正评消失；当前冲突显示 disputed。正文或真实已读前文/作者标准变化显示 stale；尚未完成的独立评估任务不能认证 resolved。status 6500 UTF-8 bytes 与 acceptance 3500 UTF-8 bytes 仍有门禁。status 最多两条问题摘要，详情列表只返回 metadata；完整摘要列表和实际观点均按 30,000 UTF-8 bytes 条目预算缩页，数字 keyset 基于实际返回的最后 ordinal，不截摘要或真实原句。 |
| 产品 | PASS：无问题任务 issue:null / []，零新增登记调用。正文问题只能经当前独立评估复核，最终自评不能解决独立正文问题。非正文未解决要求可在原有交付审查中登记及用真实旧观点引用复核。模型仍自主选读资料、方法和修订路径，负评报告正常保存，不强制每步先登记问题。 |
| UI/UX | PASS（跨端由主代理统一验收）：Native 提供 current/recheckRequired/superseded 和共享问题导航，前端不自行判同一性、解决或投票。Author 问题列表可读完整摘要，status 摘要有界；展开后实际读取观点及原句，技术 ID/精确引用进入来源详情。UI 代理报告专属八项回归及 17 场景 Electron fixture 已通过，最终统一入口仍由 Root 执行。 |

原生证据（fresh 临时 SQLite 与实际 loopback Runtime，scripted judgment 不等于文学验收）：

- `work_issue_http_tests` **6/6** 专项：`/tmp/autonomous-v3-issues-final-bounded-http.log`。覆盖跨稿稳定身份、显式复核、同稿冲突、未完成 critic 不认证、真实已读前文变化、metadata/观点 keyset、真实原句保留、无问题零登记、实际 foreign Work 和伪造报告拒绝、两类真实源限 Writer grant 整体拒绝及 whole Writer 正例；同报告两个标准共享一个问题的三种声明顺序、不同定义/伪造引用/实际跨 Work 拒绝、晚到重声明事务回滚及真正同 receipt 重放。最后一项通过真实 Native Author Work 交付审查登记16条各600字符 Unicode 摘要，实际缩页且每页不超过32,768 UTF-8 bytes，逐页读完整摘要、无重复遗漏、无预加载观点。
- `chapter_acceptance_http_tests` **6/6** 与 `work_decision_http_tests` **10/10**：`/tmp/autonomous-v3-issues-delivery-regression-final.log`。3500 acceptance 的最大真实标准仍在 6500 status 窗口内；缺漏原作者要求、负评、短稿、真实当前依赖、旧独立报告、CAS/对象锁/预算与未知执行结果保持拒绝；已授权规划接力与方法纠错仍可执行。
- §9.3 早期生产阶段 Core lib **235/235**：`/tmp/autonomous-v3-issues-core-final.log`；plan 仓储 **15/15**：`/tmp/autonomous-v3-issues-plan-regression.log`。最后批内关联及源限 guard 的全 Native workspace 回归交 Root 统一执行，不能把上述早期结果当作全量最终验收。
- 定向 Rust 格式化与所改文件 `git diff --check` 通过。HTTP 测试仅为 loopback 绑定申请 sandbox escalation；未重启/操作真实应用、未读取实际小说 DB、密钥或发送真实模型请求。

零兼容门禁 PASS：当前 strict schema/required nullable 字段同步修改 Native、Runtime、MCP、UI、prompt 和 fixture；没有旧字符串 fallback、升级/回填/迁移、双写或版本推断。当前问题固定章节作用域；跨章文学同一性需模型另作明确关联，不能用另一章 grant 解决未授权观点。上述是领域引用与交付门禁实现，不宣称真实 DeepSeek 创作或前100章已完成。
