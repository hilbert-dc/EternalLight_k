# 保存结果核定与原任务恢复

## 真实故障和责任归属

原第2章独立检查任务的两次报告保存已登记accepted；请求中的候选版本与冻结合同不符，在领域写入前拒绝，却没有精确拒绝证明。current9已修正后续调用的参数反馈和原子拒绝记录。当前仍缺少对未决原调用的可信核定入口，导致原Work不能恢复。故障证据为 `/tmp/autonomous-v3-current8-chapter2-report-argument-evidence.json`，原调用、正文与累计费用保持；它只是诊断材料，不能作为Native拒绝权威。

本轮用户明确授权继续修复核定和恢复。范围仅为当前公开保存/恢复合同；不是数据库迁移、批量回填或根据旧版本猜测结局。B03/B06/B10：Native核定领域事实；Host从Codex Core公开历史取得原请求材料并传递，不能认定成功/失败；Engine仅投影Native证明；模型仍选择纠正、检查、修稿或交付。Codex Core与Engine代码不改，没有新模型工具、文学流程、作者输入或额外批准。

## 最小方案与公开契约

增加Author-only POST `/api/projects/{project_id}/tasks/recovery-operations/reconcile`，单次只接受一份精确原调用身份及完整MCP arguments，证据JSON上限128KiB，禁止截断后提交。输入不接受outcome、receipt或调用方自称的判决。GET精确操作读取保持只读；恢复快照的既有围栏与分页语义保持。

Native在同一Immediate事务中：

1. 读取既有原操作和Task，核对project/task/thread/turn/call/tool；重算原 `{tool,arguments}` 的serde_json canonical hash及幂等键引用，与原账本精确一致。保留JSON类型，原参数不保存到新的库、日志或返回值。
2. 既有committed/rejected原证明直接返回，保持幂等。accepted只能在任务已failed/cancelled且写权限已撤销时核定；运行中的任务不核定。当前候选任务completed应具备产物，completed但零产物属于矛盾状态，保持unknown。
3. 只支持既有Native原子候选保存合同，以已有candidate kind→tool映射核对required_output_kind；冻结manifest必须明确提供真实outputObjectId和可空expected候选值，输入版本权威为TaskRecord.input_revision。原请求的project/task/thread/input revision必须吻合。不要求manifest中不存在的inputRevision字段，也不使用fallback；不以工具名字前缀推断效果。
4. 本轮确证拒绝的范围严格收敛为该Task没有任何真实task output，output_refs也为空；原幂等键在candidate及commit两类回执中均不存在；原CAS与不可变冻结CAS不相同。因此该请求在这个Task任何可能保存时都不能通过保存前guard，才将这一精确accepted操作核定rejected。不是只凭报告不存在，也不特判字符串null。
5. 有自身历史输出时不使用“当前最后输出”重演旧调用；有任何相应回执或矛盾链时保留unknown。本轮不复制各工具不同的request_digest算法去重建accepted成功证明：成功丢响应已有同事务committed账本由原查询投影；无法证明的accepted仍未知。合法CAS但存储失败仍未知。
6. 唯一写入是原操作已证明的终态，不保存报告、不创建或复活Task、不改Work状态、权限、正文、当前head、费用或预算。Storage/Corrupt失败回滚，不推断结局。任务终态与保存均在同一数据库写锁下检查，不能与迟到写入竞态。

Host沿现有reconcileRecoveryIndex处理未完成运行：先读围栏快照，针对terminal Task的accepted操作，通过Adapter公开 `thread/read(includeTurns:true)` 取唯一匹配的原turn/call、novel_workspace工具和完整arguments；不使用12轮/8项的展示摘要、模型回复、工具报错文本或本地rollout解析。调用新核定入口后重新获取Native快照并投影Engine索引。缺失、重复、截断、不完整、过大或不匹配的历史材料不提交；查询失败保留阻断。单次扫描有界，不自动循环重试模型。

