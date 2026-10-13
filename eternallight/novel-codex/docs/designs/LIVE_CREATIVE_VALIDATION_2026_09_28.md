# 真实模型创作链验证记录

2026-09-28；状态：局部改稿通过，改稿后建模未通过，联合旅程未验收。

## 环境与边界

- macOS 26.5.1，工作树基于 `c950ae3247d7830f8358ee54f7cf70c2f353a07f`，含大量未提交修改；不是该提交本身的验证结果。
- 当前 native 源码使用 `cargo build --manifest-path native/Cargo.toml --bins --locked --offline` 构建成功。
- 连接为工程目录中的 `deepseek-flash`，协议 `responses`，使用项目专用钥匙串凭据；未保存对话中新提供的密钥。
- 真实链路：生产任务提示构造函数 → CodexAppServerAdapter → ModelBridge → 真实模型 → novel_workspace MCP → Rust Runtime → SQLite。
- 临时合成作品、独立 CODEX_HOME、独立数据库；未修改实际小说、正式认证或共享 Codex 配置。
- 绕过了 UI、自然语言任务路由与自动建模调度。作者种子正文、任务目标、范围和建模任务由诊断脚本明确创建，不属于模型自动规划证据。
- 脚本为临时诊断，不是已交付的跨平台常规验收命令。产物路径可能被系统清理，下述结果不替代长期可复现的验收工具。

## 已核实结果

### 模型基础链

`node scripts/qualify-model.mjs deepseek-flash --profile base-tools-v1` 通过，未发布认证。
真实协议探测涵盖 text、streaming、structuredToolCalls、usage；核心集成证明工具选择、执行、结果消费与根回合完成。
这不证明多 Agent、文学质量或小说联合旅程通过。

报告：`.certification-reports/2026-09-28T11-32-08.069Z-50763087e1f94ffc1fa3224dda2acb748d68e4b447a116285dadba78442ccf3d.json`。

### C03 局部改稿切片

合成原稿含三个场景：`林夕走进港口。`、`她烧掉密函。`、`白塔熄灭。`；末场景锁定。
作者要求只将中间场景烧信改为藏信，使用冻结 `selectedScenes` 范围。

在 11:38:15–11:38:33 UTC 的单独成功运行中：

- 模型生成 `她把密函藏起来。`，前后两个场景的完整对象保持相等，场景数量仍为 3。
- 候选从 `draft-4ee3e3b07c154c3ab14bf0bca6d252a9` 更新到 `draft-985fca76070148448e7dd9acf1a111d6`。
- `save_draft_checkpoint` 和 `complete_task` 的事务回执均为 `committed`。
- `task-9f14f03b443e4dccbc9b9632135aeddd` 的 Runtime 状态为 `completed`，阶段及交付状态均为 `ready_for_review`。
- accepted head 仍为空；没有把候选自动采用。

原始合成作品报告：`/private/tmp/novel-live-revision.wogLsO/2f507cb82c3018dd/report.json`。
此结果证明一次精确范围改稿及持久化，不证明自然语言授权识别、长篇质量或跨轮接续。

## 失败记录与限制

1. 首次诊断脚本错误地将 `composeNovelCodexPrompt` 返回的消息数组再次包入 text 字段，App Server 拒绝 `sequence, expected a string`。已修正脚本为直接传数组；这是诊断调用错误，未修改生产协议。
2. 改稿与建模共享 120000 token / 10 请求上限的运行，改稿成功，建模被 token 预算门禁拦截。原始记录位于临时目录 `0eb482178a44d558/report.json`。
3. 在新临时作品中明确调整到 300000 token / 10 请求后，改稿仍成功，建模在请求上限耗尽前未提交。原始记录位于临时目录 `8513a6028826e3c4/report.json`。

第三次运行数据库核验：

