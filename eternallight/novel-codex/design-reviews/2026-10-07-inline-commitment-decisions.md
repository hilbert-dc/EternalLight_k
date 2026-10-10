# 在对应对话中选择伏笔建议

## 变更概览

- 目标：把任务产生的建议与选择放回该任务回复附近，摆脱按当前选中章节挂在整段对话底部的面板。
- 非目标：把可处理成果伪造成必答问题、暂停生成、自动确认、重设计管理目录或新增文学正确性判定。
- 影响模块：提取报告精确读取契约、任务结果展示投影、WorkTaskPanel、CreativeWorkspaceView、伏笔选择组件及测试。
- 验收：多任务/章节/对话各自固定原报告；报告被替换与来源过期分别只读；选择沿用真实确认链；没有底部自动显示当前章节建议；阅读、失败、重试和切换不产生自动写入。

## 方案

任务建议使用 project/workTask/step/attempt/reference/chapter/source 固定身份，在该任务答复之后显示简洁建议卡。只有提取报告的 recorded/superseded 保存证据进入独立展示投影，不能放宽其他当前成果入口。较早结果沿用本轮结果的渐进展开方式。

报告读取支持明确 reportRevisionId，按项目、章节对象和修订读取不可变 payload，绝不替换为 latest。响应分开 reportCurrent 与 sourceCurrent：只有当前报告、有效正文、当前交付且无 dirty/busy/执行阻塞才能确认。实际选择继续使用 createCommitmentDecision 的重新读取、CAS、幂等和未知写入恢复。只有真实确认状态和回执才能显示已保留。调整转入作品级原有单一编辑器，不让多个卡片抢 supplementalEditor owner。

当前章节目录/编辑入口保留为用户主动打开的管理区，离开对话底部，不再因任意 extractCommitments 步骤而自动展示。模型若需要必答选择仍用既有真实 user-input 契约；候选建议本身不创建 waiting。

## 测试阶段零兼容门禁

- [x] 无历史格式适配、双读双写、迁移、回填、版本判断或降级；当前最新报告读取与指定报告读取是不同当前需求，不是兼容通道。
- [x] 不为假设旧调用方保留失去上下文的底部自动面板。
- [x] 精确读取参数和 reportCurrent 一次性同步 Native、HTTP、IPC、preload、类型、组件及 fixtures。
- [x] 测试数据使用当前 schema，无数据库表变化。
- [x] 安全、当前一致性、归属、恢复、并发、可访问性保持验证。
- [x] 无兼容例外。

## 五角色实施前评审

### 架构专家
- APPROVE WITH CONDITIONS（label_surface_audit）：展示固定真实输出身份，历史展示与当前作者操作投影分离，报告精确读取不替代写入许可。
### 技术专家 A
- APPROVE WITH CONDITIONS（label_surface_audit）：区分报告与正文过期；完整身份限制异步生命周期，不借 latest 替旧报告；保持原确认链。
### 技术专家 B
- APPROVE WITH CONDITIONS（label_contract_audit）：superseded 报告只用于精确历史展示，不开放现有导航权限；单一编辑 owner；严格检查确认状态而非 established 映射。
### 产品经理
- APPROVE WITH CONDITIONS（label_ux_audit）：回复附带的可处理成果就地选择，不能伪造必答、已拒绝或已确认，管理入口保持独立。
### UI/UX 设计师
- APPROVE WITH CONDITIONS（label_ux_audit）：摘要、保留、调整就地可见，依据折叠；已处理和过期有真实状态，不卡住阅读、不增加全局面板。

## 综合决策

- APPROVE WITH CONDITIONS，无 P0/P1 阻断。
- 无兼容例外。
- 实施范围仅上述绑定、读取、选择展示和验证。

## 实施后复核

### 架构专家 / 技术专家 A

