# 执行结束与后台扫描状态隔离

## 前审（2026-10-11，Codex 五视角自审）

实际触发：《天衡诡契》经应用 DeepSeek advisory 已保存答复及 generation handoff proposed，但 UI 随即显示「创作引擎 已关闭」，因 failed 状态隐藏批准入口。Host disposeExecutionDomain 在正常结算后调用 adapter.close，close 无条件 fail 并发 agent/error。另一个作品缺少 novel scope 的事件读取错误逃逸项目循环，被登记成 projectId=null 的全局目录错误，阻断其他项目扫描。原失败保留在应用历史及本地 application.log。

根因分类：实现缺陷。现有设计区分领域任务结果、进程生命周期与按项目扫描；实现将空闲资源释放等同执行失败，将项目请求错误等同目录故障。无需新增领域状态、兼容层或权限。

责任：Electron Host（B03/B06/B08/B10）。adapter 负责有未完成请求时拒绝并报告关闭；正常空闲关闭只释放进程，不改已完成结果、不覆盖首个错误。Native 仍为提案、Work、权限及用量事实权威。Consumer 为每个项目独立记录已有 health 合同、退避与扫描游标；目录失败保留全局错误。Renderer 按现有项目身份投影，不忽略真实失败。其他旧作品不迁移。

公开合同：adapter onEvent 的 agent/error；WorkExecutionEventConsumer request、healthChanged、failed 和 /api/projects/:id/work-execution-events。调用方包括 Host execution domains、UI agent reducer 和恢复提示。正常关闭不新增事件；在途请求仍失败，既有 API 形状不变。模型继续决定创作组织；工程不凭自然语言批准，不新增预算、采用或发布。

| 输入事实 | 结果 | 副作用/下一步 |
| --- | --- | --- |
| 回合完成且无在途请求后 close | 保留 completed | 释放资源；既有提案批准 UI 可见 |
| 有等待回合/请求时 close | 拒绝等待并报告真实关闭 | 不声称保存或交付成功 |
| 已失败后空闲 close | 保留首因 | 不发第二个虚假关闭错误 |
| 项目 A 扫描拒绝、B 正常 | health 绑定 A，继续 B | A 退避；不改数据、不授予权限 |
| 目录读取失败 | 全局 health | 整次扫描退避 |
| 项目 A 后续读取恢复 | 清 A 扫描错误 | 原身份与原事件准入不变 |

五视角前审：架构 APPROVE（Host 生命周期边界）；技术 A APPROVE WITH CONDITIONS（在途拒绝、错误首因及隔离回归）；技术 B APPROVE WITH CONDITIONS（真实 adapter fixture 和消费器合同、架构门禁）；产品 APPROVE（消除虚假阻断，不冒充百章完成）；UI/UX APPROVE WITH CONDITIONS（实际应用看到批准入口，正常结束不失败）。条件关闭需后审证据。

验证计划：adapter 完成后关闭/在途关闭/真实错误后关闭；消费者项目失败、健康项目继续、退避与恢复、目录失败。运行受影响现有合同与 architecture:check，重新打包后从真实应用验证批准入口与启动运行。零兼容：不新增历史迁移或 fallback，不改作者数据库、授权、费用或来源条件。

## 后审
待实施与验证；前审不代表验收通过。

E1 合同通过：`node --test electron/tests/codex-adapter.test.mjs electron/tests/work-execution-events.test.mjs electron/tests/execution-domains.test.mjs`，78/78。真实 app-server fixture 完成后重复 close 无 agent/error；在途请求 close 仍拒绝；首因保留。项目扫描失败不阻断健康项目 claim/resume/ack，退避后恢复清对应 health；目录失败仍全局退避；暂停、取消、未知结果等既有拒绝回归保持。

E2 集成通过：真实 Host handlers、execution-failure-boundary、execution-scope-ui 共 7/7。第一次沙箱内 Electron SIGABRT，按同命令在允许启动桌面进程的环境重跑通过（本地日志 `.release/tianheng-migration-20261011/cleanup-integration-tests.log`）。

