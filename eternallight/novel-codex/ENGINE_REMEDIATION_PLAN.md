# Agent Engine 修复与局部重构实施方案

> 2026-09-20 审查后更正：本文件 v2.2 的整体本地完成判断已撤回，以下实施记录作为历史保留，不再作为当前完成状态。后续实施、50 项开放问题及验收标准以 [优化与重构方案 v3](ENGINE_REFACTOR_PLAN_V3.md) 为准；新方案尚未实施。

版本：2.2 · 2026-09-20 · 状态：本地实现已落地并通过纵向回归；签名双平台与真实供应商终验待外部环境。

本方案依据最新一轮 review 的 **44 项问题（F01–F44）**，替代 v1.1 中“S0–S6 本地闭环、只剩 S7”的完成判断。此前 R01–R34 是历史问题编号，不与本轮编号混用。目标仍是 [理想技术方案](MODEL_AGNOSTIC_CODEX_ENGINE_DESIGN.md) 的 G1–G10，不缩减为“能聊天、能调用一次工具”。

## 1. 决策：保留内核，重构失效边界

不推翻现有整体架构，也不继续在大文件中逐个添加补丁。保留 Codex 原生规划、工具执行、上下文管理、Skills 和多 Agent 调度；针对五个边界做局部重构，其余缺陷随所属边界直接修复。

| 边界 | 当前失效模式 | 目标责任 |
| --- | --- | --- |
| 协议与工具提交 | 名称二次转换、缺 schema 校验、终态错误仍提交、状态静默丢失 | 请求绑定的语义转换、校验和一次性执行提交 |
| 资格与路由 | 有报告即放行；scope、构建、期限、撤销未贯穿执行 | 不可变报告、精确范围、有代次的逐请求租约 |
| Host 生命周期与预算 | 多入口绕行，异步检查与启动脱节，清理失败卡住任务 | 一个执行入口、可线性化取消、可靠终结与保守结算 |
| 领域事务与恢复 | Host 观察或任务状态替代每次写入的权威回执 | Runtime Ledger、同事务结果、完整枚举和写入隔离 |
| 构建与交付 | 二进制来源自报，签名前后 hash 冲突，未运行最终产物 | 同源构建证明、签名顺序、最终包的实际行为验收 |

统一规则不等于统一最低能力：基础契约按协议复用，差异由协议 dialect 和能力证据表达，不新增模型名称特判、模型专属提示词或第二个 Agent 调度器。

本版不增加另一套架构，重点是收敛实现：**最小可信核心、唯一状态权威、先纵向闭环、第二协议验证抽象、再扩大覆盖。** 不宣称已证明这是最优方案，也不承诺未经测量的 Bug 降幅；用第 12 节的不变量测试与缺陷记录评价实际收益。

交付主线改为：可信构建基线 → 单协议完整链路 → 第二协议检验公共契约 → 四协议覆盖 → 高级能力与跨平台终验。W0–W10 保留为内部工作包，不再按模块全部完成后才集成；F01–F44 和 G1–G10 的范围不变。

本轮已按该方案直接替换旧接口与逻辑，并同步更新调用方、fixture、CI 与运行文档；没有提交或推送，也没有调用真实付费模型。沿用负责人对此引擎任务免除五角色评审的指示，本文不宣称取得专家批准。执行 [测试阶段策略](TEST_STAGE_ENGINEERING_POLICY.md)：不建设迁移、兼容、双轨或静默降级。无兼容要求不代表可以擅自删除用户作品。

## 2. 事实基线与证据口径

2026-09-20 本地落地证据：

- Agent Engine 62/62：四协议 Codec、Schema/alias/终态、原生透传、资格 scope/报告、执行租约、路由 lineage、预算、取消与恢复索引。
- Electron/App Server 102/102：当前 Codex CLI、真实 App Server、MCP、HTTP Runtime 和 SQLite；覆盖同连接/异连接子 Agent、权限拒绝、协议往返、恢复、QualificationContext 及 BuildIdentity。
- Creative Runtime 122/122；Electron UI 5/5；生产构建通过；`repo:check:strict` 通过。
- Codex Core 定向可信 MCP 身份测试 1/1；`just fmt`、`just fix -p codex-core` 完成。未运行完整 Core 测试套件，因此不把定向结果扩写为全仓回归。
- 当前源码 CLI 已由 `build:codex-runtime` 重建，并以 source tree、Cargo.lock、rustc、target、构建命令、二进制摘要和版本形成 BuildIdentity；改源码、CLI、基础指令、Agent Engine/认证 runner 或 Codec 版本都会使旧资格失效。

F01–F41 与 F44 的本地实现和自动化反例已关闭到 `VERIFIED_LOCAL`。F42–F43 的 staging、manifest、BuildIdentity、资源校验和双平台 CI 路径已经实现，但实际签名后的 macOS/Windows 安装产物仍需证书与目标主机，状态为 `IMPLEMENTED_UNVERIFIED`。W10 仍是外部终验：真实供应商矩阵、签名安装、上游同步演练与质量评测不能由本地 fixture 冒充。

状态口径仍只有：`OPEN → IMPLEMENTED_UNVERIFIED → VERIFIED_LOCAL → VERIFIED_TARGET`。本地通过不等于真实供应商或签名安装已经验收。

## 3. 目标结构与必须保持的不变量

```text
BuildIdentity + 版本化行为套件
  → QualificationReport → EvidenceStore → Resolver → SessionPlan
                                                  ↓
产品 → EngineHost → ExecutionLease + TaskBudget → Codex App Server
         │                                      ├→ 原生子 Agent 调度
         │                                      ├→ 可信请求身份
         │                                      │   → ModelBridge 准入
         │                                      │      → Codec → Provider
         │                                      └→ 可信工具身份
         │                                          → MCP → Runtime
         └→ 终结与核对 ← RecoverySnapshot ← SQLite Operation Ledger
```

不变量：

- Codex 是唯一推理循环和子 Agent 调度权威；Host 不模拟 spawn/wait，不根据自然语言猜测工具调用。
- 所有下游推理，包括子任务、review、compact 和 conformance，必须持有有效租约、预算及明确路由。
- 模型输出是候选调用，不是执行授权；未验证的工具不得生成可执行完成事件。
- 能力资格、产品权限和预算分别判定，最终取交集，任何一项不足都阻断。
- Provider ID、模型字符串、UI thread ID 不可替代可信执行身份。
- Runtime 事务是小说写入事实的唯一权威；Host/UI 只保存投影。
- 发布资格绑定实际源码、资源、二进制与最终产物；历史成功不得替代当前组合验收。
- 失败、未知、不支持必须可区分；不伪造零用量、成功回执或认证状态。

### 3.1 唯一状态权威：一个事实只允许一个写入所有者

