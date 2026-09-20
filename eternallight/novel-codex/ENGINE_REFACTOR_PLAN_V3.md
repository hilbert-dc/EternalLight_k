# Agent Engine 优化与重构方案 v3

日期：2026-09-20。状态：**v3 代码已落地；本机门禁通过，双平台签名安装与真实付费供应商认证仍需目标环境执行。**

本文取代 v2.2 的整体完成判断与后续实施排序，保留理想架构 v4.6 的 G1–G10 目标。依据本轮 50 项审查发现建立 A01–A50 台账；编号与审查报告第 1–50 项一一对应，不与历史 F01–F44 混用。已通过的具体测试仍是有效的局部证据，但不能证明整个工作包完成。

## 1. 决策与范围

采用“保留内核，重构五个边界，按纵向链路交付”，不推倒重写，也不继续以零散补丁维持多份状态。

- Codex 继续独占规划、模型循环、工具调度、上下文、Skills、子 Agent 创建与通信；不新增第二套调度器。
- 通用 Engine 负责模型接入、协议转换、能力资格、请求准入、资源预算和进程生命周期。Novel Runtime 只负责小说领域数据与事务。
- 同协议新模型通过连接配置和行为认证接入，不按模型名称增加分支或专属提示词。
- 统一规则是执行契约，不是要求所有供应商使用同一种 wire protocol，更不能补造模型没有的智能。
- 不保留旧接口、双轨预算或兼容恢复路径。每批同步修改生产调用方、类型、fixture、测试。真实用户作品的删除不在授权范围内。
- 延续本引擎任务免除多角色评审的指示；本文不声称专家已批准。当前任务只更新方案文档。

## 2. 目标结构与状态归属

```text
产品客户端 → EngineHost → Codex App Server / 原生多 Agent
                │                     │
                │                     ├→ ModelBridge → Protocol Adapter → Provider
                │                     └→ MCP → Novel Runtime → Domain DB + Operation Ledger
                ├→ Qualification Store → 冻结 ExecutionPlan
                ├→ Engine State Store：执行代次、撤销、预算预留与结算
                └→ 收尾核对 ← Runtime RecoverySnapshot / OperationReceipt
```

这些是职责边界，不要求每个方框变成服务或独立进程。复用现有 Bridge worker、App Server、Runtime 进程；Host 控制器从 Electron 大文件中抽出为可测试组件。

| 状态 | 唯一权威 | 其他组件的权限 |
| --- | --- | --- |
| Agent 树、工具执行与模型历史 | Codex | 观察或调用公开接口，不能复制调度 |
| 资格记录、凭据域撤销代次 | Qualification Store，经 Host 发布 | worker 使用带代次的授权，不自行认定仍有效 |
| 执行槽位、配置代次、取消状态 | EngineHost | cleanup 只能处理捕获的 executionId |
| 预算预留、消耗、未知占用 | Host 的持久 Engine State Store | worker 请求 reserve/settle，不能自建预算账本 |
| 小说写入事实、操作回执 | Runtime 的事务数据库 | Host 缓存/核对，不由事件日志推定未写入 |
| 构建来源与安装内容 | 受控构建记录、发布清单 | 消费者验证实际文件，不接受自报字段作为来源证明 |

Engine State Store 采用产品私有的事务存储，设计目标为独立 SQLite 库、Host 单写者；不借用领域 Runtime 承担模型预算。实施前须在项目锁定 Electron/Node 与双平台验证驱动支持、事务和崩溃持久性，再确定依赖；不能未经验证假设桌面运行时支持某个内置模块。该技术验证属于第一阶段，不引入远程数据库或新守护进程。

## 3. 五个重构工作包

### B1：协议转换成为有提交屏障的完整管线

主要范围：`eternallight/agent-engine/src/codecs/`、`src/bridge/server.mjs`，以及当前 Rust Responses 消费契约测试。

