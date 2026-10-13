# 工作区真实报告回读 HTTP 证据补齐

日期：2026-10-08。只修改新增 `workspace_revision_reports_http_tests.rs`；不修改生产、共享 helper、预算、权限或构建配置。构建和执行由 Root 统一完成。

## 方案与五视角预审

测试通过当前 Author 授权、真实 Native 候选保存和 TaskOutput 记录产生报告，随后通过同一 Work 的真实 `/decision` 新 Attempt 获取任务目录与 raw/parsed 全文。不以 SQL 构造产物、根身份或费用。复用已有修订评估 fixture 的合法单章路径、Author plan 接受/章节 materialize/Knowledge publish 路径、真实 commitmentExtraction 和 reviseCommitment 保存路径。初始源与实际保存报告分开核对；保存报告不冒称正文、作者采用或交付完成。

架构专家：APPROVE。同一 taskWorkspace/source 管道，不新增报告仓库或文学流水线。测试按当前模型选择和 Native 授权合同组织。

技术专家 A：APPROVE WITH CONDITIONS。必须真实保存后检查 producerTaskId、outputOrdinal、revisionId、objectKind、同 Work/root 与 sourceId 的 task 归属；外 Work 借用 sourceId 必须拒绝。新 reader 需要原 Task 的真实终态及 Engine/Native 费用结账，不把 queued step 当实际 Attempt。

技术专家 B：APPROVE WITH CONDITIONS。raw 连续 UTF-8 字节页、完整 hash 和 parsed 字段/字符串字符范围都核对；目录每页 ≤6500 bytes，原文片段 ≤2000 bytes，模型返回条目 ≤9000 tokens。章节范围/future 与 writer 的报告目录过滤保留独立负例。不借助 Author 全局报告读取代替 Candidate 原文分页。

产品经理：APPROVE。证明模型可回读真实分析和修订产物，不要求作者复制报告或重复生成。这里只验工程能力，不证明文学判断正确。

UI/UX 设计师：APPROVE。无新增界面，目录是按需发现入口，报告正文只在模型需要时实际读取，元数据不冒充阅读全文。

零兼容：PASS。新空数据库，当前 DTO，没有迁移、回填、旧合同分支。综合 APPROVE WITH CONDITIONS；实施后检查并等待统一构建真实终态。

## 静态实施后复核

四项独立 HTTP 测试已写入，尚未编译或运行，不声明通过。使用当前 Router 的真实 Author/Candidate API 与 Native 领域校核，由测试脚本调用工具协议；未使用外部模型，不冒称 DeepSeek 生成或文学效果验证。

- revisionOutcome：复用现有单章真实 Author行动审批→writer checkpoint→Knowledge→独立 assessment 保存链。保持完整 beforeDraft 原文/版本/来源 hash；保存后真实结账和 `/decision` 新同 Work/root Attempt 的目录与多页 raw/parsed 全文读取。真实后继 writer 的 sequence 晚于报告生产者，同 Work 仍不能在目录打开报告。
- sceneAssessment：真实 Author plan保存/接受、章节materialize、正文、Knowledge任务publish/complete/结账后提交合法 insufficientEvidence 报告；保留第一章原文，第二章仅作为显式 future 范围负例，不借用整章事件宣称规划已实现。
- commitmentExtraction：实际 task-source 全文读取证明后保存提取候选，原来源/版本/hash 不变；第一章目录和原文排除第二章。实际报告保存内容包含真实 payload envelope，不把提案写成已建立承诺。
- commitment：当前 `reviseCommitment` 有合法 requiredOutputKind=commitment 与 task-candidate 保存入口，真实创建 Author原承诺、读取冻结 task-input、保存引用正文兑现证据的候选。旧承诺仍从该既有领域 task-input 原版本读取；初始 workspace 目录现仅五种原始资料，不构造不存在的旧承诺目录项。实际 TaskOutput 承诺由同 Work 新 reader 从 workspace 完整读取，extractionOrigin 保持 null。