| 事实 | 唯一权威 / 写入路径 | 其他模块允许做什么 |
| --- | --- | --- |
| 线程、turn、父子树、工具调度 | Codex 内核及其持久状态 | Host/UI 按版本投影，不自行完成子任务或改写历史 |
| 产品 run 生命周期、配置发布、取消是否先于派发 | EngineHost 的统一控制器 | IPC handler 调用控制器；Bridge 报事件，不独立维护第二套 run 终态 |
| 能力报告、失效事实与资格代次 | 资格仓库的串行发布接口 | Resolver 纯读取快照；Host 据此发租约；worker 只持有带版本的验证副本 |
| 任务树额度、reservation、未知用量 | Host 持有的持久预算账本及原子接口 | Bridge 经 IPC reserve/settle；不复制可独立扣减的账本，控制通道失败则停止新请求 |
| 请求内 alias、候选调用、响应提交状态 | 对应 PreparedRequest 的有界转换上下文 | Codec 显式传递上下文；会话映射存储仅用于稳定分配与恢复，不另作执行授权 |
| 领域候选、operation、回执及写权限代次 | Runtime 的 SQLite 事务接口 | Host recovery/UI 读取精确回执，不按消息或任务状态猜测结果 |

不同状态不能合并为一个含糊的 completed：Codex turn 完成、Host run 终结、Runtime operation 已提交是不同事实。对账显式关联它们，但不允许相互覆盖。新增缓存或投影必须说明来源版本、失效规则、重建方法；无法说明则不增加。

跨进程/跨语言仍保留执行端授权和 schema 复验，但共同契约应来自同一 schema/固定测试向量。重复防御不等于复制几份会随时间分歧的业务规则。

## 4. 模块边界和核心契约

以下新模块名是拟议归属，不是声称已经实现。路径以 `eternallight/` 为根；保持现有 .mjs/Python，不为形式统一重写语言。

| 模块 | 责任与主要调整 |
| --- | --- |
| `agent-engine/src/contracts/` | 运行时 schema、错误分类、协议能力范围和版本化契约；定义数据而非执行编排 |
| `agent-engine/src/codecs/` | 拆分 content、tool-registry、request-context、response-commit、usage；四协议入口组合它们 |
| `agent-engine/src/capabilities/` | 报告生成/验证、scope 覆盖、显式失效、资格快照发布 |
| `agent-engine/src/session/` | Resolver、可执行扩展注册表、计划和配置编译；不自行推断认证成功 |
| `agent-engine/src/bridge/` | 每请求校验、预算预留、协议传输、错误反馈与取消 |
| `agent-engine/src/host/` | 启动/配置/终结状态机、租约控制；从 Electron 抽走通用异步流程 |
| `agent-engine/src/state/` | 持久 Run、预算、待核对索引；不保存另一份领域写入权威 |
| `novel-codex/electron/` | 产品 IPC、小说授权策略、UI 投影；conformance runner 调用通用 Host |
| `novel-codex/runtime/creative_workspace/` | Ledger、候选事务、完整回执、撤销隔离和恢复快照 |
| `novel-codex/scripts/`、产品 CI | 统一 BuildLayout、来源证明、签名资源清单和最终产物测试 |

### 4.1 需要一次性贯穿调用方的契约

| 契约 | 必须包含的字段 / 约束 |
| --- | --- |
| BuildIdentity | sourceTreeDigest、upstreamCommit、patchDigest、lockDigest、toolchain、target、cliDigest、contractDigest、instructionDigest、codecDigest、compilerDigest、suiteDigest |
| QualificationContext | BuildIdentity 摘要、连接/endpoint/dialect/providerModel、可得模型 revision、credentialRealm 与权限代次；模型未提供 revision 时明确 unknown 并限期复验 |
| QualificationReport | reportId、上下文、suite、逐 scope 样本/结果/时间/有效期、失败分类、整体 pass/partial/fail；内容不可变 |
| EvidenceRecord | 从已验证报告派生，绑定具体结果路径和内容摘要；不得单独篡改 outcome、样本数或期限 |
| SessionPlan | planId/hash、上下文、rootRoute、allowedChildEdges、required/optional extensions、预算、选中报告摘要、资格代次与最早到期时刻 |
| ExecutionLease | leaseId、runId、planHash、mode、root/调用身份约束、routeGrants、epoch、deadline、budgetId；只由 Host 产生 |
| PreparedRequest | requestId、可信身份、route、不可变规范请求摘要、授权工具目录/schema、AliasRegistry 引用、上下文/输出预算、provider-state 引用 |
| ToolProposal / ValidatedCall | 前者仅解析结果；后者已校验名称、关联、schema、ID 唯一性和终态，才可提交一次 |
| Usage | known/partial/unknown、input/output、cacheRead/cacheCreate、reasoning、计数来源及语义；缺失不能填 0 |
| OperationReceipt | operationId、run/root/caller/turn/call、tool、requestHash、终态、领域 revision/产物 ID、bounded result、resultDigest |
| RecoverySnapshot | runId、被隔离权限代次、watermark、完整 operation/receipt 枚举与完整性状态；分页必须固定同一快照 |
| BuildLayout | target/arch、资源根、staging、安装内容位置、manifest 路径；stage/verify/builder/smoke 共用 |

哈希提供完整性关联，不自动提供可信来源。报告必须由受控 runner/store 发布或校验可信发布证明；仅匹配 64 位十六进制字符串不能形成资格。当前威胁边界不承诺抵御已完全控制本机应用与私有存储的攻击者。

### 4.2 最小可信核心与抽象准入

- 可信核心只保留请求验证、资格/租约准入、预算预留、执行提交和权威回执边界；Codex 继续负责规划与调度。表中的拟议模块是职责，不要求全部建立独立服务、基类或插件框架。
- 编码、scope 覆盖、报告派生、参数校验尽量为确定性函数；时间、随机源、凭据引用、存储和传输显式注入。流式解码使用请求独占的有界状态，不读取全局可变别名或隐式模型分支。
- 外部 JSON 先在边界验证，再构造 PreparedRequest/ValidatedCall；执行入口不接受未验证 ToolProposal。冻结对象只能防止误修改，不能替代跨进程重新校验或安全授权。
- 首协议只实现当前闭环需要的契约；第二协议用真实差异验证后，才抽取重复且语义相同的部分。协议专有 ID、终态、状态及 usage 逻辑留在 Codec，不塞入大量可选字段或万能策略回调。
- 新公共抽象必须有两个具体消费路径、减少重复规则的证据和共享回归；明确的安全边界可先于第二消费者建立，但须说明不变量，不为假设中的未来厂商预建扩展点。
- 公共契约调整时同步当前所有消费者和 fixture，不留新旧实现双轨。其余协议不必提前验收，但未经当前组合认证的路线不得开放执行。

## 5. 协议边界：请求准备与执行提交分离

### 5.1 单一请求上下文

将当前 `createRequest → parseResponse → responseEvents` 改为显式携带 PreparedRequest 的流程：

