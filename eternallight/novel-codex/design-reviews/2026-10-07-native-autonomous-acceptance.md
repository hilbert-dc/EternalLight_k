# Native 自主创作验收实施评审（2026-10-07）

## 变更概览

目标：普通章节修订与连续生成共用起草前冻结的 chapterAcceptance；独立 critic 对最新完整稿逐项判定；真实保存字数和报告判定共同约束完成、章节推进与 handoff。模型选择阅读、修订方法和文学判断，Native 保证绑定和执行条件。持久保存 parsed 原文返回范围，压缩后可恢复；已返回不代表已理解。

非目标：外部人工为每轮故事纠错；章节/金额/作者词语特判；Canon 自动采用；旧版本兼容、迁移、回填。

影响模块：story-core 任务登记、章节合同、独立评估、Work 状态/完成、分页来源；story-runtime 当前 API；story-mcp schema 与提示；当前 fixtures 和测试。

接口：chapterAcceptance 继续 requirements 与 length；初次冻结且原 Work 后续不降标。assessRevisionOutcome 的 originalRevisionId 必须显式 null 或实际原稿；冻结标准派生 acceptance-N 标准身份，与原 desiredOutcomes 一并逐项评估。unmet、insufficientEvidence、报告不存在或旧稿以及长度不符均不能完成。状态给模型可读的问题和真实计量，模型选择后续动作。

## 零兼容门禁

PASS：仅当前 schema 与工具协议，一次同步 Native、Runtime、MCP、fixture；不新增版本判断、旧字段 fallback、双轨或 migration。测试用 /tmp 下独立数据库；实际作者数据库只读。

## 五视角实施前评审

以下是五个审查视角，不是五名独立评审人员。

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 复用 Work 与 critic；所有写作入口共同标准和完成门禁；报告保存不能认证质量。 |
| 技术 A | APPROVE WITH CONDITIONS | 作者/Work/writer/当前稿/来源绑定、显式 nullable 原稿、冻结标准、预算/未知写保护不得放宽。 |
| 技术 B | APPROVE WITH CONDITIONS | Core、HTTP、MCP 当前 schema 测试同步；否定结论、短稿、旧报告、缺项/重复项、首次写作与选场修订必须覆盖。 |
| 产品 | APPROVE WITH CONDITIONS | 应用可读回问题自行选择修订，通过才能续章，无法继续保留成果明确停止；无人工故事特判。 |
| UI/UX | APPROVE WITH CONDITIONS | 主对话与实际回执一致；新增状态供模型解释，不新增技术表单；阅读范围不得称理解完整。 |

综合结论 APPROVE WITH CONDITIONS。P1 条件：完成门禁必须实际调用统一独立评估，而非仅新增 schema；完整调用方和定向测试通过后才批准。

## 实施后复核

### 当前契约与完成条件

普通候选章节和连续生成共用 `chapterAcceptance`：`requirements: string[]`，以及 `length: {requirement, minCharacters: number|null, maxCharacters: number|null}`。标准在第一次 writer 登记前冻结；同 Work 同章节后续修订不能降低或删除标准。标准整体不超过 3500 UTF-8 bytes，最多 12 项唯一要求；旧选场 `desiredOutcomes` 仍可表达最多 4 项额外改稿目标，不能使用保留的 `acceptance-` 身份前缀。

独立任务仍叫 `assessRevisionOutcome`。首次 wholeChapter 的 `originalRevisionId` 显式为 null；修订使用冻结的实际原稿。评估输入只提供冻结标准、真实保存字数、完整当前/原稿和冻结的前文章节包；`read_context` 清空作者陈述、大纲/规划、`relevantText` 和额外 `queryReport`，正文仅从批准包的 task-owned 来源分页读取。评估必须覆盖每个 `acceptance-N` 和每个原 desiredOutcome，一次且无遗漏。每项状态为 `supported`、`unmet` 或 `insufficientEvidence`；引用须匹配对应保存稿和场景的 Unicode 范围，不能用泛泛评价代替原文证据。

Native 交付判定为：当前稿与来源仍有效、独立报告全部 supported、真实正文字符数满足冻结区间。报告存在、保存完整稿、模型自称完成都不能替代此条件。`fulfilled`、`finishChapter` 和同 Work 后继 generation handoff 共用门禁。评估未通过时，模型读取正式报告、选择当前作者授权内的修订范围，生成新稿并重新独立评估。未来规划、后文章节变动不会使无关前章报告失效；评估实际返回过的必要前文发生变更会使旧报告变 stale。连续生成的已交付历史章不进入当前章的必做动作清单，避免滚动规划导致全书反复检查。

### 状态、报告与来源返回

`Work.deliveryReport.chapterAcceptanceReports[]` 提供 UI 投影，含 writer/current draft/report 身份、current、verdict、实际 length 和逐项 checks；verdict 为 `pending|stale|notMet|insufficientEvidence|passed`。这是一条可见记录链。只有有当前冻结标准且满足门禁的章节才具备通过结论；历史 completed 记录缺少当前合同不会被认证为通过。投影条件不构成旧协议兼容或回填。

`read_work_status.requirements[kind=revisionOutcome].assessment` 为有界决策摘要：身份、current、verdict、真实 length、`checkCounts: {total,supported,unmet,insufficientEvidence}`。顶层 `chapterAcceptance` 完整且仅出现一次；摘要不复制标准、原因和引文。当前页容量为 1 requirement / 2 actions / 2 receipts，游标与 Work/version/source/receipt digest 绑定，coverage 明确偏移和总数。不能把首屏未出现动作当作动作不存在。最大合法标准与 12 条中文原因的真实 HTTP 回归确认每页 ≤6500 bytes。