- 改稿任务 `task-63d1cf814cd948babe37061fa005fa0a` 已完成，候选保存与任务完成回执已提交。
- 建模任务 `task-e80dd95ea1614f3792a2d08f7defc650` 和批次 `modeling-3e65bb24415a420e89e1394660d40cd2` 仍为 queued，实体和断言为空。
- 工具记录显示建模 Agent 成功读取新版 Context，并检索到 `她藏起密函。`，来源为新版 `draft-dd1324ff747449a1ae928e4cc4c3cb35`；没有 `publish_knowledge_batch` 成功回执。
- 最终预算快照：requests=10、tokens=136751、unknownTokens=111439、reservedTokens=0、overrun=false。这是引擎预算账本，不是供应商账单；未知用量不能解释成实际收费或成功生成量，未取得实际费用。

已停止重复增加预算。仅凭这些记录不能将根因归为模型能力、协议失败或生产预算过小；需进一步关联请求重试、流结束、用量结算和工具可见性。

## 下一步与验收状态

优先补充每阶段请求/失败/用量的有界诊断，定位建模未提交的原因，再验证知识发布及后续任务消费新来源。
不得通过宿主代填模型输出、伪造能力证据、无限重试或自动采用来使测试通过。
C03/C04/C05 完整旅程、自动编排、UI、作者质量、实际费用、长篇及 Windows 仍未验收；S1–S5 整体目标保持未完成。

## 后续定位：预算耗尽之前的真实失败

同样 10 请求/300000 token 上限，增加现有 ModelBridge `responseObserver` 的脱敏
错误分类后，记录在临时目录 `78e43261a81bcee6/report.json`。没有记录凭据或推理正文。

- 改稿四次模型响应均完成：读上下文、保存、完成任务、回复。
- 建模读上下文后，调用资源发现，再提交 `publish_knowledge_batch`。
- 实际提交的 entities/assertions 元素是 JSON **字符串**，且字符串内部 evidence
  是数组；Runtime 返回 `entity candidate must be an object`，未写入候选知识。
- 随后桥接层连续报告 `arguments for publish_knowledge_batch violate the declared schema`，
  错误路径包含 `/assertions/0`、`/assertions/1`、`/assertions/2`、`/entities/0`；
  最后才耗尽请求预算。旧运行的 Codex 日志也显示流中断自动重试。
- MCP 当前声明 entities/assertions 只有 `type: array`，没有 `items` 契约。
  这是已确认的工程契约缺口；必须补齐对象结构与证据形状，并验证整个 Codex 工具
  schema 传递链。不能把运行时要求藏在自然语言里，也不能通过解析字符串补兼容。
- 本记录没有证明桥接层每次失败对应模型哪个改正参数，也未证明供应商有无 schema
  转换；需补有界参数形状观测和回归，不能仅凭上述错误归咎模型能力。

因此下一步优先是知识工具 schema 与可恢复校验反馈，而非再扩预算。

## 创建作品并发修复（实施者自检，非独立评审）

新增诊断运行曾在首个创建作品请求返回 500 `database is locked`，尚未执行创作
请求。原始记录 `fff8e7384b92fe5d/report.json`。创建事务先查重再写入；使用 deferred
事务时，其他连接已持有写锁可导致读锁升级立即失败，而不是按 busy_timeout 等待。

方案：仅将 `create_project` 改为 immediate 写事务，在查重和快照检查前取得写许可，
沿用原 5 秒 busy_timeout、唯一性校验和原子提交。不增加重试、schema、迁移或兼容层。
回归通过第二连接持有 `BEGIN IMMEDIATE`，启动创建线程，确认其等待写锁释放后
返回完整作品并从新连接读到 revision=0。修改前该用例实际失败为
`Ok(Err(Storage("database is locked")))`，故不是静态实现断言。

此修复仅覆盖创建作品的锁升级问题，不宣称所有仓储写事务或后台并发已完成审计。