```text
prepare(canonicalRequest, route, trustedContext)
  → 规范化 + schema/dialect 校验 + AliasRegistry + 输入/输出预算
  → PreparedRequest / UnsupportedSemantics
encode(PreparedRequest) → ProviderRequest
decode(stream, PreparedRequest) → 文本事件 + ToolProposal + provider state
validateAndCommit(proposals, terminal, PreparedRequest) → 可执行事件或明确失败
```

编码只使用逻辑工具名到 wire alias 的单向映射；解码只在本请求授权表内还原一次。后续阶段接收 CanonicalToolIdentity，不再次剥离前缀。别名采用各协议均合法的安全字符子集与长度限制，冲突检测；映射由请求/会话持有，活跃请求引用不因全局 LRU 淘汰。目录超出上限直接拒绝，不丢工具。

校验顺序：响应结构 → 工具名属于本请求目录 → 协议要求的 ID 存在 → 同一响应 ID 唯一 → 参数有效 JSON → 符合本工具 schema → 所需终态成立 → 提交一次。缺失/null 参数不自动补成空对象，标量不是普遍合法参数；自定义工具使用已认证的独立参数契约。

schema validator 使用明确 dialect，禁止远程 schema 拉取，限制深度/大小/复杂度。Codex 执行端校验仍保留，Bridge 校验是提交前的第一道契约检查，不替代内核权限。

### 5.2 流式终态与原生 Responses

Chat 桥接的 finish_reason 只结束候选内容，不直接提交工具；须消费到该 dialect 的完整成功边界。finish 后出现 error、异常 EOF、取消、截断或过滤，不得提交缓冲工具。文本可以实时展示，UI 必须仍区分未完成与最终答复。缓冲体积有硬上限。

原生 Responses 保留逐项完成语义，不重新序列化其签名/不透明状态，也不承诺撤销此前合法完成的工具。原生路径仍通过共同的租约、输入限额、输出预算、usage 和传输审计。两类提交策略分别测试，不用通用 helper 把所有结束转换成 completed。

为消除“直连绕过准入”，当前目标统一通过 ModelBridge 的受控传输入口：Responses 使用 native passthrough codec，其余协议才做语义转换。默认不保留无门禁 direct 路径。原生远程压缩/流式等所需入口须在该入口保真支持并认证；无法实现时拒绝 required 扩展，不能冒充透传已保留全部能力。

**原生保真是门禁，不是“共用 Codec 格式”的要求。** 同一准入层可以调用不同传输实现：原生流保留已认证的顺序、逐项完成和不透明状态，不能为了共享 helper 按转换路径重新拼接。Bridge 增加一个故障点，须测断连、背压、超时、重启、取消及资源上限，并与固定 CLI 的原生参考轨迹比较；不能只测最终文本相同。

原生参考入口仅用于受控测试，不成为产品绕过租约的备用路线。同端点/同 fixture 比较新增时延、首事件和内存开销，报告实测值；有状态或高级能力损失时阻断对应范围的推广，修复受控传输，而不是隐藏差异或强开未认证能力。

### 5.3 必修协议语义

| 协议 / 问题 | 实现决定 | 必须观察的结果 |
| --- | --- | --- |
| Chat 缺失 ID | 缺协议必需 call ID 即失败，不合成假 ID | 下游畸形调用不会触发真实工具 |
| Gemini 可选 ID | 对真正可选 ID 生成 request-scoped 稳定关联，保存与原始调用映射 | 无 provider ID 的合法调用及续轮成功，不把本地 ID 冒充供应商返回 |
| Gemini schema | 优先使用支持 JSON Schema 的字段；否则只允许已证明无损的受限转换 | const/additionalProperties 等约束不静默消失 |
| Chat reasoning_content / Messages signed thinking | OpaqueProviderState 原序原字节保留，绑定 provider/model/session/turn；有界私有存储 | 工具续轮、恢复正确；不能保真时在工具提交前拒绝，不向 UI 泄露私有推理 |
| Chat max output | 由已验证 dialect 选择 max_tokens/max_completion_tokens 等真实字段 | 供应商收到的上限等于预留允许值 |
| Chat refusal | 生成类型化 refusal，保留可展示内容 | content=null 不成为空的成功答复 |
| Gemini 未知 part | exhaustive 支持表；未认证 inlineData/code 等返回具体不支持原因 | 不出现丢弃内容后 completed |
| Messages 并行限制 | 未显式 tool_choice 也编译等价 auto + disable_parallel_tool_use | 禁止并行的请求在 wire 上确实受约束 |
| Gemini 历史 | 用 canonical turn 边界组装当前未提交响应，冻结此前已发送项 | 追加调用不合并改写此前 assistant/model 项 |

Opaque 状态不能跨供应商“翻译”。切换路线前验证现有历史是否可表达；不支持时明确阻断，并允许作者显式新建任务，不默默丢历史。协议依据见第 13 节。

### 5.4 上下文和预算共用计量契约

同时限制单项、完整请求和任务树：

- 单条模型可见项硬上限 10K tokens；新注入项可超过 1K 时按根目录规则额外人工复核，记录依据。F12 的 P0 来自该仓库 review 规则。
- 有可信 tokenizer 时绑定其版本；否则使用经证明适用于目标编码的保守上界。不存在可靠界限的组合不得宣称能保证 token 上限；仅有字节限额不能证明 token 合规。
- 规范化前与最终编码后都检查，包括拼接后的 system/instruction、工具目录、工具结果及协议包装；不能逐 part 通过后拼成超大项，也不能拆项规避限制。
- 整个请求满足 route.inputBudget，并与最大输出预留一起落在模型 context window 和 run 剩余预算内。无空间则明确拒绝，由 Codex 已认证能力决定压缩，不静默裁剪。
- 缺 usage 为 unknown，缺部分计数为 partial。未知消耗保留保守预留；仅明确供应商字段可结算，不能释放为 0。
- Messages 总输入计入 uncached、cache read、cache creation；字段是否互斥按协议定义，不能对所有协议盲目相加。Token 准入和金额估算分开。

## 6. 资格边界：认证不是配置上的布尔值

### 6.1 报告、范围和显式失效

EvidenceStore 只接收由已验证完整报告派生的记录。读取时核对报告内 outcome、样本、scope、上下文和期限，不能只核对 reportRef。整体报告由逐项结果计算：必需项失败不能显示 verified，可分辨 partial 和全部通过。

scope 使用结构化覆盖函数，至少包含协议/dialect、流式模式、工具类型、参数/状态能力、扩展、父子路由有向边。不得以同一 capability 名称代替覆盖关系。A→A、B→B 通过不等于 A→B 或 B→A 通过。

证据选择先处理失效和未解除失败，再比较可用成功证据。较高 source 等级不能掩盖后来失败；解除必须显式 supersedes 对应记录，覆盖同一范围，达到要求的证据等级，并保留失败历史。声明、probe、内核 conformance、产物认证分别记录，不相互冒充。