请求管线固定为：读取冻结计划 → 解析有界历史与工具 → 协议编码 → 对最终模型可见内容计量 → 预算预留 → 最后准入检查 → 派发。覆盖 system、历史、工具描述/schema、输出 schema、合并后的单项和输出预留；不能只计输入消息。保留单项 10K-token 硬上限，新注入超过 1K-token 项按仓库规则复核。系统指令同时受 40KB 字节硬上限与 o200k/cl100k 双编码较大值的 10K-token 上限约束；其他条目先用更严格的 10KB 快速上界。不能把字符启发式当精确值。

响应区分可展示的文本增量、不可执行的工具候选、通过验证的可执行项。Chat 路径要求对应 dialect 的完整终态（包括规定的结束标记）；EOF 不等于成功。原生 Responses 保留内核支持的逐项提交语义，不强制把整个响应缓冲到结束，但每个可执行项必须在转发前通过名称、ID、类型和请求选择约束；已提交项不能在断流后假装未执行或自动重放。

用符合明确 JSON Schema 方言的成熟校验实现替代自制语义；不支持的关键字显式拒绝。限制 schema 大小、深度、引用和正则执行成本。普通同步 RegExp 加 Promise timeout 不能构成执行上界；选择有界正则能力或可终止隔离执行，并保持取消可响应。

保留协议所需的 turn 分组、供应商 ID 与本地 ID 映射、opaque 状态。不可转换的加密历史在派发前报明确不支持，不能剥掉标记继续。拒绝响应和 usage 必须通过当前 Rust 解码器与续轮验证。内部 usage 使用 known/partial/unknown；wire 只产生内核支持的表示，未知输入不伪造为零，预算保守结算。

验收：非法工具实际执行次数为 0；合法工具不重复；异常 EOF 不生成伪成功；usage/refusal 能被真实消费者读取；工具多轮历史前缀稳定；跨请求 ID 不冲突；超额内容在访问 Provider 前被拒绝。

### B2：资格、能力目录与执行计划使用同一事实源

主要范围：`src/capabilities/`、`src/session/`、`electron/codex-conformance-runner.mjs`、`electron/codex-runtime.mjs`。

QualificationReport 保留探测事实；eligibility 是按 connection、credentialRealm、protocol/dialect、构建身份、行为套件、期限和 supersedes 计算的结果。淘汰协议的 404 不应污染选中协议，过期或其他连接的失败不得阻断全局。认证失败与能力失败分开，不能将全部 403 无差别归类为凭据失效；按端点错误语义撤销准确范围。

ExecutionPlan 不可变，一次生成配置与模型目录：有效能力 = 内核可用 ∩ 适配器可执行 ∩ 认证通过 ∩ 产品允许。optional 不表示可跳过 executable 检查。计划绑定所有实际采用的 grant 与有向父子边，整体期限不得超过这些 grant 的最早到期时间；每次派发还核对所用范围与最新撤销代次。

运行时失效先记录新代次并阻止新准入，再通知 worker 取消受影响请求并确认接收；worker 失联时停止该执行，不靠旧 lease 自证有效。成功重验发布明确的新授权代次，旧失败保留审计但不继续错误阻断。

轻量探测走有界 bootstrap 执行模式：不要求被探测能力预先通过，但必须具备 Host 预算、超时、响应字节上限、取消和目标范围限制。认证器提供真正的有向边认证入口 A→B，不能手工补写证据绕过；允许仅一个异构子模型时生成必要的 model override。

验收：过期/撤销后新 Provider 请求为 0；成功重验解除对应阻断；失败 streaming 不被使用；产品禁止项不出现在目录；只认证 A→B 不自动授予 B→A；证据只授予实际测试过的行为。

### B3：Host 生命周期与持久预算收敛为单一控制器

主要范围：`electron/main.mjs`、`electron/codex-adapter.mjs`、`src/bridge/{process,worker,server}.mjs`、`src/state/`。

EngineHost 暴露统一执行入口，覆盖 turn、review、compact、认证。执行状态为 preparing → running → cancelling/reconciling → terminal；失败也经过核对，不能跳过。每次异步准备完成到真实派发之间都有 executionId/代次检查。

