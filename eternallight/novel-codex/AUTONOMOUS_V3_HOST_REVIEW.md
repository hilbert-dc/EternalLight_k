# v3.0 Host / Engine 动态协作与恢复评审

日期：2026-10-07。范围：Host 协作提示、托管角色、Engine recovery journal，以及协调授权的 Native `delegated_candidates.rs`。现有 `engine-turn-lifecycle.mjs` 和 `active-turn-recovery.mjs` 保留实现并定向验收。本记录按五种职责视角进行评审，不代表五位独立人员审查。

## 变更目标与已核对事实

- 角色模板只描述有用的方法，模型可以按当前问题临时分工；不增加必须经过的写作—评审流程。
- 实施前，角色文件用不完整 MCP transport 声明五个读工具，实际 Codex 启动发出 `invalid transport` 并丢弃角色；补齐 transport 后又证实当前 Codex role overrides 不应用 MCP 工具白名单。角色模板不能被当作工具权限。现在使用有效的纯角色配置；Native 可信身份与实际授权承担数据权限，分页调用由 Runtime 同步开放。
- 实际启用的 Codex multi_agent_v2 `spawn_agent` 返回 canonical `task_name`，不返回 UUID；`list_agents` 也不提供 UUID。写作 child 从只读 Native receipt 取得 trusted `callerThreadId` 并报告父任务，父任务授权该真实身份后通知 child。没有建立 Host 名称到 UUID 的第二个权威表。
- 隔离写作必须由 Native 受限授权保障。子任务不得通过父 task capability 写父当前稿、采用或发布；产物应为带有来源版本的候选引用，父任务核对后整合。
- 当前 `executeManagedTurn` 已保持 preparation、流和后续 RPC 的单一 operation 归属；`readActiveTurn` 只核对同一 process / operation / waiter，不另开 turn。保留这些机制，定向检验迟到结果与取消竞态。
- 活动 turn 的工具结果与子任务通信继续由 Engine 消费。仅恢复挂起执行时使用 Native 持久事件领取与确认，不新增 Host 的文学调度器或另一个事实账本。

## 零兼容门禁

- [x] 不新增版本探测、旧字段 fallback、双读双写、迁移、回填或降级。
- [x] 不维护旧角色、旧接口或旧 fixture 行为。
- [x] 角色配置、模型声明、Native 当前授权和测试必须一次性一致；接口未实现前不宣称能力可用。
- [x] 只使用临时测试作品和脚本协议，不调用真实模型或修改真实小说。
- [x] 不放宽 9,000 tokens / 38,000 UTF-8 bytes 输入限制、累积预算、当前权限和停止边界。

## 五视角预审

| 视角 | 结论 | 要求 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | Engine 负责一般工具循环与子任务，Runtime 负责原文与候选权威、授权和持久事件；Host 只桥接，不建立业务状态副本。 |
| 技术 A | APPROVE WITH CONDITIONS | 分页读取保留可信 caller / parent 身份；子任务读过不能冒充父任务验收。候选写入由服务端校验隔离 grant、冻结来源、取消状态和累计预算，禁止提示词授权。 |
| 技术 B | APPROVE WITH CONDITIONS | 用当前协议测试读工具实际可调用、越界写入被拒绝、候选引用核对、完成前后竞态、重复事件与暂停取消。已有跨 RPC 所有权保护不重写。 |
| 产品 | APPROVE | 短请求驱动真实工作；作者不需要指定角色、分页参数、恢复或重复重新生成。资料缺失如实报告，候选完成不等于作品完成。 |
| UI/UX | APPROVE | 本范围不增加角色菜单、诊断 UI 或确认框；新公开说明遵循作者语言，技术身份仅用于内部授权和追踪。 |

## 综合决策

批准按当前契约实施通用协作和可测恢复。隔离候选写入及持久恢复接线有硬性条件：Native 必须提供实际授权与幂等消费接口。已向主任务提出以下联动要求，接口未就绪不得仅开放写工具或宣称闭环：