错误分为 Authentication、Authorization、ProtocolViolation、CapabilityUnsupported、RateLimited、TransientTransport、Cancelled、BudgetExceeded。任一推理/认证入口的 401/403 保守冻结同 credentialRealm 的执行资格并提升 epoch，覆盖相关协议路线；明确端点局部拒绝只有经验证的更窄权限边界才能缩小影响。429/临时断网记录可用性，不伪判永久缺能力。

### 6.2 资格发布与逐请求租约

Resolver 编译完整 SessionPlan，最终指令与配置在摘要计算前冻结。编译器不得再接收未纳入摘要的 baseInstructions；Codec、CLI、suite、工具契约或凭据代次变化均使对应旧资格不可用于新执行。

Host 持有资格发布权，Bridge worker 持有验证后的快照与 epoch。每次请求先检查租约/当前代次/截止时间和作用域，再做预算预留；排队或准备期间有变化，发送前再次检查。撤销需要 worker 确认栅栏生效，确认前不得发布替代任务；worker 或控制通道失效则拒绝继续执行。

TTL 到期停止后续请求；认证/权限撤销同时取消在途请求，尚未提交的工具不再提交。不能保证远端立即停止计费，已提交操作进入核对，不伪称回滚。旧报告/计划可供本地审计，不提供继续执行权。

### 6.3 可信路由和所有推理入口

分开 logicalRouteId、providerModel、rootRoute 和 allowedChildEdges。App Server/catalog 使用逻辑 ID，Provider 请求最终只接收 providerModel。根 route 不要求出现在 child allowlist 中；空 child 列表仍允许根推理，只禁止委派。

请求的 root/child 身份必须来自 Codex 可信执行上下文，并由 Host/Bridge 校验其 run/lineage，不能信任模型选择的 model 字符串或工具参数。优先复用现有可信传输元数据；若没有可验证链路，增加最小通用内核出口，独立审查，不将供应商策略放入 core。该入口打通前，不能把整棵树统一路由白名单宣称为父子边授权。

review、compact、子任务与普通 turn 同样通过 Host 取得租约。conformance 使用 mode=bootstrap 的有限租约、隔离测试 MCP、请求数/token/deadline/响应大小限制，无作品凭据，不能转换为生产授权。关闭“budget 为 null 就放行”的入口。

### 6.4 扩展必须可执行

Extension Registry 的每项包含 schema、依赖、scope、验证 suite、配置编译、真实 transport/内核入口及失败原因。required 不满足即计划编译失败；optional 显示未启用原因。

remoteCompaction 必须解决当前 provider 被内核判 Unsupported 的实际路径，不能只写 catalog；REPL/Code Mode、tool search、自定义工具、视觉及状态扩展分别认证和授权，不全局硬关闭也不无证据全开。安全拒绝可关闭“误放行”的缺陷，不能替代 G8 对已选定高级能力的实际保留验收。

## 7. Host 边界：状态机与取消是同一条路径

通用状态：idle → preparing → running/waiting → stopping → reconciling → terminal。配置控制另有 configurationEpoch，但与任务准入共享短临界区。

1. 开始任务时同步占用 executionId 和 preparing 槽，登记 AbortController、Run、配置/资格快照；然后才进行异步启动。
2. 配置验证可在锁外执行，但发布配置/替换 adapter 必须 CAS 检查 epoch 与 active execution。任务启动与配置提交只有一个能先获准；另一个明确拒绝/等待，不关闭新任务正在使用的旧实例。
3. 每个 await 后验证 cancellation 和身份；sendTurn 的最后派发栅栏必须与 cancel 的状态提交有确定先后关系。
4. 栅栏前 cancel 获胜：模型请求数为零；send 已先发生：进入运行中停止，interrupt/abort 和核对，不能声称未执行。
5. 终结器幂等，统一成功、失败、取消、进程退出路径。预算收尾即使失败也须在 finally 释放对应槽位；旧 execution 不得清除后来的新任务。
6. 清理失败形成持久待处理记录、保留未知预留；不能因为槽位释放而自动清空 run 消耗或给旧任务重新发证。
7. 本地历史读取独立，不需模型资格；离线、无凭据、证据过期仍能读取，测试证明 Provider 请求数为零。

产品 IPC 只调用这些原语，不在每个 handler 复制启动/关闭。窗口销毁与监督任务分离，事件发送检查窗口存活；主进程保存审计，重开 UI 读取投影。

UI 至少呈现：需验证、资格已撤销、预算不足、正在停止、正在核对、结果未知、已失败/完成。错误携带稳定 code、run/operation ID 和可执行的下一步，不向用户堆原始 JSON。

## 8. 领域恢复：只认精确回执，不猜测写入结果

### 8.1 同事务结果与幂等边界

保留当前 SQLite Ledger，补全操作身份和回执，不新增第二个领域数据库。operationId 以 run/root/turn/toolCall/tool 的可信关联构造；requestHash 由 Runtime 对规范化请求计算，不信任 Host 自算不完整字段。

执行流程：授权 → accepted 登记 → BEGIN IMMEDIATE 内重验权限代次/revision → 领域写入 + committed + 完整回执同事务提交 → 返回。所有候选写入、事实建议、规划评审、complete_task 使用同一机制；嵌套 service 不得提前 commit。

同 operation 同请求返回原始语义结果，包括 revisionId/产物标识；不同摘要拒绝。bounded result 若必须外置，需先可靠存储并在事务内引用，不先 committed 再写结果。状态或 receipt hash 不能代替完整结果。

区分“查询已提交结果”和“执行新写入”：已结束任务仍可由被授权主体查同 run/project 精确回执，不恢复其写权限，也不让旧写 token 自行变成无限期读权限。complete_task 丢回复后的重试必须走这一只读路径。

### 8.2 可证明完整的 reconciliation

普通终态和启动恢复共用一个核对器：

1. Runtime 在与写入一致的序列化边界隔离旧 epoch，等待竞争写事务确定结果，产生 watermark。
2. 对 run 完整枚举 Ledger，返回 RecoverySnapshot；分页/重试不可跨快照混合。查询失败、权限失败或未完成分页明确不完整。
3. 用 operationId + requestHash +可信身份逐项合并 Host 投影；删除“有 critique / task completed 就证明本调用成功”的 fallback。
4. 对 accepted：只有旧写权限已隔离，且能证明当前领域变更必与 committed 同事务，才能判 cancelled-before-execution；不能证明的保留 unknown。
5. 在完整枚举和隔离均成立、所有副作用受 Ledger 覆盖时，才可证明“没有登记就没有领域写入”。未知外部副作用不适用此推断。
6. rejected 后重试成功不残留虚假 uncertain；只读失败/取消自动结束；查询失败不自动关闭任务，显示核对失败并允许幂等重试。

