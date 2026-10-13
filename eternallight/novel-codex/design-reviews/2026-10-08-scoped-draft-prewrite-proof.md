# 局部稿写前拒绝证明与原Work接力

## 实际故障与责任

current11原QA作品同对话仅输入一次“改写第2章最后一段，让悬念更清楚；其余文字不变。只保存工作稿，不采用、不发布。”实际模型登记selectedScenes范围并冻结检查标准，写手自行保存了新稿。原第一次save_draft_checkpoint被TaskOutputForbidden拒绝、操作仍accepted；第二次同Task保存成功并completed。两个attemptSettled事件pending，Native依同Work unknownWrite门禁拒绝接力。Host门禁正确，不能绕过。当前1146字与写手自报约890字不一致，新稿尚无独立报告或交付，不能判通过。

只读基线及故障 `/tmp/autonomous-v3-current11-local-revision-baseline.json`、`...-after-interruption.json`；完整原请求由相同CODEX_HOME和CODEX_SQLITE_HOME的公开Core thread/read读取，hash与原Native记录吻合。技术证据 `/tmp/autonomous-v3-current11-draft-call-public-evidence2.json`、`...-draft-body-alignment-evidence.json`：首请求body-only，1197字符，尾空白1字符；第二请求sceneGroups-only，其group body与首body精确相同，MCP导出的canonical body为1196字符。第一请求的body与scoped validator要求的trim/grouped body不等，确定性拒绝却没有核定原call。诊断材料和工具错误本身不是Native判决权威。

责任为Native候选写事务中确定性拒绝结账覆盖不完整，非模型未主动检查。写手实际长度误报仍由独立检查和领域交付处理；验收者不得替它修稿、弱化1100上限或改为通过。与current10不同：本次Task已completed且有另一原call产物；原仅failed/cancelled、零产物、错CAS的核定证明不能套用。

## 最小方案与跨层合同

B03/B04/B06/B10。Native拥有写入和拒绝事实；Host仅完整历史取证并重新冻结状态；模型自行组织复核、修稿及交付。Codex Core、Engine、Host文学流程及UI不新增逻辑、关键词路由、作者技术表单或额外批准。

1. Native既有Immediate事务、原幂等回执重放之后、validate_task_write的零领域写阶段，为constructor-owned的确定性拒绝生成精确原operation拒绝证明；不能笼统将Storage/Corrupt或提交结果不明变成rejected，不能提交包含部分领域写入的事务。原成功回执仍先重放，任务/领域/预算不被拒绝结账改变。
2. 场景body规范化使用已有无副作用算法的单一实现；若抽取，共享源只包含纯字符串/JSON转换，不依赖MCP/Runtime/存储/凭据，不复制一套历史constructor或工具私有digest。MCP、Runtime与Native当前使用保持一致，不改正文文学内容或作者输入。
3. 沿既有Author-only POST核定原完整调用。在原hash、project/task/thread/turn/call/tool/inputRevision/idempotencyRef均正确、Task terminal且写权限撤销、两类原key回执均不存在后，增加一个可独立证明的冻结scoped body alignment拒绝条件。须验证selectedScenes冻结来源、目标对象、scope完整，writer_scene_limit无前缀合并，非delegated，原body-only且原body无法等于当前确定性canonical grouped body。该条件与后来head/其他call产物无关；不能仅凭completed、后一次成功、缺报告或工具错误清账。completed有输出必须是内部一致真实输出，矛盾/损坏仍保留unknown。
4. 新证明仅改精确原operation的状态和更新时间；不删记录、不补正文/报告、复活Task、修改Work/预算或恢复写权限。合法body、其他无法证明的payload、运行中/无sealed、已有任何原key回执及矛盾链仍unknown。核定后Host沿原consumer把原持久事件交回模型，不重复作者输入、不造新预算、不重写已经保存的稿。

