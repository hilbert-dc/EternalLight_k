# 持久接续与来源检查闭环

## 目标与方案

作者通过原对话交付“先修订、核对，再接回原长篇任务”。模型负责理解目标、拆解依赖、选择原任务与检查范围；工程负责保存模型决策、验证真实证据、执行并恢复交接。不得识别“继续”等词来生成固定路线，也不得把回复中的未来承诺当作执行记录。

本轮新增模型计划中的 continuation 目标，前置未完成时可记录目标，完成后以真实 generation proposal 为证据满足它。已记录的未完成接续目标不能以无阻塞的 partial 回复悄悄关闭：模型必须登记交接，或明确记录 blocked 及真实原因。完成门禁区分前置成果通过与长篇范围最终完成，partial/needsAuthorDecision 不自动接续。Host 从持久 proposal 恢复交接，使用 Native 幂等执行回执，不依赖本次内存结果；保持原范围、预算、意图、未知写入门禁。

检查使用已授权、当前修订的正文证据，而非仅来源目录。模型选取原句并解释事实关系，工程验证来源身份、当前版本和引文锚点；证据变化后检查失效。一般检查无需先做知识建模。工程不宣称能自动判定文学质量。

范围包括 Core、MCP、Runtime、Host、当前契约及测试、现行文档；实际小说内容只经应用与 DeepSeek 创作，不直接改业务数据库。

## 实施前五角色评审

本记录由主实施者依据两名只读审计代理的实际代码与运行记录完成；角色视角不代表五位独立人员。

| 角色 | 结论 | 条件与依据 |
| --- | --- | --- |
| 架构专家 | APPROVE WITH CONDITIONS | 语义目标属于模型；能力、预算与落盘属于 Native。计划证据支持接续，持久消费可跨应用重启恢复。 |
| 技术专家 A | APPROVE WITH CONDITIONS | 保留版本 CAS、同项目同对话、前置完成、来源校验、未知写入与重复点击门禁；部分交付不能自动续写。 |
| 技术专家 B | APPROVE WITH CONDITIONS | 同步当前 schema、工具说明、fixture 与回归；测试漏登记、过期来源、伪造引文、重启交接与重复执行，不只测成功路径。 |
| 产品经理 | APPROVE | 用户只说目标即可；修订、核对、接续形成可恢复流程，避免以“下一决策”承诺结束工作。 |
| UI/UX | APPROVE WITH CONDITIONS | 正文与自然中文进度在前；已保存、检查待处理、已接回分别据真实状态显示，内部证据按需查看。 |

零兼容门禁：未新增历史迁移、回填、双读双写或旧格式 fallback；所有变化针对当前接口与当前数据一致性。新协议只用于本轮新任务，不补造历史模型决定或历史验收。

## 验收

- 前置保存但后继未登记不能 falsely fulfilled；明确 blocked 可以诚实停止。
- 已登记交接只在前置 fulfilled 后执行；partial 与作者待决定状态不执行。
- 应用在前置完成或交接边界退出后，可以从持久回执恢复，重复操作不创建新范围或重写已有章节。
- 检查引用实际当前正文；不存在、越权、旧修订、错误引文拒绝，源变化后 satisfied 失效。
- 在应用中用真实 DeepSeek 修订第7章，读回核对第6章事实，再验证原任务从第8章接续；实机结果与工程测试分别记录。

## 实施后复核

### 本轮实机发现的启动边界补充设计（实施前）

实际重启输出 Runtime 启动超时并退出，窗口未创建。多个新 Native 测试进程采样还在系统 `_dyld_start`，尚未执行测试代码；不能把这个等待解释成模型或小说交接卡住。应用每90秒终止并重试，以及等全部服务就绪才显示窗口，使等待不可见并可能反复打断正常系统加载。

方案：Host 在后端等待前创建窗口并提供基础启动状态 IPC；状态为 starting、slow、online、offline。90秒只显示“启动较慢，仍在等待”，保留同一活进程。真实退出或错误允许 single-flight 重试。初始化与恢复只在本次启动一次完成后发布 online。Renderer 用独立启动门禁，首次 online 才挂载原 App；不在正常使用后的离线状态卸载 App，保留未保存编辑。等待只有一行说明和必要的重试入口，诊断折叠。关闭窗口正常停止本应用启动，无安全设置变化、无凭据读写、无模型调用或小说写入。

| 角色 | 实施前意见 |
| --- | --- |
| 架构专家 | APPROVE：启动生命周期属于 Host，模型与小说执行协议保持原职责；UI在服务真正可用前不请求创作。 |
| 技术专家 A | APPROVE WITH CONDITIONS：同一个活进程等待、single-flight重启、一次恢复、关闭取消；不得修改系统验证与安全设置。 |
| 技术专家 B | APPROVE WITH CONDITIONS：同步状态类型、IPC、生产入口和启动 fixture；测试延迟就绪、真实退出、重复重试及在线后保留编辑。 |
| 产品经理 | APPROVE：可见的启动反馈解决当前无法打开应用的实机断点，不让作者重复提交创作要求。 |
| UI/UX | APPROVE WITH CONDITIONS：启动只显示简短中文状态，超慢等待不伪报失败；仅失败时需要重试，不增加技术表单。 |

零兼容门禁：当前启动契约直接更新，不新增旧状态适配、迁移或历史回填。该补充在实现前经过上述五视角复核；最终复核与实机结果仍需填写。

本轮已实施以下通用边界：

- 工作计划区分只规划和执行目标；接续目标必须绑定真实原 run 与本 Work 交接回执，未执行目标不能用未来承诺关闭。
- 检查与章节评审引用当前授权正文或资料，校验修订、内容哈希、权威选择及 Unicode 引文位置。正文真实性不代替模型语义判断。
- 登记写作及规划深审时，在同一事务创建最终交付核验义务；工作者完成和用量结清后仍能进入同一 Work 的最终决策。
- 保存回执说明冻结输入仍是修改前快照，并要求交还执行；同章待执行修订拒绝重复登记，幂等请求复用原行动。
- 原任务刷新、恢复、proposal 消费和执行回执原子提交。Host 重启读取持久回执，重复操作不能撤销作者之后的暂停或改动意图。
- 完成但有有效待执行交接的任务提供“接回原任务”；公开错误使用 Native 结构化代码和中文说明，诊断受控隐藏凭据。