- APPROVE WITH CONDITIONS（label_surface_audit，代码无阻断，HTTP 运行验收受本机启动阻塞）：Native 精确/当前读取共用 SQLite 读取快照，校验 project/chapter/object kind/revision；报告被替换与正文失效分别判断，缺失不替换为 latest。
- 历史展示独立于当前成果导航，不放宽作者写入权；selected thread 进入卡片 key，切换后旧异步操作停止后续写入。
- Core 提取报告测试 9/9、IPC/preload/真实 author bridge 3/3 通过；两项 HTTP 测试已编译，但本机测试程序未能进入 Rust 测试执行；没有通过结果。

### 技术专家 B

- APPROVE（label_contract_audit）：精确报告与 reportCurrent 门禁、StrictMode、未知回执只读恢复、真实确认状态验证通过。
- 默认宿主同步跨卡锁与 pending read 跨 thread 失效已验证。
- 真实 App 的非创作视图调整入口复核 task proof 后进入来源章节唯一编辑器，打开与导航不写入、明确保存只保存候选、请求消费后不再弹出。核验悬挂期间切换或新建对话会取消旧导航，迟到读取不弹出旧编辑器；当前 schema 无旧接口特例。

### 产品经理 / UI/UX 设计师

- APPROVE（label_ux_audit）：建议位于原任务回复之后，章号区分多章结果，摘要与选择直接可见，原文依据与执行详情继续渐进展开。
- 章节管理区已退出对话底部，仅主动打开；单一编辑器保护未保存输入及正在进行的操作。
- 没有伪造必答、拒绝、确认或文学兑现状态。查看旧报告与切换章节不自动写入。

### 实施与验收

- 新增 CommitmentTaskResult，精确绑定真实保存证据；WorkTaskPanel 在回复后呈现，保留现有两层执行展开及较早成果披露。
- 管理区移到章节页面；调整使用 CommitmentManagementDialog + 原 CommitmentEditor，由 supplementalEditor 单一管理保存、放弃、dirty/busy 与导航保护。
- App 的通用对话栏和创作工作区均接入同一确认/调整行为，当前作者确认链与 CAS/幂等边界保留。
- 当前 TypeScript/Vite 构建及 diff 检查通过；纯契约/投影/导航/确认测试 19/19 通过。
- Electron 定向与相关回归 13/13 通过：CommitmentPanel、CommitmentTaskResult、原位多章历史与跨卡锁、真实 App 调整、7 项工作区/对话回归、成果导航与发布检查恢复、完整答复读取。
- 验收图：`/private/tmp/conversation-inline-commitment/conversation-inline-commitment-choices.png`，中文、章号、答复→就地选择→执行详情顺序已核查。
- 仅持久化证据对应的当前 step/attempt 报告进入该展示投影；不凭模型回复重构失去归属的历史输出。保留不等于文学兑现认证。

### 最终门禁

- APPROVE WITH CONDITIONS：实现、前端与核心/桥接验收通过，无已知 P0/P1；Native HTTP 运行验收存在本机启动阻塞，尚未完成。
- 零兼容规则全部通过，无迁移、回填、双读双写、降级或旧 fixture 格式 fallback；无需负责人例外审批。


## HTTP 验证限制

`commitment_extraction_http_tests` 与 `task_evidence_http_tests` 已完成编译；两个 artifact 的时间晚于本轮精确读取实现及同步后的当前 fixture。定向 Cargo 启动和高权限直接运行这些已编译测试程序，都停留在 macOS 程序启动阶段，没有进入 Rust 测试输出。初次运行的 fixture 使用过期的 relevantText 数组断言，现已同步为当前 taskSources 注册来源及分页读取，并保留来源和章节隔离断言；修正后的 HTTP 执行尚无通过证据。本次测试进程按归属清理，其他任务进程保留。

剩余验证项仅为两组 HTTP 运行验收，不能把编译成功视为这些测试已通过。UI、当前接口的核心与桥接行为有上列通过证据。
