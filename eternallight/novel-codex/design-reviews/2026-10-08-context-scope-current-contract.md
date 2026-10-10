# 当前上下文范围与作者验收解释 fixture 同步

日期：2026-10-08，仅 `native/story-runtime/tests/context_scope_tests.rs`；无生产/授权/模型额度/schema变更，不build。原冻结8 7/12 失败完整日志 `/tmp/autonomous-v3-native-group2-current8/context_scope_tests.log` 保留。

## 方案与当前验收

三个用例把 taskSources 引用当旧 eager 正文/knowledge/plan：改用真实当前 Task/root/capability 的 parsedFields 全部分页、source/hash、UTF8 range/entry连续完整、完整JSON重建hash，每页≤6500/实际escaped MCP≤9000。scene-prefix实际源仍只含第一场景，隐藏后场景/未来正文不可出现；原写入越界拒绝、范围时间/视角/跨任务游标拒绝保持。

同Task read_context 保留初始输入，不能为了旧断言改成latest。写入后初始sourceId、hash与完整原文必须不变；Native新 workspace 实际 savedCheckpoint/nativeSavedCheckpoint 目录定位真实 producerTaskId/outputOrdinal/revision，完整回读已保存可见新前缀并排除未开放后场景。目录/历史产物不证明采用或交付。

80前章情景仍真实生成101章：当前冻结来源最多80章（生产固定构造上限，不是旧UI budget24正文窗口），实际分页原文必须含当前81章、排除82–101章；另完整 workspace 目录必须真实枚举1–81全部对象，81章实际全文回读。目录与context有界，全部原author 101章范围、水位/hash/源保护断言保持，missingScope加入当前 taskSourceContents并保留原三项。writer搜索保持metadata-only、未来正文/未开放规划负例。

两项generateChapter提案按Native当前必需目标 chapterAcceptance：从实际返回 Task.inputManifest.authorIntent 原文建立稳定author-goal要求，原话未指定长度才null bounds。原intent/预算不替换；提案仍proposed，作者审批、原计划位置冻结、child剥除字段/作者改选/重复幂等/跨scope全部原拒绝保持；完整action.target equality加入真实当前字段。

## 五职责实施前审查

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 初始比较输入与真实当前产物分开；引用同一个Native revision库，无latest/fallback |
| 技术A | APPROVE WITH CONDITIONS | 实际内容全页/hash/完整范围，原视角/未来/作者/线程/CAS负例保持；9000模型额度不放宽 |
| 技术B | APPROVE WITH CONDITIONS | 全12用例实测，原失败留存；不filter/skip，不build，Root统一编译 |
| 产品 | APPROVE WITH CONDITIONS | 作者简单输入保留，解释不是授权/原要求替代；真实原文可读与可信保存可区分 |
| UI/UX | APPROVE WITH CONDITIONS | 无新UI/技术ID要求，不把目录或历史产物当完成/文学质量 |

综合 APPROVE WITH CONDITIONS；零兼容门禁 PASS，无迁移/回填/旧格式fallback/双轨/安全降级，兼容例外无。

## 实施后复核

待Root当前完整12用例编译/退出。Native workspace由另一代理独占实现和测试；本fixture不得用SQL伪造关系或先宣称该新入口已通过。

## current10b 实测后的增量预审

完整12用例实际9/12，三个旧 shape 断言期待 `/relevantText.content.body`，当前来源构造直接投影完整获准 sceneGroups，省略冗余 body；完整 hash/分页已通过后在字段 equality 失败。唯一当前合同同步：明确要求 body 字段不存在，完整 sceneGroups 为数组且每个 body 是实际字符串，按 Native 场景分隔符重建全部已读获准正文，再严格比较原预计全文；原 chapter/revision/80章全部范围断言不改。taskWorkspace immutable revision 的真实 `content.body` 断言保留，不新增 body/sceneGroups fallback，也不从 Author/DB 补正文。