| 角色 | 当前复核 | 证据与剩余条件 |
| --- | --- | --- |
| 架构专家 | APPROVE WITH CONDITIONS | 模型仍负责语义决定，Native 保存与校验执行权威，Host 调度，UI 展示。最终编译和新协议实机仍需验证。 |
| 技术专家 A | APPROVE WITH CONDITIONS | 原子交接的11项 Native 测试已通过，包含重启、CAS、重放和后续作者暂停；正文来源5项 Core 检查通过。最终错误码断言与最新接续计划测试待复验。 |
| 技术专家 B | APPROVE WITH CONDITIONS | 当前工具、类型、提示和 fixture 同步；章节评审 Core 2项、HTTP 1项、MCP schema 1项通过。最终定向 Core、Runtime 与完整 MCP 组正在验证。 |
| 产品经理 | APPROVE WITH CONDITIONS | 必须目标不再用“下次继续”代替交付；实际第7章仍有顺序冲突，需要真实模型修订并接回原任务。 |
| UI/UX | APPROVE WITH CONDITIONS | 前端构建、1项 Electron 接续入口测试和33项 Host/提示测试通过；不增加常驻表单。实际公开进度的中文表达和保存、核验、接续状态仍需实机复核。 |

零兼容门禁复查：PASS。没有迁移、回填、旧字段默认值或历史操作权威补造。历史模型计划作为无执行权限的参考文档读取，当前活跃协议缺字段严格拒绝。既有作品正文和作者要求通过当前领域接口继续工作。

实际应用：经作者明确授权后，修订要求通过 CUA 原对话发送给已配置 DeepSeek，两项真实写作行动保存了第7章，最终4541字符。跨日问题与金额改错残句消除，但售卖仍被写到灯吏之前；实际原任务仍暂停在第8章，版本86、原目标100章。不能把保存、模型自评、工程测试或 Work 执行结束算成小说目标完成。

剩余验收：最新编译与定向测试；使用新协议在应用修正第7章，核对原文证据；实际原任务交接、恢复和第8章保存。当前尚不宣称全部闭环。完整记录保存在 `repair-verification-2026-10-07.json`。

### 在线后本地服务恢复入口补充（实施前）

交叉审核发现首次 online 后 StartupGate 保留编辑器是正确行为，但 Gate 隐藏后没有可调用 Host retryStartup 的入口。补齐现有侧栏服务状态：断线时提供一个“重新连接”按钮，starting/slow 显示等待状态，正常 online 不增加操作。重试只请求 Host 恢复同一应用服务，不能提交创作、重开范围或重新加载覆盖未保存作品；使用同步单飞锁，恢复后仍由原 App 状态轮询更新。作品尚未加载时不查询空 projectId 的建议，继续保留 Host 输入校验。恢复过程中不能提交新的模型创作。

实施前五视角：架构 APPROVE（服务恢复与模型语义职责分开）；技术 A APPROVE WITH CONDITIONS（单飞、保留 App/编辑、无隐式业务写入）；技术 B APPROVE WITH CONDITIONS（同步现有 App 与 fixture，验证断线重试/编辑不丢失）；产品 APPROVE（补齐现有服务状态的唯一恢复入口）；UI/UX APPROVE（沿用侧栏状态，只有断线显示一项操作，无常驻新面板）。零兼容门禁 PASS：直接使用当前 retryStartup 契约，不增加历史适配。

### 真实模型遗漏目标与本轮来源初始化（实施前）

新协议实机发现：模型先登记修订，未建初始计划；最终只补接续目标，未记录全文核对目标。新决策未调用read_context就复用旧任务sourceId/cursor，拒绝后用四个词零命中替代语义核对。真实交付partial，因此原任务没有执行回执；Native交接门禁正确，不能放宽。

补强方案：任何模型登记的执行行动与规划评审必须先有当前executeGoals计划和可执行activeGoal，简单只读问答可无计划；执行模式中任意pending/inProgress实质目标禁止以partial关闭，真实障碍须先明确blocked及原因，planningOnly保留未来目标。提示要求首行动前逐项覆盖作者完整目标和依赖，后继检查必须记录为check。保持模型选择目标、方法、阅读范围，不按词路由。新决策目录必须调用本任务read_context初始化；目录尚未准备与跨任务source/cursor应给具体可恢复反馈，提示读取本轮目录，不误称无正文权限。最终检查的sourceChecks仍须真实当前修订引文；词语检索零命中不能满足语义或全文阅读要求。

实施前五视角：架构 APPROVE WITH CONDITIONS（执行协议要求计划，语义覆盖仍由模型承担，不冒充Native理解作者）；技术 A APPROVE WITH CONDITIONS（不放宽来源隔离/交接fulfilled/CAS，不给历史任务补计划）；技术 B APPROVE WITH CONDITIONS（当前所有行动fixture同步，覆盖无计划、planningOnly、未完成目标partial和跨任务目录错误）；产品 APPROVE（堵住未来承诺和遗漏目标的通用执行入口）；UI/UX APPROVE（协议内部落地，不加作者表单，公开错误具体中文）。零兼容门禁 PASS。

该补强不声称从引文或目录证明模型完整理解。完整阅读与语义判断须在实机读回中核验；不能以自动化测试代替第7章质量通过。

### 原始事件恢复观察超限的最小修复（实施前）

真实应用日志多次出现 `recovery event exceeds limit`。Host 将所有原始事件交给恢复观察器；Native binding 在识别事件类型前拒绝超过 4 MiB 的 JSON，Host 随后保存此错误并在完成前 `flush()` 重抛。业务 Native 写入账本仍独立持久，但无关历史或回合聚合正文也可能阻断 Host 完成。当前日志未记录事件类型，不能断言每条失败仅来自历史或压缩。

最小方案仅修改小说 Host，不改变通用 Engine 或模型决定：在传入恢复观察器前按其当前状态机投影事件。`turn/started`、`turn/completed` 只传同一回合身份；MCP 工具事件只去掉参数中的无关聚合内容，完整保留原工具 item 引用及其 arguments/result，不截断参数、不改变请求哈希或回执。其它历史、RPC reply、文本、压缩内容不参与恢复状态，因此不送观察器。失败诊断只记录固定事件类别与原始 UTF-8 字节数，不记录正文、工具参数、结果、身份或原始错误。真正超限的 MCP 仍失败关闭，保留 `flush()` 失败与未知写入门禁，只能由 Native 账本证明后对账，不重放写入或伪造成功。

下表为实施者的五角色视角复核，依据 Root 明确批准的最小边界与现行 RecoveryJournal/operation contract；不代表五位独立人员。

