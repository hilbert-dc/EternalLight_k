# 创作质量优先优化：全量验收

本次验收获用户明确授权，使用隔离虚构作品与真实 DeepSeek / Codex Core / Engine / Runtime / MCP / Electron 链路。结论：**尚未通过全量验收，不能声称理想方案已经全面落地或质量不退化。** 以下区分工程合同、真实闭环和文学判断。

## 最新验收状态

| 范围 | 当前结果 | 证据 |
| --- | --- | --- |
| 完整 Host 回归 | **477/477 通过**，exitCode=0；包含真实 Core/Engine/MCP 集成 | [最终日志](evidence/2026-10-10-final-host-verified.log)、[退出状态](evidence/2026-10-10-final-host-verified-status.json) |
| MCP 与交付/恢复 | MCP 114/114；最新引用诊断 HTTP 3/3；终验自动证据 Core 6/6；真实账本摘要贯通通过 | [MCP](evidence/2026-10-10-evidence-scope-mcp.log)、[HTTP](evidence/2026-10-10-evidence-scope-http.log)、[Core](evidence/2026-10-10-terminal-evidence-core.log)、[跨层摘要](evidence/2026-10-10-bookkeeping-passage-final-2.log) |
| 构建与架构 | TypeScript/Host 类型检查、Vite 构建通过；365文件/7crate架构检查无诊断 | [构建](evidence/2026-10-10-final-build.log)、[架构](evidence/2026-10-10-final-architecture.log) |
| 两章真实创作 | 两章候选保存、核验及整项交付完成；52请求/1,963,332已知tokens | [原始报告](evidence/2026-10-10-evidence-scope-multi-report.json) |
| 8个固定文学样本 | 7例首批完成，Unicode启动失败后补测完成；8例分别有原文保护与领域完成证据 | [分批核对](evidence/2026-10-10-semantic-quality-combined.json) |
| 暂停接续 | 已有稿件修订在原Work恢复并完成；34请求，未知用量保留 | [原始报告](evidence/2026-10-10-bookkeeping-resume-seeded-report.json) |
| 未通过/待补 | 5万token档、长文全文压力路径未通过；完整Native900秒超时；人工读者评分延后 | 失败与限制详见下文，未计为通过 |

已落地的机械合同修复通过相应工程检查，代表性真实创作路径完成；**效率目标、完整Native覆盖和文学质量证据仍未全部关闭**。这不是发布批准，也不是全量理想方案完成声明。较早批次与失败如下保留，不能用最新成功覆盖历史费用或失败分母。

## 首轮失败基线与证据

| 验收层 | 实际结果 | 证据 |
| --- | --- | --- |
| 模型资格 | base-tools-v1 真实认证通过；期间一次指纹失配，经重新认证恢复 | 认证日志另存；非伪造资格 |
| 精确改字，50k tokens / 10 请求 | 0/4；均保存前失败，原报告缺少明确原因，不能回填猜测 | [原批次](evidence/2026-10-10-agent-acceptance-live-tools.json) |
| 精确改字，250k tokens / 10 请求 | 2/4：短文全文与补丁通过，长文两例失败 | [扩大额度批次](evidence/2026-10-10-agent-acceptance-live-tools-expanded-2.json) |
| 长文定向复验，800k tokens / 30 请求 | 0/2；全文模式实际21请求、771500已知tokens后预算耗尽；补丁模式5请求、104776tokens后执行失败 | [独立复验](evidence/2026-10-10-agent-acceptance-long-retry.json) |
| 桌面五场景 | 暂停通过；修改要求的Work完成；交付/插话后闭环受阻；并发改稿后恢复拒绝。不能按有候选稿判全部通过 | [完整报告](evidence/2026-10-10-agent-acceptance-desktop.json) |
| Host全套 | 477例：440通过、37失败；使用test-force-exit避免失败残留资源无限等待，保留首次挂起日志 | [完整结果](evidence/2026-10-10-agent-acceptance-host-bounded.log) |
| 评测与碎片合同 | 22例通过；新增选择集/脱敏诊断7例通过 | [局部验证](evidence/2026-10-10-agent-acceptance-evaluation-tests.log) |
| 依赖架构门禁 | 365文件、7crate，无诊断 | [架构检查](evidence/2026-10-10-agent-acceptance-architecture.log) |
| 修订→建模→反馈专用脚本 | Runtime预检失败，无模型阶段；不能声称该三阶段已测通 | [失败证据](evidence/2026-10-10-agent-acceptance-revision.json) |

上述各批使用不同预算，禁止合并为同额度成功率或挑选成功样本覆盖失败。Core/MCP受控测试、真实供应商与真实桌面是不同证据层级。完整Native测试和文学样本结果在本记录后续补充。

## 高token开销是否真问题

**是：高输入开销与短任务不闭环已复现；“绝大部分都浪费在搜索”尚不能据此成立。**

桌面五场景的Engine账本为93次请求、3,377,582已知tokens，另有117,310未知预留tokens（来自中断/未确定请求）。Core收到90份实际usage记录，已知总量与Engine一致：

- 输入3,320,459，占已知总tokens约98.3%。
- 输入缓存命中2,780,288，占输入约83.7%；缓存tokens不能按未缓存单价估算费用。
- 输出57,123，其中报告的reasoning output为31,001。输出也含工具参数与解释，不能全部计为正文。
- 供应商账单金额未取得，不能把token总数写成实际人民币费用；未知预留也不是实际收费。