修复后 `project_creation_concurrency_tests` 与 `sqlite_repository_tests` 共 10 项通过，
story-core 全目标严格 Clippy 通过。未改动 Codex 内核和模型适配代码；建模 schema
修复、相应回归及真实模型复验尚未实施。

## 知识契约修复后的复验

已实施 [知识建模工具契约修复](KNOWLEDGE_TOOL_CONTRACT_DESIGN.md)。首次仅改 schema
的真实运行在临时目录 `b3ce85cb236fd8e9/report.json`：改稿完成，建模三次发出对象
候选，不再是字符串数组；没有此前 schema 断流，requests=10、tokens=240091、
unknownTokens=0。候选 evidence 含来源等多余字段而被 Runtime 拒绝，故仍未完成。
已进一步修正提示歧义并提供具体的证据形状错误，原校验未放宽。

随后 `b956d72fd9306fe1/report.json` 在诊断脚本等待 Runtime 启动的 15 秒门槛超时，
未发起创作请求，不能归为模型建模失败。诊断改为 60 秒启动等待并采集 stderr，
不增加模型请求或 token 上限。失败记录保留，不因重试而删除。

### 最新真实成功切片（12:21:40–12:22:40 UTC）

修正 schema、提示与证据形状反馈后，`8afee58e90cb80c4/report.json` 的真实模型
运行通过改稿和建模提交检查。数据库只读复核结果：

- 改稿任务 `task-8cc4fd54a4d0490596bd3d3ff6cc6a41` completed，锁定与非目标场景保持不变。
- 建模任务 `task-c1a214214747452fbad2cfb7f40d0b10` completed。
- 批次 `modeling-5b0db82fc250427783a46ad423650918` ready，绑定新稿
  `draft-e9ce4a986208454d9e95c2db43c8c2e1`，持久化 7 个实体、3 个事件候选。
- 藏信事件原文为 `她藏起密函。`，Unicode 范围 [14,20)，并非旧版烧信。
- `publish_knowledge_batch` 与 `complete_task` 成功回执均 committed；未采用正文或确认 Canon。
- requests=10、tokens=233809、unknownTokens=0、overrun=false；仍是引擎预算用量而非实际费用。

限制：首次未成功的知识提交仍在操作账本保留一条 accepted、无 receipt_ref 的记录，
虽然之后修正提交和任务完成已有 committed 回执。需进一步处理失败操作的恢复终态，
不能把整个账本视为已完全对账。此成功仅证明本合成样本的指定范围改稿、新稿建模和
任务完成；自动调度、后续生成消费、UI、文学质量、费用与双平台均未由此验收。

后续实现已补上 [知识载荷确定性拒绝](KNOWLEDGE_REJECTION_RECOVERY_DESIGN.md)：
当前代码将纯载荷校验阶段的 InvalidContent 在同事务记录为 rejected，宿主可明确
终结该失败操作。15 项定向回归及严格 Clippy 通过；历史临时库不做迁移或回填，
其他未明确证明未提交的错误仍保守保留待核对，真实自动创作流程尚未再次验收。

## 关联建模与下一轮对话复验

独立新库 `533b0c127fb00160/report.json` 使用当前原生二进制，通过真实模型的
局部改稿、Runtime 后续动作查询、绑定 sourceTaskId 的建模以及新线程只读问答。
这是无 UI 的诊断脚本，不是桌面自动调度的完整验收。

- 新稿 `draft-5e6d8a218e624738af74ccee05ff1e97` 将烧信改为
  `她把密函藏了起来。`；脚本深比较非目标及锁定场景保持不变。
- Runtime 返回关联原改稿任务的 modelKnowledge 动作；批次
  `modeling-051189648ff84897953f59a72949faf6` 完成后，原任务 next-step 返回 action=null。
- 建模 publish/complete 两条账本操作均 committed。本轮没有触发失败重提，
  因此不能用此运行证明 rejected 分支；该分支依据前述确定性回归。
