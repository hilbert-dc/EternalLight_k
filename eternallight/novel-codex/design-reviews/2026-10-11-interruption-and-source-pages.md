# 执行中断收尾与正文分页

## 根因与范围
2026-10-11《天衡诡契》第6章 Attempt task-13d89e7354ce4738be616ebbe1234736：Core 已 interrupted，Host 记录 timeout/cancelled，但 Native 仍 queued；重启才恢复为 failed，Generation/Work 仍 running，手动 next 后才 blocked。两者兼有：Host 用本地 cancellation flag 推断 Native 已终止（实现缺陷）；Generation 只在后续 next 同步失败，收尾与状态投影非原子（设计缺陷）。哪个上游事件首次触发取消尚无足够证据，不能将请求超时或机器休眠推测当成唯一根因。

主线程 rollout：最后一次已知输入322437 token；累计输入18928286，其中缓存18253056。总token不是实际费用，约96.4%输入命中缓存。失败writer有21次来源读取；parsedFields共用工作决策6.5KB预算，源13次续页，是可消除的往返。根因是正文与小型控制回执共用传输政策。历史增长是放大因素，现有Core有容量相关压缩；本次不另设token硬帽或改Core。

## 方案与权威
- Host：统一所有writer/advisory/review异常收尾。核对当前原Task身份，已完成/取消/失败不覆盖；仍active则持久failed，再按原预算结算。本地取消仅是停止执行的事实。若收尾未确认，保留原失败并明确显示状态待核对。
- Story Core：在原Attempt终态事务中同步当前Generation/Work受阻状态，保留activeTaskId、成果、作者意图、用量；不改变其他Attempt和作者paused/cancelled。后台根据现有领域恢复准入决定下一步。
- 当前进程异常中断及重启中断标记为execution_interrupted；Native仅在已终止、已登记用量、无未知写入及原预算允许下开放后续模型决策。传输cancelled费用回执不再混同作者Work取消。沿用已有无进展保护；不自动重放旧请求，不清零费用。
- Story Core来源读取：parsedFields独立63KiB（两层JSON包装最大4倍扩张并预留元数据，适配256KiB真实传输边界）页容量，不改控制回执预算。页大小是传输/往返折中，非内容总量上限；超页连续分页。Runtime最终转义256KiB检查、精确cursor、原作完整、跨页coverage/来源权限保持。用相同约5章正文实测页数与累计输入字节的减少，不能冒称真实费用降幅。
- Renderer仅消费Native状态和Host错误，不自推断恢复。Engine记账不变。无schema或历史兼容/迁移，未修改真实作品数据库。

## 判定表
|事实|结果与副作用|验证|
|作者已暂停/取消，Task已终态|不覆盖，不后台恢复，账本保留|Host收尾与Native反例|
|本机interrupted，Native仍active|精确原Task→failed，原Generation→blocked，不触发模型|实际Host回合恢复测试|
|已有正文或未知写入|保留正文/写入证据，不作为零写入重试|Native现有拒绝及新增反例|
|失败已终止、用量登记、无限预算有未知费用|原Work可按现有无进展门禁决定后续；未知费用保留|Native恢复测试|
|有限预算有未知费用|继续阻断|Native预算反例|
|晚到旧Attempt终止|不改变当前新Attempt/运行|Native身份反例|
|正文跨页或多语言转义|全量重建一致、覆盖正确、最终字节边界不变|真实来源分页/HTTP|

## 前审（Codex同一执行者五视角自审）
架构 APPROVE：B01/B03/B05/B06/B07/B08/B10，Host生命周期/Native事实/Engine用量职责不变。
技术A APPROVE WITH CONDITIONS：终态幂等、并发、暂停取消、未知写入/费用反例必须通过。
技术B APPROVE WITH CONDITIONS：实际mainHandlers、Native持久记录、HTTP来源合同，不能只mockhelper。
产品 APPROVE：保持正常功能、完整正文和原100章目标，恢复不要求作者追加技术指令。
UI/UX APPROVE WITH CONDITIONS：失败不能继续显示运行；实际应用入口需复验。

## 验证计划与后审
已实施：Host 收尾/回合恢复等 54 项、阅读合同相关 28 项；Native 分页 3 项、终态恢复 16 项、HTTP 上下文 15 项、连续执行 14 项与账本 8 项通过。真实 Host+Native 集成 5 个场景通过（最终构建首次 completed 场景受冷启动 60 秒测试超时影响，预热后单独重跑通过；原失败日志保留）。architecture:check、前端/Host 类型检查通过。恢复与分页版已打包通过 6 项门禁并在应用保存、直接核验第 9 章。Engine 时限修正另见下文；前 100 章尚未完成。

### 回归发现与设计补充
真实 HTTP 回归发现：作者取消事务先结束子 Attempt，再写父 Run，新增终态观察因此先推进父版本，导致同事务取消 CAS 冲突。修复为同一事务中先持久作者取消，再结束原子任务，失败仍整体回滚；不放宽 CAS。另一个回归为已 blocked 的 /next 提前返回时遗漏未知写入原因，改为读取实时写入/费用账本投影 stopReason，不缓存可失效原因。五视角补充前审同意，沿用取消原账本、未知写入拒绝和 HTTP 回归验证。