## 五视角前审

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | Native唯一判决；复用既有Author核定、Host取证/围栏和事件consumer；无第二创作循环或Core业务代码。 |
| 技术A | APPROVE WITH CONDITIONS | 拒绝结账仅在确认零领域写阶段；明确确定性错误集合，Storage/Corrupt未知；首查原key双回执、不可变范围证明及终态输出一致性。 |
| 技术B | APPROVE WITH CONDITIONS | 同源canonical算法，无Runtime/MCP私有构造复制；真实Router/SQLite复现首拒绝、后成功、complete及原事件接力；负例不得清账。 |
| 产品 | APPROVE WITH CONDITIONS | 原稿和作者输入保持；恢复后由应用实际独立检查发现1146字，不放宽标准或外部修稿；以最终交付证据验收。 |
| UI/UX | APPROVE | 原待检查/保存状态准确，沿原入口恢复；不增加技术操作或重复用户消息。 |

Root综合为有条件实施；独立scoped_save_proof_review五视角前审为APPROVE WITH CONDITIONS，已确认后才开始生产改动。采用显式确定性错误白名单及std-only的已校验字符串序列canonical纯函数；不泛化整个save_candidate Result，不构造历史envelope，不解除既有CAS分支零产物约束。零兼容PASS：无迁移、批量回填、旧字段fallback、版本识别或授权扩张，本次是公开合同对精确原请求的事实核定。

## 原故障对应验证

| 故障/边界 | 有意义的验证 | 应用再验 |
| --- | --- | --- |
| scoped确定性拒绝留accepted | 同真实Task/source，body-only尾空白拒绝为rejected；精确其他scoped保护拒绝；同Task纠正sceneGroups后保存和complete，事件不因旧拒绝被unknownWrite挡住。 | 原首call获Native证明，记录保留；原Work接回实际模型。 |
| completed有其他call产物不能核定 | 原hash/identity/key、frozen scope/source、body alignment、双回执及内部输出一致；只原call状态变化；重复核定不变。 | 正文版本/摘要、作者、费用完全不变。 |
| 错误材料/真实未知 | missing/type/hash/identity/foreign/oversize、running/unsealed、completed零输出矛盾、合法body或scope、任意原key回执、prefix merge/delegated、Storage/Corrupt/写证明失败回滚继续unknown。 | 不改未知账本、不绕过暂停取消或门禁。 |
| 写手字数误报 | 独立报告与最终领域字数/冻结标准，旧报告不认证新稿。 | 模型自行处理实际1146字，不给技术或剧情答案；必要修稿仍只原末段。 |

Root将冻结实际源码和产物，运行Native构建、受影响完整HTTP目标、Core/Host受影响回归及architecture:check；没有新生产改动的范围不机械重复。保留首故障及所有验证失败，不以静态审查/测试数代替应用闭环。

## 实施后

独立后审五视角静态审核通过，无P0/P1阻断。实现限定在9个Native/Shared源码文件：前写确定性拒绝白名单、同源canonical字符串函数、原调用的冻结局部范围证明与completed输出一致性校验。Codex Core CLI、Engine、Host、Renderer未改。

6项新增HTTP回归包含真实单group尾换行拒绝/纠正、原accepted加其他已保存输出的核定，以及权限、回执、身份、存储与损坏边界；后审另补captured input/source head不一致和completed输出parent链矛盾负例，验证全部事实不变。最终测试源码已单独冻结，尚待统一运行。

首轮Native构建与architecture:check通过（332文件、7 crates、0 diagnostics），Core266/266、两完整恢复HTTP目标各7/7、最终完整scoped HTTP11/11、Host三完整恢复文件53/53、MCP保存合同模块4/4及摘要1/1实际通过。首轮scoped9/11中的两处测试构造失败（候选DTO字段和非法修改冻结target）已用当前DTO和真实Author注册修正；Host两次25秒超时保留，隔离分阶段链和原完整文件重跑通过，但历史超时精确原因未知，不改timeout。