- 下一线程实际调用 read_context，工具输出 relevantText 包含新稿和准确来源 ID；
  operationContext.modeling 包含同一 ready/candidate 批次。回复准确引用藏信句子
  并区分候选与正式事实；问答前后章节对象深比较相同。
- 重要边界：该 read_context 的 workingKnowledge=null，模型未调用知识查询工具。
  本轮证明新正文和建模来源元数据可被后续对话读取，不证明实体/事件候选已参与推理。
- 共 10 次创作模型请求，预算记账 tokens=201865、unknownTokens=0、overrun=false；
  上限为 16 次/400000 tokens。该数字不是供应商账单；独立连通性探针不计入此数。

仍需验证实体/事件查询消费、桌面自动任务生命周期、作者语义许可闭包、续写反馈、
真实 UI、长篇规模、文学质量及 Windows。不能由这一个合成短章推导 S1–S5 已完成。
没有保存或使用对话中新粘贴的密钥；验证使用工程既有独立 Keychain 凭据。

后续补充 [改稿后结构化知识查询验证](REMODELED_QUERY_VALIDATION.md)：16 项
Router/SQLite 定向测试通过，补证显式 query_story 的新来源读取与候选边界，
不改变上述真实模型尚未主动查询结构化知识的结论。

## 自主知识查询复验：被完成工具契约阻断

独立新库 `2450928693129461/report.json` 将第三阶段改为核对知识索引，
并使用生产 taskBoundInstructions，不指定查询工具或计划。但执行在第二阶段
modelKnowledge 收尾失败，第三阶段未启动，不能声称自主知识消费验证通过。

已观察到：改稿完成；建模阶段主动调用 query_story 的正文检索/分页并成功保存
7 个实体、3 个事件候选。随后 bridge 多次报
`arguments for complete_task violate the declared schema: invalid value at /operation`。
一次重新 publish 被 Runtime 拒绝为 IdempotencyConflict，没有覆盖已保存候选。
最终预算拒绝，13 次请求、tokens=180228、unknownTokens=195951，上限 400000，
overrun=false。unknownTokens 是未知用量保守预留，不是供应商计费断言。

实施者自检方案：complete_task 的处理器只消费任务绑定和 summary，但 schema
混入公用属性池，包含仅用于 request_candidate_task 的 operation 枚举。
收紧 complete_task 为其 required 参数集合；不增加 modelKnowledge 枚举来迁就
无效字段，不更改 Runtime 完成验证，不引入兼容逻辑或额外模型预算。
Responses codec 与真实 MCP tools/list 联合测试补充合法完成调用的传递验证。

局限：收紧 schema 消除了此处误导性的声明，不证明模型永远不会输出无效参数；
协议校验错误如何以可恢复工具反馈交还模型、避免传输重试消耗，仍需独立审计。
本轮修复之后的真实模型成功尚未证明，不能覆盖或删除这次失败记录。

修复验证：11 项 MCP 测试、1 项真实 MCP/Responses codec 联合测试通过，
story-mcp 全目标严格 Clippy 通过。未增加模型请求预算，未采用测试候选为正式作品。

## 收紧工具契约后的自主查询复验

`ba39153c3076d0d7/report.json`：局部改稿及关联建模成功，第三阶段 advisory
实际调用 query_story，而非只读 Context Pack。模型解析到 lin-xi 与 secret-letter，
候选身份及证据均来自新稿 `draft-ca413ab2da2b4564882be0ad7150fb79`；随后 state
查询取得 evt-hide-letter 藏信事件，evidenceRef 指向 `她藏起密函。` [14,20)，
confirmationState=candidate，世界真值/因果认证均未被提升为 true。

