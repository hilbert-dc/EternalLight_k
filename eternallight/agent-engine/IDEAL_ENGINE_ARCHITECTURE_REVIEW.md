# 多模型 Codex 引擎：理想架构对齐评审与落地计划

日期：2026-09-20  
评审基线：[理想架构 v1.1](IDEAL_ENGINE_ARCHITECTURE.md)，当前工作区 HEAD `1ec9f72a7` 加未提交修改。  
范围：Agent Engine、Novel Codex 生产接入、认证与评测入口、上游同步流程。  
性质：架构基线、实施追踪与本机确定性验证；不等同于真实供应商或双平台发布认证。

## 0. 实施进度（2026-09-20）

本文形成后已直接进入实现，不再设置人工评审角色或评审门禁。当前代码状态如下：

| 实施包 | 已落地 | 尚未完成 |
| --- | --- | --- |
| 1 安全止血 | Run 预占位、初始化 single-flight、准备态取消、统一 ExecutionAuthority、迟到事件不覆盖终态 | 无 |
| 2 固定执行契约 | Proof Contract、必需断言、分类依赖、可信快照、实际路由图隔离、逐边有向路由证据 | 真实供应商证据需要在具体凭据环境重新签发 |
| 3 通用运行闭环 | Session/Run 分离、恢复后新 Run、加固 App Server Client、HostRecoveryJournal、RecoveryCoordinator、`CodexSessionRuntime` 统一组合 App Server/Bridge/预算/租约 | Novel 主进程仍需从旧直接组装迁移到 EngineHost；本项尚不能标记完成 |
| 4 原生多 Agent | 可控真实 Codex App Server 已验证跨模型子 Agent、MCP 身份隔离、父级取消传播；有向边独立认证 | 外部真实模型的自主选择质量不是内核认证的一部分，需单独 smoke |
| 5 行为评测 | 固定任务集、执行事实评分、总请求/token/时间预算、命令执行器依赖绑定 | 内置 EngineHost 行为执行器仍需替换产品命令包装入口 |
| 6 发布闭环 | 候选校验、报告绑定、原子晋升、归档回退、失败保持稳定指针 | 仍需真实 Windows 与 macOS 候选报告才能实际晋升 |

本次确定性验证：Agent Engine **117/117**；Novel Agent **112/112**；Novel Runtime **123/123**；发布门禁 **5/5**；前端生产构建通过。上述数字只证明本机可控路径，不替代真实供应商及双平台报告。

## 1. 结论

**架构方向对齐，但尚未达到理想架构的执行闭环。现阶段应收敛已有模块，而不是继续增加架构层。**

已经建立的重要基础包括：模型/引擎/产品能力分层、四类协议 Codec、结构化工具校验、有向路由与可信 lineage、租约、任务树预算、恢复日志、原生 Codex 委派测试基础，以及独立引擎包。

主要问题是：这些模块尚未通过唯一生产宿主形成一致的执行事实。部分目标目前只体现在接口、字段或局部测试中，没有在所有实际执行入口强制执行。

需要修正理想架构第 14 节的验收判断：**不能认定阶段 A、B、D、E 已完成主要退出条件**。它们已有实现基础，但仍存在下述代码缺口，不只是等待真实端点报告。C、F 同样未完成。

### 对齐矩阵

| 阶段 | 当前可确认 | 未达到的退出条件 | 判断 |
| --- | --- | --- | --- |
| A 契约闭合 | 能力分层、依赖裁剪、扩展授权已进入 Plan | 无关连接隔离、完整工具/权限冻结、Profile 与实际可执行路径一致 | 部分完成 |
| B 宿主闭合 | 通用 Host/客户端 API、事件回放、基本中断 | 准备态取消、并发互斥、统一撤权、恢复后续跑、严格事件关联 | 未闭合 |
| C 子 Agent 闭合 | spawn/message/wait/有向路由的 runner 基础 | failure/idle/cancel/recovery 独立认证与按项发布证据 | 部分完成 |
| D 证据闭合 | 类型化断言与依赖字段、报告摘要、失败优先 | 依赖真实比较、断言完整性、通用可信加载、最小失效范围 | 未闭合 |
| E 工程与评测闭合 | runner/MCP 已迁移；命令评测可执行 | 生产唯一接入、内置 Codex 执行器、基于执行事实的自主委派评分 | 部分完成 |
| F 发布闭合 | 固定上游 SHA、候选 worktree、补丁登记 | 候选验证/晋升、构建绑定、真实端点与双平台报告 | 未完成 |