| 角色 | 实施前意见 |
| --- | --- |
| 架构专家 | APPROVE：事件投影属于领域 Host，Engine 保持通用恢复与持久状态职责；不把 UI 事件全文当作执行权威。 |
| 技术专家 A | APPROVE WITH CONDITIONS：MCP item、arguments/result 保留原引用与内容；真实观察失败仍污染 flush，未知写入只据 Runtime 权威账本关闭。 |
| 技术专家 B | APPROVE WITH CONDITIONS：测试超过 4 MiB 的无关历史/回合正文、大 MCP 未知写入及 Native snapshot 对账；同步当前 Host 回调与定向测试。 |
| 产品经理 | APPROVE：消除无关长历史阻断已保存成果的路径，不新增作者操作或重复创作。 |
| UI/UX | APPROVE：无新增界面；诊断仅有类型和大小，失败公开中文，不向作者展示私有原始事件。 |

零兼容门禁 PASS：只支持当前 RecoveryJournal 契约，没有旧事件适配、迁移、回填、双读双写。实施范围为新 Host observer、main 回调及 Node 测试；无实际应用操作、无 DeepSeek 调用、无业务数据库修改。

实施后五视角复核：架构 PASS（仅领域 Host 投影，Engine 契约未变）；技术 A PASS（MCP item/arguments/result 原引用，Native 拒绝与 flush 错误均保留，未知写入仅由带 fence 的 Runtime 账本证明关闭）；技术 B APPROVE WITH CONDITIONS（新5项及既有恢复报告3项全部通过，既有恢复组10项通过，唯一实际 Runtime/MCP 进程重启项超过原25秒限制取消，未报告通过）；产品 PASS（无需作者重复输入或新增恢复操作）；UI/UX PASS（只有固定事件类型和 UTF-8 大小诊断，无私有事件内容）。零兼容门禁复查 PASS。

验证证据：`/tmp/closed-loop-recovery-event-tests.log`（8 / 8）、`/tmp/closed-loop-recovery-reconciliation-after-projection.log`（10通过、1超时取消）；Node 语法及定向 diff 检查通过。交叉只读审核确认状态机字段与投影一致，并补齐“首个超大 MCP 事件尚无 Host 请求行”的测试；此时 run 仍保留待对账，Native accepted 保持未知，真实 committed 回执才关闭。实际小说任务与单项本地 Runtime/MCP 重启复验仍由主任务独立记录。

### 专门修订来源的计划入口增补（实施前）

全入口复核发现：带revisionRepairSource的讨论允许模型提出来源绑定的同章selectedScenes修复候选，但此前禁止保存模型计划。统一执行计划门禁后，该合法入口会无法先记录目标，形成协议死路。最小修改只允许该讨论保存同一模型计划；仍校验当前冻结来源，计划不提供执行权威。跨章、未选分组、未经作者审批的写作和连续生成交接保留原拒绝。

实施前五视角：架构 APPROVE（计划属于语义记录，不应与来源绑定的执行权限混为一谈）；技术 A APPROVE WITH CONDITIONS（保留冻结来源、同章分组限制及独立审批，handoff仍拒绝）；技术 B APPROVE WITH CONDITIONS（使用真实修订来源公开接口回归无计划拒绝、保存后合法候选及越权拒绝）；产品 APPROVE（作者原有局部修复流程能够采用统一计划协议）；UI/UX APPROVE（无新增面板或作者表单，计划由模型记录）。零兼容门禁 PASS：不添加历史适配、迁移或隐式计划填充。

### 执行计划门禁实施后复核

实际登记候选、规划深审、同Work后续行动和生成交接统一要求当前executeGoals计划、inProgress活动目标及已满足依赖；简单只读问答允许无计划。partial检查覆盖全部执行目标，未完成目标必须先记录真实blocked原因；planningOnly允许保存未来目标。未选择中间行动的最终决策始终显示交付审核义务，即使当前尚不能完成，也不能以一段终结回复绕过审核。专门修订来源可保存计划，原冻结来源、分组范围、作者审批和交接禁止保持。

定向验证：Core模型计划单元5、交付审核单元5、公开模型计划10、行动绑定14、恢复账本6，共40项通过；Runtime章节验收1、连续恢复14、原子交接11、规划深审5、Work决策6，共37项通过。最后在当前冻结Core上增量编译并通过真实修订来源公开HTTP回归1项，覆盖无计划拒绝、计划后合法候选、跨章/分组/旧修订及handoff拒绝、未经作者审批不能写作。最初HTTP监听受沙箱限制，复用已编译binary取得本机临时监听权限后全部通过，未修改测试来绕过业务断言。该统计只包含列出的定向组，不代表完整Cargo测试套件。

| 角色 | 实施后意见 |
| --- | --- |
| 架构专家 | APPROVE：语义计划由模型填写，Native只验证计划状态、真实证据及执行边界，原交接消费职责未改变。 |
| 技术专家 A | APPROVE：真实修订来源回归证实记录计划不扩大同章/场景/审批/handoff权限，未引入历史默认值或回填。 |
| 技术专家 B | APPROVE WITH CONDITIONS：列出的40项Core与38项Runtime定向测试通过；没有声称全工程fixture或全部Cargo套件均已验证。 |
| 产品经理 | APPROVE WITH CONDITIONS：堵住无计划登记执行和未完成目标partial收尾；计划是否完整覆盖作者语义、正文是否符合文学要求仍须实机核验。 |
| UI/UX | APPROVE WITH CONDITIONS：不新增作者表单；公开进展与小说结果是否清晰仍由主实施者在真实应用复核。 |

零兼容门禁复查PASS。本次定向验证只使用临时测试数据库，没有直接修改真实小说、原任务或密钥。

### 参数修正反馈的明确字段诊断增补（实施前）

真实模型在 `request_work_action` 的根参数校验收到 `additionalProperties` 时，只得到空根路径与约束名称，因而尝试删除合法的 `chapterAcceptance`，最终超过同任务修正次数。只读核对发现 Host executor 清单只登记工具名称，未保存参数快照；Native 当前 schema 明确包含 `chapterAcceptance` 和 `chapterReview`。反馈实际由小说 MCP 的 JSON Schema 校验器构造，Host 只消费最终错误。因此本次仅改该诊断构造器，不修改执行权限、校验 schema、Host 参数或模型决定。

方案：在已有约束路径与位置之外，列出该真实 schema 对象允许的字段名；对多余字段，仅回显限长的 ASCII 标识符，包含空白、标点、正文或超长内容的键不原文回显。参数值、原始异常和正文一律不进入反馈。新增字段诊断仍是校验失败，不自动删键、搬运字段、重试或放宽约束；模型按当前完整协议在同一任务修正。原有 required/type/enum 与三次修正门禁继续生效。

