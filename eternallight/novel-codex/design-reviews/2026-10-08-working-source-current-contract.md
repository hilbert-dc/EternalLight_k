# 当前来源分页与小说范围 fixture 同步评审

日期：2026-10-08。影响仅 `native/story-runtime/tests/working_source_tests.rs` 与 `commitment_extraction_http_tests.rs`；不改生产、共享fixture transport、授权门禁或当前schema，不构建二进制。按五个职责视角审查，不冒称五名独立评审者。

## 原失败与当前合同

冻结8的原已编译 `working_source_tests` 实际exit101、0/3：task-bound `read_context`的relevantText/narrativeGoals/modeling改为来源引用，旧断言直接把引用当正文；日志 `/tmp/autonomous-v3-native-group2-current8/working_source_tests.log`。原测试及日志保留为失败证据，不据同步fixture宣称已通过。

当前 Native `project_task_sources`只在构造器认可的路径登记资料，替换为sourceId；`task_workspace`提供有界目录及真实选定revision引用。真实 `task-source-content` 使用当前Task/capability/源身份，返回UTF-8有界parsedFields与递增cursor；原生记录完整返回覆盖和依赖，索引不能冒充原文读取。Raw源保留真实DTO，body与sceneGroups可以是合法的两种表示，不修改原始资料去迎合旧context形状。

第三项旧的“超大mandatory源read_context必须422、context内全文仅出现一次并省略body”是有意被3.0替代的运输合同。当前应证明目录及每个实际来源页有界、全文可完整读回、真实场景结构与原文不截断、版本/改选失效及候选隔离不变。两种合法DTO表示不能被误称为已经去重或删掉原文；解析后只组合真实sceneGroups进行场景断言。

冻结8的 `commitment_extraction_http_tests` 实际exit101、0/1，line408把未存在foreign-project的请求预设200/null，当前Native `require_novel_project`拒绝没有当前novel edition/main branch的作品，返回403/novel_scope_required。合法同作品的unknown report/source/章节依然可以返回200/null。该修正只拆开两种负例，不补伪edition或放宽scope检查。

## 实施与退出门槛

- 当前task来源先实际读取目录/引用，使用`task-source-content`从首段逐页读取，核对sourceId/hash、coverage连续完整、字符range、返回范围及每页实际模型额度。仅fixture重建JSON字段，不从数据库或author正文绕过来源读取。
- 保留已选历史draft与brief的真实revision、workingSelectionRevision、全文/场景结构、sourceWatermarks、writer checkpoint的工作基准更新与作者改选后的迟到写入拒绝。
- 长文110与500倍均通过实际分页完整读回，context与页保持模型额度；candidate原文与未采用状态保持。原大源422预期明确被当前分页合同替代，不过滤用例或跳过大样本。
- 承诺提案保留全部候选授权、错章节、unknown revision、历史report、幂等、完整替换、晚写拒绝与作者独占采用负例；未存在foreign-project必须403/novel_scope_required。
- Root统一编译当前fixture并运行全部3+1项；本任务不自行build或把原二进制的失败改记通过。

## 五视角实施前评审

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 只同步当前来源运输合同；真实版本和Native存储不变；无旧eager分支、生产去重或第二读库 |
| 技术A | APPROVE WITH CONDITIONS | 当前Task/capability的实际分页，连续range及hash，完整原文/场景与改选失效必须实测；跨作品范围拒绝保持 |
| 技术B | APPROVE WITH CONDITIONS | 不跳过旧失败用例，不build；由Root唯一编译并报告当前完整3+1退出，原日志保留 |
| 产品 | APPROVE WITH CONDITIONS | 大文通过有界读取而非无法创建context；作者选稿与候选未采用仍可验证，不声称模型已理解 |
| UI/UX | APPROVE WITH CONDITIONS | 不新增UI/输入；仅证明上下文运输与范围，不把页返回/阅读覆盖当成文学质量 |

零兼容门禁：PASS。当前fixture一次性替换旧运输断言，无fallback、schema迁移、数据回填、兼容运输或安全开关。

## 实施后证据

待当前fixture实际编译与3+1退出。修改完成、评审批准及已有来源测试不能替代这批通过。

## 当前初始输入与保存产物补充预审

冻结9完整3项实测2/3，剩第一项仍把 own checkpoint 当成 input sourceRevision；这是旧合同，原输入应保持初始original。继续保留全部CAS/working selection/first→second workingBase和作者改选使旧Task失效；强化实际目录定位同Task producer/outputOrdinal/revision，原sourceId/hash/完整内容不变，新 checkpoint全文独立回读。first历史产物在second保存后仍按immutableSavedVersion读取且正文hash一致，目录isCurrent只为元数据，不作采用/交付依据。

五职责预审：架构 APPROVE WITH CONDITIONS（同Native版本库/输入产物分离）；技术A APPROVE WITH CONDITIONS（源ID/fullhash/全文/ordinal真实关系与全部旧CAS负例）；技术B APPROVE WITH CONDITIONS（Root当前全3用例重建重测，旧2/3原日志保留）；产品 APPROVE WITH CONDITIONS（准确读取已保存产物，原稿比较不被改写）；UI/UX APPROVE WITH CONDITIONS（无新增UI、阅读不等于质量/交付）。零兼容 PASS。当前源码已格式化，未build；新增入口/全3用例实际退出前不宣称通过。

## 当前实测后审

Root 统一 current10b test-profile debug=0 构建后的完整 `working_source_tests` 实际 exit0、3/3、0 ignored/0 filtered；日志 `/tmp/autonomous-v3-fixtures-current10b-nodebug/working_source_tests.stdout.log`，总 manifest 同目录 `manifest.json` 记录二进制哈希及实际 profile（opt0、debug_assertions/overflow_checks=true）。同 Task 初始原稿不变、真实保存 checkpoint 两版本回读、历史 hash、作者改选使旧 Task 失效及长文完整来源分页均实际通过。五视角后审 APPROVE；仅该3项证据闭合，不代表承诺提取交付或全量终态。