全部四项核真实 Work首 Attempt.root thread、生产 step/currentAttempt/outputRefs、真实 budget-authorization 16 requests/200000 tokens 不变、真实新 advisory step绑定和原作者原文。scope/future 验证属于有界生产 Task；后继项目级协调员保持自身合法 scope，不能把它误称第一章范围。每个保存报告 JSON 超过2000 bytes，raw 从零连续分页/UTF-8完整hash，parsed逐entry/字符range完整重建并匹配同一hash；页面≤6500 bytes与实际escaped模型条目≤9000 tokens。每个域另创建真实 foreign Work，目录无产物，借用本reader sourceId的raw/parsed读取拒绝。初始 sources 与 savedOutputs 是不同证据用途，均不冒称采纳、最新验收或最终完成。

五视角后审：架构 APPROVE（一个 Native来源管道，无生产改动）；技术A APPROVE WITH CONDITIONS（静态核对所有真实保存/授权/Work身份，没有 SQL造关系或空字段 equality；等待实际执行）；技术B APPROVE WITH CONDITIONS（Root统一构建完整4项，失败不filter/skip，不扩费用/期限/权限）；产品与 UI/UX APPROVE（可发现/读取保存分析，无新界面或作者参数）。零兼容 PASS。实际构建/HTTP终态是剩余验收项，不把本静态后审当证据已闭合。

## current10f 首次完整执行后的增量预审

实际完整4项均先完成真实领域保存、费用结账、同 Work/root 后继 Attempt 与 workspace 目录定位，均在第521行旧 `purpose=savedCheckpoint` 断言失败；Native 当前报告目录实际为 `savedReport`，revision authority 仍为 `nativeSavedCheckpoint`。原4项完整失败保存在 `/tmp/autonomous-v3-native-full-current10f-nodebug.stdout.log`，不是4项通过；失败后的全文 hash、foreign Work 和 writer 隐藏断言尚待实际执行。

只将四个真实报告域共同 helper 的目录用途强断言同步为 `savedReport`，并在实际 raw/parsed 重建的 immutable wrapper 上同样强核 `purpose=savedReport`、`authority=nativeSavedCheckpoint`。不增加旧值兼容/fallback、不改变 draft 的 savedCheckpoint 合同，全部实际保存/TaskOutput/Work/root/原预算、原输入冻结、章节范围、连续分页/hash、foreign 与 writer 负例保持。

五视角预审：架构 APPROVE（报告用途区别于正文，无第二报告仓库）；技术A APPROVE（严格当前用途与 Native保存权威，仍实际全文读回）；技术B APPROVE WITH CONDITIONS（Root统一完整4项运行，不跳过后续强断言）；产品 APPROVE（保存分析可以回读但不证明交付）；UI/UX APPROVE（无新增UI或作者操作）。零兼容 PASS，静态后审后冻结，尚不声明当前4项验收通过。

增量实施后：目录与实际 raw/parsed wrapper 均强断言 `purpose=savedReport`，两处 `authority=nativeSavedCheckpoint` 保留且新增全文wrapper权威校核。原全部四域合法保存、真实同Work后继/预算、原文冻结、完整分页/hash及各负例不改。rustfmt 与 `git diff --check` 实际退出0，未构建或运行。五视角后审：架构/技术A/产品/UI/UX APPROVE；技术B APPROVE WITH CONDITIONS（实际4项必须完整达到此前失败后的全部断言）。零兼容 PASS。补丁前是0/4，当前不得声明通过。

## current10g 真实许可与证据分工增量预审

实际完整4项3/4：sceneAssessment、commitmentExtraction、commitment 全部通过；revisionOutcome 已通过真实保存、同Work后继reader目录与 raw/parsed 完整hash，在旧 fixture 第690行寻找 `generateChapter` readyAction 失败。日志 `/tmp/autonomous-v3-four-targets-current10g-nodebug/workspace_revision_reports_http_tests.stdout.log` 保留。普通 Work 的真实原批准动作仅允许原选定场景修订，Native 不因未达标报告自动继承新的写许可；此报告本身已经含真实 unmet/insufficientEvidence，伪造更严意见也不能创造许可。

