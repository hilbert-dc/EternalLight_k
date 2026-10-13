# 持续真实任务复验

授权：用户要求持续优化复验直到通过；每轮每项最多 30 请求，token 不限，允许多轮，仍仅隔离虚构作品。每轮有独立原始结果，不重写历史失败。

前审（五视角同一执行者）：架构 APPROVE（公开 IPC 驱动、Native 权限和 Engine 账本权威）；技术A APPROVE WITH CONDITIONS（仅本轮种子章节的 generateChapter 提案可批准，状态/来源由 Native 复核，批准不增加额度）；技术B APPROVE WITH CONDITIONS（helper 回归和最终 Workspace 复读）；产品 APPROVE（完整保存和领域交付分开判定）；UI/UX APPROVE（模拟真实批准操作，不伪造作者采用/发布）。零兼容 PASS。

探针改进：空作品生成已有真实交接批准；现稿编辑增加真实 approveAction，且只在原请求不在执行、插问已返回时批准当前项目种子章节。对同一提案只尝试一次。之后继续等待原 Work 后台核验与结算，最终重读作品，不用前台过期快照判断。预算仍由初始 Work 贯穿批准/恢复/核验；禁止 topup。

## 226 执行归属竞态前审

生成已登记 writer，但在发送前出现 WORK_EXECUTION_OWNER_INVALID。检查发现 Native 事件 claim 只阻止其他 host，允许同 host 的后台消费者借用前台有效 lease；Host adoptClaim 会复用相同内存记录，若后台恢复撞上正在执行的 domain，其 finally release 可以删除前台 lease。方案：Native 同一 host 也不能占用前台 lease；仅相同事件、相同 claim/lease/session 的幂等重放可复用。acquire/renew 的前台借用合同不变；不放宽暂停、来源、预算校验。五视角自审 APPROVE WITH CONDITIONS，补同 host 前台归属保持与释放后可 claim 回归，再跑现有并发/过期/重放测试。与 226 当前运行隔离，未改正在运行的二进制。

## 227 发送前失败结算前审

Host 在 Native 已授予任务额度后，先建立 Engine 持久预算，再校验执行租约；发送仍严格在租约成功之后。这样租约失败可由真实未发送账本生成 failed/零用量收据，Native 据此结算，不制造历史账本。finally 关闭所有未移交 Engine 的预算句柄。架构、技术A、技术B、产品、UI/UX 五视角均 APPROVE WITH CONDITIONS：回归必须证明未发送、真实账本终态和结算发生；历史 226 未知状态原样保留。

227 后审：Native 租约事件回归 14/14、Host 事件 29/29、任务额度 11/11（含真实 SQLite 零请求失败收据）通过，原生 bins 构建通过、架构检查通过。五视角自审代码范围 APPROVE，真实模型闭环仍待复验。226 原始报告及诊断保留；生成仅调用13次即遇租约失效，未达到30次上限，不得计为预算不足或成功。

## 228 插问期间候选自动派发前审

真实插问复验显示原对话登记候选后，Host executeConversationCandidates 仍立即派发，Native 正确拒绝 open interlude，Host 却把候选写为 failed；问答返回后因此没有可续行队列。Host 应读取 Native 原 Work，插问打开时只批准原作者明确要求的候选入队并退出当前派发，由返回后的既有 Work 调度继续。领域暂停/来源/权限校验保持。五视角自审 APPROVE WITH CONDITIONS：测试不得调用 writer 或 finish，不得谎报已保存，后续真实模型复验。

228 生成纠错证据：曾复用上一 Task 的 sourceId；读取 draft 附属信息未完成当前 workspace 正文读取；将 nativeSavedCheckpoint 元数据写成正文 quote 被拒。改进当前工具说明：每个 Task 先获取自己的目录；直接核验读取当前 workspace 正文，正文 evidence 只引用 sceneGroup 原文，非正文保存/采用状态在 reason 解释且不得伪造 quote。权限、完整读取和证据校验不变。五视角自审 APPROVE；限定说明修复，不改变报告必填约束。

229 后审：候选派发20/20、真实Host/Native插问2/2、工具指南1/1、架构检查通过。原测试两条使用旧 messageDispatch 形状且只拦截旧 base domain，按当前生产消息身份及 execution domain 更新后通过，保留失败日志。五视角自审本地修复 APPROVE；真实复验待验证。228 三项均未通过，原始报告和日志已归档，不计入通过记录。

## 230 原问答恢复前审