下列为实施者按实际代码与 Root 授权完成的五角色视角评审，不代表五位独立人员。

| 角色 | 实施前意见 |
| --- | --- |
| 架构专家 | APPROVE：诊断来自拥有当前 schema 的 MCP 层，Host 不复制协议或猜测参数。 |
| 技术专家 A | APPROVE WITH CONDITIONS：严格校验与次数门禁不变；未知键仅限安全标识符并限长，不回显任何值或任意原始异常。 |
| 技术专家 B | APPROVE WITH CONDITIONS：覆盖根字段、嵌套对象及组合 schema 的真实路径、required 缺字段、中文或超长未知键不泄漏、合法 `chapterAcceptance` 保持可用。 |
| 产品经理 | APPROVE：同任务反馈指出可修正位置，减少模型盲猜、删除合法要求与无效重复。 |
| UI/UX | APPROVE：不新增作者表单或技术操作；内部诊断用于模型修正，最终中文失败状态继续使用现有呈现。 |

零兼容门禁 PASS：使用当前 Native schema 和当前诊断协议，无旧格式 fallback、迁移、回填或双轨校验。实施范围仅 MCP 诊断与语义单元测试，不操作实际应用、不调用真实模型、不修改小说数据。

实施后五视角复核：架构 PASS（字段与引用路径来自拥有当前契约的 MCP 校验器，Host 没有第二份参数 schema）；技术 A PASS（未知键限安全标识符、长度及总量，值不回显，1600字节反馈上限与三次失败门禁保持）；技术 B PASS（完整 MCP 66 / 66 通过，包含4个新增用例；根字段、嵌套 `$ref`、缺字段、组合分支和大字段表均按真实校验位置断言）；产品 PASS（模型能在同任务指出并修正错误字段，无新增作者重复输入）；UI/UX PASS（无新增面板或参数表单，诊断供内部修正使用）。零兼容门禁复查 PASS。

验证证据：`/tmp/closed-loop-all-mcp-v6.log` 最终66通过、0失败，测试执行0.61秒。`jsonschema` 的 `validate()` 只报告首个未知属性，新增测试按安全标识符、中文正文键、超长键逐项独立验证，未改为聚合错误或猜测未选中的字段；引用错误使用调用位置的 `/$ref` 路径，并由只遍历静态 schema 的有界解析器定位真实字段表。Rustfmt与diff检查通过。此证据验证参数修正协议，不代表真实 DeepSeek 已完成小说事实判断；实际应用核读另由主任务记录。

### 已结算零写入的输出上限失败恢复（实施前）

实际writer以结构化provider_output_limit失败，Native无输出、无最终回复、无写入账本，provider已结算且unknownTokens为0、overrun为false。当前failure_code未识别该码，导致同Work纠错被executionBlocked；另control(resume)直接重排blocked工作者，未经过模型恢复决策。这是恢复协议缺口，不能解释为整个作者预算耗尽，也不能放宽未知写入门禁。

最小方案：登记该精确结构化码，沿用known_zero_failure的零输出、零写入、实际已结算与计费证明。输出上限失败第一次恢复就要求模型提交绑定failedTaskId的recoveryPlan，解释原因、改变方法和核验方式；工程不默认重跑，不替模型选择新方法。作者resume保留可由模型纠正的非advisory失败步骤及当前Attempt，接入已有同Work模型决策；advisory原恢复行为不变。已识别失败但结算或零写入未证明时拒绝resume；已有完整建模发布仍使用原权威恢复逻辑。决策与行动继续校验原总预算、来源、尝试次数、幂等重放和重复失败方法，不能通过恢复新增预算或重建Work。

实施前五视角（实施者依据实际代码与主代理只读证据复核，不代表五位独立人员）：架构 APPROVE（模型负责纠错方法，Native保存失败与执行边界）；技术A APPROVE WITH CONDITIONS（已知结构化码不等于重试权，零写/已结算/无未知计费及原总预算必须保持）；技术B APPROVE WITH CONDITIONS（公开HTTP回归覆盖provider_output_limit首次必须计划、resume不重排原writer、未知写/部分输出/未结算拒绝、幂等重放与总预算停止）；产品 APPROVE（作者一次继续交还模型调整，避免重复同条件失败）；UI/UX APPROVE（沿用原对话与状态，不新增技术恢复表单）。零兼容门禁PASS：只调整当前失败协议，无旧码fallback、迁移或历史回填；不得操作真实业务数据库或调用模型验证。


### 可信单次输出容量提示（实施前）

真实输出上限失败并非作者总预算耗尽。当前 Host 绑定执行已持有经验证的 sessionPlan 与当前 adapter.modelRoute，但模型提示未显式说明该路由的 outputBudget。只在绑定领域任务的 sendAgentTurn 前按 logicalModel 精确唯一匹配当前真实 route，读取正安全整数容量；缺失、非法、重复或不匹配均不猜测补值，不从模型名、供应商、defaultModel 或历史配置推导。该信息只是单次能力提示，不创建 Node 领域权威，不修改 route、作者总预算、范围或写入授权。

提示提供方法信息，由模型选择简洁公开说明或现有保存工具的完整参数交付，说明实际请求可能更小；保持冻结目标、CAS、保护正文与既有保存 schema，容量不足仍须如实说明并交同Work恢复决策，不能截断正文或必填参数冒充成果。该提示前置于已有完整文本，不改变作者原话或重复创作。复用当前产品完整消息的 38000 UTF-8 字节与 9000 文本 token 检查，加入提示后再次按整条消息校验，不能靠分块绕过上限。只序列化数值与固定说明，不包含连接、endpoint、凭据或原始 route；UI 本轮保持现状，失败回合普通 content 的展示问题仅记录，另行实机验证。

下列为实施者按现行代码与 Root 授权的五角色视角评审，不代表五位独立人员。

| 角色 | 实施前意见 |
| --- | --- |
| 架构专家 | APPROVE：Host 只转述已验证当前路由的能力，Native/Engine 继续拥有预算与执行权威；方法由模型决定。 |
| 技术专家 A | APPROVE WITH CONDITIONS：精确唯一绑定、坏值不猜测、能力信息不含敏感字段且不修改任何授权；完整消息加入提示后仍校验。 |
| 技术专家 B | APPROVE WITH CONDITIONS：覆盖多路由精确选择、非法值/缺失/重复、不泄漏连接信息、预算与输入不变，以及提示新增导致的字节/token 超限和真实合成绑定提示预算。 |
| 产品经理 | APPROVE：模型能在已授权单次能力内调整交付方法，避免作者重复授权或误认为总额度耗尽。 |
| UI/UX | APPROVE：无新作者表单或技术面板；公开说明仍简洁，内部长分析展示问题只记录，不扩大本轮 UI 范围。 |