[可复算账本](evidence/2026-10-10-agent-acceptance-desktop-ledgers.json)、[Core usage](evidence/2026-10-10-agent-acceptance-desktop-provider-tokens.json)、[工具归因](evidence/2026-10-10-agent-acceptance-desktop-overhead.json)保存在仓库证据目录。

102次工具调用及回传中，本地cl100k估算回传最大项：read_context 16次/64718 tokens；read_source_content 37次/21986；read_work_status 12次/12720。完全相同的重复结果至少包括read_context 8813、read_work_status 3799个本地tokens。工具局部计数不含每轮历史重放、完整schema及缓存，不能直接相加为供应商用量或反推精确浪费比例。相同读取也可能服务合法恢复，需要结合版本和中间写入判断。

按领域操作分：advisory 69请求/2666757已知tokens；generateChapter 24请求/710825。advisory包含真实创作判断与核验，不能当作纯簿记。可确认的产品风险是：约100字开场使用30请求/1134766tokens仍受阻；临时插话场景30请求/1120506tokens仍受阻。短文改字两个成功样本分别6请求/128233tokens与7请求/145556tokens，补丁本轮没有降低全流程成本。

## 对理想方案的调整依据

质量优先不变。下一阶段应以“同一创作要求和保护范围下，取得真实交付并保持质量”为主门禁，成本作为同时观察的指标。不能为了5万tokens阈值跳过必要阅读、引用验证和质量检查，也不能仅扩大预算就宣布优化成功。

优先修复：保存后正确接续终验、冻结原稿与当前候选的读取角色、报告有效性与Work闭环；接着消除重复且无信息增量的上下文及状态回传。对确有必要的阅读和文学核验保留完整能力。工具schema、方法合同与测试fixture必须同步。模型资格/可用性、保存、终验、采用是不同状态，不得合并。

已观察到：悬念样本的实际首尾保护通过，但最终说明把原首段误称为“原中段”。这属于引用/说明质量问题，不能被结构测试绿灯掩盖。真人评分、优化前可运行冻结基线及8个有效盲评配对仍缺失；不得据本批声明文学质量提升或不退化。

## 文学样本与多章结果

8个固定样本中，4例通过结构与领域交付核对（悬念、Unicode/锁定、时间线分析、证据不足分析）；2例已保存且保护检查通过但Work仍受阻（长文声音、重复引文）；2例在空段组装载阶段失败，未测到续写模型行为。长文装载问题修正后补测，原失败批次保留。两章连续创作仅保存了第一章，30请求内仍未闭环，未通过。

[结构核对](evidence/2026-10-10-agent-acceptance-quality-checks.json)、[原始全批](evidence/2026-10-10-agent-acceptance-quality.json)、[长文补测](evidence/2026-10-10-agent-acceptance-quality-long-retry.json)、[人工评审全文](2026-10-10-agent-quality-review-packet.md)。4/8表示结构/交付通过，不是文学质量通过。没有冻结可运行基线和真实人评，不能声称8个有效配对已经完成。

## 设计问题与代码问题的分界

已确认一个结构性根因：[报告生命周期证据](evidence/2026-10-10-agent-acceptance-verification-lifecycle.json)显示两份报告的稿件仍为当前版本，但生产任务failed；Story Core `direct_verification::current`直接按生产Task状态过滤。这将“证据有效性”与“是否还能执行”混为一谈。调整目标是来源/标准/依赖决定证据有效性，权限/预算/暂停决定执行可用性，完整目标与有效证据决定交付状态。不能仅去掉所有拒绝条件。

[理想方案第13节](../docs/designs/AGENT_CAPABILITY_OPTIMIZATION_PLAN.md)已按根因重新排列结构调整。全量Host失败尚未逐项归因，时间敏感项需要在无并行负载时复验；保留原失败，不以夹带旧文本的断言失配断言全部属于生产缺陷。

## 用量与验收限制

本次已落盘的真实工具、桌面与质量批次合计317次请求、10,966,509已知tokens、117,310未知预留tokens，见[分批汇总](evidence/2026-10-10-agent-acceptance-usage.json)。该汇总不含资格认证，不等于供应商账单金额。所有提高预算的复验独立保留；没有因失败丢弃费用。测试中未采用或发布任何真实作者作品。

自动化广域首轮Native在`commitment_context_tests::scene_prefix_and_pov_are_whole_item_disclosure_gates`失败处停止，之前304例通过；后续改为no-fail-fast并设900秒总上限，结果另记，未完成部分不得算通过。macOS进程采样显示一次定向测试启动等待停在`_dyld_start`且尚未进入测试代码，[采样证据](evidence/2026-10-10-agent-acceptance-native-startup-sample.txt)只能解释该进程的等待，不能据此解释全部测试失败。

## 首个结构修正的验证结果

已实现直接核验证据与生产Task生命周期分离。[修正前](evidence/2026-10-10-verification-lifecycle-before.log)复现failed导致同稿报告stale；[修正后](evidence/2026-10-10-verification-lifecycle-after.log)既有3例及新增1例通过：failed/cancelled不单独作废证据，终止任务仍不能完成，Work不自动完成，真实换稿仍使证据失效。终态在隔离fixture注入，再经真实HTTP回读/拒绝检查；未重跑供应商失败后的整条生产恢复链。源码身份见[变更标识](evidence/2026-10-10-agent-acceptance-change-identities.json)。这是已确认耦合的一处修正，不代表高开销、整体交付或文学质量问题均已解决。