五视角增量：架构 APPROVE（只同步原始源当前投影，不改初始选择）；技术A APPROVE（所有场景及哈希强证据，无字段fallback）；技术B APPROVE WITH CONDITIONS（12项完整重跑，旧9/12保留）；产品 APPROVE（用户仍读取实际获准全文）；UI/UX APPROVE（无界面变化）。零兼容 PASS。后审为静态合同核对，真实终态待 Root。

## current10d 实测后的增量预审

完整12项实际10/12。省略调用方 POV 时 Runtime 合法从冻结 Task 导出原 position，应精确等于已获准 position/queryScope/source；显式错误 POV 或 storyTime 仍须拒绝。第二项 workspace 请求401不能直接当成漏 capability：当前 call 已携带真实 CAPABILITY。补明确 source GET 的当前 tool/root/caller lineage，并在进入目录前从实际 Author endpoints 强核 Task/Work/原线程/输入版本仍活动；若仍拒绝保留失败和真实状态，不延长期限、不放宽 scope/author/capability。五视角：架构/技术A/产品/UI/UX APPROVE，技术B APPROVE WITH CONDITIONS（完整重跑与401真实定位）；零兼容 PASS，静态复核不冒称失败已消失。

## current10f 完整实测后的增量预审

完整12项实际11/12，80章 Task 原文分页、81项 workspace 范围和完整 hash 已到达并通过。最后旧 Author unbound writer 请求 `budget=24&byteBudget=60000` 实际返回422 `context_model_item_budget_exceeded`，不是 Task source 错误。原完整日志 `/tmp/autonomous-v3-native-full-current10f-nodebug.stdout.log` 保留。

保留这一原大请求为精确422负例，验证只有 error、不交付章节文本；另以同一 Author、同一101章 reading scope、`budget=8&byteBudget=20000` 获取当前合法较小正例。正例保留原 modeling 非空/实际裁剪/当前101章 source 不丢失、全部101章扫描范围、未完成 coverage 与原水位断言，并新增实际 escaped 模型条目≤9000、真实返回≤20000 bytes、完整 contentHash/id、与 Author planner 相同源水位和 position 校核。不增加模型额度、Task权限、业务期限或生产兼容分支。

五视角预审：架构 APPROVE（预算只是可选材料选择，不改变初始 Task源或作者范围）；技术A APPROVE（原拒绝负例与较小正例并存，完整 hash/范围/Candidate Task 原强断言保持）；技术B APPROVE WITH CONDITIONS（统一完整12项重跑，不能把静态拟定较小预算当已通过）；产品 APPROVE（超大上下文可明确失败并以按需读取恢复）；UI/UX APPROVE（无UI变化，不将失败解释成已阅读）。零兼容 PASS；五视角全部通过或附统一验收条件后才实施，仅 fixture。

增量实施后：上述两个请求均已写入，原101章/80章 Task 全文与 workspace 范围校验不改。原超大请求要求422、精确错误码和仅error返回，全部101个正文章节标记不可出现；较小请求要求非空 modeling、真实裁剪与当前 source、9000模型额度和20000字节额度、完整hash、真实源水位和冻结的原 Task 断言。rustfmt 与 `git diff --check` 实际退出0，未构建或运行。五视角后审：架构/技术A/产品/UI/UX APPROVE，技术B APPROVE WITH CONDITIONS（Root统一完整12项实际运行）；零兼容 PASS。11/12是补丁前实测，当前补丁通过情况尚未知。

## current10g 实测后的确定性额度预审

完整12项仍11/12，但原24/60000请求本轮合法200、JSON实际18060字节。真实 opaque ID 分词使9000边缘结果变化，不能以请求参数推断已超限，也不增加按结果接受200/422的兼容分支。历史 current10f 的422和 current10g 的200均保留。

将误设必拒绝的短正文请求替换为同一真实Author API另建的单章项目：真实保存16000个“霾”字的48000字节正文，先用当前生产Tokenizer强证正文自身超过9000tokens，真实正文存储 receipt 严格等值，目标正文投影仍低于原60000字节预算。该必需目标不能被可选裁剪删除，unbound writer 精确422 `context_model_item_budget_exceeded`、仅error/no正文交付。原101章 positive 保留较小8/20000请求、9000真实模型条目/完整hash/源水位/范围/裁剪全强断言，额外负例不会改变其数据或费用。使用固定字符和实际token count而不是猜测UUID影响，无生产修改。

