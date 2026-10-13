# v3.0 稳定问题 MCP 契约与合成闭环评审

日期：2026-10-07。范围：Native 9.3 稳定问题契约的 MCP 严格 schema、工具说明、同 Work 只读分页与两条实际 Codex 合成 fixture。Native/Core/Runtime 生产由独立代理负责；后续经该代理明确转交，同步 Runtime `tests/revision_outcome/` 与 `tests/scoped_revision_tests.rs` 的当前严格 fixture，不改 Runtime 生产。本范围不修改真实作品、UI 或调用实际模型。

## 目标与当前契约

作者解释标准采用 `{id,text}`；章节收尾引用当前独立报告和对应标准/问题身份，删除重复自评结论。独立报告的负评及证据不足必须登记模型明确选择的稳定问题，复评通过必须引用真实旧 observation。最终交付引用真实问题与 observation，不接受旧字符串格式。`issueResolutions` 明确必填，只允许已有同 Work、非正文问题通过真实旧观点引用解决；正文判断必须由当前独立报告完成，不允许最终自评代替。

问题身份、报告、来源及版本由 Native 事务验证。问题读取不向 prefix/selectedScenes writer 开放，连摘要也可能披露未授权的后段；root、独立 critic 和整章 writer 仍按真实绑定授权读取。工具发现和可信直接子代理不能扩大这一范围。MCP 只公开实际契约、保持可信 caller 与任务绑定；目录、工具发现、模型文字和旧支持报告不能替代当前独立报告。不存在以文本相似度、关键词、序号或报告多数票决定问题归属的规则。

## 五视角实施前评审

| 视角 | 结论 | 必须证明 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 同一个 Native 问题事实；MCP 不创建第二个问题账本；章节收尾引用独立报告，不保留重复判断双轨 |
| 技术 A | APPROVE WITH CONDITIONS | 同 Work/任务/可信调用身份、当前来源与稿件版本；旧报告、跨任务问题及未解决问题不能认证交付；只读分页有界 |
| 技术 B | APPROVE WITH CONDITIONS | 严格 required/null/enum 同步；schema 拒绝旧字符串和删减字段；真实 Core/MCP/Runtime fixture 使用读取到的 observation 引用 |
| 产品 | APPROVE | 相同问题跨修订可追踪，失败报告正常保存；模型选择关联与修法；不把未发现遗漏等同确定性完整 |
| UI/UX | APPROVE | 复用可读摘要及真实引用，无新增内部 ID 输入、技术菜单或作者重复要求；UI 投影由其他代理同步 |

以上是五个职责视角，不代表五名独立审查人员。实施依照主任务已授权的 Native 契约；具体字段及权限必须与当前 Native 同步，不猜测缺失 API。

## 零兼容门禁

PASS（实施前）：不新增旧字符串 fallback、双读双写、迁移、回填、版本探测或失败后兼容路径。当前 schema 的临时测试数据一次性重写。保留原文保护、CAS、独立判断身份、累计预算和未知写入门禁。

## 验收计划

- MCP 验证对象标准、问题链接、明确可空字段、当前报告收尾引用及严格拒绝旧形状。
- 同 Work 问题分页保留可信任务身份，不接受模型任意 Work ID；发现只是实际权限内加载。
- 合成自主修订保留单次作者输入：独立负评登记问题，模型读取真实 observation 后定向修订，独立复评显式解决同一问题，当前报告与真实回执支持交付。
- scoped fixture 使用当前独立报告引用，旧支持报告不得认证后续稿件；不同作者新增目标不能强行合并成同一问题。
- Runtime 原有修订 fixture 按真实负评保留 partial 与问题引用；不伪造 fulfilled，原标准和来源规则保持。
- 仅临时数据库和脚本 provider；这些证明机制，不证明 DeepSeek 文学理解或稳定成功率。

## 同一报告显式共享问题的实施前补充评审

