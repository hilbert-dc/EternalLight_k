# 3.0 工作区已保存产物回读评审

日期：2026-10-08。范围：`task_workspace.rs`、独立 Core/Runtime 测试、MCP 保存回执与来源工具必要说明。构建由统一验收负责，既有 current8 测试二进制及日志保留为修改前证据。

## 变更概览

目标：模型压缩或重新读取上下文后，仍能从同一个 taskWorkspace 发现和按需读取 Native 实际保存的本 Task 与同 Work 先前 Attempt 产物，同时保留作者选定原稿和比较稿。

初始 `story_task_source_heads_v2`、`relevantText` 与原始来源 ID 不更新。新增目录项使用现有目录、raw/parsed 来源读取及实际阅读收据。保存事实、最新候选 head、作者 working 选择和独立验收是四种不同权威。

范围合同：实际 TaskOutput row、同项目 immutable revision、产物 kind、生产 Task 的 outputObjectId 与 outputRefs、实际 Work/Attempt/root 归属都须对应；同 Work 只读 sequence 不晚于当前 reader 的产物。当前 reader 仍经 source_task、作者原意、活动权限、当前输入检查。Native 导出对象范围、章节上界、POV 与场景前缀；目录不解锁后文、规划或正文。锁定资料可按已有读取范围比较，写权限不变。

正文与规划保存来源的 purpose 为 `savedCheckpoint`、authority 为 `nativeSavedCheckpoint`。实际 revision-store 的 `revisionOutcome`、`sceneAssessment`、`commitmentExtraction`、`commitment` 报告 purpose 为 `savedReport`，authority 仍为 `nativeSavedCheckpoint`；planning critique 的 purpose 为 `savedReport`、authority 为 `nativePlanningReport`。这些来源分别保留其不可变版本和实际保存用途，不统称为正文 checkpoint。producerTaskId、output ordinal/version、completeness 与实际 producerStatusAtListing 在目录明确；isCurrentAtListing 仅表示列出时等于当前 candidate head，不表示作者采纳、全文通过或 Work 完成。失败/取消 producer 的真实保存可在相同获准范围内用于核账和历史比较；活动 reader/Work 被暂停、取消或撤销时仍拒绝。未知费用不伪完成，不新增只读核账之外的执行权。

引用当前性：已读保存来源固定各自的不可变版本与实际生产关系，不随本 Task 第二次合法保存失效；必要原稿也保持固定。目标最新版本与报告当前性仍由现有 Native 验收合同负责，不能通过旧产物引用绕过。raw/parsed 内容使用 immutable 元数据，目录的当前/历史观察不混入内容哈希。目录游标绑定筛选和相关目录状态；新保存后旧游标失效，已读正文分页仍可按原 immutable 哈希完成。

目录只返回有界 metadata；正文按需 materialize。目录 title 从选定 immutable revision 的有界 title 字段生成，不能混用 live chapter title。目录/index/子线程阅读不冒充根模型原文阅读。

## 五视角预审

### 架构专家 — APPROVE

沿用唯一 Task workspace→source→raw/parsed 管道，没有全局候选选择、作品库、模型流程分叉、新接口或第二预算。初始输入与生产输出并列而不改写权威。实际 Work 表关系提供跨 Attempt 依据。

### 技术专家 A — APPROVE WITH CONDITIONS

必须核对持久 producer/output/revision/project/object/Work/root 关系和 reader scope，不能凭调用方 ID 或 JSON 字符串授权。分页 SQL 与响应有界；新保存需失效目录 cursor，但 immutable 比较来源不依赖最新 head。目录标题不能泄露未选定版本。取消/失败产物只保留保存事实。

### 技术专家 B — APPROVE WITH CONDITIONS

需真实 Native 保存路径测试 accepted 旧稿且无 working、POV/scene 后缀、规划、重复 checkpoint、fresh Work decision、foreign Work/task、伪 output、取消和全文分页哈希。不得通过弱化 current-input/作者/TaskBindingInvalid 或最终报告门禁让测试通过。所有构建统一完成，静态审查不当作测试通过。

### 产品经理 — APPROVE

用户仍可简单输入创作要求；模型自己发现保存稿并检查。用户无需理解输入冻结、自己记住长稿或被要求重复生成。真实保存未验收可以诚实呈现，不抹掉取消前已保存内容。

### UI/UX 设计师 — APPROVE

没有新增面板或诊断正文。模型来源目录字段说明用途和权威；保存回执明确原始 source IDs 保持冻结，但保存稿可从获准 workspace 来源读取，减少误解和重复保存。