五视角预审：架构 APPROVE（区分字节预算与真实模型token额度）；技术A APPROVE（真实存储和当前Tokenizer确定性超限，原60000/9000均不变、无status fallback）；技术B APPROVE WITH CONDITIONS（Root统一全12项，未执行不能说确定性负例已通过）；产品 APPROVE（拒绝时不能当作阅读全文）；UI/UX APPROVE（无UI变动）。零兼容 PASS，按此单一当前合同实施。

增量实施后：另一个实际项目和48000字节目标正文已使用Author当前保存 API，实际Tokenizer超过9000及目录投影字节门槛先独立强校核，然后精确检查9000 gate 的422无正文。原101章数据、所有当前positive/hash/range断言不改。没有按 opaque ID 接受可变状态的分支。rustfmt、`git diff --check` 实际退出0；五视角后审架构/技术A/产品/UI/UX APPROVE，技术B APPROVE WITH CONDITIONS（Root统一完整12项）；零兼容 PASS。current10g 11/12是补丁前结果，新负例与8/20000正例仍待实际执行。

## current12 实测后的较小可读请求预审

实际完整12项11/12；本次已通过16000“霾”正文的Tokenizer、真实保存和精确无正文422负例，在后续101章 `budget=8&byteBudget=20000` 正例第2251行被最终9000模型条目 gate 拒绝。原 stdout/stderr `/tmp/autonomous-v3-three-targets-current12-nodebug/context_scope_tests.*.log` 保留。

只读定位当前生产：unbound queryScope为null，`writer_source_watermarks` 只有完整范围 digest/计数而非101 ID数组；当前必需目标是短101章正文。`context_pack` 先裁剪 optional 内容到字节/Token预算，再计算真实 contentHash/id/byteSize；最终 Runtime 再核9000额度。前者使用64个“0”的占位hash，真实 opaque ID/hash的分词变化可能使贴近额度的结果在最终检查拒绝；原日志只有拒绝，不宣称已经量得该包的精确token差额。没有发现101章必需metadata本身超过9000的证据。

唯一 fixture 同步将这一个正例请求 `byteBudget=20000` 缩到 `10000`，entry8/readingChapter101/sceneOrder1/同Author不变；Native仍先移除非目标draft，再从旧到新裁剪modeling，保留当前目标。正例必须真实200、返回≤10000 bytes且实际escaped条目≤9000、完整hash/id、水位与101完整scanned范围相同、modeling非空且有实际裁剪并保留101 source。真实80章Task和81对象workspace、原文全文/范围/hash强证明完全保留；不接受200/422分支，不改Native限额、权限或期限。若10KB仍失败或modeling为空，将保留实际失败，不能据静态估计放行。

五视角预审：架构 APPROVE（可选材料的请求额度更小，目标范围与源权威不变）；技术A APPROVE（保持9000拒绝门禁、完整正确返回才通过，非‘引用存在’弱化）；技术B APPROVE WITH CONDITIONS（Root唯一构建并完整12项验收，生产路径观察不是实测）；产品 APPROVE（优先获得真正可读的当前上下文，全文仍按需回读）；UI/UX APPROVE（无产品UI或重复作者输入）。零兼容 PASS，按唯一较小请求实施。

增量实施后：仅一个正例URL和对应byteLimit/真实size断言改为10000，原全部scope/源水位/非空modeling/实际裁剪/完整hash、80章Task源与81对象workspace、超限正文精确拒绝断言保持。rustfmt与scoped `git diff --check` 实际exit0；未构建未运行。五视角后审架构/技术A/产品/UI/UX APPROVE；技术B APPROVE WITH CONDITIONS（Root完整12项当前真实运行必须通过，不以静态结果宣布成功）。零兼容 PASS，current12仍是补丁前11/12，新10KB正例未认证。

## 唯一 finalizer 冻结后的原规模验收预审