Native 负责人确认当前批内契约：同一次真实报告的多个标准可明确选择相同问题 ID。首次定义必须有相同摘要、相同报告章节，所有首次链接的 `resolutionOf` 必须为空；同报告也可显式引用该 ID 并使用 `newIssue:null`，不依赖数组顺序。登记一次问题、逐项保留独立观点。后续报告使用已有 ID 和 `newIssue:null`，仍需真实 observation 引用；不同定义、章节、Work 或伪造引用由 Native 拒绝，不按文本自动合并。

| 视角 | 结论 | 本次最小范围 |
| --- | --- | --- |
| 架构 | APPROVE | Native 原子批内校验和登记；MCP 仅同步说明，不增加第二份问题事实 |
| 技术 A | APPROVE WITH CONDITIONS | 明确同报告/同章节/同定义、首次空 resolutionOf 与跨报告 existing 规则；不得放松身份和来源 |
| 技术 B | APPROVE | 现有严格字段保持；补充多标准显式共享问题的 schema 正向数据，原子冲突负例由 Native 专项验证 |
| 产品 | APPROVE | 一个问题可能影响多个标准，逐项判断和证据仍独立，不被迫生成多个问题 ID |
| UI/UX | APPROVE | 无新增作者操作和界面；问题归属仍由模型明确选择，不按文字合并 |

此次仅修改工具及 schema 描述和测试，不改变 schema 字段、Node 闭环输入或生产执行权限。

## 实施后复核

| 视角 | 当前结论 | 证据与界限 |
| --- | --- | --- |
| 架构 | PASS（MCP）；整链待验 | 一个 Native 问题事实；chapterReview 仅当前独立报告引用；final 自评不能解决正文问题 |
| 技术 A | PASS（MCP）；整链待验 | task-bound read_work_issues 无模型 Work ID；可信 root/直接 child 身份与 foreign parent 拒绝有实际本机 HTTP 测试 |
| 技术 B | PASS（MCP）；整链待验 | 全部 81 项通过；当前对象标准、报告引用、负评 issue、uncovered 原话引用、issueResolutions 必填和 null/旧字符串拒绝 |
| 产品 | 待两组合成闭环 | 自主 fixture 保留首次失败报告与同问题复评；复制真实旧 observation 后解决，不替换问题身份。scoped fixture 独立读取当前报告 |
| UI/UX | 边界复核通过；UI 独立验收 | MCP 描述不要求作者填 ID；模型负责标准和问题关联；无本范围 UI 改动 |

MCP 编译日志：`/tmp/autonomous-v3-issues-mcp-compile-final.log`；全套日志：`/tmp/autonomous-v3-issues-mcp-tests-final.log`，81 passed / 0 failed，实际执行 3.05 秒。脚本语法检查两条通过。之后仅补齐 readonly 报告传输测试 fixture 的当前对象字段；复验该测试 1 passed / 0 failed（80 filtered），日志 `/tmp/autonomous-v3-issues-mcp-source-fixture-test.log`，未改生产。首次旧 validator 测试 fixture 因字符串标准触发 type 而不是预期 unknown-field，已经改为当前对象标准后复验；没有改 validator 生产语义。

只读独立 fixture 交叉审核定位并完成同步：稳定标准 id 不再依赖 acceptance-N 序号；状态分页聚合完整 requirements；结束模型实际读冻结原话、当前稿及独立报告 raw pages，复制真实 reportRevisionId/criterionId/issueId。原文理解、问题是否属于同一语义及是否无遗漏仍是模型判断，Native 仅确保当前身份、来源、真实保存记录和不隐藏未解决观点。

零兼容后审：PASS。当前 schema/fixture 一次性替换，无旧字符串 fallback、迁移、双写或语义关键词判决。最终 Native 构建和两条实际 Codex/脚本 provider 合成结果待统一构建后补入；不以当前 MCP 通过宣称该阶段已经完整通过。