配置就绪后才可从原历史取得材料；启动先配置本地Core适配器，再核定，再启动既有事件消费者。结束/刷新路径复用同一个回调。该动作不打开创作turn，不请求provider，不改变当前对话选择。核定本身允许在暂停/取消后保留事实，但不能唤醒这些Work；原任务后续只通过既有明确“重试当前任务”及权限/预算/来源门禁恢复。

## 五视角前审

| 视角 | 结论 | 实施条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | Native唯一判决；Host仅传完整原请求；复用既有索引/快照，没有第二作品事实、Core改动或新创作循环。 |
| 技术A | APPROVE WITH CONDITIONS | 原hash/身份/幂等引用、sealed task、零自身输出、两类回执、不可变CAS全部同事务验证；有任何矛盾或合法CAS仍unknown，不能用absence单独拒绝。 |
| 技术B | APPROVE WITH CONDITIONS | 现有真实Runtime/SQLite回归复现未决原调用，Host合同验证精确历史与重新快照；不复制各工具请求digest，不把历史摘要当完整请求。 |
| 产品 | APPROVE WITH CONDITIONS | 修复原第2章，保留正文/范围/费用；用户无需技术指令；回归通过后还须实际应用原任务恢复和报告/交付证据。 |
| UI/UX | APPROVE | 不增加技术表单或新的批准；沿现有真实状态和重试入口显示结果，不从模型话语推断已交付。 |

综合：按上述窄范围实施；独立只读审计已确认关键边界，实施后另复核。零兼容PASS：无schema迁移、批量回填、旧字段fallback、历史版本识别或授权扩张。

独立前审（recovery_contract_adversarial）：无阻断，架构/技术A/技术B/产品 APPROVE WITH CONDITIONS，UI/UX APPROVE。条件为严格按TaskRecord.input_revision和原冻结明确CAS取证；missing不是null；保持既有MCP host_json与Engine canonical传输一致。为避免Native复制MCP私有算法，将现有纯operation digest实现移到Shared Rust源，MCP与Native同源引用，保留原数字互操作测试并补跨层实际合同验证；Shared不依赖应用层，Core/Engine生产代码不改。

## 故障对应回归及实际验收

| 原故障/边界 | 必需回归 | 实际证据 |
| --- | --- | --- |
| accepted原候选调用无法核定 | 真实报告和另一通用候选入口：任意错CAS与null/非空冻结值、同原hash核定rejected、两次原调用分别核定、重复核定不变。 | 对应真实Router/SQLite；Root构建运行，不能以静态审查冒充通过。 |
| 原材料不能认证、回放前置、成功与未知边界 | 修改JSON类型/参数/hash/身份/键、Candidate越权、running/waiting、已有Task输出/回执、真实Storage+合法CAS、损坏/核定写失败仍未知；已有committed证明和后来head变化不被改写；正文/Task/Work/head/预算不变。 | 复用既有recovery_ledger/report HTTP测试，不删除未知或付款负例。 |
| Host展示摘要丢参数，围栏仍旧 | 精确原turn/call唯一匹配、原参数JSON类型保持；缺失/重复/截断/过大/foreign server不提交；只terminal接受核定，核定后重新冻结并投影；缺原参数仍阻断，不调用模型。 | 现有Adapter/恢复测试按范围验证；Native与Host调用方一次性同步。 |
| 原应用未交付 | 新构建正常加载；从原公开历史取得两个原调用；Native核定后原账本保留、状态有可信变化；原Work正常Retry，模型自行保存检查报告、核验和交付可打开正文。 | Root实际UI观察，单条原第2章作者输入不重发，不给模型纠错答案；原费用/未知用量保留，结果如未完成须明确记录。 |

## 实施后

生产实现已冻结，清单为 `/tmp/autonomous-v3-current10-reconciliation-production-frozen.json`。Native增加单事务核定入口，Runtime暴露Author-only公开POST；Host从完整公开历史取证并重新冻结状态，启动在Adapter配置后核定；Shared复用原MCP纯摘要算法。Codex Core、Engine与Renderer生产代码未改，CLI SHA仍为 `2b7989c0a1264041227503e7d6afdd375fe4cbde38ba1fb8ffefacaa1005479b`。