广域Native no-fail-fast复验达到900秒上限后终止，状态为timeout，exitCode=-15；见[evidence状态](evidence/2026-10-10-agent-acceptance-native-all-status.json)。未完成部分不计通过。

## 语义交付与簿记下沉后的复验（保留前述失败基线）

本轮把 finishChapter 的重复核验摘要、终验已保存产物清单，以及冻结输入版本和调用幂等键交给既有领域/传输层处理。没有删除作者标准、当前稿件 CAS、必要阅读或独立评审要求。另修正自动补参后 Host 原始请求摘要与 MCP 展开参数摘要不一致的恢复缺陷。

- 原 8 个文学样本重新运行：7 个成功启动样本均完成 Native Work；稿件标题、段组身份/顺序、锁定状态与受保护文字检查通过，分析任务未修改原文。Unicode 样本在 thread/start 阶段超时、未创建领域 Task，单独补测。详见[原始批次](evidence/2026-10-10-semantic-quality-report.json)与[结构核对](evidence/2026-10-10-semantic-quality-checks.json)。7/8 是该批结构与交付通过率，不是文学质量评分。
- 单章已完成，34 请求、1,215,168 已知 tokens；旧构建两章批次保存了两章，但仅首章完成交付，60 请求、2,278,076 已知 tokens 后受阻。新构建复验另记，不覆盖该失败。
- 控制批次：暂停持久化通过；临时插话与修改要求均完成 Work；作者并发改稿后的旧源恢复被拒绝。后者不是正常暂停恢复成功的证据。[原始控制批次](evidence/2026-10-10-semantic-controls-report.json)
- 原 Host 全套 477 项中 476 通过，唯一剩余项是冻结方法目录投影的 fixture；修正后该真实集成定向通过。证据分别为 semantic-delivery-host-final.log 与 semantic-delivery-scoped-final.log，不能描述为一次全套 477 项全绿。
- 最新 MCP 114 项、终验绑定 Core 6 项、真实 HTTP 交付 3 项、真实 Core/Engine/MCP/Runtime 阅读账本摘要集成 1 项、Host/评测合同 17 项通过；构建与架构检查通过。源码及二进制身份见[清单](evidence/2026-10-10-bookkeeping-source-manifest.json)。阅读检查点保留 reading: 领域回执；其账本摘要贯通测试不冒充所有工具都会返回 runtime: 类型的直接证明。
- 最新精确改字 5 万 tokens / 10 请求档仍 0/4，四例都在保存前耗尽 token 预算；已知用量依次为 35,374、35,500、35,906、35,338，均 2 请求、未知用量 0。[失败报告](evidence/2026-10-10-bookkeeping-exact-edit.json)。这证明当前全链路尚未达到该效率目标，不能通过减少必要原文阅读或放宽保护标准消绿。
- 首次无来源变化的暂停恢复成功续起同一 Work，并保留中断费用，但最终仅 discussion_ended、没有稿件。记为执行恢复证据，不计创作交付成功。[原始恢复批次](evidence/2026-10-10-bookkeeping-resume-report.json)。已有稿件的修订暂停接续另测。

用户已明确暂缓人工读者评分，先完成工程和模型验收。本记录不要求用户补评分才能继续工程，也不宣称文学质量改善、8 组有效盲评或成本优化已通过。完整 Native、新构建两章/Unicode 与同档精确改字复验运行中，最终结果后续追加。

### 最新补测结果

Unicode/锁定样本补测完成 Work，22 请求、817,005 已知 tokens、未知 0；锁定首尾逐字相同，组合字符 `é`、补充平面汉字 `𠮷` 与 emoji 均保留。结合上一批 7 例，8 个固定样本分别取得结构与领域交付通过的证据，分批运行及首轮启动失败仍保留，不能写为同批 8/8。[补测报告](evidence/2026-10-10-bookkeeping-report.json)

已有稿件的真实暂停接续完成：同一 Work 新 Attempt 继续并保存、直接核验、完成交付，暂停后到恢复前无写入。34 请求、1,248,564 已知 tokens，35,654 未知预留 tokens 保留，不作零费用处理。这证明明确暂停后的修订接续，不等于任意网络断连恢复。[恢复原始报告](evidence/2026-10-10-bookkeeping-resume-seeded-report.json)

同为 25 万 tokens/10 请求档的精确改字 3/4 通过：短全文 6 请求/126,548 tokens，短补丁 5 请求/101,371，长补丁 8 请求/181,602；长全文 9 请求/223,530 后预算耗尽。该批超时上限180秒，旧批为90秒，因此仅预算档位相同，不声称严格同条件效率提升。[完整报告](evidence/2026-10-10-bookkeeping-exact-edit-250k.json)

新构建两章批次仍未完成：60 请求、2,351,336 已知 tokens，两章已保存，仅首章完成核验交付。第二章核验引用第一章 groupId，而当前报告合同仅允许候选稿坐标；笼统 TaskOutputForbidden 导致重复错误和工具搜索。新增具体引用范围诊断及允许段组列表的修复、原场景复验尚待完成。没有为通过增加本批请求上限或删掉跨章连续性要求。