这提供真实模型主动读取结构化知识的证据，但整轮仍失败：模型先尝试 20000、
12000 字节查询预算，Runtime 返回泛化错误；有效查询部分裁剪后又尝试 3000，
再次失败。最终达到 16 次请求上限，没有最终回答；tokens=320632、unknownTokens=0、
overrun=false。不能把“成功读取事件”改写为“端到端问答已通过”。

修正方案：不改变 4000–9000 的合法预算范围、不增加模型额度；Runtime 对越界
预算返回具体上下界和缩小查询步骤/结果数的建议，工具字段说明区分查询预算与
Context Pack 预算。Router 回归重放 3000/12000/20000 三次错误，再用合法请求
继续同一任务，验证反馈可用且不破坏任务。无迁移或兼容路径。

query_plan_tests 6 项通过；story-core/story-mcp 全目标严格 Clippy 通过。
此反馈修复后的真实问答尚未重跑。

## 新稿建模与自主查询：真实通过切片（13:20–13:21 UTC）

独立合成作品 `526ac3262db87728/report.json` 用最新 Runtime/MCP 和既定
16 次请求、400000 token 预算跑完：局部改稿、Runtime 后续建模动作、绑定原
任务的建模、下一轮只读 advisory 问答。诊断脚本核验受保护场景与非目标场景
完全相同、候选稿未被采用、保存及建模回执 committed、原任务后续动作清空，
问答前后章节对象相同。无桌面 UI 或用户正式作品参与。

新正文修订 `draft-e07742c82ea74ed284bb908da4ebf5ea` 将烧信改为藏信。
批次 `modeling-5340fff2611049fc8e0b16c1685b143d` 为 ready/candidate。
新对话实际调用 4 次 query_story，结果中藏信事件的 predicate=`藏起密函`，
sourceRevisionId 为该新稿，evidenceRef 的原文是 `她藏起密函。`，
confirmationState=candidate。回答引用实体、事件、无关系边和当前查询覆盖，明确
没有将候选提升为 Canon。15 次模型请求，tokens=297035，unknownTokens=30732，
overrun=false；这仅是引擎预算记账，不是供应商账单。脚本在检测到候选事件
证据来自新稿之前不会标记成功。

人工复核发现一个回答质量问题：模型把 `林夕 密函` 的 text 查询空结果解释为
“索引未建/未命中”。该检索实现要求一个连续的 Unicode 子串，正文没有含空格的
`林夕 密函`。因此本次证据只证明这一查询没有连续字串命中，不证明索引缺失。
同时模型正确声明 state 的 responseTruncated/requeryRequired，未把片段覆盖
声称为全局语义完整。已在检索 coverage 中增加 matchMode=contiguousSubstring，
工具说明要求多词分开查询，并补真实 Router 反例回归。这项改进尚未进行再次
真实模型验证；不能倒填为本次模型回答已完全正确。

验证补充：query_plan_tests 6/6 通过，其中现行 Router/SQLite 回归明确比较含空格
查询和单词查询的结果及新稿来源；story-core、story-mcp、story-runtime 全目标
严格 Clippy 与 native/Codex 仓库格式检查通过。未在改进匹配说明后再次调用
真实模型，因此这项回答质量风险仍需复验。

该切片证明 C03/C04 的局部改稿→知识更新→下轮结构化消费可以在合成短章上
连贯执行；不证明自然语言任务路由、桌面自动调度、知识就绪前的真实依赖阻断、
跨章传播、作者采纳/撤销、恢复、质量或双平台门槛。S1–S5 仍未总体验收。

## 匹配说明后的真实复验受建模错误阻断

独立合成作品 `875f1b2cd7a813c7/report.json` 于 13:31–13:34 UTC
执行。局部改稿与 Runtime 后续动作成功，但建模阶段两次
publish_knowledge_batch 均被 Runtime 拒绝，未产生 ready 批次或完成回执；
本轮没有进入 advisory，不能证明匹配说明改变了模型的空结果解释。
实际错误是事件断言缺少 evidence 对象，第二次修正还省略了
preconditions/effects/untilExclusive。旧反馈将多种形状问题合并为
“event requires typed identity, participants, interval…”；响应流也发生一次
未完成和一次非法 JSON 工具参数。诊断超时（180 秒），其间桥接预算记账
12 次请求、tokens=250077、unknownTokens=92495、overrun=false；这不是
供应商账单。失败的提交未记作成功候选，原局部改稿仍保留。