Native failed_actions 原来排除全部 advisory，导致初始问答中断前没有登记候选时，恢复决策没有合法动作。允许恢复失败的普通问答（排除 Work 决策、Generation 和插问），沿用现有已结算、无未知写入、失败分类、同条件尝试上限与 recoveryPlan 合同。ready action 仍为 advisory，不直接授予写作；恢复问答读取最新原要求并经既有 candidate proposal/approval 获得精确权限。恢复 target 去除旧 messageDispatch 身份，作者要求由现有 authorIntentBinding 冻结。Host 普通问答恢复完成后执行既有候选处理，再回原 Work 调度。五视角自审 APPROVE WITH CONDITIONS：Native 用例验证 ready action、同Work/额度和已完成问答排除；Host 回归与真实模型复验。

230 后审：Native交付6/6（含失败普通问答重新成为同Work advisory、原作者绑定与动作选择）、Host/Native候选链1/1、JS39/39、架构检查通过。原生构建通过。五视角自审本地修复 APPROVE，真实多轮仍未完成。229 生成13次因幻觉工具失败后被错误结束；插问30次，正文已保存但核验未完。保留其失败报告，下一轮231三项重测。

## 232 核验说明一致性前审

231 生成正文保存后选择独立核验，30请求不足以收尾。发现 Host 的 durable mission / advisory prompt 仍无条件要求独立评估，而当前 Native chapterAcceptance.verification 与工具合同已允许按作者要求选择 direct/independent。同步 Host 到现行 Native 合同：明确要求独立时必须 independent；其余由模型按任务选方式并冻结，direct 同样完整读取、提交真实证据报告、处理问题且通过领域验收，不自动交付。五视角自审 APPROVE WITH CONDITIONS：不修改 Native 验收门槛、不将自评称独立、不覆盖已冻结 independent；检验现有策略与生产上下文测试。

231 真实验收：插问通过，27请求、1042364已知tokens、未知tokens=0；原Work自动返回、96字符候选已保存、直接核验及goalStatus=fulfilled、无pending用量、未采用发布。人工判定另存verdict，原始needsReview不覆盖。生成及改要求仍未通过。
232 补充：普通问答恢复提示明确切换回本轮普通问答能力，避免沿用历史决策阶段说明；同条件计数包括无authorIntentBinding的初始问答，只在实际作者意图变化时不计为同条件。已结算但不可分类的失败不因此获得恢复动作。Native6/6、Host31/31通过；提示体积首次超过既有4096字节限制，已精简且原门槛测试通过，不增大上限。

## 234 Engine 无 token 总额限制时的未知用量前审

steer 关闭在途模型流后旧请求费用未知，Engine budget.reserve 无条件阻断整个任务；Native 已允许无token总额限制的Work保留未知费用继续，二者不一致。责任为Engine通用预算，不根据小说/steer关键词决定。无token上限时允许新reservation继续、累计原请求与unknownTokens；有限token预算仍阻断。无论预算是否有限，已结算unknown的同trusted reservation ID禁止重新dispatch，避免一笔计费换多次调用；现有lease、maxRequests、deadline、overrun及Native未知写入屏障不变。五视角自审 APPROVE WITH CONDITIONS：真实TaskBudget与PersistentTaskBudget覆盖新ID/旧ID/有限预算/次数上限，真实流断连桥接现有回归保持；下一轮真实改要求验证。此变更不消除费用未知，不补造usage。

## 235 完整正文读取证据前审

233 生成完整读过当前 relevantText，但 Native 只承认 workspace refs，重复读取同一正文耗尽30请求。由 Story Core 扩展相同版本的完整来源识别（B03/B04/B10）：只接纳当前主任务冻结 relevantText、精确 objectId/sourceRevisionId、与权威 revision content 完全一致的正文；唯一允许投影为删除可由全部 sceneGroups 精确重建的重复 body。仍要求主任务完整读取整个来源，旧版本、局部、子任务读取不得通过；current revision、原要求、引文、独立核验要求不变。公开读取与 verifyChapter API 不变，无新增权限/表/兼容层。五视角自审：架构 APPROVE（Native 唯一证据权威），技术A/B APPROVE WITH CONDITIONS（等价投影负例及实际 HTTP 验收），产品/UI APPROVE WITH CONDITIONS（真实复验闭环，不能把保存当交付）。模型仍自主选核验方法；工具说明同步，计划运行 Native 单测、HTTP 合同、架构检查、真实生成/改要求。