## 零兼容与综合决策

- 未新增版本探测、旧字段 fallback、双读双写、迁移或回填。
- 当前 schema 直接同步保存 reference 元数据；测试按当前 schema 初始化。
- 不降低 Electron/Runtime 权限、作者、原文阅读证明、账本或独立验收门禁。
- 综合：APPROVE WITH CONDITIONS，实施后逐项实际验收；无兼容例外。

## 实施后复核

源码与强合同 fixture 已实现，构建与真实执行由 Root 统一完成。current10c 的当前 Core 全库 262/262、Runtime lib 10/10、workspace HTTP 2/2、handoff 11/11、context-budget 1/1 均实际 exit0，共 286/286；MCP 全库 93/93 实际 exit0。下述新增 workspace 8 项 Core、2 项 HTTP、2 项 MCP 均在这些实际终态中通过。这是当前相关包与组的证据，完整 Native workspace、Node 全链与实机模型创作尚待统一验收。

实际变更：当前 schema 的 reference 记录生产 Task/output ordinal；目录以真实 TaskOutput→Task→Attempt→Step→Work 查询同 Task/同 Work 不晚于 reader 的产物。初始输入仍用原 source_heads；target 原比较稿用 frozenWriterInput，规划本对象原输入用 frozenTaskInput。正常正文/规划/报告 revision 使用 nativeSavedCheckpoint；planning critique 用真实 report/run/seat store 的 nativePlanningReport，不假装是 revision。标题来自所选 immutable revision，内容 materialize 不包含会变化的 listing 状态。

五视角后审：架构 APPROVE（一个工作区来源管道、原输入与输出用途分明）；技术 A APPROVE（真实 Work 根 Task/thread、生产关系、scope、sequence、缓存重读和 cursor 正负例已实际通过）；技术 B APPROVE（新增 8 项 Core、2 项 Runtime HTTP、2 项 MCP inline 断言在当前完整相关包中通过，保留首次失败与修复记录；不替代完整工程验收）；产品 APPROVE（可回读保存稿和自身报告，减少重复生成）；UI/UX APPROVE（来源和保存回执可理解，没有额外面板）。

静态风险与验证注意点：Core fresh Work 决策正例使用实际 create_budgeted_root_task、Native 铸造 ID、真实作者消息与冻结授权，再以实际 grant 结清可信测试费用；没有伪造作者来源。其后比较 Work/根线程/作者 manifest/消息数量/同一账本，另以负例破坏 producer sequence 和 Work 根线程验证缓存不能越权。取消 producer 的真实完整 report 保留 completeness=complete 与 producerStatusAtListing=cancelled，不声称完成。目录/子线程查阅不产生根原文证明；旧 checkpoint 的 sourceChecks 不能认证新版正文，frozen_comparison 未扩。

当前 schema 只支持新空环境初始化。reference 新列没有 ALTER、迁移、回填、旧字段 fallback 或版本探测。既有 QA 与书库不修改；Root 将以当前 schema 新空 QA 验收。静态 rustfmt 与差异空白检查仅是语法/格式检查，真实终态以以下原始日志与执行 manifest 为依据。

## current10c 实际验收证据

统一编译执行 manifest：`/tmp/autonomous-v3-workspace-current10c-nodebug/manifest.json`。每个测试二进制实际默认完整 assertions；profile 的 `debug_assertions=true`、`overflow_checks=true`，仅关闭 debuginfo；五组 source/bin 的 before/after hash 均一致。

- Core 全库：`novel_codex_story_core.stdout.log`，262 pass、0 fail、0 ignored、0 filtered，actual exit0。包含全部 8 项 `task_workspace::output_tests`，accepted 原稿/重复保存、immutable cursor、planning checkpoint、POV 前缀、foreign 关系、根阅读证明、fresh Work decision 和真实取消报告正负例均通过。
- Runtime workspace：`workspace_saved_output_http_tests.stdout.log`，2 pass、0 fail、0 ignored、0 filtered，actual exit0。场景正文按实际模型获准 sceneGroups 和 Native `\n\n---\n\n` 分隔符拼接；完整正文、UTF-8/hash、冻结输入、历史产物与隐藏后文断言保留。
- MCP：`/tmp/autonomous-v3-workspace-mcp-current10-nodebug.result.json` 与同前缀 `.stdout.log`，93 pass、0 fail、0 ignored、0 filtered，actual exit0。新保存回执 2 项包含在全库中；执行前后 binary hash 均为 `b121102efe740fcae99932791b333befa9dc20b0395ffd29467a7b96238a7580`。

