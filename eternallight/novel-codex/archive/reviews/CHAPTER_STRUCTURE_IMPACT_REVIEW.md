# 章节结构调整后的影响检查评审

状态：五角色 PRE / POST 均完成，本阶段 APPROVE。自动化、第三次真实模型单样本与实机加载均已验收；前两次真实验证失败如实保留。本阶段独立评审，不继承章节删除阶段的批准。

## 变更概览

- 目标：作者删除中间章节后，系统识别结构变化，在对话中检查具体衔接问题，作者再决定是否调整作品。
- 最小范围：仅章节删除后的自动只读故事衔接检查，重点验证中间章；末章也不得仅因位置跳过。只有完全无已保存正文且无真实关联的章跳过模型。复用当前对话、任务状态与恢复入口，不新增页面或工具墙。
- 非目标：自动改后文、事实、大纲、伏笔决定或正式采用；全书逻辑无误保证；自动重编号；没有真实顺序能力时宣称支持中间插入。
- 影响模块：章节删除事务/回执、冻结检查来源、Host 任务调度、对话任务与结果展示。接口使用持久结构事件、专用只读 operation 和原 Work 恢复。
- 本轮不改已完成的章节删除评审范围；结构影响检查为独立阶段。

### 五角色批准的冻结合同

- 删除确认明示：“删除后将自动检查故事衔接，不改写其他章节。”这次确认承接用户已授权的只读检查，不再要求作者手动发送检查指令。
- 删除变化持久为 event，assessment 状态为 `pending` / `skipped` / `bound`。冻结删除前依据并绑定真实回执；Host 每个 event 只建立一个首自动检查任务，专用 operation 为 `assessChapterStructure`，权限强制只读。
- 首自动任务使用现标准根 Work 的预算和计量，不继承不存在的 Work。预算来源和上限由技术方案确认；失败后仅显式恢复原 Work，不自动新建付费重试。
- 模型不可用时 event 保持 `pending`，不给运行中的 Work，在对话显示小状态；可用后按同 event 的唯一任务机制处理，不能在每次刷新/切页时重新排队。
- `bound` 后失败只通过原 Work 显式恢复，不自动创建新根任务。保存回复与完成证据复用 `finalReply` / `completionReport`，不另造模型结果来源。
- 源严格绑定 immutable 依据，分页有硬上限；来源不足或超限需明确覆盖，不改读当前新稿、不静默补无限上下文。
- 空白且无真实规划/知识等关联的章跳过模型；有关联仍检查。末章不能仅因没有后文章节而跳过。当前仅删除，不提供或宣称中间插入已完成。

## 实施前源码依据与差距

- `src/useChapterDeletion.ts` 已按新鲜工作区计算 `hasLaterChapters`，删除成功经只读核对后更新目录；目前只有中章衔接提醒，没有影响检查任务。
- `native/story-core/src/task_impact.rs` 的任务影响分析仅允许当前 advisory 项目任务，读取当前捕获输入，`revision_pair: None`；没有删除前/后的章节结构变化输入。
- `native/story-mcp/src/impact_tool.rs` 明确覆盖显式依赖与已知名称提及，限制跳数和条数；空结果或截断不能证明没有影响。
- `electron/conversation-analysis.mjs` 已区分只读问题、影响检查和明确委托候选；正式事实、身份、伏笔决定及发布保留作者决策。可复用这些边界，不能把现工具包装成完整结构检查。

## 产品经理 PRE

结论：`APPROVE WITH CONDITIONS`。

1. **冻结变化，再检查。** 删除事务前保留精确原章身份、已保存正文来源及关联依据，任务绑定实际删除回执和结果水位。不能在删除后凭“正文不存在”猜原内容，也不能按同名章节替代。冻结内容按需有界读取，不把整部作品一次注入模型。
2. **成功删除与检查进度独立。** 删除核对成功即允许继续写作；只有检查真实启动才说“正在检查故事衔接”。未启动、资格不可用、暂停、失败或来源失效均如实显示，恢复不再次调用删除。
3. **空章轻量，不等于没有关联。** 按实际有效正文判空，保留章节规划、伏笔及其他真实关联的本地识别。无正文且无关联可说明无需正文衔接检查；有已知关联则进入相应有限检查，不能统一声称“无影响”。
4. **结果能够支持作者决定。** 对话说明受影响章节、原文或关联依据、可能断裂及未覆盖范围。确定依赖与推测性文学建议分开；作者可保留现状、讨论调整或明确委托指定候选修改，检查本身不写故事。