234 后审：Engine 18/18 通过，包含真实流断连后旧ID重放拒绝、无上限新ID继续、有限预算拒绝和次数累计。费用未知保持未知。五视角本地代码 APPROVE，真实改要求仍待验证。

235 后审：Native来源12/12、Runtime HTTP 3/3（两个完整正文入口均实际完成领域交付）、构建与架构检查通过；伪造/缺失正文、旧身份、子任务或部分读取、过期报告及independent条件保持。Engine另4项真实进程结算回归通过。五视角自审本地代码 APPROVE；236真实生成/改要求复验，仍每项30请求、不设token总额上限。

## 237 交付自检字段语义前审

236生成当前正文读取与verifyChapter通过，finishChapter已登记；complete_task的fulfilled却附authorDecisions=[未来是否采用、如需可改语气]，Native正确拒绝后用尽30次。schema该数组无description，Host文字虽已有“真待决定”但未就近说明。由MCP将Native现行合同写入goalStatus/authorDecisions字段及complete_task短说明：只列阻止当前已授权目标完成的真实作者决定；未来可选采用/修改提示不属于当前阻塞，不捏造问题；fulfilled须unresolved/authorDecisions均空。五视角自审 APPROVE WITH CONDITIONS：无权限/校验/完成门槛修改，模型自主判定，保持真实未完成不可称fulfilled。验证现有schema测试、工具说明和架构检查；再真实生成。236改要求原问答已在未知用量保留时继续并保存候选，待闭环。

236真实改要求通过：21请求、743325已知tokens，36551未知预留保留；最新保留密函指令到达，94字新稿实际保存，Native completed/ready_for_review/fulfilled，无待结算、采用或发布，无人工重试/修稿/额外批准。一次后台409旧claim拒绝不影响前台闭环。人工正文核对及领域判定另存verdict，不覆盖原始needsReview。生成仍失败于最终自检字段语义，237待复验。

237后审：现行schema测试、原生构建与架构检查通过，字段语义与Native校验一致，五视角自审APPROVE。238当前构建联测delivery/interlude/change各30请求、token不限；说明影响三项收尾，因此复验已通过场景。共享工作区direct_verification.current同期另有持久证据与生产者状态分离改动，未覆盖/回退，当前运行源码和二进制哈希单独归档。

## 239 普通交接与最终自检说明前审

238生成在新范围handoff已登记后，read_work_status明确deliveryReviewRequired=false，模型仍筹备deliveryReview并猜expectedWorkVersion=0，下一响应在Engine参数解析处被拒invalid_tool_arguments（无结构化schema诊断，不能声称已确定具体JSON错误位置）。之前237短说明突出最终自检字段，未就近强调适用条件。MCP短/长说明统一以当前Native deliveryReviewRequired=true为提交条件；false时仅summary，不能猜自检版本或把未批准新范围称完成。领域/Engine参数屏障和新范围人工批准保持，非新增兼容。五视角自审 APPROVE WITH CONDITIONS：原始失败保留，工具说明测试/构建/架构检查，再生成复验。不把模型供给错误静默修正后执行，也不把历史已登记proposal计为实际写作。

238结果：插问再次通过（21请求、808074已知tokens、未知0）；改要求再次通过（28请求、937015已知tokens、未知预留36289）。原文逐项核对、Native完成、无待结算/采用/发布。生成8请求（241328已知tokens）在handoff后模型输出参数拒绝，未启动生成；原始失败及verdict分开归档。239只复验剩余生成项。

239补充前审：concise原来覆盖operation_tools已经收窄的worker完成说明，尽管schema删了deliveryReview，短说明仍提示Work review。保留当前worker专用说明，只有实际存在deliveryReview的工具使用通用审查提示；在现有真实operation_tools/exposed_tools合同测试中检查投影一致。五视角APPROVE WITH CONDITIONS，同一MCP声明责任，不修改授权。

239后审：指南1/1，完整MCP 113/113（含真实HTTP及worker exposed schema/说明一致），构建及架构通过。首次完整测试72通过/41本机监听PermissionDenied，原始日志保留；授权网络环境重跑全通过。五视角APPROVE，240生成真实复验仍最多30请求，token不限。

## 241 当前原要求与分页说明前审

