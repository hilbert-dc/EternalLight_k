# 真实模型任务体验验收

## 范围与授权
用户授权隔离虚构作品的启动交付、停止、插问返回、要求变更、并发及恢复验收；后续明确取消 token 上限。每项最多 6 次供应商请求，连接资格检查另计；未知用量如实保留。禁止采用/发布真实作品，禁止自动增加额度。

## 前审（同一执行者五视角自审）
- 架构 APPROVE WITH CONDITIONS：脚本仅驱动生产 Electron preload/main、Core、MCP、Native；作品事实和用量分别读取 Native 与 Engine。测试边界仅在本机 Runtime 创建初始预算时收紧请求数为 6，token 保持 null，禁止二次根预算及预算追加。不会改变模型回答或工具结果。
- 技术A APPROVE WITH CONDITIONS：新临时 userData、只发送虚构作品；凭据不写报告；每步保存诊断，超时停止并清理进程，隔离数据保留以便核查。
- 技术B APPROVE WITH CONDITIONS：先实际资格检查，再运行真实供应商；不注入合格证、不替换模型。与浏览器手动点击/视觉验收区别说明。
- 产品 APPROVE WITH CONDITIONS：每项分别判定，前置未满足记受阻，不以停止或预算耗尽冒充交付成功。
- UI/UX APPROVE WITH CONDITIONS：走真实 preload IPC，检查状态与持久回执；不宣称真实输入法、视觉或 Windows 覆盖。

## 验证计划
脚本语法检查；真实 Electron 运行，逐项报告状态和 Native 结果；预算账本记录实际请求、tokens、未知用量。运行后补后审及失败定位。生产代码暂不修改，零兼容门禁 PASS。

## 实际执行 216 / 217

2026-10-10，macOS arm64，生产 Electron preload/main + 实际 Core/MCP/Native + DeepSeek flash（low）。未修改生产代码。216 使用全新临时 userData；217 复用其真实认证与隔离数据，只补插问。token 上限全部为 null；216 插问消耗 2 次后，217 只授予剩余 4 次，未重置该项总请求额度。资格检查使用产品内建检查器，其自带的测试预算仍存在，与此次任务预算分别记录。

| 项目 | 真实证据 | 判定 |
| --- | --- | --- |
| 连接资格 | 基础协议、Core 工具循环与委派资格返回 verified | 通过；不等于任务交付通过 |
| 启动并保存正文 | 6 次请求、185,253 已知 tokens；模型完成来源读取、指南读取、发现，并登记一章生成提案；没有正文候选，下一请求被 request cap 拒绝 | 未完成；不是 token cap。提案尚需应用内范围批准与实际 GenerationRun，本轮未进入写作阶段 |
| 执行中停止 | IPC interrupted=true，Native Work paused；1 次已计量请求、23,930 tokens；确认停止后 1.5 秒内 snapshot/digest 未变 | 有界样本通过，不等于所有停止竞态覆盖 |
| 插问及返回 | 216 探针 focus=null 被校验拒绝，属于探针错误；217 改为合法 brief/readOnly 页面并使用只读父任务。父回答 3 次、插问回答 1 次，共 113,715 tokens；两条消息 delivered，两步骤 completed；interlude 仍 open、Work running | 回答通过，自动返回未通过验收。剩余 4 次耗尽；Native return_interlude 会检查 continuation_block_reason，不能冒充恢复完成 |
| 中途改要求 | steer 回执成功，作者“保留密函、不能烧掉”要求持久化 delivered；原请求 bridge_error 后产生未知用量，后续请求被 task_provider_usage_unknown 拒绝 | 仅投递通过，后续创作采用要求未证明。自动恢复启动过，但首轮探针结束打断了观察，不能据此认定产品最终恢复失败 |
| 作者修改后恢复 | 停止后通过正式保存入口写入“作者并发标记”候选；恢复被 inputChanged 拒绝，Work paused，作者候选存在 | 并发保护样本通过；连续恢复交付未完成。这里是停止后的来源变更，不是真实断网故障，不是执行中同时写入的完整竞争验收 |

### 费用与用量

Engine 持久账本总计 **19 次任务供应商请求、427,678 已知 tokens**；另有 **146,924 tokens 的未知用量预留估算**，不能当作实际收费 tokens，也不能当作零费用。包括首轮探针失败和恢复期间消耗。无在途预留。资格检查另计，未获得供应商货币账单，不能给出精确金额。所有任务账本 maxTokens=null。

### 证据与执行限制

- [216 原始报告](evidence/2026-10-10-task-live216-raw.json)
- [Native / Engine 只读账本快照](evidence/2026-10-10-task-live216-ledgers.json)：含已知/未知用量、作者要求、消息状态、Work 状态及 217 执行脚本指纹。
- 本轮是自动驱动真实桌面公开 API；没有人工视觉、输入法、Windows 验收。
- 216 仅在 sendPrompt 完成后检查当前项目的活动执行，未完整等待改要求后的后台恢复；关闭应用造成一个恢复 attempt 的 recovered_unknown。它是验收观测缺口，不作为产品崩溃证据。
- 217 改用只读父任务以便在插问剩余额度内验证消息，不宣称验证“写作中插问后继续保存”。
- 本次没有绕过资格、工具身份、来源 CAS 或采用权限。Native HTTP 初始预算只被脚本收紧请求数；没有改写模型输出。原始脚本结果中 needsReview/failed 以本报告与持久账本的逐项判定为准。

## 后审（同一执行者五视角自审）

架构、技术A：APPROVE WITH CONDITIONS，隔离与事实权威保持，未知用量保留；预算不扩大。技术B、产品、UI/UX：BLOCK 完整验收结论，因为正文交付、恢复后执行和插问返回没有完整证据，存在明确脚本观测缺口。不能用本地全绿或真实连接资格通过替代完整产品体验。

脚本语法检查通过；architecture:check 364 文件、7 crates、0 diagnostics。零兼容 PASS。未提交/推送。当前综合结论：**真实模型验收已执行，完整任务体验尚未通过**。

后续应优先补齐交付提案→批准→实际生成的脚本路径，以及后台执行结束/预算结算的观察和清理，再针对中途改要求产生的 unknown usage 调查请求中断与结算。不得直接清空未知用量或假造交付来通过测试。继续付费执行须遵守本轮剩余额度；已耗尽的项不能自动追加请求。

### 217 收尾证据与探针后修

[217 原始报告](evidence/2026-10-10-task-live217-raw.json)最终为 `scenario wait timeout`。Native 持久账本证明回答已完成；运行日志另记录自动唤醒 `WorkTaskStale`。请求额度耗尽是已确认事实，返回门禁会检查额度是源码证据，但不能仅据这两项断言它是本次未返回的唯一原因；需连同执行事件 claim 冲突排查。

执行版本已保存为 [桌面探针](evidence/task-live217-task-live-desktop.mjs.txt)和[启动脚本](evidence/task-live217-probe-live-task-experience.mjs.txt)，对应账本中的 SHA256。结束后仅修正探针退出码（未通过返回非零）、请求参数范围校验和异常时的公开 API 状态快照；两文件语法检查与 diff 检查通过，未再次调用模型。原始运行进程 exitCode=0 仅代表正常关闭，不表示场景通过。后修后 architecture:check 再次通过。