测试必须杀实际 Host/MCP/Runtime 进程，覆盖 accepted 前后、事务中、提交后回包前、MCP 回包后 Host 落盘前和撤销竞争。不先 flush 再模拟“未落盘崩溃”。保证限于当前领域事务；不宣称对任意 shell/远端副作用 exactly-once。

## 9. 来源、权限测试和最终产物门禁

### 9.1 同源构建，不给任意二进制补标签

建立唯一 CLI 构建入口：记录实际输入源码树/锁文件/工具链/target/补丁摘要，完成构建，校验过程中输入未变化，生成与产物 hash 绑定的 BuildIdentity。dirty 开发树按内容摘要；正式发布使用可追溯干净提交及受控 CI 构建记录。

测试 harness、模型资格与 staging 共用 provenance 验证器。修改 Rust/指令/契约后旧 CLI 被拒；`--version` 相同、文件名相同或事后记录当前 HEAD 都不能证明同源。无法验证则要求重建，不盲用 target/debug/codex。

先检查并移除确认为上轮格式化产生的无关 `openai_file_mcp.rs` 改动，只处理该已确认片段；不扩大 allowlist 掩盖问题，不覆盖其他人的变更。必要通用内核补丁逐文件独立列明理由。

### 9.2 权限 E2E 使用完整真实链路

运行当前 CLI → App Server → 真实 MCP → 真实 Runtime HTTP → 临时 SQLite。可合成 Provider，但不能手工伪造 trusted metadata 后声称验证了内核身份生产者。

正例：根写候选、子/孙读允许上下文、结果回父；反例：子/孙写、跨任务/项目读、伪造根、缺失/矛盾 lineage、过期/撤销、Review 和 resume。撤销与提交并发须验证实际数据库最终状态。dead port 只能证明网络失败/前置拒绝，不能关闭 G6。

### 9.3 签名和 manifest 的确定顺序

统一 BuildLayout，stage/verify/electron-builder/smoke 读取同一解析结果。自定义资源路径要么贯穿所有消费者，要么在入口明确拒绝，不能 stage 成功后 builder 仍取默认目录。

发布顺序固定为：

1. 构建并核验来源 → stage 完整资源；
2. 对需签名的内嵌 CLI/Runtime/MCP 及其他嵌套代码执行平台签名；
3. 对最终内嵌资源计算 hash、生成 manifest；
4. 签名外层应用/安装器，平台公证或装订按平台流程执行；
5. 从最终安装内容验证签名、嵌套资源 hash 和完整运行旅程；
6. 将最终安装器摘要和验收报告写入外部发布证明，再允许发布上传。

manifest 不包含自身或最终安装器的自引用 hash；外层签名封存后不再回写内部清单。若工具链会重签嵌套文件，必须调整 hook/签名所有权并验证最终字节，不靠忽略校验解决。本地先完成 hook 与顺序测试；有证书后的实际验收另记。

### 9.4 从最终 Electron 产物运行，不止 staged CLI

macOS/Windows 原生 runner 中安装/展开最终产物，隐藏全局 Codex/Node/Python，清除继承凭据并使用临时产品数据目录；保留系统运行所需环境。

验收 Electron 主进程、ASAR/unpacked 路径、Bridge worker、App Server、两个 frozen Runtime，执行合成模型工具续轮、模型切换、停止、重启读取同一作品历史和核对。验证真实 PID/加载路径/数据库结果，不能用固定成功脚本替代。

CI：quality → 平台 package → 最终产物 smoke → release upload。失败可上传明确非发布的诊断日志，不能发布可安装产品。真实付费模型测试与签名实机报告单独标注，不被 mock 替代。

## 10. 实施批次、依赖和退出条件

M0–M4 是交付顺序，W0–W10 是问题归属和内部工作包，两者不新增运行时状态。当前 M0–M3 已在本地可控端点完成纵向验证；M4 的本地实现已落地，但真实供应商、高级能力质量、签名双平台安装和 upstream 演练仍待目标环境。

| 工作包 | 当前状态 | 本轮证据 / 剩余边界 |
| --- | --- | --- |
| W0–W8 | VERIFIED_LOCAL | Engine 62、Agent 102、Runtime 122、Core 定向身份、strict 与 build 全绿；真实协议端点仍归 W10 |
| W9 | IMPLEMENTED_UNVERIFIED | BuildIdentity、stage/verify、资源清单、custom destination 与 macOS/Windows CI 已接线；签名后最终安装旅程待证书和目标主机 |
| W10 | OPEN | 真实供应商、签名安装、上游同步演练、长期漂移与文学质量报告 |

### 10.1 纵向交付门禁：先可验证，再扩大范围

| 门禁 | 本次完整链路 / 所取工作包 | 退出条件 |
| --- | --- | --- |
| M0 可信构建基线 | W0；当前源码 CLI、真实 MCP/Runtime/临时 SQLite harness、原生参考轨迹和当前失败用例 | strict/来源检查通过；旧 CLI 被拒；反例可重放，fixture 不无条件返回成功 |
| M1 单协议完整闭环 | 先用 Chat Completions；贯穿 W1、W2 的 Chat 语义、W3–W8 在该协议下的必需切片 | 请求→校验→真实工具→同协议子/孙 Agent→结果回父→候选提交；取消、资格撤销、预算、权限、丢回复重试及崩溃核对均通过 |
| M2 第二协议验证抽象 | 接入 Messages；重用 M1 全链，补 W2/W3 的 block、并行、cache usage、状态差异及 W4/W5 的有向路线认证 | 两协议同样完成主链；Chat→Messages 与反向组合分别通过；公共逻辑无模型特判，不需复制 Host/预算/恢复状态机 |
| M3 四协议覆盖 | 将 GenerateContent 和 Responses 受控原生透传接入同一纵向套件；关闭相关 W1–W8 剩余范围 | 四协议通过各自矩阵；Gemini 可选 ID/schema/历史与原生逐项语义通过；未登记同协议连接走同套准入，未认证组合拒绝 |
| M4 高级能力与独立交付 | W4/W5 高级扩展、W9/W10；签名后 macOS/Windows 最终产物、真实端点及 upstream 演练 | G1–G10 固定组合报告齐全；不能用全部禁用高级能力代替 G8，也不能用 mock 代替供应商/安装验收 |

每个 M 冻结其协议、参数、路由、工具和故障场景范围。M1 的“完整”是单协议范围内完整，既不意味着只测成功路径，也不意味着提前关闭其他协议的问题。M2 选择 Messages 是为了用不同内容结构和计量语义检验抽象，不代表某厂商得到特殊流程。

M1/M2 的工程验证可以使用可控 Provider，内核、MCP、Runtime 和数据库必须真实；端点实际行为认证仍需真实供应商证据。未支持的专有状态可显式拒绝，但不能把该拒绝计为该能力已交付。M2 若发现公共接口不能准确表达语义，先收敛/修订契约并重跑 M1，再进入 M3；不得靠新增模型名称分支继续推进。