240生成交接/写作均成功（9+6+7请求）；最后8请求漏读当前任务authorRequirementsSource，Native拒绝核验，模型却改正文证据重试且额度耗尽。原作者原话完整读取屏障保留。MCP短说明明确verifyChapter前主任务读取当前authorRequirementsSource及完整稿，旧任务读取不作本任务证据；read_context目录在同Task/版本内复用，发生来源变化才刷新。Host分页说明区分receipts.coverage.complete与整体actions/requirements分页，必要未覆盖信息才继续分页，不把其他分页误当缺失回执。五视角自审APPROVE WITH CONDITIONS：职责为MCP/Host指令投影，不改原要求/验证/授权/预算，当前页覆盖不等于整个目标完成。运行指南、Host提示与架构检查、构建，再生成真实复验。

241验证进度：Host25/25、提示体积1/1、原生构建及架构检查通过；指南测试二进制仍在启动。改动仅声明文字，权限/运行逻辑没有变化；242真实生成与该本地检查并行，仍每项30请求、token不限，不把待运行测试记为通过。

241后审：指南1/1补充通过，Host25/25及体积1/1、构建架构均通过；五视角APPROVE。242生成9+5+7+9=30请求，已读原要求/正文、核验通过并登记finishChapter，但未完成最后自检；无参数/Native错误，不能计通过。原始报告已保留。

## 243 精简动作目录分页前审

242核验前多读2页动作、核验后又读1页；真实可执行动作条目已是小型actionKey/operation/目标/版本投影，仍每页2项，导致简单任务额外往返。责任Native Work只读状态投影（B03/B04），增加动作页容量为8，requirements仍每页1条、receipts仍2条；动作内容、排序、权限、审批、独立要求和模型选择均不变，超出8项继续完整游标分页。公开cursor/coverage按新步长一致，零兼容无双轨。五视角自审APPROVE WITH CONDITIONS：Runtime真实HTTP首屏能找到direct核验、保留既有6500字节页面上限、独立核验与证据拒绝通过；Core分页无漏项/重复及游标失效测试；架构构建后真实生成。不是自动替模型选业务方法或扩大输出限制。

243容量补充：检查242实际页得到核验前5个动作/2条要求，三页分别3541/2642/2347字节，核验后4动作/2要求两页3066/3106字节。将简短要求页也增至2，避免仍被一条要求分页拆开；合并后继续以现有6500字节HTTP断言验证。requirements/动作coverage与游标步长同步，回执仍2条。五视角APPROVE WITH CONDITIONS，原有拒绝和权限不变。

243后审：最终HTTP3/3，确认单章首屏coverage.complete=true、无nextCursor、仍≤6500字节，两种正文来源真实交付及独立/负面证据屏障保持；Core分页3/3（含游标绑定、变化失效、完整无重复），构建与架构通过。五视角APPROVE；244再跑生成，不提高30请求上限。

## 245 核验说明一致性前审

244共30请求、1039718已知tokens，正文109字、核验与finishChapter已提交，最终complete_task版本字段误放根层被拒，故不算交付通过。另一次verifyChapter因模型将候选未采用/发布冻结为正文标准却提供空evidence被拒；现有指南“非正文标准在reason解释回执、不编造引文”与supported证据非空约束组合含糊。MCP责任为真实合同说明：冻结正文标准须可由正文验证，采用/发布授权与回执由Native单独保证并在最终summary说明；所有supported/unmet项仍须真实正文证据。最终review字段明确嵌套deliveryReview.expectedWorkVersion/expectedSourceDigest，取当前status，不接受根层或猜值。无schema/领域/预算放宽，无兼容。五视角自审：架构/产品/UI APPROVE，技术A/B APPROVE WITH CONDITIONS（指南/schema验证、构建、架构，真实生成复验）。

245进度：当前原生构建与架构通过，指南测试已编译但进程采样停于macOS _dyld_start（96K footprint，尚未测试断言）。保留本轮测试继续等待，同时246真实生成使用新构建；待测不记通过。无采用/发布，每轮30请求、token不限。

245后审：指南1/1、交付schema4/4、当前构建与架构通过；启动等待后测试实际执行成功。五视角APPROVE，246已进入真实模型生成，不因候选保存就判定通过。

## 247 同任务压缩后读取恢复前审

246生成9+6+5+10=30请求，正文102字且verifyChapter一次通过、finishChapter登记成功，最终自检仍缺请求。主因最终决策读完原要求和正文后Core自动压缩，摘要保留正文逐字全文和已读结论；模型再read_work_status/read_context后，又完整重读同两个来源。现有Host要求压缩后恢复coverage，但未区分同任务的既有完整读取与新任务。Host只澄清恢复指令：同任务同版本Native持久returnedCoverage完整且上下文仍有精确原文时复用；缺页/原文缺失/新版本/新任务仍读取；摘要本身不构成读取证明。Native屏障和Core压缩不变，无额外权限/省略核验。五视角APPROVE WITH CONDITIONS，Host指令与体积回归、架构检查，真实生成再验。