取消的线性化点是 Host 撤销后续派发权：之后不再允许新请求；此前已派发请求尽力中断，领域副作用交给事务回执确认。Bridge cancel 不能仅 abort 当前 controllers。旧 interrupt 的 finally 只清理旧实例，不能访问“当前全局 adapter”误伤新任务。

配置构建出完整不可变 bundle（plan、adapter、bridge、configurationEpoch），在串行发布点整体替换；旧 bundle 清理在发布后独立执行。并发配置更新使用代次比较，迟到结果不能覆盖新状态。新任务只捕获一个 bundle。

预算 reserve 在派发前持久提交；settle 使用 reservationId 幂等事务，返回明确的 settled/overrun/unknown 结果，不在删除预留后抛错诱发二次结算。超额记录真实消耗并阻断继续派发。进程崩溃后未决预留保持 unknown 占用，不能释放为零或重启重置预算。timer/controller 清理独立于结算结果，必须始终执行。

review/compact 先加载目标线程，再绑定目标 thread/turn 与事件；compact 按真实空 RPC 响应等待事件，不依赖 fixture 伪造 turn。

验收：取消先于派发时 send 次数为 0；旧清理不影响新执行；并发配置无混合 bundle；重启后预算守恒；重复结算不改变结果；超额无 unhandled rejection、无残留 controller；真实 compact/review 可收尾和中止。

### B4：恢复依赖写入隔离与完整事务快照

主要范围：`runtime/creative_workspace/{operation_ledger,generation}.py`、`runtime/server.py`、`electron/recovery-reconciliation.mjs`、`src/state/recovery-index.mjs`。

Runtime 在同一事务中提交领域变更与 committed receipt，操作绑定身份、请求摘要、executionId 和写代次。核对先撤销旧写代次，再读取同一数据库快照中的 task 与全部操作；快照返回 fence、watermark、complete。大结果采用固定快照的有界分页，不能在两个无关联查询之间拼接“完整状态”。

正常完成、取消、超时、进程退出全部经过同一收尾流程。核对完成前不把恢复记录设为 terminal，也不释放允许冲突写入的执行槽位。无法获取完整快照时显示 reconciliation_required，阻止冲突任务；只读查看可继续，不把读取失败当作无操作。

已隔离且确认未提交的 accepted 可以事务化转为 not_executed；无法证明时保持 unknown。已提交操作绝不重放。complete_task 撤销写权后仍可通过 Host 授权的只读回执查询重取原结果，绑定原操作身份与摘要，不重新赋予写权，也不允许凭任意 operationId 读取其他任务结果。

验收：在 accepted、领域提交、回包、Host journal 落盘前后分别强制退出；恢复后提交事实不丢、不重复、不假成功。Ledger 不可用时恢复项仍存在；已证明未执行的操作无需永久人工确认；读取既有回执不产生新写入。

### B5：构建身份与最终安装产物形成可验证链

主要范围：`scripts/build-codex-runtime.mjs`、`build-provenance.mjs`、`stage-release-runtime.mjs`、`verify-release-runtime.mjs`、打包配置和 CI。

BuildLayout 是 Cargo、stage、verify、builder 的唯一输入。读取本次 Cargo 输出的实际 artifact 路径，支持正常 target-dir 配置；绝不从默认目录捡旧文件。受控构建记录输入源码/锁文件/工具链/命令和实际输出，检查构建期间输入未变化；JSON 自哈希只证明完整性，不证明来源。可信 CI 发布记录提供来源锚点，不扩张为抵御完全控制本机的攻击者。

区分构建身份与签名后发布身份。顺序固定：受控编译 → 暂存 → 嵌套程序签名 → 生成最终资源摘要与发布清单 → 外层应用签名/平台封装 → 从最终安装产物运行 smoke。摘要清单不包含自身形成循环；后续步骤若改变被摘要资源，必须重新生成并验证。Windows 同样按实际会修改字节的签名步骤安排。