当前 workspace HTTP fixture source hash 为 `84f8b1753eb48b4f957f35c105ab2440d3a00723572ac24e43a30468492e79fe`，测试 binary hash 为 `b9c15d4173a861b75ad5187116550026e090aba7e6b6307889ea8996ff79ddd9`。生产与新增 Core fixture 的完整源码冻结清单保留 `/tmp/autonomous-v3-workspace-output-current10-fixtures2-static-frozen.json`；此文档的证据补录另存后审冻结清单，不倒改旧证据。

首次 Core 260/262 exit101 保留 `/tmp/autonomous-v3-workspace-core-current10-nodebug.stdout.log`，首次 Runtime 1/2 exit101 保留 `/tmp/autonomous-v3-workspace-http-current10-nodebug.stdout.log`。前两项为当前作者/Work 绑定门禁正确拒绝错误 fixture，后一项为 fixture body 与实际 sceneGroups 不一致；修复只同步真实合同，没有修改生产门禁或 Native 正文分隔规则。相关包通过不表示完整 Native/Node/实机通过，也不表示《万城灯火》前 100 章已交付。

## 生产实现前增量预审：报告产物的真实存储

Core 全部 `create_task_record` 都由 `attach_attempt` 在同一事务绑定实际 Work，没有合法“无 Work 任务”格式。目录聚合必须核当前 reader membership 和真实原根线程，缺关系为 TaskBindingInvalid，不添加 NULL fallback。

`revisionOutcome`、`sceneAssessment`、`commitmentExtraction`、`commitment` 为实际 revision-store 产物；章节报告的范围以 producer 原 target.chapterId 映射，不能当正文。`critique` 的 TaskOutput revisionId 实际指向 `story_planning_review_reports_v2.report_id`，须强核该报告的 task/seat/run/project/work 和 outputRefs，不假装它存在于 story_revisions 或以旧格式 fallback 读取。使用相同 lazy workspace/source 管道，但 authority/purpose 明确 `nativePlanningReport`/`savedReport`；不能查询任意 report ID。

章节报告仅全章非 writer 获准 scope 可见；章节上界/POV/场景 writer 一律不通过报告泄露摘要。项目级 planning critique/commitment 仅已有 artifacts scope 允许；原保存信息与最新验收/正文事实严格分离。未知 supplier 使用账本仍原权威，不触发新执行。

五视角增量结论：架构 APPROVE（域存储明确、一个来源管道）；技术 A APPROVE WITH CONDITIONS（真实关系/有界查询/报告范围/线程归属）；技术 B APPROVE WITH CONDITIONS（需实际保存与伪 report 负例）；产品 APPROVE（模型可回读自身分析）；UI/UX APPROVE（不加 UI，来源说明用途）。零兼容 PASS；实际测试仍待统一构建。

## current10 首轮 Core 失败后的 fixture 合同增量预审

Root 当前 debug=0、完整 assertions/overflowchecks Core 全库实际 260/262，exit101，旧失败保留 `/tmp/autonomous-v3-workspace-core-current10-nodebug.stdout.log`。没有把静态或部分通过当新全量通过。

两个 fixture 修正：其一 fresh Work 负例把原作者 producer 的 sequence 改到 reader 后，既有 authorIntentBinding 先正确拒绝整个读取；不能要求越权时目录继续返回空列表，改为目录/缓存读取均拒绝并加正例阶段定位。其二取消 direct review 的单席位会真实取消原 Work；不能随后领取第二席位。改为真实 budgeted advisory 原会话中登记 planningReview、真实席位保存 report、通过 domain finish_planning_review(cancel)停止评审，原会话仍有 author delivery pending，再由真实同 Work decision 读取 cancelled producer 的 immutable savedReport。原授权、作者消息、model plan、真实席位 grant 与结账都实际建立；不写 SQL 恢复 Work，也不修改生产 gate。

五视角：架构 APPROVE（只修真实用法）；技术 A APPROVE（保持作者sequence/暂停取消/实际domain与费用门禁）；技术 B APPROVE WITH CONDITIONS（同一原会话报告取消正例须真实跑通，Root 重建）；产品 APPROVE（保留取消前保存事实但不宣称评审通过）；UI/UX APPROVE（无UI/流程新增）。零兼容 PASS；生产冻结不变。