零兼容门禁 PASS：当前 route 当前字段直接使用；无旧字段 fallback、版本判断、迁移、回填、默认容量或双轨契约。实施范围为新 Host helper、小 main 接线及现有消息预算检查复用和 Node 测试；不修改 Rust/Engine/UI，不操作真实应用，不调用 DeepSeek，不读取密钥或小说正文。


可信单次输出容量提示实施后复核：新 Host helper 只读取已验证 sessionPlan 的当前唯一 logicalModel/outputBudget，并由 sendAgentTurn 在绑定 task 时加入一次独立 text item；无任务 Core 回合与非法/缺失/重复路由不加入提示。原作者输入、冻结 task、route 及 TaskBudget 限额未修改，凭据/endpoint/模型或连接详情不进入说明；原 product 消息预算检查仅抽函数复用，上限仍为 38000 UTF-8 字节与 9000 文本 token，新增说明参与完整消息复验。当前提示为 546 UTF-8 字节、190 保守文本 token，完整合成 writer 与同Work决策提示仍低于原上限。

| 角色 | 实施后意见 |
| --- | --- |
| 架构专家 | APPROVE：单次容量只转述当前可信路由，Engine/Native 权威与模型方法选择未改变。 |
| 技术专家 A | APPROVE：精确唯一匹配、非法值不补、敏感字段不序列化；原冻结输入及累计预算限额不变，分块不能绕过新增后整条检查。 |
| 技术专家 B | APPROVE：新6项通过，含 production sendAgentTurn 与 synthetic real Adapter 传输；既有完整提示预算回归1项通过，语法与 diff 检查通过。 |
| 产品经理 | APPROVE WITH CONDITIONS：方法能力提示已到模型，不能仅以提示测试声称真实 DeepSeek 已正确选择方法或保存小说，实机闭环由主任务核验。 |
| UI/UX | APPROVE WITH CONDITIONS：无新增界面、预算或授权操作；failed普通content直显的UI问题本轮仅记录，未用英文/关键词重写模型内容。 |

零兼容门禁复查 PASS。验证日志：`/tmp/closed-loop-task-output-capacity-tests.log`（6 / 6）、`/tmp/closed-loop-task-output-prompt-budget.log`（1 / 1）。交叉只读设计快审确认独立 Host item 不写回 authorIntent、不产生新增授权。没有实际应用操作、真实模型调用、小说正文读取、业务数据库写入或 Engine/Rust/UI 变更。

### 失败回合未交付内容的统一展示边界（实施前）

呈现修订：以下为此前轮次记录。当前公开进展与短回复采用 [失败与停止后保持可读评审](2026-10-07-readable-failed-progress.md)：commentary 不参与未交付答复折叠；短未完成 answer 带真实状态直显，较长内容及已保存 final 的残留 stream 按需展开。任务交付证据与恢复权限边界保持。

真实失败 Writer 没有产物、持久最终回复或写入回执，但其普通 `content` 被 `phase:null` 的答复回退及失败后保留直接显示为正式答复。该内容不是协议 reasoning 事件，不能按语言、长度或内部字段词推断并过滤。

方案仅改变 Renderer：失败终态中的普通过程文本与未交付答复默认折叠，可按需查看原内容；工具记录、保存成果、错误、恢复及继续入口保留。成功回合与正常最终答复继续直接显示。任务绑定的失败回合复用现有 SavedTaskReply 读取与项目、task、thread、turn核验，真实保存的回复优先显示；读取中、读取错误与归属不匹配视为未知，成功读回 null 或 absent 才能声明无持久最终回复。状态与异步读取按精确身份隔离，迟到结果不能污染新回合。历史与当前回合、Timeline及Reader共享同一 metadata 展示边界，不改文本、Engine、执行权限、Core、小说或预算。

下列为实施者按现行代码与 Root 授权的五角色视角评审，不代表五位独立人员。

| 角色 | 实施前意见 |
| --- | --- |
| 架构专家 | APPROVE：交付权威仍是 Native 回执，Renderer 只拥有展示边界，不把内容词句转化为状态。 |
| 技术专家 A | APPROVE WITH CONDITIONS：复用精确身份的读取、迟到读回取消、未知与不存在区别；不丢原内容、不修改执行或写入。 |
| 技术专家 B | APPROVE WITH CONDITIONS：覆盖失败普通中英文、成功与最终答复、真实保存凭证、读取失败和身份切换；同步当前 UI fixture、类型与构建。 |
| 产品经理 | APPROVE：未完成的长过程内容不会占据正式答复位置，保存成果和继续入口仍易发现。 |
| UI/UX | APPROVE WITH CONDITIONS：默认折叠仅失败内容，摘要用清晰中文；错误与恢复按钮留在外面，可访问 summary 与现有阅读位置规则保持。 |

零兼容门禁 PASS：只支持当前 Renderer 与 Native final receipt 契约，无旧字段 fallback、迁移、回填或内容猜测。实施范围为共享展示组件、既有保存回复 hook、精确任务边界、App/Timeline/Reader小补丁与定向测试，不操作实际应用、不调用模型、不写小说。

取消终态补充实施前复核：当前 AgentRun 将协议 interrupted 映射为 cancelled，Reader 保留协议 turn.status；这两种停止状态与 failed/error 一样可能留下尚未交付的普通 stream，因此同样默认折叠。只依据终态，不按语言、字段词或文本猜测；真实精确保存的 final 仍单独直显，成果和已有恢复入口不折叠，也不自动恢复作者主动取消的任务。架构/技术A/产品/UI/UX APPROVE；技术B APPROVE WITH CONDITIONS：新增取消普通内容与真实保存回复同时存在的用例，并验证所有旧身份展开状态不传播到新回合。此补充不修改取消或恢复执行协议。

失败/取消展示边界实施后复核：共享 `ConversationOutput` 仅按当前 error/failed/cancelled/interrupted 终态默认折叠原内容，completed 与正在生成的内容保留原展示。`SavedTaskReply` 的现行读取被抽为同一 hook；任务面板只在同项目、唯一 task 匹配、明确 thread/turn 时核读该 task，回执必须匹配 project/task/thread/turn。读取中、失败或归属错误均为 unknown，成功读回 null/absent 才为 absent；绑定身份改变会关闭新 details，并取消旧异步结果的展示。精确已保存回复直显，旧 stream 仍单独折叠，Reader 没有任务证明时保守未知，phase 不当作持久保存回执。已保存章成果、错误与现行外置 Composer 重试入口保持可见，未新增恢复执行入口。

