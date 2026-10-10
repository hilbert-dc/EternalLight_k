# 当前 Core 恢复与交付 integration fixture 评审

日期：2026-10-08。范围仅 `native/story-core/tests/tasks_tests.rs` 与 `work_delivery_tests.rs`；不修改生产、配置、权限、来源或预算，不自行构建。完整当前 workspace 由 Root 统一构建验收。

## 实际失败与当前合同

旧完整日志 `/tmp/autonomous-v3-native-current6-localhost-20261008.log` 已真实结束为 19 targets failed，保留全部原断言和失败输出。两组在该日志中分别为 tasks 10/11、work_delivery 1/5，不能引用这些旧二进制作为新源码通过。

`tasks_tests` 的 recovered_candidate 正例已经证明真实恢复完成 Task 与 claimed action，但旧断言把整个 Work 也当作 completed。当前 record_task_action 会以 Native ensure_author_delivery_pending 登记原会话后续责任；子任务实际保存与结束不能代替原会话最终核对。fixture 应断言实际 Task/action completed、同一个原 Work/根线程/真实保存产物保持、Work running 与尚无最终 deliveryReview；原重放幂等及不同 instruction 拒绝保持。

`work_delivery_tests` 三处完整报告旧对象缺当前 `issues` summary 与 `chapterAcceptanceReports`。将期望更新为实际全部字段、精确空计数/页字段和空章节验收报告；不只摘录 evidence 或忽略新增字段。proposal 正例的原建议/decline 不会自动消除原会话最终交付责任：两阶段都不能宣称 Work completed 或 discussion_ended，仍精确核对 proposal id、decline 状态、任务数量、未执行与无最终核验。真正章节 final decision 必须通过当前 Native taskSources 完整读取原作者要求，逐页检查真实 sourceId/path/hash、连续 UTF-8 byteRange、边界与完整 SHA256，再提交原有精确 CAS/evidence review；目录、子线程、数据库或另一作者输入不能代替阅读证明。

## 五视角实施前评审

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构专家 | APPROVE | fixture 使用唯一 Native Task/action/Work 与 source 权威；不另造流程或完成标记。 |
| 技术专家 A | APPROVE WITH CONDITIONS | 保留真实生产绑定、版本、thread、产物、采用与费用门禁；完整作者阅读由原根 Task 实际来源传输证明，不能伪造。 |
| 技术专家 B | APPROVE WITH CONDITIONS | 只同步两个 integration 文件，完整 JSON 期望保留所有字段；原负例/重放/篡改/预算断言不删除，Root 构建当前11+5完整默认断言后才通过。 |
| 产品经理 | APPROVE | 区分子任务产物保存和原用户目标完成，避免未执行建议或未经最终核对被声称交付。 |
| UI/UX 设计师 | APPROVE | 不新增 UI；测试保持真实状态，使用户看到可理解、诚实的进度与结果。 |

零兼容门禁：PASS。当前 fresh schema 初始化；没有版本判断、旧字段 fallback、双读双写、迁移、回填、降级或生产适配。综合 APPROVE WITH CONDITIONS，实际终态仍待 Root；不把静态修复或单包通过当完整工程闭环。

## current10 编译错误后的最小增量预审

Root 完整 Core compile 实际 exit101，原 stderr `/tmp/autonomous-v3-core-full-current10-nodebug.stderr.log`。新增两个直接 Task 状态强断言错把 `Result<Option<TaskRecord>>` 当成 `Result<TaskRecord>`。只补第二次 Option unwrap：记录不存在仍立即失败，保留全部 Task/status/Work 断言；不改 API，不提供缺失记录 fallback。

五视角：架构 APPROVE（当前返回合同）；技术 A APPROVE（None 继续拒绝，身份和状态强断言不变）；技术 B APPROVE WITH CONDITIONS（需 Root 重新编译并完整11+5实际终态）；产品 APPROVE（仅验收准备）；UI/UX APPROVE（无界面变更）。零兼容 PASS。

## current10f 实际单项结果与引用断言增量预审

完整 Native workspace 在本条预审准备时仍运行，修改等待整轮结束。现有真实终态 `/tmp/autonomous-v3-native-full-current10f-nodebug.result.json` 为 actual exit101、100 targets（包含3个doc-test库）、843 pass / 8 fail / 0 ignored / 0 filtered。原 stdout/stderr 完整保留。该轮 `tasks_tests` 11/11、`work_delivery_tests` 4/5，均 0 ignored/filtered；唯一后者失败位于 helper 第665行，`source["textDelivered"]` 为 Null，旧期望 false。其余 discussion/proposal/canonical/budget 四项已实际通过。

当前 `task_sources.rs::project_task_sources` 返回的 metadata entry 包含 sourceId/sectionPath/contentHash/payloadBytes/chapters/coverage；未交付正文的标记属于 `context.authorRequirementsSource` 的引用 `{sourceId,textDelivered:false}` 和 taskSources 整体。待本轮退出后的最小修正：保持 source metadata 身份/hash/尺寸检查，把该位置强断言改为原 context 引用完全等于实际 entry 的 sourceId 加 textDelivered=false。不只移除断言，不创造 metadata fallback。后续真实根 raw 分页/完整哈希/原话/最终 CAS/evidence 均保留。