批内问题共享补充后审：说明已限定为同一次 `revisionOutcome`，未推广到最终自评或跨报告重复首次登记；严格 schema 字段不变。新增测试覆盖两个不同标准明确选择同 ID、同定义及同报告前后 `newIssue:null` 引用，Native 专项负责真正原子登记与冲突拒绝。格式与 diff 检查通过；这一新增测试和 Native 批内专项等待 Root 统一套件，不引用上面的 81 项旧运行证明新增逻辑已执行。两条合成 fixture 当前没有重复首次声明，保持原同问题复评证据，不改变闭环语义。

## 冻结后合成测试提前排错的启动契约同步

Root 授权单独提前运行两条实际 Core 合成测试，日志 `/tmp/autonomous-v3-issues-engine-prediagnosis.log`。本批 0 passed / 2 failed，尚未执行问题修稿链：自主链的共享 Electron VM harness 缺少当前生产顶层 `protocol` 绑定；scoped 链使用旧的30秒 MCP ready等待而超时。本范围仅同步 scoped fixture 的实际 MCP `required:true`、90秒启动配置与90秒 ready等待；外层180秒保持有界，失败/退出仍拒绝，只有真实 ready 后才发送脚本模型任务。共享 harness 由 Root 协调，不以业务参数调整掩盖启动失败。

实施前五视角：架构 APPROVE（仍使用当前真实 MCP）；技术A APPROVE（ready、退出及超时边界保留）；技术B APPROVE（配置与 waiter 同一90秒，无生产兼容）；产品 APPROVE（只调整冷启动等待，不改变作者范围、预算或修稿）；UI/UX APPROVE（无界面变化）。后审仅确认语法及 diff 检查；两条业务闭环必须重跑通过才记录完成，不将本次启动失败排除出日志。

共享 harness 修复后的提前复跑记录在 `/tmp/autonomous-v3-issues-engine-prediagnosis-repaired.log`，自然退出1：0 passed / 0 failed / 2 cancelled，两项均触发外层180秒测试超时。scoped 明确最后阶段为 `launch Runtime`，没有实际 HTTP phases；自主链未给出业务阶段诊断，不能把缺少诊断推定为正文或问题引用错误。本批不证明9.3业务成功，也不据此修改语义门禁；保持90秒 MCP ready等待与外层有界上限，待当前 Native 启动状态确认及最终构建后复验。

## 当前状态摘要契约的 fixture 同步预审

冻结后只读终审发现 scoped fixture 仍在收尾阶段读取 `read_work_status` 已移除的 `assessment.checks`。当前状态只提供有界 `checkCounts`；逐标准的判断与问题引用须从已经实际完整读取的独立报告中取得。本次仅同步这条合成 fixture：用真实报告的 outcome 数量核对 `checkCounts`，逐一保留冻结标准的当前报告 status、issueId、源版本及报告版本断言，不新增旧 checks fallback，不修改 Native 或实际作品。

| 视角 | 结论 | 本次边界 |
| --- | --- | --- |
| 架构 | APPROVE | 状态只作摘要，独立报告继续作为逐项判断权威 |
| 技术 A | APPROVE WITH CONDITIONS | 保留完整报告回读、当前稿及报告版本、每项标准和问题引用；不据摘要构造假的报告 |
| 技术 B | APPROVE | 一次性同步当前 checkCounts，删除旧 checks 依赖，无双轨或 fallback |
| 产品 | APPROVE | 不改变一次输入、自主修稿、独立复评及原文保护的验收目标 |
| UI/UX | APPROVE | 仅测试契约同步，不修改界面或新增作者操作 |

本次仅运行脚本语法检查；真正 Core/MCP/Runtime 收尾链仍交由 Root 当前统一套件验证，不能用语法通过替代业务通过。