| 角色 | 实施后意见 |
| --- | --- |
| 架构专家 | APPROVE：Native final receipt 仍是交付证明，Renderer 只处理终态与读取身份，不引入模型正文分类或新的执行权威。 |
| 技术专家 A | APPROVE：当前 hook 复用精确身份与取消迟到读回，未知与 absence 分开；旧展开状态不泄漏到新回合，原全文可按需查看。 |
| 技术专家 B | APPROVE：10项定向测试已通过，新增隐藏真实组件 fixture 覆盖11场景，额外逐项验证 project/task/thread/turn 不匹配及已保存章入口；tsc、Vite构建和diff检查通过。 |
| 产品经理 | APPROVE WITH CONDITIONS：未完成过程不会再占据正式答复区域，真实保存回复与成果仍可发现；仅为合成 UI 验证，真实 DeepSeek 创作闭环由主任务继续核验。 |
| UI/UX | APPROVE：共享清晰中文 disclosure，成功正常直显；失败、取消、历史与Reader一致，外置重试及保存成果不进折叠，既有阅读位置回归通过。 |

零兼容门禁复查 PASS。证据：`/tmp/closed-loop-unfinished-output-regressions.log`（初次9/10，唯一失败是当前 flow 文案快照；八项其他回归及新用例均通过）、`/tmp/closed-loop-unfinished-output-last.log`（同步当前唯一文案后2/2通过，包含增强的新用例）、`/tmp/closed-loop-unfinished-output-build.log`（tsc随后Vite成功）。最初普通沙盒启动隐藏 Electron 以 SIGABRT 退出，获批本机合成UI复验后正常运行；不作为应用业务失败。没有实际应用操作、模型调用、小说读取/写入或Engine/Core改动；package.json由主任务维护。生产冻结时 SHA256 前缀：App `78d38995`、WorkTaskPanel `8544ddcb`、ConversationOutput `3c59eaca`、SavedTaskReply `75ce47dd`。


### 重绑定实机集成快照的当前来源选择（实施前）

当前冻结 Native v8 下受控 Runtime/隐藏 Electron 重绑定旅程已完成全部业务断言，但来源快照失败。只读定位：WriterContextDetails 的顶层摘要已是“正在创作：第1章 · 尚未发布的内容”，旧 fixture 按“当前目标稿”前缀取到了折叠“查看预览来源”中的真实 draft 修订 ID。该 ID 每次由真实 Runtime 生成，不能作为固定快照值；这不是来源丢失，不能将细节 ID 伪装回旧摘要文案。

最小修改仅 fixture 与其快照：分别捕获当前易读摘要与详情来源，断言真实修订格式合法、详情文本精确引用本旅程实际 author-edit 回执 revisionId、当前 workspace 的候选头仍是同一修订，然后只将该已证明的确切 ID 归一为明确占位符。保持全部预算、CAS、保护原文、重绑定、建模、评估、下一章及重启持久性业务断言；既有第二错误快照与后续章纲摘要保持。生产 UI/Host/Native 不变。

实施前五视角（实施者依据实际代码与 Root 授权复核，不代表五位独立人员）：架构 APPROVE（测试跟随当前展示契约，实际来源以 Native 回执为依据）；技术A APPROVE WITH CONDITIONS（只归一化已精确证实的动态 ID，不用全局正则掩盖来源缺失或误指）；技术B APPROVE WITH CONDITIONS（新增摘要与真实ID/当前候选头断言，保留整条受控真实 Runtime 旅程，由 Root 统一复验）；产品 APPROVE（快照反映当前易读摘要与详情分层，不把合法诊断当固定小说信息）；UI/UX APPROVE（现行易读摘要仍存在，未发现由此次差异证明的范围预览意义倒退，无生产 UI 修改）。零兼容门禁 PASS：同步当前 fixture，无旧展示适配、迁移、回填或双轨。


重绑定快照实施后静态复核：fixture 新增 currentDraft 真实 DOM 摘要捕获；来源文本必须精确匹配已保存 author-edit 回执 revisionId，该 ID 格式与当前候选头均独立断言，断言通过后才归一化确切 ID。没有一般性的 draft-ID 正则替换，没有删除或放宽旅程业务断言。快照同时保留当前顶层语义与折叠详情标签，第二错误快照及后续章纲摘要未变。架构/技术A/产品/UI/UX静态复核 PASS；技术B APPROVE WITH CONDITIONS：Node语法、定向diff检查通过，当前Native v8整条隐藏Electron/Runtime复验待Root统一运行，不报告本轮业务旅程已通过。零兼容门禁复查 PASS；无生产代码、实际应用或模型调用。

### 规划评审席位的专用恢复边界（实施前）

输出上限恢复增补的交叉审计发现：`reviewPlanningSeat` 由现行 Host 的 `runPlanningReview` 专门消费，恢复读取只接受 queued/completed 席位；它没有通用行动的同Work决策消费接口。把已知零写失败席位也保留 blocked 会堵住这条真实当前路径。这里按实际执行边界收窄通用模型恢复的适用行动，不新建另一套席位纠错协议。

最小方案：只从“保留失败步骤以交通用模型决策”的集合排除 `reviewPlanningSeat`，使其按专门席位流程重新 queued；advisory 仍保留既有恢复行为。恢复前共同的实际结算、无输出、无未知写入、当前来源、作者预算与权限门禁不变；未知写入或未结算仍原子拒绝，不能因专用路线排除而跳过核账。公开HTTP回归实际登记深审、创建席位、失败和结算，再验证重新queued后可由专门seat接口创建新Attempt，保持原run、role、冻结来源及作者要求，旧失败Attempt仍可追溯；另覆盖未知写入拒绝且没有新增席位。

实施前五视角（实施者依据当前代码与主代理授权复核，不代表五位独立人员）：架构 APPROVE（恢复状态必须由实际消费者支持，通用Work模型恢复只保留可消费行动）；技术A APPROVE WITH CONDITIONS（只排除步骤消费路径，不排除共同零写、结算、未知写与预算门禁）；技术B APPROVE WITH CONDITIONS（实际HTTP新seat创建与原子拒绝回归，保留旧Attempt和冻结来源，不只断言一个枚举值）；产品 APPROVE（专门深审不会因通用恢复改动卡住，未承诺模型已选择席位新方法）；UI/UX APPROVE（不新增表单或错误分支，沿用深审继续入口）。零兼容门禁PASS：当前执行原语的职责边界修复，无历史fallback、迁移、回填、双轨或新席位协议。实施范围只有Core恢复条件、公开HTTP回归与本评审记录；不操作真实应用、业务数据库或模型。