随后真实重启仍未核定原call。发现新增frozen_scoped_base要求旧source raw body精确canonical过严：真实source1151字符，合法保留尾换行1，source.body.trim与group canonical相同。当前Runtime的wholeChapter/body-only保存本就允许此形式，不是历史兼容。独立五视角增量前后审通过后，仅source比较增加trim；新selectedScenes输出仍raw body精确canonical。HTTP历史正例改为真实Author保存带newline源稿，另加trim后仍不等的实质矛盾负例。增量Native构建、architecture及Core266/266已过，三个完整HTTP目标正在重跑。

原QA数据库、任务和稿件均保留。current11局部修改实际闭环仍未通过，须由应用自行独立检查当前稿并完成交付，不能用静态审核替代。首轮及增量冻结证据分别为 `/tmp/autonomous-v3-current11-pre-recovery-build-proof.json`、`/tmp/autonomous-v3-current11-source-contract-freeze.json`，实际状态为 `...-after-actual-recovery-startup.json`，均保留原失败。

## 同一幂等键的不同请求：增量审查

合法source增量构建、architecture、Core266/266、三个完整HTTP目标25/25均通过（`/tmp/autonomous-v3-current11-final-source-contract-build-proof.json`）。再次真实启动仍accepted；只读元数据精确确认，原首次body-only与后续sceneGroups纠正调用复用了同一key，因此原入口的any receipt门禁在正文证明以前返回未知。两类main回执均为后一次真实成功版本，同digest和snapshot。此前无receipt的测试不能认证此实际条件；不能删除回执检查或仅凭后成功判原拒绝。

独立五视角增量前审APPROVE WITH CONDITIONS后实施：将Runtime现有四字段draft请求身份纯函数提取至Shared，Runtime与Native共用；区别该Domain digest与MCP operation hash，不复制正文envelope、使用当前head或做数值fallback。当前MCP先host_json归一化，body-only payload原样送Runtime，Native证据应沿这一合同计算原请求身份。

仅既有completed、撤权、完整原调用、冻结局部范围拒绝证明及真实输出一致性分支，可以进一步核验同key回执是否明确属于另一请求。两表全部匹配行须恰为main的一对、同digest/snapshot，candidate projectRevision等于冻结input；真实commit/snapshot/member关系一致，候选版本属于本Task真实输出；同Task/thread/tool/key的另一committed call精确指向该版本。两类Domain digest均必须不同于原完整body-only请求。相同digest、任意foreign branch、缺pair、矛盾、成功operation不绑定及unsupported条件仍unknown；旧CAS分支仍any receipt即阻断。

真实回归新增同key“admission原body-only→纠正场景save成功→completed→精确核定原call”，并保留matching、pair与绑定反例，全事实除原verdict/time不变、重复幂等。当前实施与统一验证进行中，原真实任务仍未闭环，作者输入、稿件及费用均未改写。

同key生产增量独立后审PASS；静态后审曾拦截测试的两个match arm放错位置，已移回正确samekey测试并逐项确认13个edge/arm均对应，不以未编译测试冒称通过。最终测试SHA `aeb1f729…07b3`，冻结 `/tmp/autonomous-v3-current11-same-key-final-test-freeze.json`。同key增量Native构建、architecture及Core266/266实际通过，三个完整HTTP目标继续统一验证；保留先前所有失败和未通过的实机状态。

最终统一HTTP为7+7+13=27/27实际exit0。构建/源码/二进制身份见 `/tmp/autonomous-v3-current11-final-same-key-build-proof.json`。2026-10-09沿原profile实际启动后，原operation-0bda…已由Native精确核定rejected，原两持久事件自动消费、原Work接回真实DeepSeek，没有追加作者消息、手动重试或人工修稿。该拒绝核定和接力实际通过。

实际后续独立critic报告绑定当前1146字新稿，三项supported，篇幅unmet并登记问题；模型自行安排定向修订。新writer零保存产物、TaskOutputMissing，领域Task failed而实际provider结算charge completed，Work再次blocked；本Work无交付评审。当前0accepted，但UI pendingRecovery，正在单独诊断，不将新故障归给本次已核定的旧call。证据 `/tmp/autonomous-v3-current11-after-same-key-startup-01.json`、`...-repair-blocked-02.json`。完整局部创作交付仍未通过，原失败与费用保留。