实施后五视角：架构 PASS（摘要与独立报告职责一致）；技术A PASS（完整回读、当前版本、全部冻结标准的真实 outcome/status/issueId 断言保留）；技术B PASS（当前 checkCounts 数量对照，无旧字段 fallback）；产品 PASS（验收范围及失败/修稿语义不变）；UI/UX PASS（无生产界面改动）。`node --check electron/tests/scoped-revision-engine-integration.test.mjs` 与限定两文件 `git diff --check` 通过。此处没有启动 Runtime 或重跑闭环；当前业务结果仍待统一套件。生产代码保持冻结。

## 终审：观点大小与实际阅读的证据边界

只读检查确认：问题分页采用30,000字节的多条目预算，首条完整观点不会为凑预算截证据；该函数本身不是32KB硬截断器。仅乘单字段schema上限，会得到超过32KB的未保存形状；真实 `revision_outcome_validation` 在准备和最终保存时均限制整份报告为24,000 UTF-8字节，Runtime另行复核同限。观点抽取已保存报告的理由、引用及证据，再增加有界身份与flags，因此不能用被真实保存限额拒绝的巨大提案证明当前已保存观点超过32KB。最终Unicode HTTP专项直接验证的是metadata分页，不能表述为已经实际测遍所有最大观点形状。

MCP传输另有2MiB硬限，超限返回错误；没有针对该读取工具的32KB静默截断。模型收到的结果仍受Core工具输出策略影响：Engine当前把路由 `outputBudget` 与10,000取小值作为工具结果默认token策略，Core加入20%序列化余量。较小路由额度可能中间截断合法的大观点，`limit:1`不能保证单条正文完整。当前合成闭环继续检查真实全页及实际nextCursor；尚无本轮最大观点在低额度路由的真实模型完整阅读证明。若需要覆盖该配置，最小扩展应将完整观点证据开放为同任务可信source的分叶读取，或明确该工具的输出策略，保留权限、原报告、引文和预算；这次只读终审没有实施扩展，也没有把Native分页等同模型理解或完整阅读。

## 独立逐工具输出预算的实施前评审

Root 已批准最小通用契约：Engine `compileCodexConfig` 的 `mcpServers[].tools[name].outputTokenLimit` 严格生成当前Core的 `[mcp_servers.SERVER.tools.TOOL] output_token_limit`。这是逐工具返回结果的预算，独立于模型生成 `outputBudget`；不在server顶层拼伪字段，不修改Codex内核或领域schema。工具名遵循Engine现行1–256字节ASCII字母/数字/下划线/连字符，map最多1024项，各配置只接受必填正安全整数 `outputTokenLimit`。server及嵌套未知字段、错误类型、非整数和越界值拒绝，不静默忽略。当前SessionPlan已授权MCP集合、真实工具目录与Native任务授权保持；配置名本身不开放工具。小说产品仅为实际问题读取工具指定8192，鉴定指纹和真实Core大输出验收由Root负责。

实际已生成的DeepSeek profile只读确认：`deepseek-flash` 的model工具默认策略为8192 tokens，当前不存在逐工具覆盖；该默认加20%余量足以容纳当前约32KB问题包。本次改动解决可配置模型生成额度较小时的通用结果截断，不虚称当前profile已经发生大观点截断。

| 视角 | 结论 | 实施条件 |
| --- | --- | --- |
| 架构 | APPROVE | Engine通用配置严格编码当前Core字段，产品额度独立声明；不新增文学或权限调度 |
| 技术 A | APPROVE WITH CONDITIONS | 输出额度不扩大工具、来源、写权限或累计资源；整个上下文仍受原Bridge/SessionPlan门禁 |
| 技术 B | APPROVE WITH CONDITIONS | server/map/name/int严格校验，生成逐工具TOML；当前配置负例与低模型额度正例，无顶层字段或旧fallback |
| 产品 | APPROVE | 阅读返回结果不跟随短生成额度缩小；不能把完整接收等同理解或质量通过 |
| UI/UX | APPROVE | 无新增作者表单；可读证据和真实错误保持，产品接线及实机由Root验证 |