已知 P0 和可误执行/越权问题不因里程碑排序延后暴露：优先修共同边界，受影响路线在验证前保持不可用。F12 的共同计量门禁随 M1 优先落地，Responses 等特定路径须在开放前补齐。控制层漏洞应一次覆盖全部现有入口；尚未完成的入口明确拒绝，不保留旧逻辑绕行。

### 10.2 内部工作包：按里程碑取切片，不按表格串行铺层

下表依赖是当前交付范围内的契约依赖，不要求 W2 的四协议全部完成后才能开始 W4/W5。公共契约及当前调用方须同批一致；W 的全局关闭仍要求覆盖表中全部范围。每批拆成可审查的小提交，复杂逻辑目标 <500 行，一般 <800 行；生成产物单列。不得把当前整个脏工作区打成一个修复提交。

| 批次 | 范围 / 依赖 | 建议最小拆分 | 退出条件 |
| --- | --- | --- | --- |
| W0 可信基线 | F39/F40；其余内核 E2E 的前置 | 无关 diff 清理；BuildIdentity 生成；harness 校验 | strict 通过；换旧 CLI/改源码后明确拒绝；失败用例可复现 |
| W1 请求与提交 | F13–F17/F26/F27；可先写 unit，E2E 依赖 W0 | request context/alias；schema；Chat 终态屏障 | 畸形/重复/错误尾帧工具执行 0 次，合法调用恰好 1 次 |
| W2 协议语义 | F18–F20/F24/F25/F28/F29；依赖 W1 | Gemini ID/schema/历史；Chat 状态/refusal；Messages parallel | 四协议矩阵无静默丢失，opaque 续轮或提前拒绝 |
| W3 计量与预算 | F12/F21–F23/F30；与 W1 并行，联调 W2 | token 计量；编码上限；usage/reservation | 拼接/总量超限阻断，输出限额真实发送，unknown 不清零 |
| W4 资格与配置 | F03–F08/F11；W0 指纹，W1/W2 行为输入 | scope/report；失效/CAS；compiler/extensions | 篡改/跨范围/低级覆盖失败均拒绝；required 真正生效或编译拒绝 |
| W5 执行租约与路由 | F01/F02/F09/F10；依赖 W3/W4 | trusted route context；逐请求 lease；全部入口/bootstrap | 根空子列表可运行；异构边未认证拒绝；过期/撤销无新请求 |
| W6 生命周期 | F31–F33；依赖 W5 | 通用 Host 状态机；配置发布；取消/终结 | 每个 await 障碍点均覆盖；无错关 adapter、取消漏发、清理死锁 |
| W7 事务与恢复 | F34–F38；Ledger 可并行，终结接线依赖 W6 | 完整 receipt；fencing/snapshot；统一核对/UI | 丢回复幂等返回原结果；查询失败不收口；崩溃不漏写/重写 |
| W8 权限闭环 | F41；W0 后先建 fixture，最终依赖 W5–W7 | root/child/grandchild；Review/resume；撤销竞争 | 真实内核到数据库的允许/拒绝结果全部成立 |
| W9 发布实现 | F42–F44；layout/hook 可并行，最终依赖 W0–W8 | BuildLayout；签名/manifest；最终包 smoke/CI | 双平台可运行产物完成旅程；失败不能发布 |
| W10 外部终验 | 依赖 W0–W9 | 真实供应商；高级扩展；签名安装；upstream 演练 | G1–G10 对固定组合有完整报告，无阻断项 |

执行优先级由 M0→M1→M2→M3→M4 决定，不再先把所有 Codec、再把所有资格、最后把恢复整体集成。允许并行准备 fixture 和边界明确的数据模块；不得因并行开发跳过已有纵向回归或发布未认证路线。

首个可交付切片：恢复 strict 边界，建立真正的当前源码 CLI 来源检查，并把“错误 finish 后仍提交工具”做成失败 E2E；随后只修请求上下文与该提交屏障。不要第一批同时改整个 Codec 框架、数据库、Host、UI 和签名流程。

M1 也应拆成小步：先根工具往返，再接可信子任务和共享预算，再接取消/撤销，最后接事务恢复。每一步保持已通过测试，但只有所有规定场景通过才能标 M1 完成。减少单次改动规模不等于缩减里程碑的验收范围。

## 11. 44 项问题到修复与验收的映射

路径为定位入口而非承诺最终文件名；Engine 路径相对 `../agent-engine/src/`，Electron/Runtime/scripts 相对本产品目录。F01–F41、F44 已由第 2 节本地测试集关闭到 `VERIFIED_LOCAL`；F42–F43 为 `IMPLEMENTED_UNVERIFIED`，只有签名后的双平台最终产物完成真实旅程后才能升级。下表保留原始问题和验收断言，避免因实现完成而丢失回归依据。