用户已授权本阶段自动模型只读检查。首任务可以建立标准根 Work，但不得虚构前序 Work 或借检查扩展故事写权限；仍须沿明确范围、预算、可用连接及取消机制。失败后的重新执行只由作者显式恢复原 Work，不自动创建新付费尝试。

## UI/UX PRE

结论：`APPROVE WITH CONDITIONS`。

- 检查出现在右侧对话，标明“自动检查”，沿现真实任务状态与结果入口；不伪造一条作者发送的指令，不另加全页阻挡面板。模型不可用可显示“章节已删除，故事衔接待检查”，而不是假运行或假完成。
- 正文继续可编辑。检查中的输入草稿、停止及恢复遵循当前任务能力；检查结果迟到不覆盖当前正文、输入或其他作品的对话。
- 主文用章节名和可理解的问题，例如“第六章仍引用被删除章节中的发现”；内部 ID、任务身份及水位只在已有详情内核对。
- 失败提示首先区分“章节已删除，影响检查未完成”；局部错误可读、可恢复，不把分析失败呈现为删除失败或要求重复确认删除。
- 作者决定入口仅在真实问题或明确修改提案存在时出现；不强制每次询问“是否核对”，不自动确认设定。
- 键盘可访问现任务详情、停止/恢复和结果入口，状态不抢编辑焦点，不用反复朗读完整模型正文。

## 五角色 PRE / POST

| 角色 | PRE | POST |
| --- | --- | --- |
| 架构专家 | APPROVE WITH CONDITIONS（root 汇总本阶段结论） | APPROVE（见实施与验收证据） |
| 技术专家 A | APPROVE WITH CONDITIONS（root 汇总本阶段结论） | APPROVE（见实施与验收证据） |
| 技术专家 B | APPROVE WITH CONDITIONS（root 汇总本阶段结论） | APPROVE（见实施与验收证据） |
| 产品经理 | APPROVE WITH CONDITIONS（本文条件） | APPROVE（见实施与验收证据） |
| UI/UX 设计师 | APPROVE WITH CONDITIONS（本文条件） | APPROVE（见实施与验收证据） |

综合 PRE 决策：`APPROVE WITH CONDITIONS`，五角色门禁齐，按上述冻结合同实施。若不能精确读取删除前依据、重复回执会重复创建模型任务、检查可写故事或失败阻挡正文，则为 P1 阻断，不能以文案代替闭环。POST 已完成；实际结果及范围限制见下文。

## 当前版本验收标准

- [x] 专用作品删除有正文中章：一次删除回执对应一次结构检查登记；具体问题有真实后文章节及来源，不把检查登记声称为完成。
- [x] 删除确认明确告知自动只读检查；成功删除后无需手动发起，专用 operation 不能调用正文/规划/事实写入或作者确认能力。
- [x] 首自动任务按标准根 Work 预算实际计量；重复 event、启动恢复及刷新不建立第二个首任务，失败仅显式恢复原 Work且不重复删除。
- [x] 删除回执未知或读取失败：只读核对恢复，检查不因核对重试重复创建，删除不被再次执行。
- [x] 空正文无关联与空正文有真实关联分别验证；前者不产生不必要模型检查，后者不被误判为无影响；有内容或关联的末章不因位置跳过。
- [x] 检查进行、失败、暂停或连接不可用时，目录删除状态明确，正文和下一条输入保留；无模型保持 `pending` 且无运行 Work，`bound` 失败只显式恢复原检查。
- [x] 检查结果区分可证引用/依赖、推测建议和未覆盖章节；不保证全书逻辑完整，不把缺失建模结果当不存在。
- [x] 检查完成后，其他章正文、规划、正式事实、伏笔确认与发布状态不被改写；作者明确后续委托另按原候选/决定门禁执行。
- [x] 项目切换及 A→B→A、重复回执、晚结果与来源变化验证精确归属；旧结果不污染新会话，不偷用新稿替代冻结来源。
- [x] 对话“自动检查”状态、失败恢复与结果入口键盘可达；不新增主页面遮挡、伪造作者发送或强制焦点跳转。
- [x] 若没有真实中间插入顺序能力，界面及报告明确未覆盖，不以新建尾章或归卷模拟插入成功。

上述按下方证据完成验收。真实模型是隔离短篇局部样本，不等于长篇全书质量保证；用户原作品只验证加载和保留，未操作实际删除。

## 测试阶段零兼容门禁