已在领域校验中将“缺少原文证据”和“缺少事件 value 的四个字段”分别给出
可执行的形状反馈，不放宽来源校验、事件身份或效果权限；新增相应回归。
此项反馈尚未真实模型复验。流式故障与非法工具参数仍属
[引擎参数恢复方案](../../../agent-engine/TOOL_VALIDATION_RECOVERY_DESIGN.md)
的未实现范围，不能由领域报错修复推断解决。

深入核对工具链：原始 publish_knowledge_batch schema 为 6229 字节，超过 Codex
5000 字节工具 schema 压缩门槛，深层证据字段可能被压缩。已用共享 `$defs` 缩至
4812 字节，保留校验结构；真实 MCP/Responses codec 测试确认缺 evidence 被拒绝。
领域单元 3 项、Router 纠错用例 1 项、MCP 11 项、联合编解码 1 项通过；
story-core/story-mcp/story-runtime 严格 Clippy 通过。压缩机制与现场现象相符，
此时尚未捕获真实桥接请求中的已注册 schema，也未完成真实模型复验，故不能
据此确认整条路径持续可靠。详见
[工具参数恢复设计](../../../agent-engine/TOOL_VALIDATION_RECOVERY_DESIGN.md)。

## 独立任务预算下的真实模型复验（14:00 UTC）

先前一次 `1bece3ea84bb8a26/report.json` 已完成改稿、发布建模批次并在下一轮
实际查询四次知识索引；但诊断脚本把三个独立任务合用 16 次请求/400000 token
预算，问答生成最终回答前第 17 次请求被拒绝。生产 `sendAgentTurn` 每次启动
新的 Engine Run，按该 Run 的 lease 创建预算，因此这个全局限制并非生产
任务预算的归属方式。那次运行不能记为端到端通过。

修正诊断脚本，仅把预算分成改稿、建模、问答三个明确上限（分别为
8/180000、10/260000、10/220000；请求/token）。独立合成作品
`3ede371502b6dfce/report.json` 的真实 DeepSeek→Codex→MCP→Story Runtime 链路
通过 `passed-linked-modeling-and-next-dialogue`：

- 改稿仅将指定场景的烧信改为藏信，非目标与锁定场景原样保留；形成新候选
  `draft-5e799cd550c64afaaf5d1d4ebfb79da2`，未自动正式采用。
- 后续建模发布 `modeling-11950af6a9e141c2a3b83fdb2ec87ea6`，
  `publish_knowledge_batch` 和 `complete_task` 均有 committed 回执；
  未将候选事件提升为 Canon。
- 下一轮只读问答主动调用四次 `query_story`，回答引用新修订、实体原文锚点、
  `藏匿密函` 事件、关系与状态的空覆盖，明确所有这些是候选主张，并说明
  截断重查及语义完整性仍未知；问答前后章节对象相同。
- 实际预算记账：改稿 5 次/101063 token，建模 5 次/119007 token，问答
  6 次/132744 token；三个阶段 unknownTokens 均为 0、未超支。这是引擎
  记账而非供应商账单；密钥未写入项目或验证报告。

另外用实际 Codex App Server、相同 MCP 配置和本机模拟模型端点截取首个
`/responses` 请求，未向外部发作品或密钥。脱敏结果在
`/private/tmp/novel-live-revision.wogLsO/schema-22f827e4283b83b8/result.json`：
转发后的 `publish_knowledge_batch` schema 为 4283 字节，顶层必填字段仍含
`entities/assertions`，`$defs.E.required` 仍为 `quote/start/end`，事件 value
仍要求 `participants/preconditions/effects/untilExclusive`。这证明该实际
Codex 转发路径没有抹掉上述结构约束；模拟端点未执行任何模型推理。