| ID / 优先级 | 缺陷与主要入口 | 批次 | 关闭所需回归 |
| --- | --- | --- | --- |
| F01 P1 | 运行中资格过期仍派发；bridge/server | W5 | TTL/epoch 改变后旧 lease 的下一次 Provider 请求为 0 |
| F02 P1 | review/compact/认证绕过租约预算；electron/main、conformance-runner | W5 | 全入口无 lease 拒绝；bootstrap 请求/token/time 均有硬限 |
| F03 P1 | scope 未覆盖仍放行，异构边未认证；session/resolver、electron/codex-runtime | W4 | 协议/工具/父子边逐维不匹配失败；A→B 不能由 A→A 替代 |
| F04 P1 | 高 source 成功遮蔽后来失败；capabilities/evidence-store | W4 | 未显式解除的失败阻断；合法同范围 supersession 才恢复 |
| F05 P1 | 401/403 仅作协议失败；verifier、conformance-runner | W4 | 任一入口认证失败撤销同 credentialRealm 的相关路线 |
| F06 P1 | 报告状态/样本/期限未绑定；report-store、resolver、codex-runtime | W4 | 篡改 outcome/sample/expiry、伪造 digest 均无法认证 |
| F07 P2 | 报告整体假 verified；capabilities/report-store | W4 | 子项失败形成 fail/partial，保留全部失败样本 |
| F08 P1 | 构建/指令/Codec 未入指纹；connection-spec、codex-config | W4 | 改 CLI/指令/Codec/suite/编译器使旧资格失效 |
| F09 P1 | logical ID 与 provider model 混用；main、session/codex-config | W5 | id≠model 的四协议根子往返正确，供应商收到真实 model |
| F10 P1 | root 必须列入 child allowlist；bridge/server | W5 | 空子列表根成功；只允许 B 的根 A 成功，非法子 route 拒绝 |
| F11 P2 | required remoteCompaction 不生效；extension-registry、内核 provider | W4 | required 缺可执行路径时编译失败；支持时实际压缩成功 |
| F12 P0 | 10K 单项估算/拼接/Responses 绕过；codecs/canonical、responses | W3 | ASCII/emoji/多 part/原生路径边界测量；>1K 注入人工复核记录 |
| F13 P1 | 工具名二次解码；codecs/canonical | W1 | 原名 f__save 等经一次映射准确执行、不误指另工具 |
| F14 P1 | 调用未按本请求 schema/目录校验；codecs/canonical | W1 | 未声明工具、标量、null/缺必填参数执行数 0 |
| F15 P1 | Chat 缺 ID 伪造执行；codecs/chat-completions | W1 | 缺必需 call ID 拒绝，无执行事件 |
| F16 P1 | 重复 call ID 可提交；codecs/chat-completions、canonical | W1 | 同响应重复 ID 整体提交失败，无重复派发 |
| F17 P1 | finish 后 error 被忽略；codecs/chat-completions | W1 | finish→error/异常 EOF 不执行；合法完整流只执行一次 |
| F18 P1 | Gemini 可选 ID 被当必需；codecs/generate-content | W2 | 无 ID 合法函数调用与结果续轮稳定关联 |
| F19 P1 | Gemini schema 字段/dialect 错误；codecs/generate-content | W2 | const/additionalProperties 等约束真实保留或发送前拒绝 |
| F20 P1 | Chat reasoning_content 丢失；codecs/chat-completions | W2 | thinking 工具续轮及恢复保真，不支持时提交前拒绝 |
| F21 P1 | Chat 未发送输出额度；codecs/chat-completions | W3 | 真实请求携带 dialect 对应上限且不超过 reservation |
| F22 P1 | 缺 usage 被按 0 结算；codecs/canonical、state/budget | W3 | 空/部分 usage 保留未知预留，不额外放行后续请求 |
| F23 P1 | Messages 缓存输入漏计；codecs/messages | W3 | 10+700+100 输入与 3 输出按协议计量合计 813 |
| F24 P2 | Chat refusal 消失；codecs/chat-completions | W2 | content=null + refusal 不再显示空成功 |
| F25 P2 | Gemini 未知 part 静默丢弃；codecs/generate-content | W2 | inlineData/code 等未认证 part 明确不支持 |
| F26 P2 | alias 字符不合法；codecs/canonical | W1 | Messages 名称长度/字符边界及冲突测试通过 |
| F27 P2 | 全局 LRU 淘汰在途别名；codecs/canonical | W1 | 超过旧 4096 条压力下活跃映射稳定；新超限目录明确拒绝 |
| F28 P2 | Gemini 改写已发送历史；codecs/generate-content | W2 | 多轮追加后原有已发送项前缀逐项不变 |
| F29 P2 | Messages 默认选择时 parallel=false 丢失；codecs/messages | W2 | 未传 tool_choice 仍能观察 wire 上的并行限制 |
| F30 P1 | route.inputBudget 未执行；bridge/server | W3 | 多个合法小项组成超限总输入时发送数 0 |
| F31 P1 | 配置切换与启动竞态；electron/main | W6 | 验证暂停→启动→发布配置，不能关掉活动 adapter |
| F32 P1 | preparing 晚取消仍 sendTurn；electron/main | W6 | 每个异步准备屏障 cancel 获胜后 turn 发送数 0 |
| F33 P1 | endTaskBudget 异常卡 active；electron/main | W6 | 清理故障释放匹配槽位并保留未知账，不影响新任务身份 |
| F34 P2 | 正常终态不核对；main、recovery-reconciliation | W7 | 只读失败/取消、拒绝后重试成功无需重启即可收口 |
| F35 P1 | Ledger 查询失败仍关闭任务；recovery-reconciliation | W7 | 无 Host 写事件且 Ledger 查询失败时仍保留待核对 |
| F36 P1 | 用任务状态伪造逐调用回执；recovery-reconciliation | W7 | 无关 critique/completed 不得确认未知 operation |
| F37 P2 | accepted 崩溃永久人工项；runtime/creative_workspace/generation | W7 | 撤销隔离后可证明未提交的 accepted 自动安全收口 |
| F38 P2 | 幂等不返回原结果；runtime/server、operation_ledger | W7 | 提交后丢回包重试返回同 revision/result；complete_task 同样成立 |
| F39 P1 | strict fork 检查失败；scripts/verify-fork-boundary | W0 | 消除确认的无关 diff，不扩大白名单，strict 实际通过 |
| F40 P1 | CLI 二进制来源无法证明；integration harness、refresh/stage | W0 | 同版本旧产物和伪报当前 HEAD 均拒绝；当前源码重建通过 |
| F41 P1 | 权限测试未贯穿真实 Runtime；codex-engine-integration | W8 | 真内核→MCP→HTTP→SQLite 根写/子孙读与拒绝竞争全部通过 |
| F42 P1 | 签名前 hash 与签名后验证冲突；scripts/stage-release-runtime | W9 | 嵌套签名后生成 hash；最终签名安装内容逐项校验成功 |
| F43 P1 | 未测试最终 Electron 产物；verify-release-runtime、CI | W9 | mac/Windows 最终产物启动、切换、工具往返、重启恢复 |
| F44 P2 | staging override 与 builder 路径不同；package、stage/verify | W9 | 自定义路径全链一致或入口拒绝，不误打默认资源 |

## 12. 验收、记录与最终完成标准

### 12.1 每批必须提交的证据

先记录当前反例，再实施，再记录同一断言的通过结果。测试检查实际请求、工具执行次数、返回内容、数据库与进程结果，不只检查配置字段、事件名称或模型自称完成。

每批记录：M/W/F/G 编号、准确 scope、源码指纹、CLI/资源摘要、平台、命令、样本、失败记录、未覆盖范围、测试时是否 dirty。禁记供应商密钥和用户正文。固定 fixture 不能不检查收到的随机指令就返回成功。

状态只按证据升级：OPEN → IMPLEMENTED_UNVERIFIED → VERIFIED_LOCAL → VERIFIED_TARGET。无法本地验证的代码不能直接标“外部待验收且已完成”；需明确实现是否存在、本地替身证明了什么、目标环境缺什么。

关闭 F 项须覆盖其所有受影响路径；单协议通过只更新该 scope 的证据，不把整个 F 项提前关闭。W 完成必须关联相应 M 链路报告，不能仅提供模块 unit 测试。

当前可用命令包括产品目录下 `npm run test:engine`、`test:agent`、`test:runtime`、`test:ui`、`build`、`repo:check:strict`；新增契约/产物命令在实现后才写成可执行步骤。Rust 测试按仓库规则使用 `just test -p …`，新增 Agent 逻辑须有 integration test。代码改动后执行规定 fmt/fix；完整 Rust workspace 测试须先获得用户同意。此文档不是执行这些命令的记录。

### 12.2 不变量、随机化与故障注入门禁

