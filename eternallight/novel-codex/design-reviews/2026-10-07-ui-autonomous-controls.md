# 自主创作的任务控制与真实进展表达

日期：2026-10-07。五项为审查视角，不代表五名独立审查人员。

## 变更概览

目标：重开及模型不可用时，作者仍能从当前对话明确选择并暂停持久运行任务；进展标签只说明实际读取内容及当前评估结论。

影响：Renderer 对话菜单、任务控制、Work 状态表达和来源工具投影；当前接口的定向 UI fixture。

不增加常驻任务管理面板，不直接写入小说，不匹配作者词语或特定小说事实。暂停目标由作者点选；领域状态和暂停权限仍由 Native 决定。

## 实施前评审

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 使用既有 controlWorkTask 与 useWorkTaskControls；菜单明确枚举当前对话 running Work，不根据最近模型执行推断原任务。 |
| 技术 A | APPROVE WITH CONDITIONS | 执行前读取并核对 project/thread/id/version/status；实时导航 refs、单飞、迟到结果隔离；成功以真实 paused 回执为准。 |
| 技术 B | APPROVE WITH CONDITIONS | fixture 覆盖多个 running、离线、重开、状态变化、导航竞态、失败与刷新；类型、展示及当前 fixture 一次更新。 |
| 产品 | APPROVE WITH CONDITIONS | 可暂停原长篇任务和局部改稿，目标明确；暂停可恢复、已保存内容保留；保存不等于验收。 |
| UI/UX | APPROVE WITH CONDITIONS | 对话菜单内按需选择，键盘可达，动作后显示回执和错误；读取目录不冒充已读全文，评估技术信息收进详情。 |

零兼容门禁 PASS：无版本兼容、迁移、回填、双读双写或旧字段 fallback；并发、失败恢复和归属检查属于当前协议可靠性。

## 实施后复核

已落实对话菜单目标选择暂停，当前 `controlWorkTask(projectId, workTaskId, command, expectedVersion, expectedThreadId)` 由 Renderer、Host 和 preload 一次同步。Renderer 读取并核对当前 Work，Host 继续核对当前身份/版本和 Native CAS；回执必须是对应项目、对话、任务以及更高版本的 paused 状态。

读取来源投影使用当前返回页的 `returnedChapterNumbers` 和 `contentRead`，目录范围不充当实际阅读范围；`read_source_content(view=index)` 标记目录且不认证已读。未知当前页章节身份则只展示资料片段。分页始终说明片段，不声称全文理解。

Work 的正式 `deliveryReport.chapterAcceptanceReports[]` 已接入主对话。正文保存后显示待独立检查；当前 notMet 报告只在同章 writer 实际运行时显示正在修订，未安排执行时显示待修订；依据不足、旧稿失效和当前通过各自清楚表达。每条报告须绑定同一 Work 的 writer attempt，按登记步骤选择当前章最新写作结果；报告到达顺序和旧通过报告不能认证新稿。检查要求、正文实测篇幅和理由放在默认收起的详情，更早章节进一步收起。Native 决定 verdict，Renderer 不对文学内容做判断。

当前协议缺少必需报告数组时明确显示读取错误及重读入口，不替换为空数组、不解析其他旧字段，也不显示已通过。

模型选择的 `pause_generation_run` 使用“暂停连续创作”动作名，属于安排类控制操作。仅当 Native `committedGenerationControl` 回执为 paused、runId 对应原请求且版本为 expectedVersion+1 时展示“已暂停连续创作”；缺少或冲突回执只展示执行记录及“暂停状态待核对”。不从模型文字推导暂停。

定向证据：`node --test electron/tests/autonomous-controls-ui.test.mjs electron/tests/multiple-retry-ui.test.mjs`，2/2 通过，日志 `/tmp/autonomous-controls-ui-2026-10-07.log`。新增 actual App 旅程覆盖 15 组检查（其中模型暂停正确/缺失/错误回执、六种验收状态、旧报告顺序、外来 writer、协议缺字段，以及错误状态/对话/版本回执各自执行）；原恢复旅程保留 11 组检查。使用隔离的 Electron 内存 fixture，无模型调用或真实业务数据库写入。

| 复核视角 | 当前结论 | 依据 |
| --- | --- | --- |
| 架构 | APPROVE | 单一既有控制入口和账本；不新建根任务，不从回复文字推断控制成功。 |
| 技术 A | APPROVE | 读取后再次检查实时选择，单飞；迟到读取和暂停结果不会污染新项目/对话。 |
| 技术 B | APPROVE WITH CONDITIONS | 当前 5 参契约同步；定向 UI 通过，根任务统一 TypeScript/build 和 Native/Host 集成。 |
| 产品 | APPROVE | 模型不可用、重开后，作者可明确暂停原长篇任务；保存结果保留。 |
| UI/UX | APPROVE | 菜单按需展示任务目标和步骤，限制视觉长度，完整目标仍可访问；独立检查状态按当前报告表达，原文证据和技术信息不默认展开。 |