1. 子任务只读分页：可信 child caller / parent 元数据传到 Runtime；来源仍绑定同一冻结 task，阅读进度分清实际读取者。
2. 隔离候选授权：父任务选择有权限的目标和任务范围，绑定实际 child 身份；保存仅影响候选 namespace，返回来源版本与候选引用；父整合使用当前目标版本 CAS。
3. 持久唤醒：Runtime 按 event / Work / Attempt / session 领取和确认，活动 turn 不重复 Host 派发，已暂停或取消的目标不启动新执行。

## 实施后复核

实际变更：

- 九个角色文件全部与单一渲染源一致，删除无效 MCP override。公开回复语言规则直接进入各角色 developer instructions；范围、资料原文与他人报告的非权威性、真实分页和父任务核对责任一起保留。角色指令为 2,460–3,077 UTF-8 bytes，child 持久指令 2,015 bytes。未改 9,000 tokens / 38,000 bytes 总输入限制。
- 临时分工继续由模型选择。研究 child 读取资料不等于 root 完成保护性阅读；可选写作 child 必须获得 Native exact identity grant，仅保存当前 task 的不可变隔离候选。父任务核对后传 `delegatedCandidateId`，不把全文转抄成第二个 payload。
- Native grant 固定 root、实际 child、input revision 和 target hash；保存核对直接 lineage、当前任务/来源、Work 和 Task 取消状态、候选 CAS 和幂等键。整章锁定段落、selectedScenes 和 throughScene 使用现有原生保护。前缀写作的隐藏后缀仅在 Native 私下合并校验，隔离稿存储与读取不返回隐藏正文。
- 分支目录每页八个当前候选，具有 total/nextCursor/truncated；snapshot 只提供明示摘要。隔离稿正文按 UTF-8 边界分页，保存不改变 manuscript head、project revision 或 task outputs。
- Host recovery journal 只接收绑定 root 的事件；缺少身份、child 或其他 root 的同名 turn/item 不能终结 root 账本。现有活动 turn 归属和恢复读单飞逻辑保留，没有新增文学调度循环、固定关键词或第二套完成判定。

测试证据：

- Host 定向四套测试 54/54；Engine authority-state 16/16。覆盖重复请求、停止后迟到 start/failure、stream 与 RPC 归属、compaction 冲突、原 waiter 完成/失败/中断、跨线程/回合拒绝、child 事件归属、未知写结果和重开账本。
- `managed-collaboration.integration.mjs` 1/1：实际本地 Codex + Bridge + 已安装角色；脚本模型与脚本 MCP。两类 child 实际读取四个来源分页，writer 回报真实身份，root 授权并通知、读取隔离引用、凭引用提交，根 turn 只启动一次。每个 child 的 developer 上下文含公开中文规则，无 invalid transport。该测试不证明文学质量或生产 Native/MCP 保存链；生产原生整合由主任务另行验证。
- Native 七组测试 7/7（0.30 秒）：grant、lineage、冻结目标、重放/CAS、Unicode 分页、正式稿无变化、暂停取消迟到写、锁定组与前缀保护；当前 schema 数据库重开后授权、候选与 CAS 保留；九分支完整目录分页且 snapshot summary 明示 total/truncated。

五视角复核：

| 视角 | 结论 | 实施核对 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 通用协作与活动结果留在 Engine；Native 决定作品写权限。生产 API 完整整合测试由主任务补齐，不以脚本 MCP 替代。 |
| 技术 A | APPROVE | 角色白名单假设已移除；身份和写入权限具有原生拒绝测试，隐藏段落不会成为 child 候选输出。 |
| 技术 B | APPROVE | Host/Engine 定向、实际角色 wire 与 Native 持久/分页测试已通过；未使用脚本模型输出作为作品质量证据。 |
| 产品 | APPROVE | 作者无需提供角色或 UUID，无新增固定写作流程；错误在内部协调解决。保存候选不冒充交付。 |
| UI/UX | APPROVE | 无新增角色菜单或授权对话框，公开语言政策持续传入 child，既有错误与来源只在适合的位置展示。 |