### 多个原任务的明确恢复选择（实施前）

重启后同一对话存在多个尚未结束的创作任务。当前 conversationRetryTask 只允许一个候选，多个时返回 null；原任务控制已经收敛到 Composer，因此图标也消失。作者无法选择原Work继续，容易重复输入并创建新任务。这是展示入口缺口，不应让关键词、任务时间排序或最近一次失败决定授权目标。

最小方案仅Renderer：保留既有单任务/current-run隐式选择语义，另列当前project/thread的明确恢复候选；只有Native当前摘要提供有效resumeMode、状态允许恢复且无resumeBlockReason的任务可以选择。一个候选维持直接重试；多个候选由同一图标打开轻量菜单，以原作者目标、现行步骤动作和已保存证据状态区分，不显示内部ID，不默认选择、不按“继续”词路由。选择后核读精确Work的project/thread/id/version/mode及阻断状态；同步ref阻止重复请求，project/thread/scope改变或候选版本更新使旧选择失效。复用既有taskControls.control、Native来源/未知写/预算/权限门禁及错误展示，不新建Work、不扩大授权、不改Host/Core/Engine。保存状态只来自已有deliveryReport，回复文字不作交付证明。

下列为实施者按当前代码与Root授权完成的五角色视角评审，不代表五位独立人员。

| 角色 | 实施前意见 |
| --- | --- |
| 架构专家 | APPROVE：作者明确选择已存在Work，Renderer不替模型决策创作方法或成为执行权威。 |
| 技术专家 A | APPROVE WITH CONDITIONS：候选精确隔离项目/对话，选择核读当前版本，迟到读回与同步重复点击受控；未知写和来源变化不自动绕过。 |
| 技术专家 B | APPROVE WITH CONDITIONS：真实Renderer回归覆盖多目标、重启恢复列表、身份切换、版本变化、Native拒绝与零新root/模型调用，同步类型和构建。 |
| 产品经理 | APPROVE：用户可以选择原长篇任务或修订任务，无需重述目标；目标、当前动作、保存情况可区分。 |
| UI/UX | APPROVE WITH CONDITIONS：保留单个重试图标，仅多候选展开轻量列表；默认无选择，键盘/焦点/关闭行为明确，无任务ID或密集常驻按钮。 |

零兼容门禁 PASS：只使用当前Work摘要/读取/恢复接口，无旧结构fallback、迁移、回填或双轨。本轮不操作实际应用，不调用模型，不读取或写入小说。

多个原任务选择实施后复核：保留 conversationRetryTask 原单任务/current-run绑定行为，新增显式 owned candidates 只接受 Native 当前两种 resumeMode 白名单、可恢复状态及无阻断理由，并拒绝重复身份。Composer 使用原重试图标，多候选才展开目标/动作/保存情况菜单；无默认选择、时间排序或输入关键词路由，内部ID仅供DOM精确归属，不进入作者文字。核读项目、task、thread、所选版本、恢复方式及新阻断状态后，复用原 taskControls.control，不新建Work、不复制正文、不改变预算权限。该版本核读是UI新鲜度检查，既有Native执行门禁保持最终权威；没有新增声称原子携带所选版本的IPC契约。

独立代理只读交叉审计指出，恢复保存成果不经过 onBeforeResume，因此不能仅用 render 快照核对thread。现 fresh-read后同时验证 selectedConversation.current.current、taskControlScopeRef、currentProjectRef，以及同步dirty/promptSubmission/agentRun活跃refs；同一个JS任务中先排旧读取continuation、再通过实际对话按钮同步切换，旧选择不得恢复保存成果。首次读取阶段自有同步ref单飞，原控制hook接续单飞。所有新增hooks放在既有taskControl区，无条件先于加载/登录早返回，真实App由初始loading恢复到signed-in的测试覆盖运行时，而非仅孤立Composer。

| 角色 | 实施后意见 |
| --- | --- |
| 架构专家 | APPROVE：作者明确选择原Work，模型仍负责后续方法，Native仍负责来源、未知写、版本与授权执行门禁；未扩大执行协议。 |
| 技术专家 A | APPROVE：项目/对话精确候选、所选版本/模式核读、live refs与两阶段单飞保持；迟到旧thread/project及旧菜单无新执行。 |
| 技术专家 B | APPROVE：最终6/6通过，新真实App旅程11场景包含同实例live-thread竞态、版本/模式变化、Native阻断与拒绝、重启恢复列表、recoverSavedResult、原Work精确选择和单任务直达；旧控制完整旅程及未交付内容回归保持。 |
| 产品经理 | APPROVE WITH CONDITIONS：多项未完成任务不再使原恢复入口消失，无需重复输入目标；合成Renderer只证明选择/拒绝/恢复调用边界，真实DeepSeek交付由主任务继续核验。 |
| UI/UX | APPROVE：同一图标的按需轻量菜单，没有常驻密集控制按钮或内部ID文本；键盘导航、Escape、焦点与身份切换关闭行为可用，错误留在现行对话区域。 |

零兼容门禁复查 PASS。日志 `/tmp/closed-loop-multiple-retry-final.log` 最终6通过0失败，执行5.77秒；`/tmp/closed-loop-multiple-retry-build-final.log` 为tsc随后Vite成功。旧单任务fixture只把一个布局敏感innerText错误断言改为role=alert精确文本+checkVisibility，原禁止恢复/新root计数与全部快照断言保留；其定向复验1/1通过。生产冻结哈希前缀：App `15c0002b`、conversation-retry `167ebc06`、ComposerRetryControl `7c4e40b6`。新增入口 `electron/tests/multiple-retry-ui.test.mjs`；实施者未修改package.json，主任务已将该入口与unfinished-output入口统一加入test:ui，并已只读核对。无真实应用、模型、小说数据或Host/Core/Engine改动。