### 长篇读取根因复核与前审
进一步审计 context_read_api、task_workspace、task_knowledge_search、task_sources 和 context-reading-contract：已有冻结目录、按章节定位、原文搜索与同任务阅读回执；Native 不要求 writer 通读全部前文章节，直接验收只强制当前稿及作者原要求。实际 writer 自选全量前文，是模型策略，不应通过限制可读章节或降低验收解决。当前提示只明确“普通问答不默认遍历全书”，对首次起草的必要原稿与可选前文区别不够明确，属于执行契约表达缺陷。继续使用现有检索/覆盖架构，明确 coverage 是已交付范围，不是必须穷尽全库的待办；首次起草按本章承接、人物、物证、时间与因果疑问读取，证据不足继续扩读，作者明确全书审查时照办。无固定最近 N 章或 token 硬帽；既有原稿、保护范围、明确评估/抽取范围和验收完整读取不变。五视角补充前审 APPROVE WITH CONDITIONS：验证最终 Host 注入的原稿/保护/验收与按需读取条款并存；真实模型效果仅按实际调用记录报告，不能以提示存在宣称模型必然最优。

### Engine 时限职责修正：补充前审
审计发现 Bridge Control、通用 HTTP Transport、Loopback 等待响应头均把 requestTimeoutMs 当作整次模型响应截止。持续有数据的长响应和 responseBarrier 缓冲响应可能被错误中止；不能仅增加常数。Engine 负责通用通信活性，作者总时限仍由原 TaskBudget/lease 权威控制。目标合同：通用 HTTP 显式 timeoutMs 为可选绝对截止、idleTimeoutMs 为可选逐次网络/背压等待时限，至少设置一项；模型桥仅使用停滞时限。Host RPC 每次调用、上传与消费者背压仍有界。Bridge 所有者负责响应生命周期时，Loopback 不另设整次响应头截止，由所有者的有界阶段/授权取消/关闭收尾；独立 Loopback 保留其响应等待时限。排队受原租约/作者总时限及容量控制，不误计为供应商无响应。实际供应商超时保留未知用量、防重复请求门禁；没有证据不把所有历史超时归为健康流被截断。
五视角前审：架构 APPROVE（B01/B05/B06，通信策略仅在 Engine）；技术 A 有条件通过（健康长流、停滞头/体、背压、显式绝对截止、取消与字节限制）；技术 B 有条件通过（原生/自定义 fetch、进程桥、缓冲响应及用量恢复回归）；产品 APPROVE（功能按真实容量完成，不新增 token 帽）；UI/UX 有条件通过（原任务暂停后升级、恢复，区分真实正文保存与计划完成）。不增加领域流程、兼容层或费用推测。

### Engine 实施与验证
移除 Bridge Control 的整次响应 deadline；HttpTransport 增加独立 idleTimeoutMs，绝对 timeoutMs 可显式关闭，但至少一个有效时限必需。逐次等待响应头、数据块及有界消费者队列均受闲置时限；Bridge 写回消费者也独立有界。Loopback 的 owner_manages_response 仅 Rust 所有者内部设定（serde skip，外部不能关闭通用保护）；托管响应由 Bridge 负责结束，独立 listener 仍有响应等待截止。原作者 TaskBudget 截止和 lease 检查、取消、字节/并发边界不变。进程桥公开选项同步透传 requestTimeoutMs；首次进程停滞测试发现该选项原来被丢弃，修复后真实进程复验通过。
最终 87 项网络/桥接/原生生命周期/原生与进程预算测试通过；其中新 18 场景覆盖原生、自定义 fetch、进程桥的流式/缓冲/非流式、停滞、作者截止与取消。另 4 项进程预算测试通过。健康 fixture 每 40ms 提供数据、共约 480ms，在 200ms 停滞窗口下完整结算；显式绝对截止仍会切断持续流。该证据证明原整次截止设计会误伤长流，不证明每次真实 DeepSeek 超时都有持续数据。既有未知用量不重放、失败工具不释放等反例通过。
五视角后审：架构 APPROVE（职责权威、托管响应所有者、原任务事务）；技术 A APPROVE（取消 CAS 回归修复，有限/无限预算与未知写入保护）；技术 B APPROVE（真实 HTTP、进程桥、Unicode/转义边界和类型门禁）；产品 APPROVE（无新增内容/token 总量帽，保留原目标与原账本）；UI/UX APPROVE WITH CONDITIONS（最终包启动和原任务恢复需记录实际结果，不能将保存 9 章写作 100 章完成）。

最终包 2026-10-11T09:11:23Z：staging、runtime-verification、frontend-build（含类型检查）、packaging、packaged-resources、application-smoke 六项全部通过。新增调度回归 6 项通过（原生/进程 provider 与 budget 排队、取消、容量复用）；最终 Engine 相关共 97 项通过。应用使用当前发布包启动，模型环境证据变更后通过原应用验证入口重新验证中。原作品保留 9 章，原任务已暂停待接续。

### 最终应用验收与后审结论
应用内 DeepSeek 重新验证并应用成功；点击原任务“重试当前任务”后只读核对 Native：原 run generation-d6db3e3374164a09809d9dadfc000ab8、原 rootThreadId、intentGeneration=1 保持，completedChapters=9，currentChapterNumber=10，状态 running，已创建原运行下一 generateChapter Attempt。详细真实状态在 interruption-validation/application-resumed-final.json；没有直接写作品数据库。UI/UX 条件已满足，后审 APPROVE。新版真实第 10 章尚未完成；前 100 章目标继续，不能用回归通过宣称小说全部完成，也不能宣称已在真实模型证明所有超时消失。