零兼容门禁：PASS。未新增旧格式探测、迁移、fallback 或双轨。与预案的必要偏差是依据实际 Codex 源码与运行证据，把无效角色工具权限改成 Native exact identity grant；没有修改 Codex 内核。语言规则存在于模型上下文，不承诺模型永不忽略中文要求。

## 角色按需发现与生产保存链补充预审

本增量以同一 `NOVEL_AGENT_ROLES` 和 developer instructions 渲染函数同时生成实际角色 profiles 与 Native 内嵌 JSON。资源只提供可选方法、中文检索主题和明确的权限说明；角色名、说明及检索结果均不能授予 Native 权限。Native 发现接口由主任务接入，默认返回简要目录，按实际 roleId 读取指令；不要求模型调用全部角色，不新增用户配置入口。

| 视角 | 结论 | 门禁 |
| --- | --- | --- |
| 架构 | APPROVE | 唯一角色源位于 Host 模块，JSON 和 profiles 是同一个生成器的产物；Native 只消费生成资源，不复制角色定义或权限权威。 |
| 技术 A | APPROVE | 保留 Native trusted identity、任务范围及显式 writer grant。实际角色指令和完整单角色资源均须低于 4KB，公开语言与阅读归属必须一致。 |
| 技术 B | APPROVE WITH CONDITIONS | `--check` 检验实际生成文件；验证引用指向可用发现接口、中文主题、模板指令一致性和预算。生产 MCP 的可选字段省略不能触发进程 panic，需真实 Runtime/MCP 链回归。 |
| 产品 | APPROVE | 模型按问题查找有用方法，作者不用了解角色名、内部 UUID 或重发任务。 |
| UI/UX | APPROVE | 不新增角色菜单和诊断对话框；角色资料属于内部按需上下文。 |

零兼容门禁：PASS。仅使用当前格式和当前 schema 的临时测试数据，没有旧资源解析、fallback、迁移或第二个 profiles 生成器。

## 补充实施后复核

- 角色源新增中文检索主题，`novelRoleDeveloperInstructions` 同时供 TOML 与 JSON 使用，实际模型指令内容没有另一个副本。`scripts/generate-managed-agent-roles.mjs` 是两种产物的唯一生成器，`--check` 精确拒绝漂移且不隐式修复；Native 可内嵌 `shared/novel-agent-roles.json`。权限模式只是方法说明，Native grant 和可信身份仍是实际权威。
- 单角色完整 JSON 为 2,771–3,489 UTF-8 bytes，完整 profile 为 2,694–3,363 bytes。指令仍为 2,460–3,077 bytes，保守 token 预算每角色不超过 1,000；未放宽持久或总输入预算。
- 真实 Runtime + MCP 临时当前数据库验收发现并修复两处 `serde_json::Map` 可选字段索引 panic：`read_delegated_draft` 首次目录/正文没有 cursor，以及普通 `save_draft_checkpoint(payload)` 没有 delegatedCandidateId。现在均用可选字段读取。新增候选工具的 required 字段继续经既有 schema 和 oneOf 门禁；没有把参数错误转化为作者需要理解的内部步骤。
- 生产候选链集成 1/1（4.76 秒）：当前 Native Runtime/MCP、临时数据库与可信 Engine-shaped 测试元数据，验证真实身份、资料分页、未授权和跨 child 拒绝、grant、不可变旧引用、分支与正式 CAS、locked 段落保护、payload/ref 互斥、实际正式回执及跨 task 拒绝。该测试没有启动真实模型；实际 Codex 子任务身份与角色 wire 已由前述独立集成验证，两层证据不混称为文学质量验收。
- 现有实际 Codex + Bridge + Runtime/MCP 普通草稿纠错集成 1/1（1.86 秒）：4,500 汉字正文按真实单一表示保存并读回，同一预算三次请求完成。新增 MCP 可选字段单测 1/1（0.00 秒）；角色资源/生成器测试 2/2，托管角色定向回归 3/3；生成 `--check` 和所拥有文件 `git diff --check` 通过。