E3 静态及产物：architecture:check 365 files/7 crates 无诊断。package:dir 全部阶段通过，2026-10-11T00:33:54.416Z；verify:package 通过。使用已验证 debug Runtime/Core；本次仅 Host JS 改动，Core build identity 沿用 `16a9358ffa7c5b8cd275d915f126391c5dc4829885f54c12fadb61ac03833f30`。本地包不等于正式签名、公证发布。

E4 应用再验进行中：新包日志证明旧作品 scope 错误已绑定各自 projectId，不再产生目录级全局错误。输入法 typeText 丢失中文的一次回合由验收者停止，完整中文改用 paste；应用按原消息交付门禁阻止后续派发，点击原任务恢复后继续核对。此操作不计为无人干预闭环。批准入口及真实百章生成仍待验证，未宣称完成。

后审：架构/技术 A/技术 B APPROVE（E1–E3；Host 边界、在途拒绝和原账本不变），产品/UI APPROVE WITH CONDITIONS（E4 尚待真实批准与启动）。零兼容 PASS；未迁移其他作品，未修改权限、预算或正文。

## 同链路补充前审：后台完成提案丢失

真实再验：原回合恢复后，通过「核对交付」取得原 task 的后继 turn 证明，原消息已交付，下一条完整中文消息自动开始并完成，提案仍未显示。源码确认 ConversationMessages.dispatch 丢弃 execute 返回值，agent/message-ended 只传错误；App 只有同步 sendPrompt Promise 接收 generationHandoff。属于设计与实现兼有：后台完成合同缺少仍需作者处理的领域结果投影，调用方不同导致丢失同一 Native 提案。

责任仍为 Host/Renderer，延伸 E4 同一交接链路：消息完成传递既有 Native generationHandoff（只限成功、已交付、与消息 task/project/thread/原文一致），Renderer 核对当前 request/scope/任务身份才显示。同步与后台共用同一个纯提案绑定检查。不把完成通知当作作者批准，不创建或修改提案、预算。事件形状增加可空提案字段；旧实现直接替换，不留兼容双轨。失败、外项目、外任务、不同原文、resumeRun 提案不得显示成新范围批准；重复完成不执行生成，批准仍过现有 workspace CAS。

五视角补充前审自审：架构 APPROVE；技术 A/B、产品、UI APPROVE WITH CONDITIONS（transport→Host→App 回归，后台真实应用再验）。补充验证：消息成功返回值、失败零提案，真实 App 后台完成批准入口与外身份拒绝、原批准仅一次及来源过期拒绝。重启后的历史待批提案重新发现不在本次活跃完成事件修复声明内。

E5 补充合同与 UI 通过：ConversationMessages + execution-domains-handlers 19/19；真实 Host 原 dispatch→Native fixture 回执→agent/message-ended，只有 delivered 且 task/project/thread/authorIntent 相符的提案通过，外任务回执为 null；完成后无虚假 agent/error。实际 App Electron 测试通过（conversation-generation-handoff-ui），新增后台成功显示批准、外项目/外对话/外任务/不同原文/resumeRun/失败拒绝，并保留既有来源过期拒绝与一次批准只启动一个 run 的用例。后台完成本身零生成。tsc -b 与 architecture:check（366 files/7 crates）通过。

E5 后审：五视角自审技术与合同条件关闭；同一纯绑定函数同时用于同步与后台显示，Native 授权及 CAS 未变。真实模型的新版批准入口和实际生成启动仍待 E6，不能由受控 UI 通过替代。

E6 实际应用：第二次修复包 package:dir 2026-10-11T00:44:22.128Z 全阶段通过，verify:package 通过。DeepSeek 真实回合结束后不再报关闭错误，UI 显示「生成 99 章工作稿」「批准生成 99 章」及真实用量提示。验收者沿用户已有授权点击批准；UI 转为「正在接续任务」，批准卡消失、编辑禁用、停止控件可用。交接入口故障已闭合；正在进一步核对 Native run 与实际保存，尚不证明 99 章完成。后台传递的真实模型路径由 E5 控制链验证，E6 本轮为前台原请求完成；不混称全部后台真实模型场景已覆盖。

五视角后审：生命周期误报和活跃完成提案传递 APPROVE（E1–E6），项目扫描隔离实际日志已验证。前 100 章完整创作仍为用户任务未完成范围；作品保护、采用/发布及额度门禁保持。