按 Root 明确分工：本文件保留普通 Work revisionOutcome 完整 save→真实同Work/root后继→有界目录→raw/parsed全页/hash→foreign Work拒绝，删除不可达的自动writer派发预设；测试名只陈述本项已实现证据。**后继 writer 报告隐藏要求不取消**，迁移到已有真实 generation fixture `support/authoring_selected_chapters.rs::three_chapters_with_feedback`：第一次负评后模型实际选择修稿 writer 的阶段，拥有同一原 Run/Work、真实保存报告、正确Attempt先后、原作者许可/原预算，Root分配代理独占补此目录和已知source读取拒绝验证。未统一运行前不能声明迁移后的覆盖通过。

五视角预审：架构 APPROVE（按各Work真实权限组织证据，不使报告成为grant）；技术A APPROVE WITH CONDITIONS（普通源/费用/hash/跨Work全部保持，另一真实generation路径必须有同Work后继writer目录及known-source拒绝）；技术B APPROVE WITH CONDITIONS（统一完整4报告及完整authoring12项，原3/4失败保留，不能skip）；产品 APPROVE（模型在真实授权内灵活选择，非报告强行触发重写）；UI/UX APPROVE（不造作者新授权或新界面）。零兼容 PASS，无生产修改、无额外GenerationRun fixture、无SQL许可。此前静态部分所述writer后继是原fixture意图，未通过；以本分工和最终真实验收为准。

增量实施后：本文件只移除不可达的自动writer选择/派发后半，保留本条全部实际报告读回与末尾 foreign Work 目录/双原文接口拒绝。测试名改为 `real_revision_outcome_is_readable_by_later_same_work_attempt`，source注释精确引用 `support/authoring_selected_chapters.rs` 的真实generation修稿选择。另3域完全不改。rustfmt、`git diff --check` 实际退出0，无构建。五视角后审架构/产品/UI/UX APPROVE；技术A、技术B APPROVE WITH CONDITIONS（要求该受权writer隐藏证据在另一owned fixture实际实施并完整运行，本文件4项也须重跑）。零兼容 PASS。当前3/4为补丁前结果，不把迁移要求当作已通过证据。

## final-direct13 当前实际验收

Root 已统一实际运行当前完整四报告组4/4、actual exit0，无filter、skip或ignored。证据 `/tmp/autonomous-v3-final-direct13/manifest.json` 与 `/tmp/autonomous-v3-final-direct13/workspace_revision_reports_http_tests.stdout.log`；本测试源码前后SHA均为 `d316119e2dd615c3094326c7f24166913278ba1dca22e4f7dccc43f1c7d07261`，执行二进制前后SHA相同。当前test profile opt_level=0、debug_assertions=true、overflow_checks=true，仅debuginfo=0。先前 current10f 0/4、current10g 3/4完整失败日志保留；此前 current12 4/4属于较早同组实测，本段记录最新冻结后结果，不用历史数字替代当前结果。

四域均完整通过真实领域保存→实际TaskOutput→结账→同Work/root真实后继Attempt目录→raw/parsed连续全文/完整hash及原来源冻结、章节/未来范围、原作者与预算、foreign Work目录和双读取拒绝。报告用途严格为savedReport、权威为nativeSavedCheckpoint。revisionOutcome普通Work报告回读和跨Work负例保留；其后继writer隔离证据由同轮完整authoring组12/12验证：已有真实generation首次负评后合法修稿writer，原Work/root与真实先后身份保持，实际报告目录隐藏且已知source的raw/parsed均拒绝，合法原稿读取正例仍成功。报告本身没有产生新grant，不新增Run或SQL授权。

同轮Runtime lib 15/15、authoring 12/12、context 12/12及本组4/4合计43/43、所有actual exit0。五视角实际后审架构/技术A/技术B/产品/UI/UX均 APPROVE，零兼容 PASS；上述报告域和writer隔离直接验收条件已完成。Root最终完整Native和原Node201仍在运行、尚无全量终态；这些真实本机HTTP协议测试未调用DeepSeek，不证明文学判断正确、实际应用创作体验或前100章完成。