“复用 Codex 核心能力”的合理边界仍然是：协议可表达、路径经验证、权限允许的能力可以复用；不承诺所有模型具有相同推理质量，也不能以一个模型未主动选择工具来判定内核不支持该工具。

## 2. 本轮验证与证据边界

- 在 `agent-engine` 执行 `npm test`：**94 通过、0 失败**。首次沙箱运行因回环端口 `listen EPERM` 失败，获准在沙箱外重跑后通过；不把首次环境限制算作产品缺陷。
- 额外运行无真实模型、无网络、无持久化写入的 Node 复现探针，确认三项行为：
  1. 正常连接可生成计划；加入未认证、未被任务使用的连接后，同一任务报 `connection unrelated lacks verified capabilities`。
  2. 证据同时包含 `passed` 和 `failed` 断言时，仍可授予 `text / structuredToolCalls / toolLoop`。
  3. 用屏障挂起 `ensureSession`，先 `cancelRun` 再释放屏障，仍调用一次 `sendTurn`，最终返回 `completed`。
- 其余问题标为源码判定，需按本文验收用例补充自动化复现。没有运行真实 DeepSeek/第二供应商 smoke、Windows 故障测试或上游候选构建。
- 本轮没有重新运行 Novel 全量测试或 Rust 全量测试；当前 dirty 工作区不是可发布、可复现的提交基线。既有测试通过不能覆盖新增复现问题，也不能证明模型能力等价。

## 3. 主要发现

优先级：P1 为执行正确性、授权一致性或证据可信性问题，应先修；P2 为工程、观测及验收完整性问题。以下源码行号对应本次工作区，后续修改可能移动。

### R1 · P1：取消没有形成统一执行权撤销，并存在准备态竞态

证据：[engine-host.mjs](src/host/engine-host.mjs) 第 60–106 行；[Bridge](src/bridge/server.mjs) 第 240–274 行；[ExecutionLease](src/state/execution-lease.mjs)。

Host 取消只增加本地 `run.epoch` 并调用 `interruptTurn()`，没有要求 runtime 更新 Bridge 的 `leaseEpoch` 或撤销租约。Bridge 有独立撤权实现，但二者并未由通用 Host 契约连接。仅在状态查询中看到 epoch 增加，不证明旧请求已失去执行权。

已复现：等待会话创建时取消，随后 `startTurn` 无条件改回 running 并发送任务。此时用户看到的取消不等于停止执行，可能继续消耗额度或触发工具。

落地：同步登记 preparing run；每个异步边界检查取消标记；由唯一执行权组件先撤权，再中断 Provider/App Server、清理子树，最后记录终态。中断失败时不能恢复旧执行权。存在未知写入时保留 `outcome-unknown`，不能直接标为 cancelled。

验收：准备、turn/start 尚未响应、工具运行、子请求进行中四个屏障分别取消；撤权后新增 Provider/工具请求数量必须为零；迟到成功事件不能覆盖取消或未知结果。

### R2 · P1：运行与会话创建的并发保护晚于异步边界

证据：[engine-host.mjs](src/host/engine-host.mjs) 第 61–83、109–115 行；[app-server-client.mjs](src/host/app-server-client.mjs) 第 51–89 行。

Host 在等待 `runtimeFactory` 后才登记 run，同 runId 并发启动可能创建两个 runtime。客户端 `ensureSession` 没有共享创建 Promise；`sendTurn` 在 `await ensureSession()` 前检查、之后才设置 waiter，并发请求可能同时通过并覆盖 waiter。

落地：runId 在任何 await 前占位；session 初始化/创建 single-flight；会话 turn/resume 串行化或明确拒绝并发；factory、订阅、初始化失败均回滚并关闭已创建资源。不能只在正常路径删除锁。

验收：同 runId 并发启动只能创建一个 runtime；并发 ensureSession 只发一次 thread/start；并发 turn 不覆盖 waiter，所有调用都有明确结果，无孤儿进程。

### R3 · P1：恢复接口目前主要是 thread/resume，不是任务恢复闭环