- [x] 方案不要求版本判断、旧字段 fallback、双读双写、迁移、回填、降级或 deprecated 路径。
- [x] 不为假设的旧行为降低目标设计；当前可靠性和来源恢复按本阶段边界处理。
- [x] 接口/schema、生产代码、类型、fixture、测试与现行文档已同步复核。
- [x] 当前 schema 专用环境的数据一致性、故障恢复及安全验证通过；路径和平台分支经源码复核，实际执行平台为 macOS，未宣称运行过 Linux/Windows 实机。
- [x] 无兼容例外授权；本阶段不新增兼容要求。

## 实施后复核

实际变更、构建/测试、实机与模型证据见下文。本阶段完成自动只读检查与恢复闭环，文学质量判断仅限于实际验证样本。

### 初版中期只读审查（不是 POST 批准）

- 已读 `ChapterStructureNotice`、`chapter-structure-events`、删除确认、`WorkTaskPanel` / App 接线和 Host consumer / prompt。pending 状态读取不执行模型；Host 自动任务不写 composer；系统消息标“自动检查”，正文主区不另挂任务面板。删除确认已说明自动检查及作者决定调整。
- 初版曾发现、现已关闭的恢复问题：自动 started 替换 AgentRun，但主文错误恢复仍取原 `executionRetryRef`。前轮作者讨论的重试身份可能使自动失败后的按钮标签错误或被 history guard 拒绝无动作。必须精确指向原检查 Work，不走重发旧作者讨论或新建根任务；验收覆盖前轮已有讨论失败身份的情况。
- 初版曾发现、现已关闭的表达问题：自动检查的非 task-bound 当前/历史说明仍沿“复制我的创作指令”可访问名称，应区分系统检查说明，不误称作者发送。
- 本节保留中期审查过程；上述问题已修复，最终测试与五角色 POST 结论覆盖此中期状态。


### 实施范围与分层复核

本阶段分为两个可独立审查的层次：Native 持久事件、唯一预算绑定和只读来源契约；Host 自动调度、对话状态及恢复。跨层合同冻结后实施，未把全部逻辑继续堆入 App 或 Runtime 单一文件；新增独立结构检查模块。此处的分层是本次较大功能的审查边界，不表示已提交多个提交或 PR。

- 删除事务保留精确 saved / accepted / working 引用及相邻章身份，返回 structureChangeId；冻结依据随任务输入保存，禁止调用方伪造或用 latest 替换。
- 空正文无真实关联跳过；已有规划关联的空章保留 pending。末章依据内容和关联判断，不按位置跳过。
- Host 在模型资格可用、没有未保存编辑且引擎空闲时启动；pending 不假建运行 Work。唯一绑定防止重复付费；未知创建回执只读核对并暂停原 Work，切作品后仍正确收尾。
- 对话以“自动检查”系统记录显示，保留输入和光标；结果在原对话保存。失败恢复仅读取原任务并从原 Work 恢复，不能重发旧讨论或重复删除。
- 多个 pending 首启动失败按 projectId / changeId 分开保存；只显示当前一条，其他成功不清除它的重试入口。此窄修经架构、技术 A、PM/UX PRE 确认后实施。
- 当前仅覆盖删除；未新增中间插入、移动或自动重编号能力。

### 新模型片段超过 1K token 的专项人工复核

Root 与技术 B 已人工复核并接受本阶段新增片段。事件元数据最大 4096 UTF-8 字节，旧正文页默认 600、最多 1000 Unicode scalar；两者都可能超过 1K token，不以字符数冒充 token 上限。正文页的 escaped MCP wrapper 另有 9000 字节硬限制，Runtime 最终 model item 保留 9000 token 门禁，超过限制返回错误，不交付部分证据。

事件与页面绑定作品、事件、原始 revision、Work、当前 attempt capability 和任务输入；cursor 包含角色、偏移及这些身份，跨角色/attempt 不复用。`removedHistoricalSource` 明示历史来源；`globalImpactComplete=false` 不因读到末页改变，末页只说明该来源后缀结束，不代表全书检查完成。元数据纳入 context 的整体 fit/hash 和 token/byte 闸。不存在任意 revision/snapshot 读取或无界正文注入。

### 自动化证据

- Host / workflow / conversation：45/45；新增未知创建回执及跨作品暂停收尾：20/20，0 skip。
- Adapter / agent events / deletion：47/47；最终专用 prompt / event：2/2。
- 实际 App 删除与对话、作者输入回归：3/3；新增多事件失败恢复实际 App 回归：1/1，快照已读并接受。
- Renderer TypeScript 与 Vite production build 通过；已有大 bundle 提示保留，不作为本轮新增故障。
- Native 新结构 HTTP 两项已通过，包括精确旧来源、plan-only、空正文跳过、同 Work 两 attempt 单预算、旧 cursor 与当前输入变化拒绝。其余 Native / 实模 / 实机结果由最终验收追加。
- 证据日志：`/tmp/novel-structure-host-tests.log`、`/tmp/novel-chapter-structure-host-settlement.log`、`/tmp/novel-structure-adapter-tests.log`、`/tmp/novel-structure-ui-regression.log`、`/tmp/novel-structure-ui-multi-event.log`、`/tmp/novel-structure-build-multi-event.log`。


