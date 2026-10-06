# 本地诊断入口收敛评审

日期：2026-10-06。范围：对话驱动创作落地任务中的诊断 UI 子任务。

## 变更概览

目标：底层数据校验作为高级排障能力，作者不需要逐项执行才能创作。设置只显示一个默认收起的入口；展开后按需查看紧凑诊断行。未运行与旧报告过期均不暗示创作尚未完成。

非目标：不在 UI 中增加诊断自动调度、不声称六类全量诊断已自动运行、不修改小说内容、工具权限或 Runtime 数据结构。

实现前已向主任务提交方案。下列为本子任务的五视角自审，供主任务综合评审，不代表实际产品厂商人员参与。

## 测试阶段零兼容门禁

- [x] 无版本判断、旧字段 fallback、迁移、双读双写或废弃入口。
- [x] 直接替换当前卡片布局，保留的是当前排障能力，不是旧 UI 兼容层。
- [x] 无 API/schema 变化；组件调用与相关 fixture 一次性更新。
- [x] 不修改测试作品或数据库；本项无重建需要。
- [x] 在途请求、项目隔离、诊断暂停/取消及错误状态验证保留。
- [x] 无兼容例外。

## 角色评审

### 架构专家视角

APPROVE。Renderer 只改变入口与呈现；展开才读状态，之后折叠保留轮询。诊断执行和持久性仍由 Runtime 负责。

### 技术专家 A 视角

APPROVE。沿用项目范围检查、请求序列、旧返回值隔离、重复点击保护。UI 展开没有触发任何写操作。

### 技术专家 B 视角

APPROVE。未启动诊断不等于通过检查。来源变化显示“报告已过期”，真正失败、不一致和读取不可用继续显示异常。全文本、测试 fixture 与新的状态标签同步。

### 产品经理视角

APPROVE。设置页显示高级排障而非六项作者待办。无“全部通过”式误导；不将索引一致性当作故事或文学质量保证。

### UI/UX 设计师视角

APPROVE。使用原生 details/summary 的键盘交互，保留 focus-visible。六张告警卡改为紧凑行，状态靠右，详细范围与操作在各行内展开，作品名称位于排障内部。

## 自动维护事实

- 正文保存/采用调用 search_index::index_revision，检索索引随写入维护。
- 知识发布在 modeling.rs 同步写入实体、关系、断言、人物认知等索引。
- knowledge_audit_worker 在没有在途工作时调用 schedule_knowledge_audit；当前门槛为超过 200 个已发布建模批次，并有队列和状态约束。
- 其余五类全量诊断 worker 处理已持久化的队列，没有在本子任务中新增自动启动策略。

因此界面不承诺六类诊断全部自动执行。普通写作维护与全量排障是不同职责。

## 实施后复核

- 实际变更：高级排障入口；首次展开加载；保留折叠后状态跟踪；紧凑检查行；状态语义与文案收敛。
- 与方案偏差：无。
- 构建：npm run build 通过（已有大 bundle 警告）。
- 专项：workspace-diagnostics-ui.test.mjs 1/1 通过，包括默认不读取、重复点击、后台完成、错误恢复、来源变化、项目切换与外部项目结果隔离。
- 设置回归：author-session-ui.test.mjs 1/1 通过，包括浅深色与布局矩阵。
- 扩展创作回归：诊断阶段操作通过；后续 work-conversation fixture 的回复显示断言失败，creative-workspace fixture 在任务恢复阶段因 resumeFailure 缺失失败。这两个问题超出本诊断 UI 修改范围，已交给主任务，不能将扩展套件报为全通过。
- 零兼容门禁复查：PASS。
- 剩余项：后端审计自动触发策略由主任务评估；本记录不声称已完成。

## 追加：普通改稿的未建模状态不作为作者待办

### 实施前方案与五视角检查

- 架构：`model_pending` 描述当前正文尚无知识批次，不能推导分析任务已经失败或正在执行；执行状态以当前章的持久任务步骤为准。APPROVE。
- 技术 A：只移除通用待办提示和按钮，不改发布来源匹配、显式只发布正文授权、保护冲突或真实任务错误。APPROVE。
- 技术 B：保留当前章/其他章的执行隔离，检验待执行与执行中两态，并更新通过对话发起知识分析的 fixture。APPROVE。
- 产品：普通改稿可结束，不通过一条顶部告警继续要求作者分析；作者需要分析时从对话提出目标。APPROVE。
- UI/UX：只在当前章的知识分析步骤实际 running 时显示无边框的简短状态；无待处理按钮，无重复警告。APPROVE。

### 实施后复核

- CreativeWorkspaceView 移除所有 `model_pending` 的常驻 warning 和“继续分析本章”按钮，保留实际 running 的轻量状态。
- 不修改 Runtime 和发布校验，不启动或关闭真实应用。
- 普通 pending 不显示待办、实际分析/跨章隔离、发布授权与回执验证：`chapter-document-experience-ui.test.mjs`、`chapter-publication-ui.test.mjs` 共 2/2 通过。
- `app-recovery-ui.test.mjs` 的 domain recovery 子测试 1/1 通过；fixture 从手动分析按钮改为对话发送目标，验证恢复不重放。
- `npm run build` 通过；无新增兼容路径，零兼容门禁 PASS。

### 收尾清理

确认 App 中 `resumeKnowledge` 仅供已移除的手动分析按钮使用，删除该函数、CreativeWorkspaceView 对应 prop 与 App 传参；相关 fixture 不再构造不存在的回调或保留其专用 UI 快照。底层 `api.agent.resumeKnowledge` 的 IPC/引擎能力保持。显式知识分析已由对话恢复专项覆盖。清理后再次 `npm run build` 通过，未重复运行整套 UI。

### 当前接口 fixture 收尾验证

`creative-workspace-ui.tsx` 中四处 `resumeWorkTask` 成功模拟此前返回 `undefined`，不符合当前返回持久 Work 结果的接口，导致读取 `resumeFailure` 出错。已仅更新 fixture 返回绑定原任务的对象（延迟返回也捕获原任务），未增加生产兼容分支。重新运行 `creative-workspace-ui.test.mjs` 的 `actual workspace preserves scene identity, author intent, and save boundaries`：1/1 通过，约 18 秒。此前本记录提及的该项恢复失败已解决；独立 work-conversation 断言不在此次范围。