本范围只改Engine配置生产与配置测试，保留共享Native/UI/main改动。不修改当前 `.node` 或build脚本，不启动Runtime/provider；新Native构建后才能用配置单测证明生产行为，旧绑定探测不作为新代码通过证据。

逐工具配置实施后复核：Engine `config.rs` 的私有生成helper及当前 `compileCodexConfig` 入口已经实现上述契约，补充3项Rust配置单元和3项Node配置单元（`tests/contracts.test.mjs`）。`node --check`、Rust格式检查和限定diff检查通过；独立只读代理核对当前Core字段、调用方白名单、TOML注入边界及跨平台整数范围，未发现阻断。五视角后审：架构 PASS（通用配置无权限路由）；技术A PASS（静态边界，实际大输出待验）；技术B 待当前Native构建与新增6项测试（不引用旧绑定）；产品 PASS（独立阅读额度，无当前实机失败的夸大）；UI/UX PASS（无新增表单）。Root负责产品policy与鉴定指纹同步、真实Core大输出的脚本provider测试；原自主/scoped链未额外配置该policy，不能把它们的通过算作逐工具额度专项。此范围代码冻结，不自行重建 `.node` 或启动Runtime。

## 当前 Engine 边界测试同步的实施前评审

Root 的当前 Engine Node 回归日志 `/tmp/autonomous-v3-engine-node-final.log` 为285 passed / 10 failed，不能表述为当前全套通过。本范围只处理其中4项：真实HTTP503的observer现在明确给出 `retryable:true`，而外部payload仍不添加该字段并完整保留原始downstream；namespace的40,001个重复x未超过当前真实保守10K token及256KiB字节门槛，旧字节代理断言不成立。独立只读代理确认native/flatten分支前都验证namespace描述，flatten保留并重验完整定义。本次仅同步三个测试文件，不修改生产Native或另一用户线程的6项媒体fixture。

| 视角 | 结论 | 实施条件 |
| --- | --- | --- |
| 架构 | APPROVE | 区分observer诊断元数据与外部原始HTTP错误payload；复用当前字节和实际token双门槛 |
| 技术 A | APPROVE WITH CONDITIONS | 保留raw payload字段、完整downstream、错误来源、未知usage、零执行及单次结算断言，不放松生产限制 |
| 技术 B | APPROVE | observer503断言一次性改true；native/flatten均覆盖可压缩x正例、界12,000字符token负例及超过256KiB字节负例 |
| 产品 | APPROVE | retryable标志不代表已自动重试；当前失败和未知计费证据保持真实，不宣称全Engine已通过 |
| UI/UX | APPROVE | 没有生产界面、作者输入、执行权限或恢复流程变化 |

只运行当前三个Node测试文件，使用已有 `.node`，日志 `/tmp/autonomous-v3-engine-current-boundaries-final.log`；不构建Native，不访问真实provider或作品数据库。

实施后复核：三个文件的当前测试50 passed / 0 failed / 0 cancelled，自然exit0，实际测试2.91秒。运行使用明确 `sandbox_permissions:require_escalated`，仅本地loopback模拟provider与已有Native绑定。两个namespace模式均实际验证完整保留可压缩描述、实际token超限拒绝及硬字节超限拒绝；三个503 observer断言已同步，同时原始payload字段、完整downstream、来源分类、未知usage、零执行和单次结算断言均保留并通过。限定三个测试文件的diff检查通过，没有生产Native或媒体fixture改动。

五视角后审：架构 PASS（两种错误表示职责明确、当前双门槛一致）；技术A PASS（原安全和计费断言未削弱且实测通过）；技术B PASS（50项当前绑定测试通过，无旧门槛fallback）；产品 PASS（不把retryable等同已重试，不把本次50项等同完整Engine通过）；UI/UX PASS（无生产界面变化）。本范围冻结。原全套285/295的历史结果仍如实保留，另外6项媒体失败由其所属线程处理；本次不宣称全Engine当前已通过。