### 丢创建回执恢复收口

初版仅暂停 Work 的实现会留下 queued attempt，原 Work 随后无法结算并恢复。修复限制在本地 task POST 未返回且控制流尚未到模型发送的路径；从本次 authorizationId 派生确定任务身份，核对事件、作品、线程、输入、目标、模型及作者消息后，获取原 grant 创建唯一预算记录。既有记录拒绝覆写，不能从 queued 推测零费用。原 catch 将 attempt 结束为 failed（明确取消为 cancelled），生成真实零请求收据并沿原路径结算；原 Work 可显式恢复。

架构专家、技术 A、技术 B 和 PM/UX 的 PRE 条件确认完成；架构/技术 A 已独立交叉 POST APPROVE。生产 Host / Adapter / 持久预算回归 23/23，0 skip，含两 attempt 单根授权、跨作品、错误归属、已有账本及取消。证据 `/tmp/novel-chapter-structure-unsent-final.log`。

### 首次真实模型记录（不通过）

可信 DeepSeek 连接已验证，隔离当前 schema 的虚构作品执行了 read_context、read_chapter_structure_source、query_story，实际 3 请求、45311 tokens、未知用量 0，完成费用结算及临时数据清理。没有完成回执或最终答复，不能评为模型验收通过。初版探针仅保留 PROBE_FAILED，缺少可区分预算/超时的安全终态信息；后续先补诊断，不猜测根因。报告 `.../novel-live-chapter-structure-V9pKdH/report.json`。

### 实机加载

已完整重启开发窗口，使用原 `/tmp/novel-planning-live-current-290Hdi` 数据目录，确认《规划对话验收：雾港钟声》、原 1 卷 5 章与第 5 章标题/正文仍在，Runtime 在线。原作品仍有暂停的未结束任务，目录删除按现权限禁用；未取消该任务或删除原作品章节来强行验收。新删除确认及自动检查的交互由真实 App fixture 回归覆盖。


### 第二次真实模型与有限交付修正

第二次在 120000 token 测试上限下调用 6 次，实际 108791 tokens、15.388 秒。第 7 次准备发送的输入估计 26231 token，加 8192 输出预留，超过剩余 11209，被明确拒绝为 `task_token_budget_exhausted`，未发送第 7 个供应商请求。这个样本证实是预算拒绝，不是超时；不倒推首样本缺失的终态原因。

模型已经读取精确旧源和当前章节，但进行了 4 次 query_story（其中一次失败）仍未交付。检索次数本身不能证明所有查询无用；专用提示缺少局部检查的收束标准，则是可修正的产品问题。五角色重新确认后，仅收紧本 operation 的任务说明：优先旧源与邻章证据、一次有针对性的批量查询；有足够依据就报告局部问题，只为具体缺口追加查询，未查范围明确说明。提供符合当前 schema 的查询形状，移除 handoff 中重复的整段说明，未扩大任何生产权限、预算或隐藏模型历史。最终 prompt / workflow 18/18 通过。诊断本地 7 项检查通过，无模型调用；报告仅保留安全枚举、计数、hash 与公开最终答复。

### 第三次真实模型最终验收

使用相同 120000 token / 12 请求 / 240 秒测试上限，实际 **5 请求、82973 tokens、13.677 秒、未知用量 0**。工具路径为 read_context → read_chapter_structure_source → 单次 query_story（一项文本检索，1 个结果）→ complete_task → 最终答复。可信连接资格未绕过，实际预算已结算，隔离数据已清理。

- 完整读取删除前第 1 章的 68 个 Unicode scalar，正文 hash 一致；实际读取保留的第 2 章来源。
- 最终中文答复指出后章对“昨天的约定”“老周交付钥匙”的回指失去铺垫，并区分一次性钥匙伏笔的可能影响。
- 提供补交代、合并必要铺垫、调整回指三类作者选项，明确未检查范围；没有堆砌内部 ID 或声称全书无误。
- finalReply 与 canonical completionReport 实际保存；前后章节 hash、sourceDigest、revision、snapshot 和正式知识均不变，没有自动修稿。
- Root 独立阅读公开答复和验收字段；PM/UX 对本合成局部断裂样本的可理解性、有据性、可行动性 POST APPROVE。架构、技术 A/B 对边界、恢复及实现 POST APPROVE。

