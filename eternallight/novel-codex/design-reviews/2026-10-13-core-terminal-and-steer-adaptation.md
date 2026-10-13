# Core 终止失败与轮次收尾适配

执行者：Codex 主代理，五视角自审；不是五人独立复核。上游 `12cca49c8` 已完整合入 `4ccbfbdf3`，构建身份 `640b0ac5e01ea866565b0238d378de196f211a772b96a4f5d11969a50ff7a2f2`。

## 目标与责任

目标：Core 终止失败不被 Host 自动新建尝试；确定未投递与投递未知分别保留；原作者要求、保存回执、权限及累计用量不改变。非目标：新增固定创作流程、清零费用、自动扩大权限、安装跨运行时消息板、修改 Core 或为旧二进制增加兼容。

缺陷判断：实现与设计两者兼有。Host/Engine 丢弃 JSON-RPC 错误码是实现缺陷；产品将任意失败纳入隐式后续尝试、没有可信终止边界，是设计缺陷。源码证据为 `codex-adapter.mjs` 的请求拒绝和终止事件处理、`creative-task-coordinator.mjs` 的 `continueFailedCreativeWork`、Engine `rpc_pending.rs` 和 `app_client_events.rs`。尚未将源码风险冒称为真实作者故障；合同复现见下方证据。

Native 已有 `retain_unsent_task_intent`：原 intent 的 pending→notSent 与原 Work 暂停/撤销在一个事务中完成，并禁止未确认追加要求下的完成/写入。保留该事实权威，不另建消息队列。Host 只把确切 `turn/steer` 响应中 `-32600` 的 InvalidRequest 转为 notSent；不解析错误文案，超时、断连、丢回复均保持未知。所有转发仍绑定原连接代次、线程、轮次、租约和作者 intent。

Core 已处理采样重试，matched `turn/completed(status=failed)` 是对 Host 的终止事实。Host 使用进程内可信登记识别该事实，保存有界线程/轮次/诊断，阻止失败后的自动续跑。供应商字符串、任意异常或伪造属性不能获得该可信登记。现有本地有界输出纠正属于已明确准入的独立路径，仍受原任务和费用约束。手动恢复仍经 Native 原 Work 的恢复准入。

实施中的设计复核：仅阻止前台 Host 续跑不足以保护重启后的后台唤醒；同时，把终止错误视为永远不可恢复又会阻断作者显式恢复。因此将可信终止诊断保存为 `core_turn_failed; 原诊断`：Native 识别此原因供显式恢复准入核对，但拒绝其旧 durable wake，返回 `authorResumeRequired`。不新增数据库 schema、重复费用账本或永久锁死任务。原写入/费用未知、暂停/取消、授权与版本检查仍先后执行，不因诊断可识别就获得执行权。补充五视角自审：结构沿 B03/B05/B06，使用既有错误和事件合同；E3 增加真实数据库重新打开后的唤醒零租约/零 Work 变更验证。

同工作区同期的上下文恢复改动予以保留：只有明确的 `contextWindowExceeded` 与其独立、受控的同线程压缩路径可尝试容量恢复；它不是供应商 `response.incomplete`，不能把普通终止失败升级为容量恢复。最终验证使用共同源码快照。

公开合同：JSON-RPC `turn/steer` 与 `turn/completed` → Engine/Host → Native intent/任务/费用 → 受信 IPC → preload → Renderer。Core 的 Other 不等于已知 provider failure reason，不按自然语言升级原因；错误投影仅说明 Core 本轮失败及禁止隐式接续。错误通过真实 IPC 值往返，不依赖跨进程 Error 原型或对象身份。

## 判定表

| 受信输入与权威 | 结果 | 原事实/副作用 | 回归 |
| --- | --- | --- | --- |
| 原租约/轮次有效且 steer 被接受 | delivered | 原 intent 原样绑定；不新建任务 | 现有追加输入正常路径 |
| 对应 steer 的 Core InvalidRequest | notSent | Native 保留 intent、暂停原 Work；不标记 delivered，不自动重发 | Host 与 Native 实际路径 |
| 未发送前本地撤销 | notSent | 沿用现有 Native 事务 | 原 notSent 回归 |
| 超时/断连/响应丢失 | 未知 | pending 与阻塞保持；不得宣称 notSent | 原 unknown 回归与相邻错误负例 |
| matched Core failed terminal | 停止自动接续 | 原保存和费用继续结算；不替作者恢复 | 终止失败与出站调用次数 |
| Native 持久记录 core_turn_failed 的旧唤醒 | authorResumeRequired | 不颁发租约、不更改 Work/额度；显式恢复仍需当前准入 | 真实数据库重开后的拒绝 |
| 外来文本/异常伪造终止属性 | 普通错误 | 不授予可信来源 | 伪造反例 |
| 原消息/连接/线程已变化 | 拒绝/忽略晚到响应 | 不影响新实例 | 现有代次与绑定合同 |

