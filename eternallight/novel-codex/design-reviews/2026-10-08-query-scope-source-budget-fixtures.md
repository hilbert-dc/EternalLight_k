# 查询范围与按需原文的当前验收合同

日期：2026-10-08。五项为评审视角，不代表五位独立人员。

## 原失败和确证

冻结 8 `query_scope_tests` 默认全量实际 1/3；原日志 `/tmp/autonomous-v3-native-group1-current8/query_scope_tests.stdout.log` 保留。rolling-plan case 原 227 行把 relevantText 引用当数组；modeling case 原 305 行期望 60000-byte context 拒绝，但实际返回 200、22343 bytes，原文在 source 引用中而非模型单项内。

`native/story-core/src/task_sources.rs:158–247` 缓存当前正常可见 builder 已准入的原文，把 relevantText、workingKnowledge、modelingBatch、modeling、entityCatalog 转成 frozen source 引用；该引用没有读取证明。`:267–308` raw source HTTP 读取必须对应当前 Task/thread/revision/source，复制精确 cursor，连续 UTF-8 每页最多2000 bytes、完整页6500 bytes，并持久化 root 原文返回区间。`native/story-runtime/src/task_source_api.rs:42–106` 再执行绑定与9000模型单项tokens门禁。`server.rs:2088–2129` 在 source 投影后执行实际模型单项和输入 byteBudget 门禁；因此原大文本内联失败条件不再成立，必须验证实际大原文被完整保留且每页受限。

## 最小方案与验收

仅独占 `native/story-runtime/tests/query_scope_tests.rs` 和本独立评审；生产、shared helpers、主实施记录冻结，无构建或外部供应商调用。

- rolling scope=0 必须真实为空，无正文 source；scope=2 必须通过全 raw source 回读精确第1、2章原文，不能只检测引用。workingKnowledge 同样回读，再验证所有 sourceChapter < from。原 queryScope、报告全对象、watermark、后章搜索和作者视角拒绝保留。
- modeling 必须实际回读 current revision-3 完整大原文、batch、catalog 与 modeling 投影，对 exact revision/object/内容/sceneGroups 作强比较，排除 revision-4；每页连续/hash/长度与9000tokens受限。
- 60000预算的当前 source 目录可以成功，但内容未内联、目录不表示完成阅读；完整投影如整体内联超过模型单项门限，证明大原文不能悄然减少来适配预算。当前目录与每页都必须符合实际门限。
- 原16000输入预算仍明确422拒绝；拒绝后重读原冻结 source，完整内容/hash/ID不变。缩小 query 仍可读同一真实完整原文，不能缩减 frozen source；真实报告查询、预算 byteSize、queryScope和watermark继续精确比较。

## 实施前五视角

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | Native 来源和范围权威；目录/原文/理解分开，模型按需阅读，不以引用代替交付。 |
| 技术 A | APPROVE WITH CONDITIONS | 真正读取连续全部页、精确hash/source/revision，当前 budget 拒绝保留；后章和错误视角不泄露。 |
| 技术 B | APPROVE WITH CONDITIONS | 保留原1/3，不能只改200或删大文本测试；当前目录和原文完整性以及门限各有断言。 |
| 产品 | APPROVE | 查询能给出正确范围内完整材料，预算失败不被误报为阅读完成。 |
| UI/UX | APPROVE | 仅测试合同，未新增技术界面或用户审批步骤。 |

零兼容门禁 PASS：无 fallback、迁移、双轨、预算/超时提升或权限扩大。综合允许当前 fixture 同步，实际全量3/3由主任务集中构建后提供。

## 实施后复核

当前源码增量已完成并冻结：`query_scope_tests.rs:99–203` 新 local helper 执行实际绑定 HTTP source 读取，复制 cursor、拒绝重复页、连续全部 UTF-8 payload、总 bytes 与完整 SHA256 比较，检查每页2000/6500-byte和实际9000tokens模型单项限制。原文 sceneGroups 全对象匹配不可变 revision_content，重组后精确等于原完整 body，至少2000字，不仅看source引用。

rolling case `:314–355` 保留空起点真实无正文 source，其他起点回读全部第1至from-1章与 workingKnowledge，严格执行之前章节范围。modeling case `:421–585` 实际回读四份完整原文投影，精确批次、revision-3 内容与无 revision-4；缩小 query 后四份 source ID/hash/bytes 和全内容保持。60000目录和每页的模型单项≤9000；完整大来源重新组合为一个内联项必须>9000，从而保留原大文本门限目的。16000-byte 输入仍明确422/context_budget_exceeded，并在拒绝后重新完整读取原冻结正文。原 queryReport 全对象、queryScope、watermark、byteSize、非授权视角与取消 task 查询拒绝均保留。

| 视角 | 后审结论 | 证据与边界 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | Native 目录不被当作原文；预算改变不替代或缩减冻结 source，当前二进制待集中验收。 |
| 技术 A | APPROVE WITH CONDITIONS | 完整hash/精确revision/连续分页与受限模型项、实际输入拒绝都有强断言；待当前默认3项实际终态。 |
| 技术 B | APPROVE WITH CONDITIONS | 原1/3和完整日志保留，无过滤或泛状态替换；不是只把422改200。 |
| 产品 | APPROVE | 允许大原文按需交付，预算失败与未读材料仍明确。 |
| UI/UX | APPROVE | 无界面或用户理解成本增量。 |

rustfmt 限定当前文件 actual exit 0；限定文件和本评审 `git diff --check` actual exit 0。未构建或宣称当前3/3通过。零兼容复查 PASS，无生产限额、权限、超时或OS修改。剩余项仅主任务集中重建及完整实际执行。
