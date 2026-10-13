# 多模型原生多 Agent：可靠性优化与重构实施方案

日期：2026-09-20  
状态：核心重构已实施并完成本地验收；高级生命周期、双平台和第二真实端点仍是独立验收项。  
范围：Agent Engine、通用认证设施及 Novel 的薄入口；不重写 Codex 原生 Agent 循环。

## 1. 决策

采用定点重构，保留现有 Host、SessionPlan、Bridge、Codec 和 Codex App Server 架构。优先修复观测与归因，再拆分认证、闭合证据发布，最后依据真实轨迹优化协议和模型设置。

核心目标仍是：模型自主决定创建、分工、通信和使用子 Agent 结果，Codex 内核执行工具与子线程生命周期，宿主负责执行权限、预算、取消及恢复。不能通过宿主替模型固定执行 spawn→message→wait，来宣称模型已经具备自主委派能力。

本文是 [理想架构](IDEAL_ENGINE_ARCHITECTURE.md) 和 [架构收敛方案](ENGINE_ARCHITECTURE_CONVERGENCE_PLAN.md) 的专项实施补充。已有 Plan 完整冻结、未知写入恢复屏障、双平台发布等待办继续有效。测试期直接替换重复内部逻辑，不为旧认证格式增加兼容分支。

## 2. 事实、纠正与未知项

### 2.1 当前证据

| 观测 | 可以证明 | 不能证明 |
| --- | --- | --- |
| DeepSeek 基础工具档曾真实通过 | 当次文本、结构化调用及真实 Codex/MCP 闭环可用 | 长期稳定、所有设置均可用 |
| 多 Agent 探针 90 秒超时，6 次请求，末尾有 reasoning delta | 当次未在期限内完成认证 | 四项工具都无法执行，或必然是模型能力不足 |
| 报告预算字段 tokens=41798、unknownTokens=36885 | 存在已报告用量及独立的未知用量保守计量 | 41798 全是输出/思考 token，或上下文一定过重 |
| 延长到 180 秒的尝试在基础工具阶段失败 | 该次未进入多 Agent 探针 | 延长时间仍无法解决多 Agent 问题 |
| 本地回归 112/112 通过 | 对应受控用例通过 | 真实 DeepSeek 自主调度可靠 |

上一轮“工程 70%、模型 30%”没有测量依据，撤销该比例。“DeepSeek 不理解协作协议”“严格顺序稳定性不足”目前只是待验证假设。基础闭环失败也可能来自网络、协议、构建或测试设施，不能直接归因于模型行为。

参考原始报告：

- `../novel-codex/.certification-reports/2026-09-20T11-55-33.409Z-f2985d02311218bbcf6b0950c8b48afcd0afbfa5f41a095a37fcabaeac6b411b.json`
- `../novel-codex/.certification-reports/2026-09-20T11-59-45.924Z-8eb09dbe235df2d4b8ab4a2654bf463faddeba23be15561b7f698917b7418d33.json`

这些是本地诊断产物，不要求提交；未来正式证据必须由报告存储按摘要引用。

### 2.2 已确认的工程缺口

| 位置 | 缺口 | 后果 |
| --- | --- | --- |
| `../novel-codex/scripts/certify-model.mjs` | coreSuite 失败时先 throw，再执行不到 diagnostics 入库 | 具体错误 message 丢失，只剩 core-tool-loop-error |
| 同上 `boundedReportValue` | 整个报告按固定深度递归裁剪 | requests、responses、trace.events 被 DEPTH_LIMIT 替代 |
| `src/observability/execution-trace.mjs` | 缓存满后丢弃所有后续事件；超时优先返回通用分类 | reasoning 增量挤占关键事件，终态和直接错误不可见 |
| `qualification/codex-conformance-runner.mjs` | 四项断言共同要求 child-mcp-result-observed | 尾端失败连带使已完成前序步骤无法获得独立结论 |
| 同上 `observedCollaborationOperations` | 调用名称、事件匹配均可加入已观察操作集合 | 请求调用与执行成功未严格区分 |
| 同上 MCP trace 读取 | 正常完成后才读取；异常清理删除临时目录 | 失败现场的 MCP 拒绝原因可能未进入报告 |
| 同上有向路由观测 | 同模型路径依赖子身份，跨模型路径主要看模型是否出现 | 路由证明缺少精确父子关系与 request 关联 |
| `qualification/conformance-mcp.mjs` | gate 立即返回，没有测试驱动的保持/释放屏障 | 不能仅由 gate 文件证明 send_message 时子任务仍运行 |
| 当前认证发布器 | 成功才更新认证，失败主要另存诊断；普通 writeFileSync 写清单 | 新失败不一定参与现有资格裁决，且与通用原子报告发布存在两条路径 |