| 视角 | 结论 | 补充核对 |
| --- | --- | --- |
| 架构 | APPROVE | 隔离稿用真实引用整合，未复制正式写入权威；角色资源与 profiles 共用源函数，发现入口由主任务接入。 |
| 技术 A | APPROVE | 未授权、跨 child/task、immutable reference、正式 CAS、locked 保护均有生产链拒绝证据；生成角色仍不能授予权限。 |
| 技术 B | APPROVE | 可选字段 panic 在真实链复现后修复，普通保存与角色生成一致性回归通过；未将脚本模型当作真实创作品质证据。 |
| 产品 | APPROVE | 作者无需重发全文、配置角色或纠正内部引用；参数修复继续在同一预算内完成。 |
| UI/UX | APPROVE | 新方法目录按需读取，无额外角色配置或诊断交互。公开语言规则在实际 role 指令和发现内容中一致。 |

零兼容门禁：PASS。Native 按需角色发现工具与构建入口联动由主任务负责接入；本记录不将该联动未验证的部分列为已完成。

## 当前恢复契约 P1 修复预审

实机只读证据：当前作品的 20 个未决 run 含 593 个被 conservative contract 标记为 write 的请求，其中 566 个是已核对的只读工具，27 个是真实写入或动作。590 个请求在 Native operation ledger 没有记录（含 563 个只读、24 个真实写入或动作），3 个 exact call/hash/root 匹配但状态 accepted、没有 receipt。只读工具被标记为未知写入是当前工程错误；没有记录的真实写入和 accepted 写入不能仅按终态或泛错误解除。

方案：为小说应用注入领域 RecoveryOperationContract；只读工具按已核对的实际工具效果分类，未知工具继续 write。调用 hash 使用 Engine 原生 serde_json 稳定序列化，保持与 Native MCP host_json 有效数字域一致。MCP 仅返回绑定实际身份和 hash 的可信证明：明确且尚未 dispatch 的本地 ArgumentValidation 拒绝；或者精确只读 Runtime operation ledger 查询返回的 committed/rejected 状态。调用过 Runtime 后的异常、泛 HTTP200、模型文字、旧日志、目录或 task completed 均不能制造保存回执。Engine journal 仍排除迟到 child/其他 root；活动查询不调用 recovery fence。

| 视角 | 结论 | 门禁 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 领域效果和证明留在应用/MCP，Engine 使用通用契约，不新增第二个保存账本或重新执行写入。只读 ledger GET 由主任务接入。 |
| 技术 A | APPROVE WITH CONDITIONS | proof 严格核对 task/project/root/caller/turn/call/tool/hash；beforeDispatch 由真实 private dispatch 状态保障；unknown 与迟到/跨线程证明 fail closed。 |
| 技术 B | APPROVE WITH CONDITIONS | 用真实 MCP 对照 hash 和生产持久回执，覆盖当前负面参数、纯只读、未知写、伪造/cross-root/late-child，以及 dispatch 后异常。 |
| 产品 | APPROVE | 参数纠错不制造永久未决保存，也不要求作者提供技术字段；真实未知写保留可理解的阻止。 |
| UI/UX | APPROVE | 不增加确认框、调试字段和手动清空按钮；当前恢复记录只能经领域已有只读核对收敛。 |

零兼容门禁：PASS。不得改真实恢复 JSON/DB，不依据旧格式创建回填或迁移，也不根据取消字段抹除可能已提交的写入。

## 当前恢复契约实施后复核