实现中使用两段式证明：`BuildIdentity` 只证明受控编译产生的未签名字节、源码树与工具链；`release-manifest.json` 和 `supported-cli.json` 证明嵌套签名后的交付字节、BuildIdentity 引用以及全部资源文件集合。运行时不会再错误地要求签名后 CLI 与未签名摘要相等，但仍要求两段证明完整衔接；外层应用签名负责保护最终发布清单本身。测试 signer 会真实改写 CLI、Creative Runtime 与 MCP 三个文件，随后验证它们仍可启动；任意 Skill 或其他声明资源被改写都会使发布验证失败。

逐项验证所有已声明交付资源，包含 Skills、agents、协议/资格实现及领域代码，不只验证三个程序。最终应用 smoke 运行实际 Electron 主进程、ASAR/worker、CLI、Runtime/MCP 和一次工具读写旅程；隐藏全局 CLI/Node 与桌面登录态。失败产物可保留为诊断附件，但不能发布为通过验收的安装包。

验收：旧默认 CLI、自定义 stage 目录、资源遗漏、签名修改字节都有会失败的测试；macOS/Windows 最终安装包能独立运行。证书缺失只能解释签名实测待办，不能掩盖本地 hook 与门禁尚未实现。

## 4. 分阶段交付与依赖

| 阶段 | 交付内容 | 必须通过的门禁 |
| --- | --- | --- |
| S0：建立可信基线 | 固定源码/CLI 身份，修正实际 artifact 路径；50 项反例台账；验证 Host 存储实现 | 能确认测试的是当前内核；关键反例在现有实现失败；无整体完成标签 |
| S1：单协议可靠闭环 | Chat 的 B1 提交/计量；B2 准入；B3 持久预算与取消；B4 根任务提交恢复 | 配置→资格→Host→当前 Codex→工具→真实 Runtime/SQLite→故障恢复全链，无手工证据注入 |
| S2：原生多 Agent 与第二协议 | 同模型父子、Messages，以及 A→B/B→A 生产认证；真实身份/权限 | spawn/send/wait/汇总、子失败、嵌套、取消、恢复；根写允许、子孙写拒绝、子孙读允许 |
| S3：协议与能力完整性 | Responses/Gemini 完整多轮；四协议有向边矩阵；按路径开启高级扩展 | 每个支持组合有独立证据；未支持项明确拒绝；同协议未知模型只加配置即可认证 |
| S4：独立发布与上游演练 | B5 完整打包门禁、双平台签名安装、真实供应商、一次 upstream 同步 | 最终包旅程、真实模型行为结果、同源重建与旧资格失效记录 |

S1 是纵向迭代，不是把四个大模块分别写完再集成：先根读取，再根写与回执，再取消/崩溃。B5 的路径与来源工作从 S0 开始，签名/最终安装在 S4 收口。四协议全部支持时，有向边最多为 4×4；按实际支持范围认证，不能用单条异构边宣称覆盖全部组合。

每个复杂逻辑批次目标小于 500 行、一般批次小于 800 行；每批包含反例、实现和调用方更新。现有大规模脏工作区不能一次整体提交。删除旧逻辑随对应替换批次完成，不保留待切换双轨。

## 5. 50 项发现的完整实施台账

以下为需求台账；实施状态与验证证据见第 9 节。括号为工作包/阶段；单项关闭需要对应反例和纵向证据，不能以邻近模块通过代替。