此外，`autonomous-delegation-v1` 所需 failure/cancel 与优选 idle/recovery/context 能力，尚未由当前四项 runner 完整产出。修改 profile 名称或放宽门槛不能补齐执行证据。

## 3. 职责边界与证明层次

生产链路保持：产品 → AgentEngineHost → CodexSessionRuntime → Codex App Server；模型请求经 Bridge/Codec 到供应商。原生子 Agent 由 Codex 创建和管理。

认证分为三层，报告明确 layer，不能互相替代：

1. **内核执行契约**：可控 Provider 返回合法工具调用，真实 Codex、Bridge、MCP 执行。验证身份、消息传递、取消、恢复等确定性语义，不评价模型智能。
2. **真实模型组合验证**：真实端点自主选择工具，观察实际成功步骤和组合闭环。失败定位到具体转换、执行或行为阶段；一次通过仅证明该次路径可用。
3. **自主行为质量**：明确要求委派、自主决定是否委派、不应委派三类任务，评估成功率、错误委派、延迟及成本。报告不产生额外权限。

测试驱动器可以在第一层控制 MCP 屏障、故障和 Provider 输出；第二、三层不能偷偷代替模型补调用或补回执。若需要强制 tool_choice 探针，必须单独标记 forced-call，不算自主选择成功。

## 4. 工作包 A：先让失败可定位

### A1. 结构化报告与事件存储

复用 `src/capabilities/report-store.mjs` 与现有 trace 模块，不增加第二套存储。建议引入版本化 QualificationReport v2：

```text
runId, suiteId/version, layer, connection/config/build/suite digests
startedAt, completedAt, terminationReason
stages[]: stageId, dependencies, status, assertions, traceRefs
failure: category, responsibleLayer, confidence, retryability
artifacts[]: relativePath, digest, bytes, truncated, droppedCount
usage: reportedInput/Output/Total, estimatedUnknown, outstandingReservation
```

阶段状态使用 passed / failed / inconclusive / not-run / unsupported；若现有测量契约只有 unknown，则将 inconclusive 映射为 unknown 并保留 reason，不把它映射成 failed。unsupported 必须有明确的协议/目录约束依据。

核心关联字段为 runId、threadId、parentThreadId、turnId、requestId、callId、connectionId、单源序号和时间。不同进程的时间戳仅辅助排序，因果由调用 ID 和输入/输出关联证明。

关键事件只保存白名单元数据与必要有界参数，不保存密钥、授权头、原始思维链或整份小说上下文。收集工具暴露、调用、接受、返回、结果投递、终态和清理；reasoning delta 聚合成计数、字节数、首末时间。

初始上限建议：摘要 256 KiB、单事件 8 KiB、每次 trace 4 MiB、报告保留 14 天且总量不超过 200 MiB，全部可配置且参与 suite 设置。关键里程碑有独立保留空间；达到硬上限时记录缺口，相关断言不得推断通过。报告先落盘、校验摘要，再清理临时目录；落盘失败必须返回 artifact-write-failed。

失败或成功都在统一 finally 路径汇集 MCP trace、coreSuite、预算和清理结果。core 失败先记录，再终止后续依赖步骤。不得靠提高通用递归深度来维持报告结构。

### A2. 归因与终止原因分离

`terminationReason=deadline` 与 `failure.category=identity-rejected` 可以同时成立。先按可信关联找最早阻断点，再记录最终超时；不能让超时覆盖所有直接错误。

分类顺序：目录未暴露 → 模型未选择/参数不合法 → Codec 转换失败 → 内核拒绝 → MCP 身份/参数拒绝 → 工具结果未投递 → 后续行为不符合任务 → 生命周期/清理异常。缺事件或已截断时使用 inconclusive，责任层可为 unknown。

