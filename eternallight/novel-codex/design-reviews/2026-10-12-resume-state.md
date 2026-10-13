# 原运行续接状态合同

## 事实与方案（前审）
2026-10-12 05:21 UTC，维护后追加的作者消息建立 advisory task-7dca7a3e694a5546148cbe025eac3709。不是应用无故重复触发。read_work_status 两次返回原运行 running/version537；模型仍请求 resumeRun，Native 拒绝为 TaskBindingInvalid，误导模型诊断成绑定限制。原任务继续到第35章，拒绝未阻断生成。

根因：设计与实现缺口。拒绝运行中任务恢复是正确前置，但状态投影缺少明确的恢复选择状态，错误合并了身份不符与不需要恢复。不能通过放宽校验或重试解决。

责任：Native 从同一运行状态推导 resumeSelection，候选 paused/blocked 仅表示状态可选择，不授予权限或承诺可执行；running 为 notNeeded，其他状态 unavailable。事务内共享该判断，专用结构化冲突说明真实 runId/runStatus，保留前写拒绝账本。MCP 同步现有指南，不追加工作流提示。Host/Engine 不变，不自动改变作者意图或发起新的请求。

## 状态与验证
- running：notNeeded；不注册交接，返回 generation_resume_not_needed、/resumeRun、read_work_status；保存和账本不变。
- paused/blocked：candidate；仍校验范围、版本、身份、授权、前置成果和结算，绝非可直接执行。
- 其他：unavailable；拒绝交接，不扩大范围。
- 外项目/错误绑定、过期版本、范围变化继续拒绝；已有交接幂等与执行检查不变。
- E1：HTTP 真实读状态→错误恢复→无交接副作用→暂停后合法恢复；原有外项目/版本/范围及幂等回归。
- E2：Core 状态分类与前写拒绝结算、MCP传输、架构门禁；运行实例不为测量另开模型请求。

## 五视角自审
|视角|前审|后审|
|---|---|---|
|架构|APPROVE：领域状态归Native，其他层只传递|APPROVE：共享状态判断，架构门禁通过|
|技术A|APPROVE WITH CONDITIONS：事务重检、拒绝结算、并发边界需回归|APPROVE：HTTP 11项、Core 2项通过|
|技术B|APPROVE WITH CONDITIONS：HTTP及MCP错误内容需保留|APPROVE：MCP 7项通过，错误与拒绝回执完整传输|
|产品|APPROVE：减少错误恢复，不改正文、范围、用量或标准|APPROVE：仅改状态及诊断合同，无文学/预算门槛变化|
|UI/UX|APPROVE：明确无需恢复，不错误要求用户修复身份|APPROVE：错误定位原运行，维护后UI真实显示暂停|

零兼容前审 PASS：无迁移、双轨或旧版本分支。仅隔离数据库测试，不修改真实作品数据库。实际模型收益需部署后的自然样本，不宣称节省比例。

## 阶段证据
- E1 已通过：`cargo test -p novel-codex-story-runtime --test generation_handoff_tests` 11/11，日志 `/tmp/resume-state-http.log`。包含 running 的真实HTTP结构化409、公开状态/交接无变化、同任务暂停后恢复及原有范围/版本/幂等边界。
- 架构门禁通过：369文件、7 crates、零诊断；差异空白检查通过。
- 测试启动曾停在 `_dyld_start`，随后正常进入测试并通过；没有更改系统安全设置。不能将该等待归因于本次逻辑死锁。
- 只读生产观测 05:44:19 UTC：原运行 version563，36章完成，正在第37章；未发送第二条续接指令。

## 前审补充：工作暂停与章节运行状态
维护暂停先后两次观测分别读到 generation running/version572 与 Work paused/version796；它们不是同一快照。随后同一只读JOIN查询确认 generation paused/version573、Work paused，未证明持续不一致，不作为故障根因。状态投影仍显式返回 workStatus；notNeeded 仅限两者都 running，generation.running 但 Work paused/blocked 的相邻状态返回 unavailable，同一函数供投影和事务使用，错误保留两种状态。五视角自审均 APPROVE WITH CONDITIONS：增加双状态分类用例并重跑相关合同；不改已有工作控制或作者权限。

- 最终HTTP回归（含 workStatus 输出）11/11通过：`.release/resume-state-validation/http-final.log`。最终打包检查通过，完成于 2026-10-12T05:56:45.224Z；资源/最终应用启动验证见同目录 `package-report.json`。
- 维护边界：原运行 generation-d6db3e3374164a09809d9dadfc000ab8，paused/version573，完成37章，第38章已登记。第37章《有号无行》2854非空白字符，7项持久核验均 supported，14条引文均存在当前稿，未覆盖项为空。新版启动后UI显示任务已暂停；未追加模型恢复消息。

## 实施后复核
- 20项相关测试通过（HTTP11、Core2、MCP7），最终构建、架构门禁、差异检查、最终应用启动验证通过。均是隔离环境合同测试，不算真实模型效率样本。
- 代码、工具指南、公开状态、事务拒绝、HTTP诊断与MCP可见内容同步；新错误纳入前写拒绝结算，未知存储结果仍保留unknown。B01/B03/B04/B05/B06/B08相关边界 PASS，零兼容 PASS。
- 首次Core命令误选整包测试导致额外集成目标编译，已中止并改为 --lib；双状态假设修正期间中止一次过期测试运行。上述中止均不计入通过数量。最终日志以 *-final.log 为准。
- 未更改 Core/Engine、工作流方法、真实数据库或作者要求；未为验证新开模型请求；没有全章节省比例结论。
- 部署恢复核验已通过：使用 UI“重试当前任务”，未发送新的作者消息。原运行保持 generation-d6db3e3374164a09809d9dadfc000ab8，version575，Work/Run 均 running，完成37章，当前第38章，活动写作 task-04c6144a9b374f7583c87552e44fddb8。持久快照见 `.release/resume-state-validation/resumed.json`；UI已显示第38章并出现读取作者要求/资料目录活动。系统 NoIdleSleepAssertion 来自新版应用PID16972，显示器休眠未禁止。
- 真实模型是否减少无效交接尚待自然后续样本；本次恢复本身没有再次发出模型续接请求，不能把合同测试通过冒充完整章节成本改善。
