# 独立评审恢复的产出者与失败来源绑定

## 实际故障与责任

current11 在原对话仅点击一次既有重试后，模型自行登记写作、保存新稿、登记独立评审并在评审失败后登记重评。现 action-40ab29bc… 的冻结目标仍指向已完成写手 task-ffe942e… 与其真实稿件 draft-c3c386b…；通用失败恢复的 source_task_id 则准确记录失败评审 task-1326b4ec…。revision_outcome::validate_target 将这个恢复来源等同被评稿件的写手，真实 POST /tasks 返回 TaskBindingInvalid，排队步骤未创建执行任务。不是模型没有登记行动，也不是保存结果未知。

故障证据：`/tmp/autonomous-v3-current11-queued-critic-binding-diagnostic-20261008T173123Z-a9c871ed.json`、`/tmp/autonomous-v3-current11-choice-contract-after-critic-retry-registration.json`；原失败及队列保留。另有本轮写手 provider receipt 的44708未知用量，独立调查，不能通过本修复清零或承诺自动派发已解阻。

## 模块归属与最小方案

责任为 Story Core 的领域产物来源验证，适用 B01/B03/B05/B06/B10。公开链为模型选择现行 readyAction → Native 登记领域动作 → Host 经 Runtime POST /tasks → Core 验证注册目标和可信执行身份。MCP、Engine、Codex Core、Host、Renderer不推断或重写这个来源关系。

仅调整 `native/story-core/src/revision_outcome.rs`：

- 被评稿件产出者取自 Native 已登记目标的 writerTaskId/draftRevisionId，并保留实际 writer 为同project、同Work、completed、最新chapterComplete own output、作者来源及原版关系的校验。
- 逐值核对动作与步骤冻结目标、实际目标去 resumeStepId，父决策与登记 decisionTaskId/同Work/完成状态相符。
- 分别验证 selection source 的明确角色。正常 generateChapter 来源必须是该产出者；失败 assessRevisionOutcome 来源必须是同project/同Work此前的失败评审，已具备现行可恢复的known-zero持久证明，支持的失败码，且 failure_target 去内部绑定后的冻结目标与本次一致。若本次附恢复计划，其 failedTaskId 必须绑定这个失败来源。其他角色、跨项目/Work、伪造/不确定来源均拒绝。
- 保留 current inputs、grant、取消、原Work预算、同源恢复次数和策略限制。不修改 generic failed_actions，不补写旧记录，不绕过 queued action，不新增作者输入。

目标是当前领域身份正确，不是对旧格式兼容。正常注册与失败恢复是当前协议的两个语义角色，无版本探测、旧字段fallback、迁移、双写或错误推断。若实现需要额外模块或策略，先复核范围。不能新增时变证明要求使已完成且发布的正常报告在后来head变化时失去历史来源；当前性仍由既有独立检查处理。

模型继续决定是否评审、如何评审和如何修订。工程只保证被评对象、来源、恢复和账本精确一致。当前新稿1211非空白字符，并改动原末段之前一处标点，因此这次参数/派发修复不构成创作交付通过。

## 五视角实施前审

| 视角 | 结论 | 验证条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 两种身份由同一领域权威分别验证，邻层不补写来源。 |
| 技术A | APPROVE WITH CONDITIONS | 保留来源、权限、当前输入、取消、预算与unknown fence；不因删除错误等同关系放松source真实性。 |
| 技术B | APPROVE WITH CONDITIONS | 使用真实HTTP入站、现行fixture产出的writer/failedcritic/provider receipt/ready action，逐个测试拒绝事务无副作用。 |
| 产品 | APPROVE | 原对话可自行重评；不要求作者解释工具、追加内部参数或放松标准。 |
| UI/UX | APPROVE | 只展示真实执行/保存/核验状态，保持原作者行数与稿件入口，不虚报完成。 |

独立前审确认：父决策Task必须是completed parent步骤的currentAttempt；失败source必须严格早于本次parent决策。恢复来源不依赖现readyActions/剩余预算/当前作者头或活动grant判断历史有效性，复用既有known-zero终态证明。综合允许窄实施，无兼容例外。生产由Native agent负责；真实HTTP回归由独立review agent在`revision_outcome/recovery_source_tests.rs`与入口模块登记，Root统筹构建、冻结和应用再验。

## 故障与回归映射

实际source角色混淆 → 真实writer保存完成、critic零产物失败且已真实结算、模型决策登记ready action、父决策完成、原排队target加入resumeStepId的POST /tasks应成功。覆盖正常来源、首次失败重评及再次失败的恢复计划来源。

拒绝边界包括：跨project/Work或错误writer、非latest own output、原版/稿件/章节/batch错，步骤与登记目标或父决策错、未完成parent、source其他operation或foreign/错误writer/非known-zero/未结算/unknown/unsupported failure，错恢复计划/失败冻结目标。受影响取消与当前head变化仍拒绝，不改变Candidate与Author准入权限。拒绝后任务、产物、队列和账本不产生新的成功事实。

执行受影响真实HTTP目标与Core回归、生产Native构建、architecture:check及五视角后审。测试文件由实施分工后记录。已有Engine/MCP/Codex合同可引用已冻结的构建证据，不能冒用为本次Native验证。应用再验沿同QA原对话和queued Work进行，保留旧费用和失败；未知用量未核实时保留阻断，并明确实际尚未通过。

## 实施后复核

Native已实施：冻结`revision_outcome.rs` SHA `18af0829710556e01d015331b9ab6143c02be260c2cf7d2b945c2f156e46afa2`，真实前快照及本轮精确增量为`/tmp/autonomous-v3-current11-revision-assessment-recovery-before.rs`和`/tmp/autonomous-v3-current11-revision-assessment-recovery-production.diff`。被评稿件的实际writer与恢复选择的失败critic分别验证；父决策、Work/project、全局attempt顺序、完成写手的最新自有正文、原版本、batch及失败来源冻结目标仍精确绑定。没有把当前readyActions或剩余预算加入历史来源核验，也没有改通用恢复策略、费用或原记录。

五视角静态后审批准上述职责分离与拒绝边界；第一阶段Core266/266、Work决策HTTP12/12、事件HTTP7/7、Native bins与架构332文件/7crates/0diagnostics均实际exit0。随后另发现未知用量跨入口准入故障并修复，最终统一源码的相关验证另行记录，不能冒用第一阶段结果覆盖后续改动。

真实Router回归保留前三轮日志：新增fixture先后错误读取非公开字段、终态后获取活动grant、跨project负例引用未创建实体。第三轮43/44，两条公开正常恢复与报告历史路径已通过；剩余负例fixture已改为创建真实外部project/Work，不禁用外键或降低断言。第四轮完整44/44实际exit0，日志`/tmp/autonomous-v3-recovery-source-revision-outcomes-fourth.log`；真实HTTP源身份回归包括首次和再次失败评审恢复及25项来源/权限/CAS拒绝边界。随后最终统一Core273/273、Work决策HTTP12/12和Native bins构建均实际exit0；这些新证据不覆盖尚未结束的事件/预算目标或实际界面验收。原应用44708未知用量无法用历史汇总补核，新Task应被费用门禁阻断，因此本次不能声称原queued评审已再次执行或局部改稿已交付。