独立后审架构、技术A、技术B、产品与UI/UX均静态APPROVE；以下实际原任务路径完成后，产品验收记为PASS。核定只证明原请求不能通过该Task的不可变CAS，不能证明payload文学有效，也不增加工具特有的payload或路由推导器；其他前置守卫只可能更早拒绝。

Root实际完成：Native构建exit0（1m26s）；Native Story Core完整266/266 exit0；架构332文件、7crate、0diagnostic exit0；Host三个完整受影响目标53/53 exit0，包含真实Runtime/MCP丢Host事件后的保存重启回执恢复；MCP摘要定向1/1 exit0；两完整Runtime HTTP目标14/14 exit0。构建、源码、测试和实际二进制身份及各日志统一记录在 `/tmp/autonomous-v3-current10-reconciliation-build-proof.json`。原生编译和进程启动等待保留在实际日志，不能计作测试失败或跳过。

保留验证失败：Host首跑47/53（受限环境Runtime监听失败，旧事件fixture漏真实root身份）；第二次受控loopback52/53（新建作品fixture缺当前必需DTO）。只修测试为当前公开身份和title/logline/medium/idempotencyKey，生产SHA未变、原断言未放宽；真实保存前后正文payload一致、崩溃exit17、exact proof及unknown边界保持。两个失败日志 `/tmp/autonomous-v3-current10-reconciliation-host-tests.log`、`/tmp/autonomous-v3-current10-reconciliation-host-tests2.log` 保留。对应测试小修经独立静态APPROVE，属于当前协议验证，未加兼容。

## 实际原应用验收：PASS

使用原验收profile `/private/tmp/novel-autonomous-v3-live-final13-20261008`、原作者对话和第2章Work。启动自动从Codex Core公开完整历史取得两份原请求；Native分别产生精确拒绝证明。原29条操作全部保留，仅两条原accepted的state和updated_at变化，原Task、Work、正文、报告、作者记录及Native/Engine费用不变。核定前后证明为 `/tmp/autonomous-v3-current10-original-operation-reconciliation-proof.json`；没有直接SQL改账、代写报告或模型参与存储判决。

Root随后在实际应用中只点击一次原“重试当前任务”，没有重发作者要求或追加技术纠错指令。真实DeepSeek自行读取原文与冻结标准，收到明确字段反馈后纠正JSON版本参数，再完成独立报告保存和任务完成。原Work的持久事件由既有consumer交回模型决策，模型完整读取报告与作者要求后，自行登记finishChapter并提交fulfilled交付评审。该过程保留两次新的确定拒绝及后续成功记录；不能将自行纠错描述为没有失败。

报告 `revision-outcome-482d504675944404954b7cfd5ee25642` 绑定当前原稿及原writer；critic与writer线程独立、均为当前attempt。四项冻结标准均supported，0作者遗漏、0issue；真实finish回执authority为persistedDraftAndIndependentModelAssessment，最终评审引用同Project/Work的真实正文与报告、0未决。原Run/Work completed，当前24committed/11rejected、0accepted、0活动Task或预留。完整持久证据为 `/tmp/autonomous-v3-current10-original-chapter2-terminal.json` 与 `/tmp/autonomous-v3-current10-original-chapter2-delivery-proof.json`。

Root实际UI确认“已保存章节检查结果”“第2章·当前稿检查通过”和“本次已保存1章”，展开检查详情为四条可读中文标准与证据，再从成果按钮打开《槐树下的鞋》1100字正文。第1章839字及第2章1100字的revision/digest均未变化，正文未重写、未采用、未发布；作者元数据仍为原4条，未新增作者输入。应用保留打开的正文与原对话供用户查看。

独立只读后审核对上述固定JSON及基线：原身份/hash保留，报告、finish与fulfilled评审精确同稿同标准；原12条Native charge和12条Engine预算记录完全保留。新增两个attempt各12次请求，334643/527940已知tokens，合计862583、0新增unknown；历史第1章112525未知估计保持。此次核定及单次原任务恢复闭环，不代表文学质量全面通过、非空作者遗漏修正、实际正文修稿、全部暂停/中断、多章、发布或300万字目标已验收。