长文全文路径的80万tokens/30请求复验仍未通过：一轮1请求/15,999tokens执行失败且旧报告未保留足够类别；加入白名单Core终态诊断后，独立复验21请求/773,005tokens预算耗尽。后一轮16次read_source_content、5次save_draft_checkpoint，至少2次保存错误，且模型混用patch而未遵守全文对照模式。保留[首次失败](evidence/2026-10-10-bookkeeping-long-full.json)、[诊断复验](evidence/2026-10-10-bookkeeping-long-full-diagnostic.json)，不继续靠增加额度宣称完成。明确的生产可用证据是长文patch路径在25万档完成，全文压力路径与5万档仍未达标。

完整Native编译耗时10分27秒，其后首个authoring测试进程采样仍处于dyld入口、物理内存96KB，尚未进入测试代码；[采样](evidence/2026-10-10-native-authoring-startup-sample.txt)。这仅解释该启动等待，不将未执行套件计为通过，也不修改系统安全设置规避等待。

完整Native在900秒上限终止：编译627秒，运行了authoring空套件，Core套件未取得完成结果；状态timeout，exitCode=-15。[本轮状态](evidence/2026-10-10-bookkeeping-native-all-status.json)。这不是全套通过。最新核验引用诊断的定向HTTP与MCP套件，以及原两章场景仍另行验证。

最新引用范围修复定向HTTP 3项、完整MCP 114项通过；拒绝不写入，纠正后仍可完成交付。架构检查通过，最新[源码/二进制指纹](evidence/2026-10-10-evidence-scope-source-manifest.json)保存。新编译Runtime在启动等待约5分钟后进入运行，随后两章模型复验开始；未修改签名、隔离属性或系统安全配置。

### 两章闭环最终复验

最新两章样本52请求、1,963,332已知tokens、未知0，两个候选稿均保存并核验，generation Work completed / ready_for_review；父讨论Work completed / discussion_ended，未采用、未发布。第二章正文实际到达海关并保有密函，途中左手受伤与第一章相衔接。此段内容核对是模型侧检查，不是真人评分。[完整原始证据](evidence/2026-10-10-evidence-scope-multi-report.json)

模型轨迹出现1项schema纠错；Native事件领取另有2次WorkTaskStale/409冲突，最终事件消费与Work交付完成，保留原始冲突记录。没有新增额度。它证明当前样本完成闭环，不能从单次随机模型运行推导稳定成功率或成本改善，也不能把旧60请求失败改为通过。单章、两章、局部修订、Unicode保护、插话、修改要求与明确暂停恢复分别已有真实交付/控制证据；并发源变更仍正确拒绝旧恢复。

当前仍未通过：5万tokens精确编辑档、长文全文压力路径、完整Native回归（环境启动等待导致900秒超时）；真人质量与8组盲评按用户指示延后。最终完整Host回归已477/477通过。真实用量明细包含所有失败和复验，见[累计账本](evidence/2026-10-10-semantic-bookkeeping-usage.json)，不含资格认证，不等于账单金额。

## 保存后收敛诊断：执行失败但正文正确（2026-10-10）

本次先修验收观测，不修改生产写作协议。失败后停止执行进程，经 Runtime 回读正文、结构与领域 Task；未知不再默认为正文损坏。10项合同测试、包含4个正常样本与1个保存后预算拒绝样本的真实 Core/MCP 集成、架构检查通过。代码身份见 [观测清单](evidence/2026-10-10-readback-source-manifest.json)，[集成日志](evidence/2026-10-10-readback-core-integration-final.log)。整体未通过项仍保留。

真实长文使用原上限30请求/80万tokens/180秒，实际20请求/728552已知tokens/未知0，预算耗尽。**最终候选2406字、指定改字、标题、章节身份、段组结构与锁定内容全部匹配；执行失败，清理前领域Task仍为running。** 不能把正文正确记为任务完成，也不能再把这个失败记为正文损坏。[原始报告](evidence/2026-10-10-readback-long-full-warm.json)、[轨迹分析](evidence/2026-10-10-readback-long-full-analysis.json)。资格验证消耗不包含于该样本receipt。

轨迹：5页冻结原稿→全文保存version2→目录定位新候选→5页新候选→补丁保存version3→目录定位新候选→5页新候选→预算耗尽。共17次来源读取，无完全相同请求/结果，无工具错误，未调用complete_task。后两段读取的锚点分别关联真实保存的version2/version3，**本批不是反复误读冻结旧稿**。初次全文结果未独立逐字回读，不能断言中间补丁没有必要。全文模式混入补丁，不能作为纯全文与补丁的有效效率对照。

代码核查进一步发现：解析阅读按600字符切片，单页条目使用固定3400字节预算，外层响应上限6500字节；savedCheckpoint包含整章body及sceneGroups正文，通用字段遍历会分别展开。对中文长文，部分页只能容纳一个正文切片；同一新候选可能先读聚合body，再读段组中的相同内容。这是确定的表示与分页设计成本，是否必要的文学阅读必须与此分开。下一生产切片应评审基于实际响应字节的分页打包，以及保留完整信息/定位/来源证明的单一正文表示；不能靠增大响应硬上限、删除必要原文或禁止模型核验来消绿。

本次同时保留：首次受控Runtime启动失败、连接资格失效、资格通过后的Runtime启动失败。独立Runtime探测约110ms就绪后同额度复验才进入创作。资格证据写入隔离目录，未更新发布认证。工作区生产二进制已有并行改动，各批实际hash保留，不能把行为差异全部归因于本轮观测代码。人工评分仍按作者要求延后，完整Native与效率目标仍未关闭。