证据：[engine-host.mjs](src/host/engine-host.mjs) 第 109–125、143–159 行；[Novel main](../novel-codex/electron/main.mjs) 对 RecoveryJournal、预算和任务生命周期的独立组装。

通用 Host 恢复没有调用回执核对、预算恢复或未结算请求处理；恢复后只返回 runId/threadId，没有在该已恢复 run 上继续提交 turn 的公共入口。`startTurn` 使用同 runId 会拒绝，使用新 runId 又创建新 runtime。已有日志模块的测试不能替代该入口的端到端证明。

落地：区分可复用 Session 与单次 Run，允许向恢复的 Session 提交新 Run；恢复先重建预算与 lineage、核对领域回执，再发新租约。领域事务仍由产品 Runtime 负责，引擎只通过通用回执接口核对。

验收：模拟“工具写入已提交但进程在接收回执前崩溃”；恢复不得重复写入；无法确认的写入阻塞相关续跑；确认只读会话可在原 thread 继续下一轮。

### R4 · P1：分类依赖尚未用于准入，既有过度失效，也有漏失效

证据：[evidence-store.mjs](src/capabilities/evidence-store.mjs) 第 140–156 行；[resolver.mjs](src/session/resolver.mjs) 第 141–148、235–237 行；[createQualificationContextDigest](../novel-codex/electron/codex-runtime.mjs) 第 142–155 行。

`evidenceAdoptionStatus(expectedDependencies)` 可检查分类依赖，但实际 resolver 使用的 `activeEvidence` 只检查整体 environmentDigest 和到期时间。Plan 收集 dependencyDigests 不等于准入时验证这些依赖。

同时，Novel 资格摘要包含整个模型目录和引擎 src：新增无关模型也可能令既有连接证据失效。runner/MCP 移到 `agent-engine/qualification` 后，现有摘要仍读取 Novel 的薄包装文件；引擎 src 摘要不涵盖 qualification，实际认证实现变更可能不改变该资格摘要。

落地：按能力域定义必需依赖结构，resolver 逐项校验当前 Codec/模型、BuildIdentity、工具契约、父子连接与原语证据。suite 实现需包含实际 runner/MCP 的版本或内容摘要。禁止依靠整个目录 hash 替代语义依赖。

验收：新增无关模型/UI 改动不影响旧连接；改子连接只失效对应有向边；改 runner/MCP 必须使对应旧 suite 证据不可采用；改内核只失效依赖该构建的执行证据。

### R5 · P1：passed 证据不保证必需断言全部通过，通用可信加载未闭合

证据：[evidence-store.mjs](src/capabilities/evidence-store.mjs) 第 95–117、159–174 行；[resolver.mjs](src/session/resolver.mjs) 第 11–30 行。

`verified` 只要求至少一个 passed 断言；存在 failed 断言也可通过规范化并被 resolver 采用，已复现。suite 授权校验除 toolLoop 外只判断两个委派 suite 名称之一，缺少 capability → suite/version → 必需断言集合的严格对应。

通用 Host 的 evidenceProvider 可以直接返回记录；resolver 只检查 reportRef/reportDigest 的格式，不读取报告校验。Novel 有更严格的报告加载，但尚未成为独立引擎的统一生产入口。把 fixture 工厂移到 test-support 降低误用，不等于证明报告可信。

另外，当前 supersedes 不限制替代记录必须完成测量；unknown/not-run 语义不应删除仍有效的成功证据。当前测量状态还是旧 status 投影，缺少独立 unsupported 状态。

落地：建立能力契约注册表及可信 EvidenceRepository；只有完整必需断言通过、报告内容绑定和当前依赖成立时才能准入。显式区分测量、采用、撤销状态；拒绝异常未来时间/无效时间窗口。发行可信性可用本地受控发布和引用校验实现，不必无条件引入远程签名服务。

验收：混合失败、缺必需断言、错误 suite、失联报告、错误依赖、未执行记录覆盖成功均不能授予资格；真实报告的部分能力通过仍可单独采用。测试双工厂与生产 loader 隔离，但不以包 export 作为安全边界。

### R6 · P1：SessionPlan 尚未固定全部实际执行面

证据：[resolver.mjs](src/session/resolver.mjs) 第 134–175、200–259 行；[codex-config.mjs](src/session/codex-config.mjs) 第 61–69、79–117、130 行。

三个独立缺口：