247后审：最初说明增长触发既有4096字节上限，精简同一提示后25项Host、1项指令体积/责任及架构通过，上限未放宽。五视角APPROVE，248真实生成复验，每轮30请求且token不限。

## 249 可信候选版本默认绑定前审

248写作将JSON null反复编码成字符串null、空白或空串，Native正确拒绝，未保存。现有声明已有明确null说明，继续叠加文字不能消除机械抄录。目标：MCP模型契约将expectedCandidateRevisionId改为可选断言；遗漏时通过受信Runtime绑定取得本任务唯一outputObjectId的预期候选版本（冻结初始值或本任务最近成功输出），绝不取其他任务最新head。Native task_input已有expected_task_target权威计算，新增只读公开投影供Runtime绑定使用。每次需要默认值的保存前刷新绑定，避免本任务多checkpoint时缓存旧值；显式CAS原样保留，错值依旧拒绝。模型请求摘要在补默认值前固定，既有operation digest/idempotency保证重放不因版本前进变新写；身份/范围/源版本/并发CAS仍由Native最终校验。无字符串null强转、无吞错误或兼容。五视角自审：架构APPROVE（B03/B04机械绑定复用现有权威），技术A/B APPROVE WITH CONDITIONS（空候选/本任务前次输出/外部头并发/显式错误/真实HTTP及MCP重放），产品/UI APPROVE WITH CONDITIONS（减少模型技术参数负担，保留拒绝与真实保存回执）；检查通过后继续30请求真实验收。

249调用方补审：原保存证据读取仍强制三项机械字段，会拒绝新模型原请求。Host改为仅校验显式字段、保持原始哈希和缺省不填值；Native对缺省字段的核定只返回精确匹配的既有ledger状态，不从现在head推断历史默认值、也不将accepted未知操作升级为rejected。保留完整原参数下原有只读反证核定；已知committed/rejected依旧由原ledger证明。五视角APPROVE WITH CONDITIONS，增加原始缺省哈希、篡改拒绝、未知状态不升级回归。

### 按用户要求补充根因与修复边界

根因并非统一的“模型不听提示”：1）模型手工回填运行时已有身份/版本，产生类型与CAS错误，249改为可信绑定且显式值只作断言；2）工具发现说明、schema与领域条件曾不一致，245对齐正文证据和生命周期事实，保留所有Native屏障；3）压缩误作新任务，持久阅读证据早已存在，247澄清同任务同版本复用条件，仍由Native验证，未声称提示能保证不再冗余读取；4）本地验证冷启动与业务失败混杂，原始超时保留并用热启动单项复验分开证明。验收不采用“直到随机绿一次就宣布稳定”的标准：保存、重放、外部并发、原始请求哈希与未知恢复分别用确定性合同验证，再用真实模型验证产品闭环。尚未覆盖跨平台/长期稳定性的结论保持明确，不以本轮短篇样例替代。

249后审：MCP116/116、绑定HTTP2/2、恢复账本8/8、Core任务权威链1/1、两种真实Runtime/MCP重启恢复2/2通过；Host原批次52通过1冷启动取消，受影响真实进程用例热启动复验通过。原始超时与旧合同断言失败日志保留，未放宽断言。当前原生构建、架构365文件7crates及改动diff检查通过。五视角自审APPROVE（仅本地合同与并发/恢复证明）；250将三项真实旅程复验，每项30请求、token不限，不将本地通过宣称产品闭环通过。

## 251 保存与完成事实分离前审

250首次省略CAS保存成功，但writer保存chapterPartial后，MCP一律返回nextAction=complete_task，Core却将已保存未完整与完全无输出都报TaskOutputMissing，导致重复完成与猜参数。责任：Core区分TaskOutputIncomplete并保留完成屏障；Runtime传递实际保存completeness及明确409错误；MCP仅按回执投影可完成/继续创作，不替模型把partial升级complete。完整的含义仍由当前作者目标和授权范围判断，不要求额外扩写完整长章。五视角自审APPROVE WITH CONDITIONS：不放宽完成条件、原候选保留、失败操作已知拒绝、真实HTTP/MCP回执与Core部分保存拒绝/完整后完成回归。250仍独立完成观察，不以正在修改的代码冒充该轮运行版本。

## 252 作者修改要求的优先级合同前审