真实短章运行与模拟转发探针共同证明当前工具契约可支持一次完整的改稿→
建模→下轮查询回答，且关键 schema 必填项到达模型端点。一次成功不能证实
模型稳定性，也不能单凭结果证明先前失败唯一由 schema 压缩引起；仍需
多样本/失败恢复验证。自然语言路由、桌面交互、长篇跨章影响、正式采纳/撤销
与 S1–S5 整体验收仍未完成。

为避免上游 Codex 更新后再次丢失嵌套必填项，已将本机探针转为
`electron/tests/codex-tool-schema.test.mjs`，接入默认 `test:agent`，并提供
`npm run test:codex-tool-schema` 单独入口。测试用 fixture 资格证据生成独立
Codex 配置，启动真实 App Server/MCP，将首个供应商请求送至本机模拟端点，
断言模型可见 schema 保留原文证据与事件值字段；不调用真实模型，也不使用
任何开发密钥。本机该项 1/1、Agent 集合 160/160 通过（0 skip）。

## 2026-09-29 小样本模型资格复验

经作者明确授权，用当前项目独立的 `deepseek-flash` 连接执行三次非发布
`qualify:model`；新测试凭据只放在被 Git 忽略、权限 `0600` 的 `.env.local`，
诊断报告不含凭据。本节与 9 月 28 日的历史环境说明分别记录，不覆盖旧结果。

- `text-v1` 通过：协议探测与 Codex 核心集成均通过。
- `base-tools-v1` 通过：结构化工具调用、工具结果消费及根回合完成得到证据。
- `directed-delegation-v1` 未通过：`childSpawn`、`childMessageRunning`、
  `childDirectedRoute` 通过；`wait_agent` 调用完成，但 `child-terminal-result-observed`
  失败。三个阶段本身均结束，不能将此归为请求超时或未创建子 Agent。

对应本地报告分别为
`.certification-reports/2026-09-28T23-45-17.351Z-242899bd8afa90cd8639d737dcb302a7da532b464edcc728416c2ee2d1113cc3.json`、
`.certification-reports/2026-09-28T23-45-46.149Z-84e90ec80a5c786274cb9557f3f41ddefc424ccd0ae67f811de86b5719731638.json`、
`.certification-reports/2026-09-28T23-46-44.173Z-a2e19acc593758d15c85eaeb5df46602c7a8f8e2d39386596e5fcc0bf059623b.json`。
多 Agent 失败目前只能定位为“等待工具完成，但未取得可验证的同一子 Agent
终态正文”；现有报告没有足够内容判断是模型未按约定输出、工具事件遗漏，
还是终态关联条件过严。未发布完整能力认证，也未继续付费重试。
针对这个证据缺口，认证器已增加仅含布尔值的等待证据分解：区分等待完成、
可信子任务关联、子任务完成、终态文本摘要、完成事件及父任务消费；不记录
子任务正文或凭据，原来的严格通过判定不变。离线 Agent Engine 182 项、
Rust 工作区测试和严格 Clippy 通过。随后以获授权的小样本真实供应商调用
复验一次，非发布报告为
`.certification-reports/2026-09-28T23-56-27.342Z-9b07cbfa8e916835d2402684de1e9e701b5bbbf6c00e0aeb4b3c46ca95fdc1b9.json`：
等待完成、可信子任务关联、子任务响应完成均为真；终态文本摘要匹配、子任务
完成事件、父任务等待结果匹配及父任务消息匹配均为假。定向委派仍未通过。
这进一步限定了问题范围，但不能单凭该报告断定是模型限制还是工程事件关联缺陷；
没有发布完整能力认证。