## 无损正文与精确分页切片复验（2026-10-10）

当前工程已落实上一节指出的两项结构修复：初始上下文与保存候选共用Core无损正文投影；解析分页按完整响应实际字节取连续条目，原6500字节上限、Unicode坐标、不可变来源、锁定内容与权限均保留。完整parsed阅读后的跨段精确引文也通过同源覆盖校验，后缀及虚构引文仍拒绝。

| 验证 | 本轮结果 |
| --- | --- |
| Core / MCP | 295/295；117/117 |
| Runtime单元 / 来源HTTP / 候选报告HTTP | 15/15；5/5；4/4 |
| 受控供应商真实Core/Engine/MCP/Runtime | 长短×全文/补丁4例通过，保存后额度拒绝仍正确回读；不是DeepSeek成本实验 |
| 评测合同 / 架构检查 | 11/11；365文件、7crate无诊断 |
| 真实DeepSeek长文补丁 | 8请求、189113已知tokens、未知0；正文保护匹配、执行与领域Task均完成 |
| 真实DeepSeek长文全文 | 4请求、90645已知tokens、未知0；provider_output_limit，未保存，原文哈希未变，失败保留 |

证据：[Core](evidence/2026-10-10-source-paging-core-final-tests.log)、[MCP](evidence/2026-10-10-source-paging-mcp-final-tests.log)、[HTTP](evidence/2026-10-10-source-paging-http-tests.log)、[受控链路](evidence/2026-10-10-source-paging-core-integration-warm.log)、[评测合同](evidence/2026-10-10-source-paging-evaluation-final-tests.log)、[架构](evidence/2026-10-10-source-paging-architecture-final.log)。

两条真实轨迹均只需2页读取原长文，旧批为5页。补丁只调用1次补丁保存，再定位候选、读取候选首页、完成Task；没有工具错误。其8请求/189113tokens与旧补丁8请求/181602tokens相比，不能宣称总成本降低。全文本次在第4次响应失败，未执行保存；不能把90645tokens与旧728552tokens直接对比为成功成本优化。实际出站输出额度及推理/输出拆分未被观察到，不据配置文件猜测实际供应商限制，也不据一个强制全文工具实验推断全部生产Work恢复行为。

[全文原始报告](evidence/2026-10-10-source-paging-long-full-live.json)、[补丁原始报告](evidence/2026-10-10-source-paging-long-patch-live.json)、[脱敏轨迹与本批用量](evidence/2026-10-10-source-paging-live-analysis.json)。仅这两例合计12请求/279758已知tokens/未知0，包含失败，不含受控供应商、资格认证或其他线程费用；不改写此前519请求的历史累计。两例二进制hash一致，工作区仍有并行改动，不能作严格单变量A/B归因。全文报告当时未识别queued，领域状态保留unknown；已修正后续观测枚举，不回填旧报告。

启动证据独立：全文Runtime约231.2秒、MCP约53.2秒才就绪；补丁分别约98.5毫秒、444毫秒。诊断startupTimeoutMs显式设为600000，模型请求/token/180秒限制未改变，生产默认也未改变。原30秒启动失败、dyld入口采样、沙箱监听拒绝和首轮新增测试错误断言均保留；最终对应测试已纠正后重跑。延长诊断启动等待不等于生产启动故障修复。

评测另修复纯模式分母：所有保存尝试都遵守指定模式且至少一次成功，才允许记该模式通过；混用全文与补丁保留真实交付事实，但不算纯模式成功。当前两份原始报告保持不变，分析文件单独核对模式；新增门禁经11项合同测试通过。

结论范围：本切片工程验证通过，补丁实际交付通过，全文压力仍失败。5万token目标、完整Native回归与启动稳定性仍未通过；文学质量未取得真人证据。整体方案尚未完成，不能以页数减少或单例完成替代上述门槛。

## 输出额度与输入成本观测复验

Engine与评测新增数字观测，记录协议编码后的实际额度、载荷组成、按摘要关联的归一化用量；不回传作者正文或推理内容。字节统计不是token估算，观察也不作为第二套账本。当前认证指纹变化导致首轮准入拒绝（没有创作调用），隔离资格重验通过后才进入真实复验；未发布认证。

真实样本：[原报告](evidence/2026-10-10-output-metrics-long-full-qualified.json)、[统计及边界](evidence/2026-10-10-output-metrics-analysis.json)。13请求/408571已知tokens/未知0；所有逐回合总数与持久receipt相等。398324输入、10247输出；输入中370176为归一化缓存计数。指令与工具定义每回合分别31341/17153字节，输入历史随读取与保存累积。高输入占比证明传入量大，不证明全部无效，也不说明缓存输入与未缓存输入费用相同。

本次每回合实际输出上限8192、输出峰值4650，没有输出额度失败。最终独立读回匹配、执行完成、Native Task completed；因为全文保存version2后又补丁保存version3，comparisonModeHonored=false，纯模式验收保持failed。上次输出额度失败仍有效，不能用本次随机成功覆盖。中间候选未独立校验，不能认定补丁多余。

验证：数字观测4项Rust测试通过；Engine完整lib为41通过/2失败；桥接37项通过，追加实际wire匹配验证17项通过（后者包含于前者，非另加17项）；评测合同12项通过；架构365文件/7crate无诊断。完整lib两项失败为当前并行budget策略的null-token计量准入与旧断言冲突，非观测测试失败，本轮未修改预算策略或断言。原测试失败与过滤器误匹配0项日志都保留，不以0项作为通过。