权限冷恢复、连接回复和远程退出使用已经合入的上游实现，做合同验证而不复制实现。独立消息板缺少当前产品消费需求，不增加共享授权。代理安全和 TUI 不新增 Electron 入口。

## 五视角前审

| 视角 | 结论 | 依据与关闭条件 |
| --- | --- | --- |
| 架构 | APPROVE | B01/B03/B04/B06：Core 历史、Engine 通用执行、Native 领域事实分离；不另存第二份结果/费用。 |
| 技术 A | APPROVE WITH CONDITIONS | 精确关联拒绝；未知零误分类；取消/费用/保存不被自动继续绕过。实际合同和拒绝矩阵通过后关闭。 |
| 技术 B | APPROVE WITH CONDITIONS | 生产者/IPC/消费者同步，Engine 重建、类型/架构及实际应用 Host 路径验证后关闭。 |
| 产品 | APPROVE | 作者追加原文保留、失败可解释，继续原任务仍经过现有准入，无重复确认。 |
| UI/UX | APPROVE WITH CONDITIONS | 复用当前对话错误/追加要求状态，无新按钮面板；验证序列化后状态及说明可读，不展示未确认成功。 |

零兼容前审 PASS：无版本探测、历史 fallback、双轨或用户资料重建。当前身份、版本、预算与未知副作用校验保留。

## 验证计划及后审

E1：Host 实际 Adapter 输入/错误/事件合同与假来源拒绝。
E2：Engine 真实 JSONL 进程、拒绝/未知和租约合同，重建绑定。
E3：受信 IPC→preload 序列化→Renderer 纯投影；Native 原 intent/Work/用量回归。
E4：产品自动接续零新尝试及原保存保留；空环境 Core 权限/历史基础合同。
E5：类型、architecture:check、Fork 门禁及差异检查。

以上为前审计划，未执行项不记通过。后审结果实施后补充；受控服务不证明真实供应商行为、文学质量或外部发布。

### 已执行证据（2026-10-13）

- E1：最终源码上的 Host 实际 Adapter 与 terminal-adaptation 合计 48/48 通过。覆盖 matched failed 后保留健康进程/原线程、后续显式成功清除当前 transport 错误、`-32600` 与 `-32603` 分离、伪造属性/诊断拒绝、旧连接同 ID 晚到事件不影响替换轮次。
- E2：Engine release 重建成功；真实 JSONL 进程与 app-server-client 合计 16/16 通过。错误码只用于精确请求拒绝，Engine 无新增领域恢复决策。
- E3/E4：最终源码与重建运行时下 Host 70/70 通过，包括真实 Electron contextIsolation/preload 值往返、Native 未送达/未知 intent、原 Work 重启后显式继续，以及真实 Core 冷恢复 readOnly/never 权限、历史 CAS/item 读取。隔离构建目录中的 Native 持久事件整组 21/21 通过，包含新增 core-terminal 单测、数据库重开、原 Work/费用零变更、手动恢复资格及 authorStopped 优先级。恢复资格不等于免除现行授权、未知费用或写入检查。
- E5：最终源码 Host 类型覆盖 126 文件零问题、前端 `tsc -b`、架构扫描 1408 文件/7 crates 零诊断、`repo:check` strictReady=true；适配路径差异空白检查通过。门禁反映检查时共享工作区，不宣称其他并行改动由本任务实现。
- 运行时重建成功；统一脚本的重复 pretest 被另一项共享构建锁挡住，因此停止仅本任务的排队构建，分开运行同一 Host/Engine 合同和隔离目录 Native 整组，合计 107/107 通过。不将未完成的统一脚本冒称通过。等待期间采样仅见 `_dyld_start`；未移除系统安全属性或绕过加载检查，也未中断另一项构建。

测试修正记录：Core 冷恢复最初使用未持久化空线程、随后通过 Adapter 选到了不同线程；改为官方公共 RPC 在原线程记录一轮后再停止/修改默认权限/冷恢复，最终验证的是同一持久线程。属于验收前提修正，不冒称 Core 缺陷。执行中保留同期明确 contextWindowExceeded 恢复路径，移除合并时重复 import；普通 terminal 与 response.incomplete 不纳入该容量恢复。

Native 初版夹具沿用了 completed 用量回执，`retryable` 正确拒绝失败恢复，因此初次单测失败。增加 fixture 的结算状态参数，为本用例使用真实 failed 回执后重跑通过；未放宽生产恢复规则。受控本地模型服务未产生付费模型调用，也不证明真实供应商流式行为或文学质量；未打包、提交或发布。

### 五视角后审

架构 APPROVE：职责、原事实权威和零兼容边界不变。技术 A APPROVE：拒绝/未知分离、原保存/预算/暂停与终止防护由实际合同覆盖。技术 B APPROVE：Engine/运行时重建、IPC 值往返、类型与门禁通过。产品 APPROVE：作者原要求保留，后台停止不等于任务永久不可恢复。UI/UX APPROVE：沿用当前对话错误与恢复说明，跨桥错误保留诊断、不显示未确认送达。以上为主代理自审；无需因补丁新增人工审批面板。