| 编号 | 需要关闭的问题 | 归属 |
| --- | --- | --- |
| A01 | 输出 schema 计量遗漏 | B1/S1 |
| A02 | 工具定义、合并单项超过硬上限 | B1/S1 |
| A03 | finish 后缺结束标记仍提交工具 | B1/S1 |
| A04 | Schema 深等价、布尔 schema、自有属性、Unicode 语义错误 | B1/S1 |
| A05 | 正则阻塞校验和取消 | B1/S1 |
| A06 | 原生 custom tool 可执行项绕过同等校验 | B1/S3 |
| A07 | 加密历史静默丢失 opaque 语义 | B1/S1 |
| A08 | partial usage 与内核 wire 不匹配 | B1/S1 |
| A09 | Gemini 并行历史丢失 turn 分组 | B1/S3 |
| A10 | 缓存和推理用量映射丢失 | B1/S2–S3 |
| A11 | refusal 无法被真实消费者展示与重放 | B1/S1 |
| A12 | Gemini 跨请求调用 ID 冲突 | B1/S3 |
| A13 | tool_choice 未约束提交 | B1/S1 |
| A14 | optional grant 与有向边过期未阻断 | B2/S1–S2 |
| A15 | 运行时认证失效未撤销执行资格 | B2/S1 |
| A16 | supersedes 后仍被旧认证失败阻断 | B2/S1 |
| A17 | 已失败的 streaming 能力仍被使用 | B2/S1 |
| A18 | worker 内存预算不是 Host 持久权威 | B3/S1 |
| A19 | 超额二次结算及资源清理遗漏 | B3/S1 |
| A20 | 目录未与产品授权求交集 | B2/S1 |
| A21 | 淘汰协议失败污染成功认证报告 | B2/S1 |
| A22 | 历史失败报告阻断其他范围与成功重验 | B2/S1 |
| A23 | 唯一异构子模型没有 override | B2/S2 |
| A24 | 生产认证入口不能生成异构边证据 | B2/S2 |
| A25 | 轻量探测绕过预算与响应字节限制 | B2/B3/S1 |
| A26 | 图片能力宣称可执行但路径拒绝 | B2/B1/S3 |
| A27 | 高级能力假启用或无通用启用路径 | B2/B1/S3 |
| A28 | Ledger 查询失败却关闭恢复记录 | B4/S1 |
| A29 | 无 fenced 完整快照，正常完成跳过核对 | B4/B3/S1 |
| A30 | 已证明未执行的 accepted 永久人工处理 | B4/S1 |
| A31 | complete_task 丢回复不能重取回执 | B4/S1 |
| A32 | 取消之后仍能派发新请求 | B3/S1 |
| A33 | 旧 interrupt 清理误伤新执行 | B3/S1 |
| A34 | 配置跨 await 非原子发布 | B3/S1 |
| A35 | review/compact 线程恢复与事件绑定错误 | B3/S2 |
| A36 | Cargo 输出与认证文件路径错配 | B5/S0 |
| A37 | 自声明身份不满足受控来源验收 | B5/S0–S4 |
| A38 | 资源摘要生成早于嵌套签名 | B5/S4 |
| A39 | stage/verify 与 builder 输入目录不同 | B5/S4 |
| A40 | 最终 Electron 安装包未执行旅程 | B5/S4 |
| A41 | 清单声明资源没有全部校验 | B5/S4 |
| A42 | 权限测试没有贯通真实 Runtime/SQLite | B4/S2 |
| A43 | 根线程协议回环冒充跨协议多 Agent 证据 | B2/B1/S2–S3 |
| A44 | 测试直接取消 Bridge 掩盖内核取消传播 | B3/S2 |
| A45 | 认证成功条件不足以授予完整 G5 | B2/S2 |
| A46 | handler harness 未接真实控制面 | B3/S1 |
| A47 | compact fixture 返回不存在的 turn | B3/S2 |
| A48 | 崩溃测试缺少 Host journal 落盘前窗口 | B4/S1 |
| A49 | 缺 finish 后无 DONE 的 EOF 反例 | B1/S1 |
| A50 | 缺 Gemini 跨请求、多工具和前缀测试 | B1/S3 |

原生路径尚未完成 B1 校验时不得授予其受影响能力；分阶段验收不代表允许未修复路径绕过共同边界。

## 6. 测试与关闭标准

1. **纯契约测试**：schema、终态、用量、历史、ID；有界生成不同分片/工具顺序，使用稳定种子。
2. **组件交错测试**：真实 Host/Bridge/State Store 配合可控 await 屏障，覆盖取消、配置切换、撤销、结算。不能仅靠 sleep 猜竞态。
3. **内核纵向测试**：当前构建 CLI + 真实 App Server + 可控 Provider + MCP + HTTP Runtime + SQLite。既测正向允许，也测拒绝和数据库最终事实。
4. **独立取消测试**：只 interrupt 父线程验证内核传播；另测产品 stop 的 Host/Bridge 全树撤销。不能由后一项替代前一项。
5. **真实进程故障测试**：worker、Host、Runtime 在关键屏障退出；重启后读取持久事实，不用内存 stub 证明恢复。
6. **真实端点与最终包**：认证通过生产入口生成证据，不手工 seed；付费请求需要明确执行授权、限定额度。平台签名与上游演练分别记录，不由 fixture 冒充。