用量区分供应商报告与本地保守预留；没有输出 token 数据时不作“过度思考”结论。费用需要价格版本与计费口径，默认只报告 token 及来源。

## 5. 工作包 B：独立能力断言与组合场景

将 500 行级组合 runner 拆为驱动器、版本化场景和纯断言模块，放在 `qualification/` 下。复用已有客户端、预算、租约及可信身份抽象。

| 场景 | 必需证明 | 不作为通过前提 |
| --- | --- | --- |
| spawn | 原生调用成功、可信父子 lineage、子请求到达正确连接 | 子 MCP 最终回执、父模型复述 |
| message-running | 子任务运行屏障已建立；消息成功入队；释放后后续子输入包含唯一 marker | 全组合最终成功 |
| wait | 等待调用被内核接受；指定子任务终态进入父线程工具结果 | 父模型原样复述字符串 |
| route A→B | 原生 spawn 的有效模型选择与父子身份；对应子 request 经 B 连接发出 | 仅观察 B 名字出现 |
| failure | 注入子失败，原生工具/线程结果向父线程传播有类型失败 | 模型编造失败总结 |
| idle-followup | 子任务完成后再发起合法续跑，新 turn 与同一子身份关联 | 再创建一个新子 Agent |
| cancel-tree | 根撤权后禁止新请求，子树在途请求停止，清理结果可追踪 | interrupt RPC 返回成功 |
| recovery | 中断/崩溃后恢复有明确边界；未知写入阻塞、不会重复提交 | 仅恢复会话文件 |

MCP server request ID 与模型 tool call ID 可能不同，映射必须从可信 App Server/MCP 执行事件建立，不能让模型参数自行填写身份来证明关联。某版本无法观测消息消费时，应标记观测不足并调查最小事件接口补丁，不能声称消息入队等同消费。

四项组合场景保留，用于检验串联与并发，但只生成组合结论；独立通过证据可以保留。依赖阶段失败导致的下游未执行记为 not-run，不能把每个下游能力都写成 failed。

message-running 的确定性测试使用 MCP 保持/释放屏障：子调用进入后保持 pending，驱动器确认消息入队后释放，随后观察子模型输入及返回。真实模型测试同样只控制工具完成时机，不替父模型发送消息。每个屏障有独立截止时间和最终释放/取消清理。

## 6. 工作包 C：诊断运行、认证发布与生产准入分离

引擎提供通用 qualification 入口；Novel 的 qualify/certify 脚本成为薄入口。默认运行只写报告；发布命令显式把报告交给现有可信 Repository。修正 --help、未知参数和缺失参数处理，参数解析不能触发模型调用。

拟定 CLI：

```text
qualify-model --connection deepseek-flash --suite child-spawn-v2 --mode live
qualify-model --connection deepseek-flash --suite delegation-v2 --mode live --repetitions 3
publish-qualification --report <report-path>
```

认证发布先验证报告、发行来源、断言、依赖与完整性，再原子发布索引。禁止将自动重试后最后一次成功覆盖此前失败；所有 attempt 可追踪，重试属于同一运行预算。

准入按能力及其依赖闭包计算。spawn 已测通过不必连带等待最终组合通过，但只有内核能够安全暴露对应工具子集时才允许启用；若上游 multi_agent 开关一次暴露全部工具，就保留对应工具集合的整体门槛，或实现经过验证的最小工具过滤边界。不得仅靠提示“不要调用未认证工具”授予部分权限。

同范围的新明确执行失败应阻断受影响资格，旧成功不得静默遮盖。not-run 不覆盖旧证据；inconclusive 保留旧测量并展示最新不确定状态，由版本化策略决定是否暂停执行。质量评测中的输出不佳不自动撤销无关内核能力。供应商 401/429/5xx 作为访问健康处理，影响当前执行资格但不抹掉历史能力事实。

## 7. 工作包 D：有证据后再优化真实模型行为

先在相同 CLI 构建、Codec、工具目录、提示和参数下保存完整基线，再做单变量对照。180 秒试验必须实际进入多 Agent 阶段；此前提前失败的尝试不算延长超时实验。

比较项按证据选择：

