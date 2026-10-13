# Agent 三项现行合同 fixture 收口

## 前审
职责：Host 管执行与可信身份，Native 冻结作者要求，Engine 保留失败事实。仅改测试，不改生产。证据：advisory-coordinator 给原 Work 初始 target 加 defaultMediaBudget；composeNovelCodexPrompt 内部执行通过 authorRequirementsSource/read_source_content 懒读原文且 textDelivered:false；settleFailedTask 在读取到异线程任务时拒绝 PATCH，返回原错误 cause 与终态待核对诊断。

B01/B03/B04/B06：原 Work 测试精确验证新增有限媒体额度并保留单 Work/无新授权；generation 测试验证冻结全文与原作者绑定、分页读取描述及未声称送达，不再要求在触发消息重复全文；foreignTask 验证原失败 cause、身份待核对、无终态写入及无后续决策。未知终态不伪造已失败，不改权限或作者意图。

五视角前审：架构 APPROVE；技术A APPROVE（精确合同而非宽松匹配）；技术B APPROVE WITH CONDITIONS（三文件完整定向）；产品 APPROVE（行为不变）；UI/UX APPROVE（无界面改动）。

## 后审
三个完整测试文件 30/30 通过（/tmp/agent-three-fixtures.log）。原 Work 的单 Work、无二次作者批准与无新根额度断言保留；媒体上限和有效期精确验证。generation 四操作均保留 Native 冻结全文及 target 等价断言，并检查分页读取描述与来源绑定不丢失。foreignTask 检查原错误 cause、待核对原因、零终态写入及零后续决策。生产零改动；五视角后审均 APPROVE，技术B条件关闭。


## 派发前暂停/取消 fixture 补充前审
四个测试只把 Work 改为 paused/cancelled，却保留当前 task=queued，与 Core work_tasks::pause_active_attempts/control_work_task 的原子终止旧 Attempt 合同不一致。Host catch 读到非终态才 PATCH，并非覆盖 Native 已提交控制。模拟控制时同步 task=cancelled、stage/capabilityRevokedAt 及 step 状态；保留零模型派发、零Host控制/终态写入、零新预算断言，新增取消 task 仍保持终态。五视角同意，仅测试修正；条件为四项定向全部通过。

派发前控制补充后审：四项4/4通过（/tmp/host-pause-fixture.log），全部原拒绝写入断言保留；生产无改动，五视角均APPROVE。