250改要求已真实送达，模型也读到了新旧原话，但将明确替换视作必须再问作者的冲突而只结束讨论。Host author-steer原说明仅声明意图代次变更、未说明语义更新关系。由Host投影同一作者后续消息的通用优先级：明确变更替换对应旧要求，兼容的原目标继续，只有无法确定的新歧义才询问；不删除历史原话、不按文学关键词路由、不自动扩大授权或宣布fulfilled。继续沿原user角色传递原话，绑定来源与版本不变。五视角自审APPROVE WITH CONDITIONS：验证原话/角色/身份/不可变性与生产prompt，真实change仍须实际保存与完成。自然语言解释仍由模型，不声称措辞更新等同确定性成功。

252验证：Host7/7、真实Codex/Engine本地模型桥4/4通过（native/system/instructions三种角色适配及动态推理）。首次集成3失败因夹具未绑定现行execution lease/expectedThreadId/expectedTurnId，生产正确返回executionChanged；夹具同步真实bridge lease与adapter绑定后重跑通过，未修改生产门禁。251首次MCP114通过3失败（新增完整度字段缺失的旧fixture、说明超既有1000字节），已同步fixture并压缩回执，保留上限，正在复验。

251后审：MCP117/117、Core完成权威链1/1、真实HTTP保存/部分完成拒绝/原请求重放/外部头冲突1/1、构建及架构通过。首次HTTP发现draft_revision投影丢completeness，已从保存回执对应immutable revision投影，绝不从当前head或当前请求补值；修复后全部断言通过，旧失败保留。252 Host7/7、生产提示链4/4及架构通过。五视角自审APPROVE，253只复验未通过的delivery/change，每项30请求、token不限。

## 253 真实复验后审与结论

生成通过：30请求、1,091,770已知tokens、未知0，99字正文实际保存，direct核验与finishChapter、deliveryReview全部成立，Native completed/ready_for_review/fulfilled；最后Engine请求成功生成最终回复，非用完额度后仅剩工具提交。保留一次后台过时claim的409 WorkTaskStale，获胜执行及全部计费attempt正常完成，无pending动作/用量。

改要求通过：24请求、918,988已知tokens、38,432未知预留保留。steer真实送达；110字正文将信折好贴身保留并计划收好，没有烧毁；同Work自动保存、核验、fulfilled并最终回复，未再向作者要求重复决定。两项均未采用、发布、追加额度或人工修稿；隔离作品清理观察全部settled无错误。源文件与二进制运行前后哈希无漂移。

原始探针needsReview及退出1不改写；正文、Native领域完成、Engine最终回复和费用人工核对另存 `evidence/2026-10-10-task-live253-verdict.json`。插问最新通过证据为250（20请求、749,445已知tokens、未知0；回答林夕、自动返回、97字候选与fulfilled）。当前三条目标旅程各有真实闭环证据，结束此轮修复/复验循环；不宣称短篇样例证明跨平台、长篇、多Host长期稳定性或视觉交互验收。249的并发/重放/恢复确定性测试、251的保存与完成语义及252的生产提示链证据共同支撑修复，而非仅靠随机一次绿灯。五视角自审APPROVE，范围限定上述合同与三条真实旅程，无提交/推送。

## 本轮范围提交核对（2026-10-10）

按作者选择，仅提交本轮任务修复及验收记录；其他架构、UI、媒体、构建等工作区改动保留未提交。代码分为 Runtime/MCP/Engine 恢复与保存合同、Host 明确变更语义两个提交，证据另行提交。工具绑定为本轮默认参数和原始调用摘要修复的必要前置，调用方与夹具同步；工具指南、来源导航和局部 patch 等相邻改动不纳入。

针对暂存内容导出的独立快照验证：Runtime 库离线 cargo check 通过；MCP 109/109 通过；Host 恢复相关 43/43 通过；作者语言测试 7/7 通过；architecture:check 无诊断。拆分时发现测试夹具入口遗漏并补齐；早期 106/109 的失败记录保留。随后沙箱内 HTTP 监听被拒绝的运行记录也保留；允许本机监听后的同一快照 109/109 通过，没有跳过测试或改低断言。证据为本目录 evidence/2026-10-10-task-commit-*.log。

本次提交核对未新增付费模型调用。250/253 真实验收对应各自 source.json 所记录的完整工作区源码及构建身份；它们证明当时场景结果，不冒充拆分后提交的再次真实模型验收。独立快照检查也不等于全量 Native、跨平台和长期稳定性验收。其他工作区改动仍需各自评审提交。