- 工具名称/schema/namespace 经编码后是否保持语义、call_id 是否往返一致。
- 是否向模型暴露了无关工具，是否存在相互冲突的系统与测试指令。
- 上游工具描述是否与固定 CLI 的真实工具版本一致。
- 供应商已支持并经验证的 reasoning/output 设置是否改善完成率与耗时。
- 首 token、请求执行、等待子任务和总运行各自耗时，区分无进展与仍在工作。

简化提示或工具集合只能作为独立实验配置；通过后需更新生产配置并重验，不能用简化测试替代生产等价测试。Provider 特性写入有界连接选项，不添加按 deepseek 名称硬编码的调度分支。

保持总任务预算，增加阶段耗时观测与空闲超时策略。只对明确可重试的网络故障进行有界重试；非幂等工具、副作用未知、已创建子任务不得盲目重放。重试不能改变原生调用身份或隐藏重复子任务。

## 8. 文件级实施与顺序

| 切片 | 修改位置 | 交付与退出条件 |
| --- | --- | --- |
| P0 诊断完整性 | trace、两个 runner、Novel certify 入口 | core 错误完整保存；异常路径保留 MCP 事件；大量 delta 后仍保留终态；无敏感信息 |
| P1 分项契约 | qualification/scenarios（新增）、proof-contract、能力契约 | spawn/message/wait/route 各有独立断言；请求不等于成功；阻塞下游为 not-run |
| P2 发布与准入 | report-store、evidence-repository、model-certification、resolver、extension-registry | 单一原子发布路径；新失败影响正确范围；工具暴露与授权一致 |
| P3 生命周期补齐 | failure/idle/cancel/recovery 场景、Host/Runtime 必要修复 | 故障注入证据与能力编译路径闭合；不靠测试标签授予高级 Profile |
| P4 DeepSeek 对照 | 通用 CLI、版本化配置及行为任务集 | 生产等价 smoke 与行为报告；给出成功率、耗时、用量来源和未决故障 |
| P5 上游与平台 | 现有 sync/构建身份/CI | 固定上游候选跑相同矩阵；macOS/Windows 进程、取消、恢复实测 |

P0 完成后立即重现原故障，使用报告决定是否需要 Codec/Host/内核修复，不预先大改所有层。每个切片独立提交和回退，复杂逻辑尽量控制在 500 行左右。

## 9. 验收与归因规则

必须补充的确定性回归：core 在每个阶段抛错仍保留 message；trace 超量保留里程碑；MCP 拒绝后超时保留两个事实；spawn 成功而 echo 失败不否认 spawn；send 已请求但内核拒绝不能通过；消息入队但未进入子输入不能宣称消费；路由名称出现但 lineage 不匹配不能通过；取消及恢复不重复写入；发布中断不产生半份有效证据；--help 零请求零认证变更。

真实 DeepSeek 先每个基础场景运行 3 次作 smoke，组合场景运行 3 次；失败保留并停止无意义消耗。3/3 只算初步复现通过。自主质量采用固定任务集、预算和预先声明的重复次数，报告样本数和置信区间；可采用现有 35 次/Wilson 策略作为发布质量目标，但不得把该数字当作通用模型接入门槛。

归因标准：

- 可控 Provider 下同路径也失败：优先定位引擎/Codec/认证设施。
- 真实输出合法但在转换、执行或结果投递中断：工程问题。
- 工具正确暴露、输入/输出传递完整，模型多次不选择必要工具或输出不合要求：记录该配置下的模型行为限制。
- 证据截断、未进入目标场景、无法关联身份：归因未知，先修观测。
- 超时只是终止原因；鉴权、服务错误、配额和协议问题分别记录。

## 10. 保持 Codex 上游可合入

新代码优先位于 agent-engine，产品侧只映射领域需求。固定 CLI 工具/事件契约；真实不足集中在 App Server 版本边界，确需改内核时登记补丁动机、符号、diff 指纹、测试和删除条件。沿用 origin 产品 Fork、upstream 官方主分支及候选构建晋升流程。

不复制原生多 Agent 调度，不引入另一套父子线程数据库，不为了认证成功更改上游工具语义。上游升级后按构建和协议依赖失效相关证据，失败不覆盖稳定内核产物。

## 11. 完成定义