1. 对所有 connections 应用同一必需能力集，而非任务实际需要的路由图。新增未认证无关模型阻塞根任务，已复现；父子承担不同能力时也不能按节点分别解析。
2. Plan 固定了扩展列表，但未固定具体 MCP/tool catalog；编译器仍从 Plan 外接收 mcpServers、并发上限及提示，权限也固定为 never/read-only，而非解析 permissionsRef 得到的快照。
3. `authorizedCapabilityProfiles` 只检查 enabledExtensions，没有同时检查实际授权扩展、有向边与工具暴露，可能形成“有资格标签但不能执行”的分裂状态。

落地：先解析实际路由图，再计算节点能力依赖；可选子路由失败只移除该路径。Plan 保存实际工具清单/摘要、授权快照、预算数值及影响执行语义的配置摘要。编译器和 UI 只消费同一 Plan；禁止覆盖其授权和额度。

验收：无关坏连接不阻塞 text；必需路由失败返回结构化原因；未授权工具不可见；Plan 外增加 MCP/提高并发被拒绝；Profile 声明与真实可调用工具、有向边完全一致。对上游整组 feature flag 应按真实暴露工具验收，不能只验证开关值。

### R7 · P1：协议客户端在子线程事件、无换行输入与退出路径上仍有缺口

证据：[app-server-client.mjs](src/host/app-server-client.mjs) 第 75–89、108–129、188–246 行；[engine-host.mjs](src/host/engine-host.mjs) 第 163–195 行。

- 客户端收到任意 thread/started 就覆盖根 threadId。若服务端通知子线程创建，后续操作可能引用子线程而非根线程；需以真实内核通知序列补测。
- 超时发生在取得 turnId 之前时，不会真正发送中断；迟到启动仍可能继续运行。
- JSONL 大小在 readline 产出完整行后才检查，无换行流能在检查前持续增长。
- 宿主等待状态按事件名称推导，任意 item/completed 都可清除等待，没有关联具体 pending item 或根线程。
- close 的超时分支发 SIGKILL 即 resolve，不等于退出已确认；异步审批返回时进程已关闭，回复失败后再次写 error 也可能抛出未处理拒绝。

落地：增量有界字节 framing；根/子身份独立；pending turn 从发起前占位，超时先撤权；按 thread/turn/item 跟踪等待集合；退出后统一结算 pending 请求，服务端请求响应要有进程代次检查。事件除条数外还要有单项/总字节与保留期限上限。

验收：超长无换行流、迟到 turn/start、交错子事件、关闭中审批回调、强杀进程分别故障注入；无未处理 rejection，无错误根线程切换，无无限缓冲，所有等待可结束。

### R8 · P2：生产产品仍自行组装执行链，通用 Host 尚不是唯一入口

证据：[Novel main](../novel-codex/electron/main.mjs) 第 9、65、222、853 行；[Engine Host](src/host/engine-host.mjs) 构造参数 runtimeFactory。

Novel 继续直接使用 CodexAppServerAdapter、BridgeProcess、日志与预算进行组装。通用 Host 需要外部 runtimeFactory，却没有随引擎交付统一装配这些组件的生产 runtime。两条入口的取消、恢复和权限行为可能长期分叉。

落地：提供产品无关 CodexRuntime 组合实现，由 Host 持有 App Server、Bridge、执行权、预算及恢复服务；产品注入领域 MCP、凭据提供器、审批策略、回执读取器。Novel 仅保留 IPC/领域需求映射；迁移后删除重复调度，而非永久保留双路径。

验收：不依赖 Novel 源码即可运行最小消费者；Novel 通过同一 Host 完成启动、取消、恢复和审批；测试检查产品不得再独立实例化执行调度组件。依赖注入可保留，不允许把核心一致性义务留给每个产品重复实现。

### R9 · P1/P2：生命周期认证未完整，部分认证路径还有输入/归因问题

证据：[conformance runner](qualification/codex-conformance-runner.mjs) 第 175–193、198–242 行；[能力契约](src/contracts/capabilities.mjs)。

当前 runner 主要覆盖创建、运行中消息、等待、有向路由。失败传播、空闲续跑、取消树及恢复尚不能仅靠已有局部回归获得独立证据。

