# Story Core 子域与存储边界（2026-10-12）

## 实施前方案
责任为 Story Core（B01/B03/B05/B06）：按领域事实把平铺实现组织为真实 Rust 子模块；公共 DTO 与用例由子域导出，crate 根仅为现行公共 facade 和少量跨域基础设施。此组织降低导航和职责辨认成本，不声称改变 crate 编译边界或自动减少编译时间。

各领域共享同一 SQLite 连接及事务。保留跨子域事务内的内部函数；不为拆目录制造第二数据库、额外提交或跨层依赖。首个实际窄存储边界为 WorkExecutionStore，直接拥有连接的独占借用、实现事件投递与租约方法；Host 专属 Runtime API 仅拿到该 store，无法在该闭包内调用作品写入。删除同名 Repository 广接口；一次性同步 Runtime 和现行测试。CandidateStore 由主代理同时实现，不重复维护。

模块划分：task、work、generation、conversation、manuscript、planning、knowledge、world、events、query、sources、verification、revision、recovery、media。真实 schema 校验、连接池和提交用例的四个进行中文件由主代理持有，完成后按协调路径组织，不修改其事务设计。

## 五视角前审（同一代理自审）
|视角|结论与依据|
|---|---|
|架构|APPROVE：真实模块拥有各自公开导出，连接借用限制当前消费者能力，跨域事务不拆。|
|技术A|APPROVE WITH CONDITIONS：Work owner/event 冲突、过期、租约与暂停取消回归须通过。|
|技术B|APPROVE WITH CONDITIONS：Core/Runtime 编译、测试和类型导出验证，处理相对共享文件引用。|
|产品|APPROVE：作者授权及创作流程不变，改善维护能力。|
|UI/UX|APPROVE：无界面变化，状态由原事实投影。|

零兼容约束：不保留旧 Work 事件 Repository 方法；根级公共 facade 是当前统一访问层，不维护重复实现或历史字段。仅路径与实现所有者变化；真实数据不迁移、不重建。

## 验证计划
E1 Core/Runtime cargo check；E2 WorkExecution 现行单元及 HTTP 集成覆盖归属、并发、租约、分页、幂等和取消；E3 architecture 与生成 DTO 门禁；E4 库单元回归确认目录/测试解析完整。后审补实际结果，不把静态检查作为真实模型或作者验收。

### 实施中设计收敛

进一步移出有实际 Runtime 消费者的三个边界：ConversationStore（对话及持久消息/作者历史，10方法）、TaskIntentStore（原任务追加与答复交付，5方法）、QualityStore（质量笔记、报告与读者证据提交，6方法）。实现直接拥有原 Connection 的独占借用；QualityStore 额外只借用原恢复绑定，并复用现有 connection 级任务输入校验及快照函数。无 Deref 暴露整个 Repository，无转发壳，无新增事实。公共请求结构和原事务不变；旧 Repository 同名方法删除，当前调用方一次同步。前审五视角同样适用，以现行这些子域单元/HTTP验证关闭条件。

其余跨域查询、计划/生成与知识审核仍共享 Repository 用例：其中组合当前输入验证、来源读取快照、产物和费用/恢复事实，不为了目录对称拆散事务。15目录组织不等于所有领域都已经完成窄store切换，God object 的剩余依赖需按具体消费者和事务所有者继续解耦。

## 实施后边界审查

当前已移入真实子目录的文件为 258 个，crate 根只保留统一导出和跨域基础设施。Rust 仍以 story-core crate 为编译单元；本次目录组织只改善代码定位与领域归属，没有缩小 crate 重编译范围。

|能力边界|现行消费者与约束|
|---|---|
|WorkExecutionStore（7 项操作）|Runtime 事件投递 API 仅获得该 store，执行事件查询、owner 与事件租约的获取/续期/释放及确认；不能借该参数执行作品修改。|
|ConversationStore（10 项操作）|对话、消息和作者历史 API 通过该 store 读取/更新；消息状态机仍在原事务内。|
|TaskIntentStore（5 项操作）|追加意图、作者答复及交付 API 通过该 store 执行；不对外提供原始 connection 或 Repository。|
|QualityStore（6 项操作）|质量笔记、packet、review 与读者证据 API 通过该 store 执行；仅借用必要恢复绑定，复用同连接的读取快照和任务输入校验。|
|CandidateStore|由提交用例方案覆盖草稿/产物提交的领域 ID 与 envelope 构造；与本项共享同一 Repository 连接基础设施。|