Root 已在旧生产实现上实跑确定性单项：0 pass / 1 fail、actual exit101，仍可裁剪的前文令最终真实包达到9008 tokens。原完整日志 `/tmp/autonomous-v3-finalization-old-regression.stdout.log` 保留。这里只据该确定性反例确认算法缺陷，不把 current12 的单条422归因当成另一份完整机制证明，也不把10KB请求当成修复通过。

生产负责人已将三处统一收尾静态冻结，manifest `/tmp/autonomous-v3-context-finalization-production-static-frozen-20261008.json`；独立只读复核 `context_budget`、`context_pack` 和 `server`：每轮先更新计数、真实字节数、SHA/id再核模型额度，bound完整可见来源先缓存，角色、冻结目标/来源、原9000 gate、字节额度和最终当前性/水位校核保持。该静态结论不能代替新实现实际测试。

本次仅撤回较小请求的临时验收规模，将101章正例 URL、预期byteLimit及真实返回size上限三处恢复为 `budget=8&byteBudget=20000`。保留完整真实模型条目≤9000、独立完整hash/id、101章扫描/原源水位、modeling非空与实际裁剪/当前101章来源、80章Task全文及81对象workspace原断言。真实16000个“霾”的48000字节必需正文仍须精确422、只error且不交付正文；不修改生产、权限、作者要求、业务期限或其他测试目标。

五视角增量预审：架构 APPROVE（唯一Runtime finalizer已静态冻结，fixture恢复原可选规模，不另建预算或流程）；技术A APPROVE（所有正文/范围/hash强证明与真实必需超限负例保持，原9000和20KB额度不放宽）；技术B APPROVE WITH CONDITIONS（仅三处当前断言同步，Root唯一构建并完整运行当前12项和统一回归，旧失败保留）；产品 APPROVE（不能靠作者调小请求掩盖可选资料误拒绝）；UI/UX APPROVE（无新UI或技术参数要求，不声明实机创作闭环）。零兼容门禁 PASS，无fallback、迁移、双轨或兼容例外；按此最小增量实施。

最小增量实施后：原101章请求和两处byteLimit/真实size断言已恢复20KB，其余测试断言保持。scoped rustfmt 与 `git diff --check` 实际exit0；未构建、未重跑测试、未发模型请求。五视角静态后审：架构/技术A/产品/UI/UX APPROVE，技术B APPROVE WITH CONDITIONS（Root统一当前完整回归，必须真实200并满足全部强断言）；零兼容 PASS。本段静态冻结时12项最新实测仍为修复前11/12，不以源码冻结或历史其他组通过声明当前12/12。

## final-direct13 当前实际验收

Root 已统一构建并实际运行完整四个目标，无filter、skip或ignored：Runtime lib 15/15、authoring 12/12、context 12/12、reports 4/4，共43/43，各进程actual exit0。证据 `/tmp/autonomous-v3-final-direct13/manifest.json` 和 `/tmp/autonomous-v3-final-direct13/context_scope_tests.stdout.log`；本fixture源码前后均为 `9cee7518274bd8a87971471d9ecfe22269dc9d61528eb9ecfdfacff9b1c6a621`，对应恢复原20KB后的冻结版本，执行二进制前后SHA相同。使用当前test profile：opt_level=0、debug_assertions=true、overflow_checks=true，仅debuginfo=0。

本组12/12完整到达并通过原101章 `budget=8&byteBudget=20000` 真实200正例：实际模型条目≤9000、真实字节数≤20000、完整hash/id、101章范围与原水位、modeling非空及实际裁剪并保留当前来源。80章Task实际全文/版本/hash与81对象workspace范围断言全部保持并通过；另真实48000字节必需目标的Tokenizer超限、精确422和无正文交付负例通过。修复前 current10f/current10g/current12 11/12及旧9008-token确定性失败日志均保留，10KB临时规模没有用于认证本次修复。

五视角实际后审：架构/技术A/技术B/产品/UI/UX均 APPROVE，范围仅本上下文修复和上述当前完整直接回归；零兼容 PASS。当前完整Native workspace和原Node201仍由Root运行，终态尚未取得；此43项工程测试未调用外部模型，不代表真实DeepSeek创作、文学质量、前100章或整个3.0实机闭环通过。