`verifyDirectedEdge` 接收 parentCredentialScope，但生成预备计划时使用 connection.credentialRef，且没有对称的 childCredentialScope 参数；使用不同凭据作用域绑定 probe 证据时可能在实际路由测试前就失败。聚合 suite 失败又会把多个断言统一写 failed，无法区分未执行、已通过与真正失败。

落地：先修凭据作用域贯穿，再按能力独立屏障测量；每个断言从真实 trace 生成结果，缺观测为 unknown/not-run。可控 Provider 的内核确定性证明与真实供应商 smoke 分开，模型未选择工具不写成内核失败。

验收：A→A、A→B、B→A 分开；凭据轮换按作用域失效；子失败、取消中子请求、idle 新 turn、重启未知写入各有真实内核测试和独立报告；完整自治 Profile 只在所有必需路径通过后授予。

### R10 · P2：行为评测已有命令运行框架，但不是完整的 Codex 能力评测

证据：[command-executor.mjs](src/evaluation/command-executor.mjs) 第 58–107、111–152 行；[behavior-evaluation.mjs](src/capabilities/behavior-evaluation.mjs)；[CLI](qualification/evaluate-model.mjs)。

execute 模式确实执行外部命令，不再只是包装 samples；但命令返回 completed/文本不证明真实调用了 Codex 或自主委派。当前 required-facts 字符串评分无法区分伪述委派和真实子线程执行。默认报告允许缺运行依赖；executor 摘要不绑定程序内容和模型设置，args 又直接进入报告。

运行可靠性也需补齐：等待 exit 而非输出流关闭可能漏尾部输出；stdin 错误缺处理；仅杀直接子进程；全任务集总调用/token/时间预算缺硬约束；runtimeDependencies 在执行后才校验。

落地：提供内置 Engine Host 执行器与版本化任务集，三类任务覆盖必须委派、自主选择、无需委派。评分输入包括可信调用/lineage/工具结果与答案质量；模型设置及执行器产物摘要进入报告。命令插件保留为扩展，明确其证明边界。所有配置在运行前验证，凭据只以引用传入且报告脱敏。

验收：只声称“调用子 Agent”的文本不能获得委派分；成功、失败、超时全部计样本；总预算硬停止；报告能回溯 BuildIdentity/Plan/任务/设置/评分器；始终 `authorizationEffect: none`，不提升权限。

### R11 · P2：上游同步只有候选生成，没有晋升闭环

证据：[sync-upstream.mjs](../novel-codex/scripts/sync-upstream.mjs) 第 78–98 行。

脚本固定 SHA 并隔离合并是正确方向，但输出明确为 merged-unverified、validation not-run。现有 CI 或构建工具不自动等于针对该候选的完整晋升证据。

落地：增加候选验证和显式晋升两个阶段；所有报告绑定 candidate tree、patch set、toolchain、平台和 binary BuildIdentity。验证成功才原子更新稳定内核指针；失败不覆盖稳定产物；保留可验证回退对象。

验收：候选变化后旧报告失效；缺 Windows/macOS 任一必要报告不能晋升；未登记上游差异失败；升级失败稳定版仍可启动。可以继续同步官方主分支，无需复制第二份调度内核。

## 4. 收敛后的目标实现

保留现有四层边界与 Codec，不重写 Codex Agent 循环，不建设外部子 Agent 调度器。建议收敛到以下生产调用链：

```text
产品（领域需求、工具描述、凭据/审批/回执接口）
  → EngineHost：Session/Run 生命周期唯一入口
    → Resolver + TrustedEvidenceRepository：生成固定 SessionPlan
    → CodexRuntime：统一装配客户端、Bridge、执行权、预算、日志
      → 固定构建 Codex：原生工具和子 Agent 调度
      → Bridge + Codec：模型协议、路由、预算与撤权校验
    → RecoveryCoordinator：回执核对、预算恢复、明确未知副作用
```

命名可随实现调整；重要的是所有生产路径强制满足同一契约，而非一定新增这些类。优先复用已有 state/bridge 模块，合并职责重复部分。

最小一致性要求：一个 run 只有一个执行权来源；一个会话只有一个 turn 串行化入口；一个 Plan 决定所有实际授权；一个 EvidenceRepository 决定可采用证据；一个恢复协调入口决定副作用是否可重试。

## 5. 实施顺序与交付门槛