这些边界实现原 SQL/事务本身，并非调用 Repository 广接口的转发层。旧 Repository 同名方法已删除，生产消费者和测试同步切换。未引入 Deref、平行 DTO、兼容方法、额外连接或事务提交。

其余跨域用例保留的理由是原子性：例如输入版本校验与产物/恢复事实提交必须观察同一事务，组合查询必须共享读取快照。将这些用例机械分配给各自独立提交的 store 会造成检查与使用分离。后续解耦需先明确应用用例的事务所有者，并让领域函数借用同一事务；不能用为每目录生成一个无真实调用者的 trait 冒充边界完成。因此本次是多个高频边界的实际收敛，并非消除全部 Repository 耦合。

|五视角后审|结论与证据|
|---|---|
|架构|APPROVE：领域目录和公开导出明确，五个实际消费者边界限制可用操作；跨域原子性保留，编译收益不夸大。|
|技术 A|APPROVE WITH CONDITIONS：实现保持原事务及恢复绑定，仍须主集成运行事件租约与四个 store 的现行单元/HTTP回归。|
|技术 B|APPROVE WITH CONDITIONS：rustfmt 解析及 diff 空白检查通过；architecture 检查通过（714 文件、7 crate）；最终 Native 编译、测试及合同导出结果待主集成补充。|
|产品|APPROVE：没有改变作者授权、创作准入或数据生命周期；不因模块重排迁移真实数据。|
|UI/UX|APPROVE：HTTP 载荷和作者入口保持当前合同，错误与执行状态沿用原事实投影。|

验证分级：当前静态证据不能替代 Native 测试、真实 Runtime/MCP 联调或作者桌面验收。主集成结果完成后再关闭上述技术条件。

### Task 创建构造下沉补充前审

Runtime create_task 仍构造 task UUID、领域输入 manifest（含候选版本/分支上下文默认值）。将此构造移入 task 域的 `CreateTaskRequest::from_author_submission`：Core 接收已解析的作者输入，拥有 ID、目标规范化、manifest 序列化；Runtime 保留 HTTP 校验/错误映射与现有 target 校验、初始预算分支，现有创建事务/CAS 不变。内部输入结构不增加 serde/HTTP DTO，不泛化 ID 工具，也不转发 Repository。

五视角前审：架构 APPROVE（领域 ID/envelope 回归 Core）；技术A APPROVE WITH CONDITIONS（保留检查次序及预算事务，运行现行创建 HTTP 回归）；技术B APPROVE WITH CONDITIONS（构造单测核对目标与 manifest 同源、可选方法和ID独立性，统一Native编译）；产品 APPROVE（创建条件/错误不变）；UI/UX APPROVE（接口及作者操作不变）。

### 集成实证与技术条件关闭

- Core lib 502/502 通过，覆盖新增作者任务构造、Conversation/TaskIntent/Quality/WorkExecution 窄 store 的原有事务测试。作者意图集成 7/7、任务生命周期集成 11/11 通过（日志 `/tmp/host-core-recheck.log`、`/tmp/host-task-retry.log`）。
- Runtime revision_outcome 44/44、report_prewrite_recovery 7/7、work_budget_authorization 25/25 通过（`/tmp/host-native-fixtures.log`、`/tmp/host-budget-recheck.log`）。注入回滚测试现在先初始化 RuntimeState，再注入触发器并从同一数据库池获取连接；没有放宽启动 schema 校验或删除回滚断言。
- 校正三个与现行合同脱节的测试：分页使用实际创建的任务ID；累计作者历史允许超过单条上限并验证无丢失，另保留单条超限零变更断言；进程中断先结算可信失败用量，作者恢复不自动重跑 writer，旧 action 和 resumeStepId 均不能绕过领域恢复选择，原失败证据保留。产品逻辑未为旧断言改变。
- 备份 tar 采集跨越实施时段，包含部分新 Store 代码，不能视为精确实施前基线；未宣称基线隔离复现。上述校正依据当前领域明确规则与实际失败原因。

五视角技术A/技术B本项条件关闭为 APPROVE：上述现行单元、事务及相关HTTP验证通过；全工程最终合同导出/完整Native汇总由主集成记录。架构/产品/UI结论不变。作者任务构造前审条件由Core lib及Runtime任务生命周期相关集成关闭；不声明真实作者桌面端到端验收。