日志：[Engine完整lib](evidence/2026-10-10-output-metrics-rust-tests-final.log)、[桥接](evidence/2026-10-10-output-metrics-bridge-tests.log)、[wire](evidence/2026-10-10-output-metrics-wire-tests.log)、[评测](evidence/2026-10-10-output-metrics-script-tests-final.log)、[架构](evidence/2026-10-10-output-metrics-architecture.log)。本样本用量不含隔离资格验证，不能冒充本轮全部API费用。

新增Engine身份记录后，受控Core/Engine/MCP/Runtime复验再次通过：四个长短/全文补丁案例，以及保存后预算失败仍回读正确的反例；[日志](evidence/2026-10-10-output-metrics-core-integration.log)。这不产生真实模型费用，不替代上面的真实模式失败。

## 保存回执直接导航验收

本轮从Core已有输出事实生成精确不可变来源，MCP回执给出savedSource.read，省去模型查询目录找自身新版本。实际正文读取仍经过原身份/版本/可见性校验；引用注册本身不增加阅读覆盖，前缀权限保持。导航异常单独报告unavailable，已保存事实不被改为写入失败。

真实DeepSeek长文patch：7请求、157420已知tokens、未知0、无工具错误；执行completed、Native Task completed、正文/标题/身份/结构/锁定内容全部匹配，纯patch模式通过。调用顺序为read_context→原稿两页→patch保存→已保存版本正文一页→complete_task，另有最终回复请求。已保存版本的阅读锚点与真实回执revision一致，全程无workspace目录查询。

[原始报告](evidence/2026-10-10-save-navigation-long-patch-live.json)、[轨迹核对和比较边界](evidence/2026-10-10-save-navigation-analysis.json)。前一同类样本8请求/189113tokens，本次请求少1、tokens约少16.8%；前一轨迹额外读了要求来源，且构建不同，不能按严格单变量实验解释全部下降。此为原文保护和工具效率证据，不是文学评分。未进行新资格验证付费调用，此样本用量独立保存，不覆盖此前失败累计。

| 验证 | 结果 |
| --- | --- |
| 完整Core lib | 295/295 |
| 完整MCP | 118/118 |
| 已保存输出HTTP | 2/2，含无目录直接读取及前缀保护 |
| 受控Core/Engine/MCP/Runtime | 四个长短×全文/补丁及保存后额度失败回读均通过 |
| 构建/架构/差异检查 | 通过 |

[Core](evidence/2026-10-10-save-navigation-core-final.log)、[MCP](evidence/2026-10-10-save-navigation-mcp-final.log)、[HTTP](evidence/2026-10-10-save-navigation-http.log)、[受控链路](evidence/2026-10-10-save-navigation-integration-warm.log)、[构建](evidence/2026-10-10-save-navigation-build.log)、[架构](evidence/2026-10-10-save-navigation-architecture.log)。受控供应商现强制读回新候选来验证直接导航，故长文9请求不能与旧测试6请求直接比较。

保留的限制：第一次Core过滤器未匹配、运行0项，不记作通过，随后完整295项通过；首次受控Runtime冷启动30秒失败，预热后才通过。真实复验Runtime启动157.4秒、MCP会话约1.15秒、模型约14.12秒，启动问题未修复。Engine两项预算合同冲突、完整Native验收、5万token档、纯全文压力与文学人评仍未关闭。默认指令/工具schema本轮未删减。

### 可选查询说明按需加载验收

仅MCP read_context.queryPlan.description改为短入口，完整说明从现有准入工具原定义返回，无第二份语法。三类任务测试逐一证明描述以外schema深相等、完整字段指南保留、query_story不变。MCP119/119（2.57秒）、生产MCP构建、受控Core四样本和保存后预算拒绝反例、架构365文件/7crate检查通过。日志统一为evidence/2026-10-10-query-guidance-*；最终只做rustfmt格式整理，未改运行语义。

紧凑MCP六工具数组13613→12200字节；字段1700→287字节；实际provider工具编码17153→15740字节。after文件已统一数组括号/逗号口径，individual tools的bytes不含这些标点。字节不是token。

真实DeepSeek长文补丁：6请求、122937已知tokens、未知0；执行/领域均completed，pure patch成立，0错误/0纠错。独立持久回读正文、标题、章节、组结构和受保护组全部匹配。Runtime启动147ms、MCP486ms、模型12秒；这是温启动样本，不能关闭此前冷启动问题。

实际轨迹read_context→原稿首个content页→locate→patch保存→complete_task；没有指南查询、目录扫描和保存后模型回读。相较上次7请求/157420tokens，阅读策略不同，故只作为完成与正文保护证据，不是单变量效率或文学质量证据。高级查询按需指南仅有合同测试，尚无真实高级查询模型验收；不能将本次声明为全面验收通过。人评延后，5万档/纯全文/完整Native/Engine预算合同仍未闭合。原始报告、分析、源码与实际运行二进制身份均保留。

### 同条件阅读、目录一致性与预算合同验收

新增独立实验合同completeOriginalAndFinal：同一冻结来源的完整parsed游标链必须在保存前送达，最终保存来源的完整游标链必须在完成前送达。locate、目录、缺页、来源/内容hash混合、保存后的原文阅读不能冒充满足。报告将执行、正文、领域完成与实验义务分开；少读仍可能已保存正确，但不进入配对分母。普通生产流程不因此被强制成固定阅读步骤。配对核对原稿摘要、实际准入目录、同一模型/预算/运行构建与脚本身份，失败、缺失、重复、未知用量或源码变化不给效率比值。