实际变更：新增应用领域契约 `electron/novel-recovery-contract.mjs`，按唯一资源 `shared/novel-recovery-read-tools.json` 明确工具效果。main 仅在 Host journal 初始化注入实际 task/project/root 绑定。Native MCP 用私有 dispatch 状态证明本地参数拒绝，并经当前 Runtime 精确 ledger GET 获取真实持久状态；授权隔离写作的 grant 和正式 root 操作在既有事务中记录 receipt。Engine journal 接受当前 app-server 的 `failed` 且有 result 事件进行领域证明验证，transport failure 无 result 仍为未知。

`_meta.callId` 由当前本地 Codex Core `core/src/mcp_tool_call.rs` 的 `build_mcp_tool_call_request_meta` 发出；`toolCallId` 是 Engine 内部规范化字段，不新增旧 wire fallback。Host hash 与真实 MCP Native operation hash 在生产集成中逐次比较，不从模型文字或 HTTP200 推断保存。

验证证据：当前 Engine release 编译通过；Native Runtime/MCP bins 编译通过；Engine authority-state 16/16 通过；领域恢复测试 4/4 通过；真实临时 Native Runtime → MCP → Host journal 集成 1/1 通过（2.89 秒）；Native MCP 完整测试 76/76 通过（3.00 秒）。生产集成覆盖 child 实际分页身份、Native grant、隔离不可变稿、CAS、locked 保护、父引用正式保存及 actual committed receipt；当前 failed wire 的参数拒绝可以正确 settle。负面测试覆盖无 proof 的 JSON 错误、错误 hash、跨 root、child proof、accepted proof、迟到事件、无 result transport failure，全部保持 unknown。Native 两项 proof 测试还验证了 dispatch 标记、真实只读 HTTP ledger query、错 hash 拒绝及 accepted 无回执仍 unknown。测试只使用临时当前 schema 数据，没有真实模型或真实作品写入，不能把这组生产集成等同于全文文学验收。

| 视角 | 结论 | 实施后核对 |
| --- | --- | --- |
| 架构 | APPROVE | 领域事实与 hash/proof 留在应用和 Native；Engine 只处理通用 journal 状态，不新增保存判定账本或文学调度器。 |
| 技术 A | APPROVE | 精确原身份与原 hash、原生持久 receipt 和私有 dispatch 标记共同约束结案；调用 Runtime 之后不能冒充 preflight，真实 unknown 不会重放。 |
| 技术 B | APPROVE | 当前 failed-result wire 已用真实 Engine addon 和生产 Native MCP 证明；测试同步新增 required 字段和 proof，未降低权限或绕过 ledger。 |
| 产品 | APPROVE | 读资料和可纠正参数错误不再制造虚假的未决保存；真实不能确认的写入仍保留门禁，作者无需填写内部字段。 |
| UI/UX | APPROVE | 不增加调试栏或人工清空入口；应用继续用已有状态核对给出可理解结果，不把 unknown 伪报为完成。 |

零兼容门禁：PASS。实际作品 journal/DB 保持只读。旧工作区的 24 个 absent 真写/动作和 3 个 accepted 无回执写入仍需要现有领域核对提供真实证据；本次代码修复不能将它们清除或视为已完成，也不证明《万城灯火》第 7 章或前 100 章已验收。

最终执行记录：`cargo test -p novel-codex-story-mcp -- --test-threads=1` 在工具 session 63861 返回 exit 0、76 passed / 0 failed（3.00 秒）；`node --test electron/tests/delegated-candidate-native.integration.mjs` 在 session 46110 返回 exit 0、1 passed / 0 failed（3.21 秒）。后者还以每个真实 child/foreign caller 的实际 callId 核对根 journal 没有对应请求，不使用固定空 ID 代替线程隔离证据。主任务新增 `recovery_operation_read.rs` 与 Runtime 精确 GET 的只读边界复核无必须修正项，真实 HTTP 2/2 的执行证据由主任务记录。