- [x] 任意失败能定位最后完成步骤、首个阻断步骤及可追踪证据；不再只返回四项 timeout。
- [x] 基础能力、内核执行、真实组合、自主质量分别报告，不能互相代替。
- [x] 模型仍自主调用 Codex 原生多 Agent 工具，宿主不代替模型完成认证。
- [x] 报告发布、资格读取、Plan 配置和真实工具暴露闭合。
- [x] DeepSeek 的可用范围以重复实测呈现，包含失败和不确定项。
- [x] 高级生命周期、第二真实端点、双平台及上游候选缺项明确标注；未测试项不宣称完成。

实施优先级是：可信现场 → 独立断言 → 准入闭合 → 原生生命周期补齐 → 模型行为调优。这样才能判断该改工程的哪一层、该调哪些模型设置，并保留用户要求的自主多 Agent 能力。

## 12. 2026-09-20 实施结果

已落地：关键事件保留与直接错误优先归因、失败路径 MCP 轨迹回收、spawn/message/wait/route 独立断言、版本化三场景认证、可信父子身份、运行中消息 gate、工具目录按阶段收敛、模型已选调用的确定性交付屏障、诊断与认证发布分离、原子清单写入、CLI 零副作用 help、供应商 `toolChoiceMode` 能力声明，以及能力级准入报告。轨迹现可在小写 `/delta` 洪泛后保留 MCP 与生命周期错误，并分别识别结构化工具调用、文本模拟工具调用、参数拒绝和历史工具编码错误。

执行兼容性场景允许明确标注的 `forced-child-tool-choice`，仅用于工具执行契约，不计为自主行为质量。对于声明 `auto-only` 的连接不会向供应商发送强制 tool choice；模型必须自行选择唯一可见工具。

修复前的 DeepSeek 实测结果：`childSpawn`、`childWait`、`childDirectedRoute` 通过；`childMessageRunning` 未通过。报告位于 `../novel-codex/.certification-reports/2026-09-20T14-48-30.454Z-2627cabfb01d53c215c89313fde7f6d34408620d5ab5c81b01daef4896ddd55b.json`，未发布为生产认证。该报告只证明父模型生成了正确的发送参数，不能证明精确 token 已排队或进入子模型上下文；因此撤回此前将最终失败主要归因为模型消费能力的结论。

### 消息投递证据修正（2026-09-20）

- Responses 扁平工具适配为普通 collaboration 委派调用补充 `encrypted_function_args: []`，与其他明文适配路径一致。保留供应商显式提供的加密元数据，不修改原生非扁平路径的加密语义。
- gate 只在父线程收到与发送 call ID、精确参数摘要匹配的 `subAgentActivity/interacted` 完成事件后释放。该事件由 Codex 在 `deliver_message` 成功后发出；模型选择工具不能作为释放依据。
- `parent-followup-token-queued` 现在要求上述真实投递回执。新增独立断言 `child-followup-visible`：目标子线程的编码后请求必须包含精确消息行的可读文本摘要。随后仍需 MCP 私有回执证明消费成功，三项不可替代。
- 请求诊断只增加有上限的文本行 SHA-256 摘要，不记录原始提示词；加密内容和工具调用参数不作为可读输入证据。达到摘要上限后可能产生保守的不通过，不会据此认定模型缺陷。
- 回归验证：引擎 133 项测试通过；真实 Codex + 本地模拟 Chat 模型 + MCP 的现有集成测试通过；新增真实 Codex + 本地模拟 Responses 模型 + MCP 的集成测试通过，覆盖 spawn、运行中发送、消费、wait。
- 新集成入口：在 agent-engine 运行 `npm run test:delegation-integration`，需先构建仓库的 `codex-rs/target/debug/codex`。此测试不调用付费模型，也不发布真实模型认证。

本次尚未重新实测 DeepSeek，因此工程回归通过不等于真实模型认证通过。后续必须使用新的投递、输入可见性、消费三段证据重新认证，再判断剩余模型行为问题。

本地验收：Agent Engine 129 项测试通过；Novel Agent/Electron 112 项测试通过；生产构建通过。高级 failure/idle/cancel/recovery 认证矩阵、第二真实端点、Windows 实机与上游候选构建仍未执行，不宣称完成。