第一批真实四例（旧六工具实验目录）：两次补丁分别9请求/213398与215302tokens，原稿与最终候选完整送达、全部正文保护检查通过、领域完成；一次全文9请求/225260tokens耗尽下一次请求预留额度，正文2356字而预期2406字，另一全文1请求/15738tokens在发现后403。两对均无效，所有费用保留，不把未进入后续请求算零成本成功。该批工具目录冲突是验收脚本设计缺陷，不能直接解释为生产模型无权限。

修正实验目录为MCP实际generateChapter准入17工具后，受控provider使用生产portable discovery规则并强制发现query_story，继续执行成功；Core本地工具与子任务仍不在Engine允许列表，Native授权范围不变。新的真实一对：指定全文样本9请求/211121tokens，实际只调用patch保存（不是先全文再补丁），完整阅读、正文及领域完成均通过但指定模式失败；补丁9请求/213590tokens，全部通过。仍无有效纯全文/补丁成本配对，不再付费重复尝试以追求绿色。两批工具范围不同，不能互相充当单变量A/B对照。

本轮6个付费样本总46请求、1094409已知tokens、未知0，无新增资格调用。成功的三次补丁完整阅读成本稳定在约21.3–21.5万tokens；这不能外推总体成功率或文学质量。此前12.3万样本未完整阅读，不能作为同条件改善幅度。当前实际输入成本包含必要原文、工具说明和历史重放，响应归一化缓存细项约占成功补丁输入的九成，输入比例不能直接当搜索浪费或按未缓存价格收费。

评测合同16项、实际Core集成1项通过，合计正式test:tool-evaluation入口17/17；集成包含四正例、保存后预算拒绝、正文正确且领域已完成但未读最终稿仍验收失败，以及真实目录发现。firstSaveAccepted与firstAttemptSucceeded已分开，后者要求完整验收通过、仅一次保存、无工具错误；上述原始真实报告不回填，旧firstAttempt字段只代表首存接受。字节分类用于原稿/候选/其他来源、工具指南、目录、保存回执等载荷观测，不冒充供应商逐类别token统计。

Engine原41/43故障已按现行B06合同复现并修订测试：有token上限继续拒绝未知用量后的新请求；计量模式允许新收费请求，未知ID重放拒绝、已知ID幂等、请求/时限限制和终态未知账单均保留。生产准入代码未改。Rust44/44、JS持久/内存与进程终止结算8/8通过；首轮JS沙箱监听拒绝日志保留，授权监听后通过。初次Engine命令错误manifest路径在编译前失败，修正命令后先复现原两项失败再验证修复。

证据统一在evidence/2026-10-10-comparable-*，总分析见comparable-analysis.json；两批原始报告保留。Native全量与加载环境诊断单独记录，不能以以上局部通过声称整体完成。

#### 短稿负载补充（不替代长文失败）

为区分固定协议成本与长文分页成本，另外执行短稿的一对同条件样本，仍为每例10请求/25万tokens/180秒，不重试长文。全文与补丁各6请求、123649/122288tokens；完整原稿/最终候选、正确模式、正文保护、领域完成均通过，无工具错误，原稿/准入目录/构建稳定一致，形成1个有效短稿配对。补丁总token仅低约1.1%，单对且额外资料阅读不同，不作稳定收益或代码优化A/B结论。这说明短稿固定协议和轮次成本明显，不能期待仅把全文载荷换成补丁就解决总成本。

包括该补充后，本轮共8个付费样本58请求、1340346已知tokens、未知0；此前6例累计为阶段小计，原报告不覆盖。所有长文失败仍保留，有效长文配对0。实际两批成功补丁没有工具指南/能力搜索调用，主要为原稿、候选和少量其他来源读取；关于“绝大多数token用于工具文档搜索”的强结论不获支持。原始报告comparable-short-live.json和总分析comparable-analysis.json已保存。

最终报告口径：关闭执行资源后再冻结工具轨迹与阅读判定；首存接受以Native saved=true完成回执为准，与端到端首次成功分开。顶层用量汇总包含失败账单及未知保留额，缺失/溢出不填零。最终test:tool-evaluation 17/17，证据comparable-evaluation-final-receipt.log；静态边界最终复核无诊断。原始真实报告不按后续代码语义回填，所有真实模型费用仍由原账单证明。


### 最终 Native 回归与设计修复验收

本轮最终源码在隔离Linux环境完成全部106个测试程序，1019/1019通过，失败、忽略、过滤均为0，没有重试。Core lib为296/296；其余含MCP及Core/HTTP集成。源码清单698个Rust/Cargo输入在构建前后SHA一致。使用本机官方Rust 1.98.1 arm64镜像、只读源码与registry挂载、网络关闭、独立构建目录；test profile无调试符号，但opt-level=0、debug assertions与overflow checks开启。按Cargo实际artifact清单执行，四程序并发、各四线程，每程序300秒超时。三个doc目标均0项、退出0，不计入1019。

首轮完整Linux为988通过/30失败。合同同步后曾出现1013/6、1017/2等中间失败，全部原始日志保留；文件名带final的阶段报告若passed=false仍是失败，最终通过依据是[verified报告](evidence/2026-10-10-comparable-linux-verified.json)。没有删除测试、忽略用例、降低作者验收标准或放宽权限。源码、环境、构建和执行脚本身份见[最终清单](evidence/2026-10-10-comparable-manifest.json)。