零兼容复核 PASS。未新增迁移、旧版分支、双读双写或旧 UI 入口。当前 Native、Host、Renderer 接口和 UI fixture 一次同步；剩余为根任务统一集成构建与真实应用流程验收，不把隔离 fixture 记为 DeepSeek 自主质量验收。

## 交叉审查与当前报告契约

只读交叉审查 Native 与 Host；不修改其他实现负责人的文件，不操作真实应用或小说数据库。

| 发现 | 影响与处理 |
| --- | --- |
| `task_revision_outcome` 对 writer 调用 advisory 专用 `model_work_plan::work_for` | 真实协调器读取报告必被拒绝，阻断自主修订。已向 Native 与根任务提交 P1；Native 已改为 completed generateChapter attempt/step/Work 精确绑定，正在补同 Work 完整报告读取及跨 Work 拒绝验证。以 Native 最终测试结果为准。 |
| Host generation 控制只对 Run 做原子 CAS | 第二次 Work 读取之后，模型登记 action 可只改变 Work.version；原 Run CAS 仍可成功。已向 Host、Native 与根任务提交 P1，当前 Host 已统一 Work control 端点，Native pause/resume 已在相同写入事务核 Work 版本。后续又发现 cancel 自有事务遗漏此 guard，已向负责人提交修复；取消与暂停均需根任务确认原子回归证据。 |
| 新报告读取未列入 finished-task 只读恢复清理集合 | 保守 RecoveryJournal 分类下，Native 报告 GET 无写入 ledger，可能留下多余 manual-reconciliation。已向 Host 与根任务提交当前 quiet-read 集合遗漏；暂停是实际写入，仍必须校 committed ledger，不能静默关闭。 |
| 旧 UI 详情只按可选 desiredOutcomes 验证报告 | 当前报告还必须包含全部冻结 `acceptance-1…N`，真实报告原本会被误报不匹配。已在本 UI 实现中同步完整冻结标准：不接受任意额外标准；章节标准证据可来自未改场景，额外局部修改标准仍限定其授权场景。详情展示中文标准正文；首次起草的原稿版本为显式 null，显示“首次起草”。 |

确认的门禁：普通对话 candidate writer 和连续任务 writer 首次登记均冻结章节标准；当前 Work readiness 将独立报告列为必需；连续任务选择 finishChapter 及提交最终 checkpoint 再次核对当前独立报告和实测篇幅。独立评估使用不同任务能力凭据；`prepareOperationEnvironment` 重建进程并清空 thread，不复用 writer 会话历史。旧报告和来源变化通过 Native 当前性校验失效；Renderer 不从报告到达顺序推导当前通过。

模型在自身连续任务中提交暂停后，Host 的 `forwardAgentEvent` 已接入 committed pause helper；再次读取实际 Run、Work 和当前 execution，核验对应暂停回执后只中断该 lease。局部对话暂停另一原运行时不误中断局部对话。单看工具文字或当前 UI 状态都没有租约控制权限。

详情定向回归：revision-outcome、revision-decision、revision-repair 三个 UI 文件，4/4 检查通过（`/tmp/autonomous-revision-details-ui-2026-10-07.log`）。当前 fixture 与快照一次同步，新增整章标准正向、整章范围与局部范围区别、缺漏或未知章节标准拒绝；保留来源失效、迟到请求、作者决策和修复讨论旅程。

完整定向回归无快照更新：autonomous-controls、multiple-retry、revision-outcome、revision-decision、revision-repair 五个 UI 文件共 6/6 检查通过（`/tmp/autonomous-controls-all-ui-2026-10-07.log`）。新 `read_revision_outcome` 友好标签追加后，仅补跑其相关 autonomous-controls 文件，1/1 通过（`/tmp/autonomous-controls-final-ui-2026-10-07.log`）。实际报告、目录投影、缺失或外来 writer 各自判别；标章只使用返回身份，不从请求或内部 ID 推测。

五视角追加复核：架构、技术 A、技术 B 的批准条件增加上述 Native/Host P1 修复和最终集成证据；产品与 UI/UX 保持 APPROVE。这些条件是本轮工程问题，不能由 UI 测试通过代替；真实模型文学质量仍由应用自身独立评估并修订，根任务另做真实应用验收。