每项状态只能为 OPEN → IMPLEMENTED_UNVERIFIED → VERIFIED_LOCAL → VERIFIED_TARGET（适用时）。记录源码/CLI 身份、命令、断言、范围、产物与日期；失效反例重新打开对应范围。完成定义是反例被阻止、正常路径仍工作、真实消费者通过，而非新增多少测试或多少模块。

## 7. 性能与维护性优化：在正确性后实施

- 按 schema 摘要缓存编译结果，缓存必须有容量上限，资格变化不得复用错误范围。
- 流式解析有响应字节、事件、候选参数大小限制和背压；保留文本流式，不为统一抽象强制全量缓冲。
- 对预算事务、首次 token、取消完成时间、恢复耗时、峰值内存建立基线再优化，不先承诺百分比收益。
- 发布不可变配置快照，减少逐请求重复编译；撤销和期限检查不能省略。
- 使用构建身份缓存认证结果，输入变更精确失效；不能只按模型名称缓存。
- 测试中保存脱敏 trace 与请求摘要，不保存密钥、认证头或默认完整用户作品。

## 8. 需要删除的旧机制与最终判定

随对应批次删除：自制不完整 Schema 语义；worker 自持预算及结束时复制账本；原始失败记录的全局 some 判定；配置/目录独立能力推导；cancel-only-abort；依赖全局当前 adapter 的迟到清理；以 task terminal 代替 Ledger 完整核对；硬编码产物目录；只验预打包组件却声明发布通过的门禁。

不删除 Codex 原生多 Agent、不降级成纯聊天、不永久禁用强模型能力来使测试变绿，也不为不同模型另建执行核心。需要上游内核改动时只保留最窄的身份或公开契约补充，并附真实内核集成测试和同步演练。

**实施后的下一步：保持五个边界的本地门禁，执行双平台签名安装、真实供应商矩阵和 upstream 同步演练；目标证据不足时不能标记 VERIFIED_TARGET。**

## 9. 实施结果与验证状态（2026-09-20）

v3 的生产代码、调用方、协议 fixture 与本地门禁已经同步更新，没有保留旧预算权威、旧恢复查询拼接或旧打包路径作为兼容分支。

- **VERIFIED_LOCAL**：A01–A23、A25–A33、A35–A38、A41–A42、A44–A50。覆盖四协议真实 App Server→Bridge→MCP 回路、真实 Codex 子 Agent 身份与取消、Runtime/SQLite 原子写入、提交后 Host journal 前崩溃、fenced 恢复、持久预算、Cargo 实际 artifact 解析和逐文件发布清单。
- **IMPLEMENTED_UNVERIFIED**：A24、A34、A39–A40、A43。生产入口已经支持有向父→子连接认证、配置代次迟到结果拒绝、统一 BuildLayout、最终解包应用 smoke；仍需在包含两个真实异构连接的环境和 macOS/Windows 最终安装产物上执行目标验证。
- **VERIFIED_TARGET 待执行**：平台证书签名、macOS/Windows 安装包、真实供应商付费行为矩阵与 upstream 同步演练。缺少这些外部条件不会回退本地实现状态，但不得把本地 fixture 结果标记成目标环境通过。

本机已通过的主要门禁：Agent Engine 72 项；Novel Agent/Electron 103 项；Runtime 124 项（含新增崩溃窗口）；Electron UI 5 项；TypeScript/Vite production build；真实当前 Codex CLI 的 build identity 生成；发布 Runtime 暂存与执行测试。计数随后续新增用例可增加，以命令退出状态和对应报告为准。

后续工作不是继续保留 v3 OPEN，而是执行目标环境验收，并在失败时重新打开对应编号。