生产设计修复两项：完整决策响应按实际字节分页，合法最大验收标准不再因外层新增元数据越界；预算耗尽在状态、决策和幂等控制回执中准确保留，不误报输入漂移。新增分类对未拨款Work采用可选账本观察，原可读性及拒绝继续执行反例保留。Host只映射Native拒绝原因，命令传输7/7通过；正式工具评测17/17、Engine Core44/44、Engine预算JS8/8均通过。最终架构检查365文件/7crate、诊断0，相关diff空白检查通过。这些计数各有独立范围，不相加冒充独立案例总数。

macOS默认profile全量仍未完成：本轮已记录测试启动及过程宏加载阶段长时间等待，停止的运行不记通过；未关闭系统保护、重签产物或提高生产超时。Linux通过不代表macOS启动问题已解决，也不代表当前macOS应用已部署最新Native构建。真实模型8样本/58请求/1340346tokens的成本基线先于上述Native修复，保留原指纹；后续结构修复由本次确定性完整回归验证，未再付费复跑。5万token目标、有效长稿纯全文对照和人工文学质量评审仍开放。


### 启动跟进与单次回执账本投影

唯一生产源文件变化为work_decision_status.rs。账本每次投影读取一次，以taskId索引失败观测；仍逐任务校验并核对项目，无跨请求缓存。新增同一27回执fixture逐项诊断核对、后续账本更新可见、损坏账本拒绝；分页无遗漏/重复及陈旧游标反例保留。macOS Core296/296、决策HTTP12/12通过；架构365文件/7crate无诊断。未再运行完整Linux1019用例，该历史结果仍绑定原清单。

孤立fixture20次投影耗时573.197→491.488ms。全套并发测试中955.764ms也保留在Core日志，不能挑单个值宣称普遍提速；结构上整本账本加载解析次数由每次投影27次降至1次，是可确定结果。无模型请求，不能声称token降低。

macOS启动证据：现有产物约151/80/79ms；首次重新构建后Runtime2393/98/98ms、MCP664/2/2ms，26项启动合同通过。新Core测试再次在_dyld_start等待后自行恢复，说明间歇故障仍存在。后续最终Runtime的真实Host首次20秒超时无输出（失败保留）；同一SHA产物独立诊断91ms监听、health200，温启动Host1/1通过。不能据温启动通过认定冷启动已修复；未提高生产超时、修改系统保护、清理或重签产物。

日志和指纹见[receipt-ledger-manifest](evidence/2026-10-10-receipt-ledger-manifest.json)，启动基线/新构建/采样为startup-followup-*；可复现的临时诊断脚本保存在证据目录，仅创建随机隔离数据库和本地进程，并清理自有测试资料。实际桌面应用没有重新发布或重启，模型成本/文学质量等未完成项保持。


### 创作基础提示同源与真实模型对照

产品engine-prompt.md取代20903字节上游编程提示（新2791字节），开发/发布/认证/探针同步；发布合同验证正文逐字相同，Core实际供应商请求验证全文送达。39/39连接与发布测试、17/17工具评测、1/1实际schema传输，Host类型与架构检查通过。初次本地合同运行存在沙箱监听EPERM；授权监听后通过。初次出站断言仅查instructions字段失败，实际协议允许放入消息内容；检查完整请求的精确文本后通过，未放宽生产协议、权限或改用简化Core模拟。

旧资格证据在新提示下真实拒绝，0案例发起。新资格首次执行被自动审批阻止，没有发生该调用；作者随后明确授权新提示、现有执行规则和合成材料外发DeepSeek，并规定协议≤4请求（每次512输出tokens/20秒）、Core≤16请求/20万tokens/60秒、两例验收各10请求/25万tokens/180秒。按该范围实际资格通过text/structuredToolCalls/streaming/usage/toolLoop；无假造或资格旁路。

基线短稿passed：6请求121686tokens；新版短稿passed：6请求99066tokens。原稿、准入目录、Native/Engine/Codex二进制、预算和完整阅读一致，两个运行buildStable=true，构成1个有效短稿配对，减少18.6%，仅为单对。长稿基线failed：9请求210915tokens，持久正文与完整阅读、领域completed但执行因预算预留失败；新版长稿passed：10请求200102tokens，全部交付/阅读检查通过。保留失败，长稿成功成本比值为null。两例新版均无工具错误。实际instructions字节31341→12936，tools15740与首轮input24735不变；不把字节降低当作等比例token/价格降低。

四例改字累计31请求631769tokens、未知0。资格费用另列：2次协议请求，仅一条479tokens用量被本次观察器完整提取；当前Core资格返回不导出预算快照，流式协议另一条亦未提取到。资格总用量为null，绝不填零或宣称全轮费用就是631769。所有原始报告与代码摘要见[creative-base-analysis](evidence/2026-10-10-creative-base-analysis.json)。未重复付费追求补齐统计；该计量导出缺口作为后续工程项。

这是提示装配与执行合同验收，不是文学质量评审；没有减少原文阅读来换成本，没有达到5万token目标，没有修复macOS间歇加载，也没有重新部署/重启作者应用。当前连接资格受新提示摘要约束，安装后必须按正常验证流程取得该上下文的资格。