每个阶段拆成可独立 review 的小变更，复杂逻辑单次尽量少于 500 行、总变更通常不超过 800 行。不要一次把全部重构混入同一 diff。

| 顺序 | 实施包 | 对应发现 | 必交付与退出条件 |
| --- | --- | --- | --- |
| 1 | 安全止血及失败复现 | R1/R2/R5 | 将三项已复现问题变为自动化测试；修取消屏障、同步占位、必需断言校验；现有回归无退化 |
| 2 | 固定执行契约 | R4/R5/R6 | 可信加载、分类依赖采用、路由图解析、工具/权限/预算冻结；最小失效矩阵通过 |
| 3 | 通用生产运行闭环 | R1/R2/R3/R7/R8 | runtime 统一装配、客户端加固、恢复后可续跑；迁移 Novel 并删除重复调度；无 Novel 最小消费者通过 |
| 4 | 原生多 Agent 逐项资格 | R9 | 独立生命周期 suite、精确 scope/trace、失败归因；未测项不得授予能力 |
| 5 | 真正行为评测 | R10 | 内置 Codex 执行器、三类固定任务集、事实评分、完整依赖与总预算；质量结果不改变准入 |
| 6 | 升级发布闭环 | R11 | 可重复的候选验证/晋升/回退、macOS/Windows 报告、目标真实端点 smoke |

阶段 1 的取消修复需建立最小撤权契约，不能仅增加一次 `if (cancelled)` 就宣布完成。阶段 3 再把该契约收敛进唯一生产 runtime。

阶段 2 的证据格式可以直接升级；测试期不保留旧内部格式回退或双 resolver。旧记录失效后按需重新测量，不能把旧 passed 自动迁移为新契约通过。对于上游公共 App Server 接口则继续固定版本、生成契约 fixture 并验证，不以“不考虑兼容性”为由忽略上游演进。

### 建议新增测试组

- Host race：同 runId、初始化/创建并发、preparing 取消、迟到启动、factory 失败清理。
- Authority integration：真实 Bridge + 可控 Provider + Codex，证明取消后整树无新增请求。
- Recovery integration：commit-then-crash、未知回执、预算预留重建、恢复后新轮次。
- Evidence adoption：分类依赖失效、必需断言集合、scope、报告篡改/缺失、not-run 不覆盖通过。
- Plan contract：无关连接、不同父子需求、工具清单、权限快照、Profile、实际配置一致。
- Protocol lifecycle：无换行限长、根子事件交错、超时无 turnId、关闭中审批、进程退出结算。
- Native delegation：failure/idle/cancel/recovery 单独 suite，不以文本回显代替执行断言。
- Consumer parity：最小消费者与 Novel 调用同一 Host 的启动/取消/恢复行为一致。
- Promotion：候选变更使旧报告失效；缺报告拒绝晋升；稳定产物不被失败候选覆盖。

新测试优先集成边界和故障注入；避免只断言静态枚举、字段存在或函数导出。不要用不断增加 fixture 通过次数替代真实链路证明。

## 6. 最终验收清单

以下各项同时满足后，才能说“达到当前理想架构”，而非只说“模块已建立”：

- [ ] R1–R7 的执行/准入问题修复，并有失败先行的自动化回归。
- [ ] 产品通过唯一 Host 使用固定内核，不重复实现取消、预算和恢复调度。
- [ ] 任何已授权能力可追溯到必需断言、当前依赖、可信报告及实际工具面。
- [ ] 新增模型不牵连无关连接；未知或不支持的能力逐项解释，而非全局封禁。
- [ ] 原生子 Agent 生命周期按路径测量；认证与模型自主行为质量分别展示。
- [ ] 取消、重启和未知写入不会产生未授权续跑、重复副作用或假成功。
- [ ] 行为评测真实消费引擎且可复现，不以文本声称代替执行事实。
- [ ] DeepSeek 与至少一个不同协议族的真实端点完成所需 smoke；失败按责任层记录。
- [ ] macOS/Windows 完成进程、取消、恢复与打包验证；候选内核可以验证、晋升和回退。
- [ ] 实现及产物来自固定已提交基线；文档状态引用对应报告，不引用泛化测试数量。

**下一步建议：先实施第 1 包，随后以第 2、3 包建立唯一执行事实。此时再扩展多模型认证，能避免反复出现“局部测试通过、产品链路仍不稳定”。**