席位回归基线修正的实施前五视角增补：v9的5项既有回归与新增用例安全重试分支通过；未知写分支失败仅因fixture在注入accepted未知操作之前读取整个Work，却拿它与注入后的读时恢复门禁比较。当前`load_work_task`从权威账本派生resumeBlockReason/resumeMode；人工注入本身应将null/continueExecution变成pendingRecovery/null，控制拒绝并未修改version、步骤、Attempt或交付证据。方案仅把“拒绝操作无改变”的完整Work基线读移到故障注入之后，保留全对象与run等值断言，额外证明注入前后version/steps不变、明确新门禁、拒绝之后旧Attempt/授权计数和accepted未知状态不变。架构 APPROVE（区分故障注入与被测原子事务）；技术A APPROVE（不改生产门禁、仍验证未知阻断与状态原子性）；技术B APPROVE WITH CONDITIONS（完整对象断言保留且加强计数和派生门禁，重跑当前6项）；产品 APPROVE（未掩盖错误恢复，专用席位授权不增加）；UI/UX APPROVE（当前状态来自真实未知账本，界面与入口不变）。零兼容门禁PASS：当前fixture的真实门禁基线修正，无兼容路径。

### 输出上限与专用席位恢复（实施后复核）

实际变更：Core精确识别当前结构化`provider_output_limit`；该失败在第一次通用行动恢复就要求模型提交绑定实际failedTaskId的cause/change/verification。作者resume对可由现行通用决策消费的失败行动保留原blocked步骤与失败Attempt，再由同Work模型选择方法；advisory与专用reviewPlanningSeat按其真实消费原语重新queued。所有路线继续使用原结算、零输出、未知写入、当前来源与总预算门禁；完整权威建模发布仍可原位恢复成果，不以输出上限错误重新发布或增加Attempt。没有自动重建Work、补预算、解析作者关键词或替模型决定方法。

| 角色 | 实施后意见 |
| --- | --- |
| 架构专家 | APPROVE：通用失败保存完整Attempt供同Work决策，专用席位保留真实Host消费者；Engine结算与Native执行权威未复制到Host。 |
| 技术专家 A | APPROVE：未结算、accepted未知写、部分成果、未知计费与总预算耗尽分别受现行硬门禁控制；原完整建模发布、来源核对和失败追溯保留。 |
| 技术专家 B | APPROVE：Core建模恢复8/8、Work决策HTTP8/8、规划评审HTTP6/6通过；专用席位实际重新claim第二Attempt并保留原run/role/来源/作者绑定，未知注入后的全Work/run原子拒绝断言与计数通过。当前全MCP66/66与Native全部bins构建通过。 |
| 产品经理 | APPROVE WITH CONDITIONS：作者继续可进入纠错方法选择，已知单次输出上限不会误报总预算耗尽；自动测试不证明真实模型已完整修订小说，主任务继续实机核验。 |
| UI/UX | APPROVE WITH CONDITIONS：Core不增加作者技术表单，专门深审入口按实际路线恢复；多项未完成工作下的明确重试目标选择由主任务另行验证，不以Core测试宣称UI闭环。 |

零兼容门禁复查PASS；无版本判断、旧字段fallback、迁移、回填或双轨。日志：`/tmp/closed-loop-output-limit-core-v8.log`（8通过，0失败）、`/tmp/closed-loop-output-limit-runtime-v8.log`（8通过，0失败）、`/tmp/closed-loop-planning-seat-recovery-v10.log`（6通过，0失败；v9唯一fixture基线问题按前评审修正）、`/tmp/closed-loop-all-mcp-v10.log`（66通过，0失败）、`/tmp/closed-loop-production-native-v10.log`（全部bins成功）。Rustfmt与定向diff检查通过。以上均为当前schema的临时数据库与真实Native/Engine/API协议验证，由主实施者统一执行；不是DeepSeek实际创作测试，也没有本代理对真实小说、任务或密钥的读写。生产代码保持冻结。


重绑定快照最终复验证据：Root在当前冻结Native v9执行整条受控真实本地Runtime/隐藏Electron旅程，1 / 1通过、0失败（`/tmp/closed-loop-rebind-v9.log`，总用时8.58秒）。新增当前易读摘要、真实author-edit修订与候选头断言全部通过，随后重绑定、原文/CAS保护、预算沿用、建模/评估、下一章及Runtime进程重启后的持久状态读回均通过。技术B条件已完成，五视角最终复核PASS，fixture冻结。本旅程仅使用脚本模型与临时测试数据库，没有DeepSeek调用、实际作者应用操作或生产代码改动。


## 实际应用恢复与当前稿验收复核（2026-10-07）

本节为当前事实复核，不把五个视角称为五名独立人员。主代理通过CUA恢复同一修订Work；两个只读代理独立复核当前正文、保护哈希、账本与协议，第三代理核对压缩及源读取。没有直接写小说或更改账本。

- 架构：有条件通过。真实恢复保持原Work，失败Attempt保留，模型提交绑定原失败的改变方法计划，新writer真实保存；本Work交付评审为空、原任务交接执行0，长篇链路仍未通过。新发现内部任务提示作为用户消息重复保留、解析源缺持久阅读进度，需要后续工程修复。
- 技术A：有条件通过。第1、6章hash与保护值严格相符，原run86/意图4/尝试24/重试2未变；402后的未知token781044保持原账数。累计58请求不能解释为58次402重试，未知计费不等于未知写入，也不直接证明所有恢复被拒绝。
- 技术B：有条件通过。Core8、Work HTTP8、规划席位6、MCP66、chooser6及Native/前端构建有实际定向证据；脚本provider旅程不能充当真实DeepSeek质量验收或全仓测试。最新当前稿为5594…/5703字符，旧1470/4826状态只作历史；顶层与本轮currentAcceptance已统一。
- 产品：BLOCK。最新第7章存在63+7=72、缴款63/70矛盾、丢失关键情节与重复第6问答，不能只凭chapterComplete或writer摘要推进第8章。
- UI/UX：有条件通过。显式选择原任务在实机有效，已保存结果可见，失败状态没有伪装完成；402错误目前只在详情显示英文，正在增加通用账户计费中文说明。来源摘要范围第1—7章不等于实际全文阅读范围，第6后半仍未读取，显示语义仍待校正。

综合结论：端到端验收未通过。服务商余额不足当前阻断新的真实模型工作；外部账户由作者处理。余额恢复不能代替正文修订、全文核验和新工程缺口处理，不清零原用量、不扩大授权、不伪造来源或交接。

UI/UX后续实机复验：HTTP402主说明及编辑器通知已显示清晰中文，当前稿5703字符保存状态可见，原始详情保留。纯展示修复31项Node、1项UI及类型/构建通过；不改变恢复/预算协议。来源目录范围与实际阅读全文的语义差异，以及压缩后的持久阅读进度，仍是未完成工程项。