新增当前工具 `read_revision_outcome({writerTaskId})`：仅同 Work 协调任务可读实际 completed writer 的报告。返回不可变 source 引用、`state: current|stale`、writer/draft 身份和 `assessment: {sourceId,textDelivered:false}`；完整 checks/reason/quote 统一用 `read_source_content` 的 parsed 游标读取，无按大小选择双轨输出。跨 Work writer 被拒绝。

parsed 来源返回 `contentRead:true` 和本页实际所属记录的 `returnedChapterNumbers`；index/raw 不把目录/编码字符串冒充已读正文。`read_context.taskSources.entries[].returnedCoverage` 持久恢复 entryRanges、returnedEntries、totalEntries、complete、nextUnreadCursor，并显式 `semanticUnderstanding:false`。返回进度按 task/source/hash 绑定且幂等，不称模型已经理解。实际 parsed 字段和字符范围、完整 raw 字节返回范围分别记账；报告来源当前性只依赖实际返回过的必要前文章节，目录本身不成为阅读或 freshness 证据。

parsed/raw 返回记录使用短 `BEGIN IMMEDIATE` 事务，在获得写快照后读取并记录返回；避免在 DEFERRED 已读快照上升级写入导致 Host 并发事件持久化触发 SQLITE_BUSY_SNAPSHOT。没有添加重试或修改故事内容锁策略。Unicode 分页回归同时在同一临时 DB 连续写入 200 次 metadata，完整正文返回、进度恢复与引用校验仍通过。

### 控制与隔离

模型新增通用 `pause_generation_run`，要求同 project/thread/Work、当前 Run version、作者 intent revision 和幂等键，返回真实 committedGenerationControl paused 回执。不会解析“暂停”关键词直接决定动作；模型理解后调用控制能力。相同请求仅在 Run 仍为该回执 version 且 paused 时可重放；作者恢复或其他状态更新后，旧暂停回执被 stale 拒绝。后续任务取消/预算结算保留 paused，不覆盖为 cancelled/completed。

UI/Host 对所选任务使用统一 `/work-tasks/:id/control`，expectedVersion 是 Work.version。generation pause/resume/cancel 在 Native 状态变更同一事务内验证 Work CAS 和 Run CAS；已登记动作只改变 Work.version 时，过期 pause/cancel 仍被拒绝，Run/Work/lease 均不变化。请求内 guard 只用于事务绑定，不持久化第二权威状态，也不比较两个不同实体的 version。

实际 writer/Work/作者归属、finite/unbounded 原预算、未知写恢复门禁、锁定场景与未选中场景、Canon 人工采用边界保持。保存成果可在不满足文学判断时继续保留；通过门禁不是 Native 的文学认证。Native 保证实际来源、完整标准和真实计量，语义判断与修订方法仍由独立模型承担。

### 五视角实施后结论

| 视角 | 结论 | 实际核对 |
| --- | --- | --- |
| 架构 | APPROVE | 普通/连续写作共用冻结合同与独立 critic；Work 完成、章节推进、handoff 无报告存在即通过的旁路。 |
| 技术 A | APPROVE | 负报告、短稿、旧稿、必要前文变更阻断；Work CAS 覆盖 pause 与独立 cancel 事务；旧暂停回执不能伪称当前状态。 |
| 技术 B | APPROVE | Core/HTTP/MCP 定向当前契约同步；来源并发写回归、最大标准状态预算、真实正式报告分页和跨 Work 拒绝已覆盖。 |
| 产品 | APPROVE WITH CONDITIONS | Native 已支持应用自主读失败原句、修订、再评估；完整真实 Host/Core/MCP/Native 旅程由 Host 代理验收，实际 DeepSeek/app 文学效果仍须根任务完成体验，不能将脚本判断当作已验证文学效果。 |
| UI/UX | APPROVE | UI 接收稳定 current/verdict 投影；技术原因和引用按需查看；正文保存、检查通过、已返回文本和语义理解分开表达。 |

综合结论：Native 定向技术条件已满足；实际应用和真实供应商体验的最终整体验收由根任务报告，本评审不替其声称完成。

### 验证证据

全部数据库在 /tmp 下新建，未写实际作者数据库，未调用 DeepSeek、UI 或读取密钥。

- Core：`cargo test --manifest-path native/Cargo.toml -p novel-codex-story-core --lib --test model_work_plan_tests`，219 +13 通过。日志 `/tmp/autonomous-core-plan-final-2.log`。
- Runtime：`cargo test --manifest-path native/Cargo.toml -p novel-codex-story-runtime --test chapter_acceptance_http_tests --test task_sources_http_tests --test revision_outcome_tests --test continuous_recovery_tests --test work_decision_http_tests`，5 +4 +37 +14 +8 =68 通过。日志 `/tmp/autonomous-http-16.log`。
- MCP：`cargo test --manifest-path native/Cargo.toml -p novel-codex-story-mcp`，67 通过。日志 `/tmp/autonomous-mcp-final-4.log`。
- 当前 Runtime/MCP 等所有 bins：`cargo build --manifest-path native/Cargo.toml --bins` 通过。日志 `/tmp/autonomous-build-bins-5.log`。
- 以上 CARGO_TARGET_DIR 均为 `/tmp/autonomous-native-target`；本机开发应用固定使用 `native/target/debug`，需要根任务增量构建该位置，不能拿 /tmp 验收 binaries 代替已启动应用版本。
- 关联 generation_author_source 13 项、generation_handoff 11 项通过（`/tmp/autonomous-http-adjacent-2.log`）；scoped_revision 当前计划与冻结标准 fixture 同步后 5 项通过（`/tmp/autonomous-http-scoped-final.log`）。这是定向与关联验收，不声称全 Native workspace 测试已执行。