第三次报告：`/var/folders/dz/j3hn9zvs2q7gnh6w4m48cktr0000gn/T/novel-live-chapter-structure-RnhVkh/report.json`。第二次报告同目录前缀下 `novel-live-chapter-structure-swnpds/report.json`；首次失败报告保留。未执行第四次。

### 最终决策与限制

**APPROVE；本阶段无剩余 P0/P1 阻断，零兼容门禁 PASS。** Native 新结构 HTTP 2/2、原删除 HTTP 2/2、真实 loopback MCP handler 1/1 通过，两 binaries 构建通过；MCP 首次 sandbox 拒绝 loopback 后在授权环境重跑通过。最终 Native 格式化及仓库 `just fmt` 在测试之后执行，不重跑测试。

本次实模是单个短章节局部断裂样本，不是长篇全书质量、稳定速度或成本保证。中间插章/移动/重排未实施；执行环境实际验证为 macOS。真实作品原有暂停任务保留，没有为测试而取消任务或删除用户章节。

## 后续对话衔接 · PRE（2026-10-06）

目标：补齐“作者确认删章 → 自动检查 → 对话指出具体问题 → 作者决定是否调整”的最后一步。当前自动检查独立会话，普通发送却恢复旧 advisory，导致可见报告与模型上下文脱节。

方案：ConversationFocus 增加有界 chapterStructureTaskId；仅可见当前成功自动检查后的普通发送自动携带，显式来源/改稿入口优先。Host 核验同作品 completed assessChapterStructure 与 recorded、非截断最终回复的 task/thread/turn，派生精确线程恢复；新 advisory 使用新能力、当前冻结输入和独立预算。引用保存在 target，失败不换线程；已有重试保留原输入与 focus。不复制报告，不改历史，不新增按钮。旧报告是建议背景；多方案歧义先澄清，明确修改仍走现有候选授权与当前来源 CAS。

五角色 PRE：架构专家 APPROVE；技术 A APPROVE；技术 B APPROVE WITH CONDITIONS（精确回执核验、target 完整回读、拒绝路径零模型）；产品经理与 UI/UX APPROVE WITH CONDITIONS（仅当前可见成功检查、显式来源优先、歧义先澄清）。无阻断。

零兼容门禁：不增版本判断/fallback/迁移/双轨；同步类型、生产、fixture、测试与文档；测试用当前 schema；保留当前安全、并发、失败恢复边界；无兼容例外。新增模型说明有硬预算 <1K token，无报告正文注入。

验收：生产 handler/Adapter 续接精确检查线程且新 advisory 权限；普通后续同线程；错误来源/截断/缺失/跨作品/target 丢失拒绝；实际 App 成功检查后的发送携精确引用，输入与页面定位保留。

### 后续对话衔接 · POST

实施约 60 行生产逻辑与 157 行 Host/Adapter 集成测试，沿用现有 ConversationFocus/ExecutionRetry，不增加 UI 按钮、Native schema 或报告复制。仅普通 composer 发送可自动接当前成功检查；显式来源优先。IPC 提前拒绝 repair 混用，Runtime 回读 target 丢引用时零模型。失败原输入和精确引用保留在对话与“重试讨论”中。

五角色 POST：架构专家、技术 A、技术 B、产品经理、UI/UX 均 APPROVE；本阶段无 P0/P1，零兼容门禁 PASS。新增结构检查来源说明有 1K token 硬上限，历史报告不重新注入。

验证：
- 最终 scoped Host/Adapter/讨论回归 34/34：`/tmp/novel-structure-followup-final.log`。真实 handler/Adapter 使用本地合成 AppServer 与受控 Runtime，验证新 capability/current input/exact thread，非付费模型。
- 真实 React App 的隔离 Electron UI 1/1，新增快照已人工阅读：`/tmp/novel-structure-followup-ui-final.log`。验证检查完成后普通输入携精确来源，正文/删除次数不变。
- `npm run build` PASS：`/tmp/novel-structure-followup-build-final.log`，只有已有 bundle 大小提示。
- 讨论回归首次有一条既有标题文案断言过期（“当前任务”与现生产“当前唯一任务”），改为验证“只分析绑定的已保存正文”职责后通过；没有改变生成权限。
- 原有真实模型检查样本仍有效，本阶段没有新增真实模型的后续修改验收；不宣称多方案澄清或后续写作质量已通过新实模。