五视角预审：架构 APPROVE（一个当前 source 与其引用合同）；技术 A APPROVE（Native 真实 sourceId 和未交付标记精确绑定，未冒充根阅读）；技术 B APPROVE WITH CONDITIONS（等整轮结束后一次修改/冻结/构建，完整5项实际终态）；产品 APPROVE（目录不冒充已交付正文）；UI/UX APPROVE（无界面变更）。零兼容 PASS。本记录只准备修正，当前没有写测试源码或重构建。

整轮真实退出后已按上述唯一补丁同步 `work_delivery_tests.rs`：完整引用强等值绑定实际 sourceId 和 textDelivered=false，所有 metadata、全文分页、hash、原话与最终 CAS/evidence 断言不变。五视角后审：架构 APPROVE（单一当前引用）；技术 A APPROVE（未降低根阅读与来源身份）；技术 B APPROVE WITH CONDITIONS（静态差异/格式检查通过，完整5项复验等待统一构建）；产品 APPROVE（不误报已交付正文）；UI/UX APPROVE（无界面变更）。零兼容 PASS。没有改生产，没有自行构建，当前不宣称失败已通过；tasks 的11/11是前一冻结源码的真实结果。

## 实施后复核

当前两个 integration fixture 已同步并静态冻结。tasks 的恢复断言区分实际 Task/action complete 与仍待原会话核对的 Work，新增同 Work/根 thread、两条真实已结束 Step、保存 revision 与任务数精确检查；原 finish_action 重放、幂等键冲突与 altered instruction 拒绝不变。delivery 的完整报告补齐精确 issues 空状态与 chapterAcceptanceReports，proposal decline 后仍明确 original Work pending 且没有创建新任务或 final review。最终章节 review 从实际 Native 返回 sourceId 全 raw 分页完成读取，复用原根 Task、范围与 frozen author intent；每页 source/path/hash/连续 byteRange、2000/6500字节与实际转义模型项9000token上限、最终完整哈希与原话均强检查。采用、presentation metadata 篡改、canonical output 缺失、superseded 版本、知识发布、真实费用幂等与原配额断言均保留。

该次静态冻结时五视角后审：架构 APPROVE（没有生产与第二完成权威）；技术 A APPROVE WITH CONDITIONS（静态确认当前关系与完整阅读检查，待真实测试）；技术 B APPROVE WITH CONDITIONS（rustfmt 和 git diff --check 已通过，尚待 Root 构建11+5完整断言 actual exit）；产品 APPROVE（子任务结束不误报整体完成）；UI/UX APPROVE（无界面变更）。零兼容 PASS。rustfmt 对两文件已有长调用仅格式整理，没有更改原负例行为；原失败日志保留。该时点没有自行构建或运行，不宣称11+5通过；最新实际证据见下节。

## current10g 实际交付组通过后的实施后复核

Root 统一构建后的 `/tmp/autonomous-v3-four-targets-current10g-nodebug/manifest.json` 已记录 `work_delivery_tests` 完整默认断言的 actual exit0、5 pass / 0 fail / 0 ignored / 0 measured / 0 filtered，进程耗时 1.19 秒；完整 stdout/stderr 位于同目录的 `work_delivery_tests.stdout.log`、`work_delivery_tests.stderr.log`。测试二进制 profile 为 opt0、debuginfo0，debug_assertions 与 overflow_checks 均 true，没有过滤或降低运行时断言。

本组源码执行前后 SHA256 均为 `562570c98d11c251cdedaf7d6f5c1498e9e58775d3c934ca95a9b26f941b3df5`，本次只读重新核对当前源码仍相同；二进制执行前后 SHA256 均为 `81ed3539990b2285daa42dbb34d0178d5a334b7688938e4b270de1b46dd6aea9`。主任务本轮整仓检查记录 1524 个文件前后不变。本次只更新本独立评审证据，没有修改生产、fixture 或自行构建。

五视角实际后审：架构 APPROVE（Native Task/action/Work 与正文事实仍是唯一权威）；技术 A APPROVE（采用、来源、版本、预算、真实根作者全文阅读及篡改负例均经本组完整实际断言）；技术 B APPROVE（本组源码/产物身份稳定、5项完整实际 exit0）；产品 APPROVE（保存、子任务结束与原目标最终交付边界实际验证）；UI/UX APPROVE（无界面变更，不将保存标记或目录当作通过）。零兼容门禁 PASS，没有 fallback、迁移、生产适配或门禁放宽。

`tasks_tests` 的当前 11/11 实际结果仍由前一完整 current10f 轮认证，未用本组运行伪装为重跑。完整 Native current10f 的 actual exit101、843 pass / 8 fail 和本组首次 4/5 失败日志均保留。四目标 manifest 中其他目标存在真实失败，本组 5/5 不代表该 manifest 全部通过，也不代表最新完整 Native workspace、完整 Node 链、实际应用或 DeepSeek 文学验收通过。