| 不变量 | 生成/注入方式 | 断言位置 |
| --- | --- | --- |
| 分片不能改变同一合法响应语义 | 在 UTF-8、多字节字符、JSON/SSE 边界随机切块，包含空块和极小块 | 解码后的语义、调用 ID、实际执行次数一致；不要求文本 delta 分块相同 |
| 未确认候选不得执行 | 缺 ID、重复 ID、schema 错误、finish 后 error、异常 EOF、取消交叉组合 | 转换路径候选工具执行 0 次；原生路径只允许已合法提交的项，不伪称撤销 |
| cancel 先于派发则不发送 | 在每个 await 屏障暂停，排列 start/cancel/configure/revoke 顺序 | Host 的线性化点、Provider 请求计数、真实工具调用记录 |
| 额度不能重复释放或绕过 | 并发 reserve、重复 settle、缺 usage、Bridge/控制通道断开 | 账本 reservation 唯一、准入额度守恒、unknown 仍保守占用 |
| 失效资格不能取得新派发权 | 可控时钟、过期、撤销广播延迟、旧 epoch 重放 | 发送前门禁及 worker 确认；不靠 sleep 恰好命中竞态 |
| 同一领域操作不重复提交 | accepted/事务/回包/Host 落盘间杀真实进程，重放相同或冲突请求 | SQLite 精确操作/候选数量及原回执；不以 UI completed 为准 |
| 核对不完整不得声称成功 | Ledger 查询错误、分页中断、无 Host 事件、迟到旧请求 | RecoverySnapshot 完整性及任务保留待核对状态 |
| 适配不改写已提交历史或私有状态 | 多轮追加、恢复、共享别名压力、原生/转换差分 | 既有前缀、opaque 字节/顺序、会话关联；差异必须显式拒绝 |

随机测试固定可重放 seed，失败保存经过脱敏的最小输入与事件顺序；CI 同时运行固定回归 seed 和有时限的生成样本，较大故障矩阵分到独立 job。用屏障/可控时钟验证竞态，不依赖长 sleep；性质测试不能代替真实进程崩溃 E2E。

测试还要有反证：对关键断言用受控错误 fixture 验证测试确实会失败，例如模拟重复提交或把未知 usage 当 0。不得为通过测试放宽 oracle。所有新增反例永久加入回归，并重跑之前已通过的纵向门禁。

### 12.3 G1–G10 的最终证据

| 目标 | 必须验收 | 主要依赖 |
| --- | --- | --- |
| G1 共用内核 | 同一当前 CLI 的原生/桥接共享行为套件，真实工具与任务正文/结果 | W0–W3/W5 |
| G2 通用接入 | 未登记同协议模型只增连接/必要规格，走同套认证，不加专属代码/提示词 | W2/W4/W10 |
| G3 四协议 | 语义/streaming/状态矩阵及四类真实参考端点；不支持范围可见 | W1–W4/W10 |
| G4 自动资格 | scope、报告、构建、凭据、TTL、撤销、失败替代贯穿新请求 | W4/W5 |
| G5 原生多 Agent | 同模型和双向异构委派、通信、wait、汇总、失败/取消/恢复、嵌套 | W2/W4–W8/W10 |
| G6 可信权限 | 当前内核身份来源与真实领域允许/拒绝、Review/resume、撤销竞争 | W5/W7/W8 |
| G7 可控可恢复 | 预算/取消/配置竞争、全崩溃窗口、完整核对及 unknown 展示 | W3/W5–W7 |
| G8 高级能力 | 声明所需扩展实际执行；原生强模型能力不被共同接口无端删除 | W2/W4/W5/W10 |
| G9 独立交付 | 签名后 macOS/Windows 干净安装真实旅程，凭据/资源/进程隔离 | W0/W9/W10 |
| G10 上游持续同步 | 实际同步新上游→重新构建→契约差异→旧证据失效→回归→产物报告 | W0/W4/W9/W10 |

安全拒绝不支持组合是正确行为，但不能把“拒绝全部高级功能”当成 G8；合成端点证明工程，不证明真实模型规划质量。真实端点按固定 suite 保留成功/失败分布，不能重复试到成功才记一次。

### 12.4 何时可以宣布完成

- 本轮代码修复完成：F01–F44 各有相应证据，W0–W9 的实现与目标测试完成，并通过对应 M 纵向门禁，无新增绕过路径。
- 理想架构目标完成：再完成 W10 与 G1–G10 全部固定组合报告；没有“实现完成但本地阻断仍在”的表述。
- 需要外部资源时精确列出端点/模型/参数范围、平台、证书和执行权限；不得借用 Codex 桌面或普通 CLI 的认证/额度。
- 每批同步理想设计与 ARCHITECTURE 的状态，任何反例出现立即撤回对应资格，而不是保留旧的全绿标签。

### 12.5 用结果衡量 Bug 风险，而不是数模块和测试

按固定测试范围记录误执行、重复写入、取消先于派发却仍发请求、无资格请求、恢复误判的次数与分母。这些安全/一致性不变量在验收集中必须零违反；零观察不等于证明生产绝无 Bug。

持续记录已关闭 F 项重新出现的数量、集成后才发现的缺陷、变更范围及最小复现。原生透传另记录同基线的时延/内存变化。对比前后必须标注源码、样本范围和未覆盖项；不以测试数量增长、接口数量下降或一次全绿直接宣称缺陷率降低。

## 13. 协议参考与文档关系

下列官方资料用于协议决策，实际实现仍需绑定版本和回归测试：

- [Gemini FunctionCall](https://ai.google.dev/api/generate-content#FunctionCall)：调用 ID 的可选语义。
- [Gemini FunctionDeclaration](https://ai.google.dev/api/generate-content#FunctionDeclaration)：parameters 与 parametersJsonSchema 的边界。
- [DeepSeek thinking mode](https://api-docs.deepseek.com/guides/thinking_mode/)：工具续轮的 reasoning_content 状态。
- [Claude cache usage](https://platform.claude.com/docs/en/build-with-claude/prompt-caching#tracking-cache-performance)：缓存输入计数。
- [Claude 工具定义](https://platform.claude.com/docs/en/agents-and-tools/tool-use/define-tools)：工具名与工具 schema 约束。

[理想技术方案](MODEL_AGNOSTIC_CODEX_ENGINE_DESIGN.md) 定义目标；本文定义修复路径和本轮问题台账；[当前架构](ARCHITECTURE.md) 描述产品分层。v1.1 的完成判断已被最新 review 撤回，不再作为实现通过依据。

本次交付仅完成上述文档和两份架构文档的状态校正。没有把任何 F/W 项标为已修复，没有运行模型认证、修改代码或清理用户数据。

v2.2 相比 v2.1：记录协议、资格租约、可信路由、生命周期、Runtime Ledger、BuildIdentity 和 CI 的实际落地；更新本地测试证据与 F/W 状态；明确区分本地完成和 F42/F43/W10 的目标环境终验。F01–F44 台账与 G1–G10 目标保持不变。
