# 当前项目创建契约的真实集成 fixture 同步

日期：2026-10-08。五项为评审视角，不代表五名独立审查人员。

## 变更概览

统一 Node 验收日志 `/tmp/autonomous-v3-node-current-frozen5.log` 169 项中 163 项通过，6 项在实际 Runtime 的创建项目请求处快速失败。明确栈为 HTTP 422 `missing field logline`；五个测试对文本错误直接调用 `response.json()`，将原错误掩盖成 SyntaxError。当前 `CreateCreativeProjectRequest` 一次性要求 `title`、`logline`、`medium`、`idempotencyKey`，创建时同时建立当前默认小说 edition。六个旧 fixture 仅传 title。

目标：让这六个真实集成 fixture 通过当前项目创建 API 从空库创建小说，保留全部自主修订、授权、隔离、预算、独立验收、Runtime 重启及 CAS 业务断言。补齐四个必需字段；HTTP helper 先保留响应文本并断言当前 JSON 响应类型，让非 JSON 的状态和文本清楚可见。

非目标：不改生产 DTO、不增加默认字段或版本分支，不改共享 main/App/schema，不迁移任何数据，不提高超时、不替换真实 Core/Native/MCP、不放松门禁，不操作实际应用或外部模型。

## 零兼容门禁

PASS：只同步当前 fixture；不新增旧字段 fallback、迁移、回填、双轨或兼容接口。临时数据库仍从空环境初始化。原失败日志保留。

## 实施前五视角评审

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 使用当前统一项目创建契约和当前默认 edition，不绕过实际 Runtime。 |
| 技术 A | APPROVE WITH CONDITIONS | 项目创建提供当前必需参数及每库独立幂等标识；原 token、任务、预算、CAS 与拒绝断言不变。 |
| 技术 B | APPROVE WITH CONDITIONS | 仅修复六个终态失败 fixture；先定向验证，后由 Root 运行完整 Node 链；保留非 JSON HTTP 状态和文本。 |
| 产品 | APPROVE WITH CONDITIONS | 验收仍证明一条作者输入的应用自主闭环；fixture 同步不能被写成真实模型文学质量证据。 |
| UI/UX | APPROVE WITH CONDITIONS | 不改实际产品界面；重启后的界面快照与恢复交互断言原样保留。 |

综合批准最小 fixture 同步，无生产改动或兼容例外。若后继错误涉及生产或改变业务含义，先与 Root 协调。

## 首轮后继合同同步评审（实施前）

补齐创建参数后的定向日志 `/tmp/autonomous-v3-current-project-fixtures-escalated-20261008.log` 为 1/6 通过：实际子稿隔离与整合已通过；五个后继错误保留。Root 负责 Host 最终交付错误的生产定位，另一代理负责当前 UI 来源预览合同；本代理继续只同步三项 fixture。

- 初始失败恢复与候选链的合成模型，最终提交前须通过真实 Candidate Context 发现作者要求来源，按 Native cursor 连续读完，核对 byteRange/hash 和实际原话。不制造阅读回执。
- scoped 实际 Core fixture 保留目录非全文、完整分页、受保护正文及知识断言，对当前 workspace reference 明确断言新增的 `view=workspace` 和说明字段；使用产品当前每工具输出策略，不用旧 config。
- 候选链不再将未来简报写入前预登记的大纲当作有效冻结来源。新简报实际保存后，合成模型在同 Work 的新 advisory 中读取当前来源、选择 Native 提供的原已授权大纲动作，再起草。仍只允许一个最终交付协议修订，保留未采用、预算不扩张和 Host 不能代写 deliveryReview 的断言。

五视角再次为 APPROVE WITH CONDITIONS：架构要求只模拟模型决定而不替 Host 决策；技术 A 要求实际来源 cursor/hash 与当前作者阅读全文证据；技术 B 要求原失败保留、仅必要定向测试、不抬超时；产品要求同一作者输入和同一 Work 的闭环；UI/UX 要求不变更产品或掩盖交付失败。零兼容 PASS，无生产改动。

## 实施后复核

### 当前方法计划与最终交付合同同步（实施前）

Root 修复当前规划批次准入后，真实候选链已到最终 advisory；`/tmp/autonomous-v3-planning-batch-current.log` 显示旧 fixture 把方法计划的 pendingGoals 当作 canFinish 的领域义务。当前计划明示 `authority=modelMethodsOnly`，不能替代原作者范围、保存回执与最终审核。本次改为断言最终审核仍必需、实际 Work 的 deliveryReview 尚空，并通过真实 Candidate 裸 complete 请求证明 Native 拒绝未阅读全文/无自检交付；随后仅一次同租约模型协议修正读取完整作者源并提交当前审核，实际 Work 才完成。rebind 末步选择动作与计划更新后重新发现并完整读取当前原要求源，保留独立 passed 报告、hash/cursor 与最终 CAS 门禁。

五视角均 APPROVE WITH CONDITIONS：架构坚持模型方法与领域义务分离；技术 A 保留真实 Native 拒绝及完整来源读取；技术 B 保留原失败、预算和一次修正断言；产品坚持同一输入实际成果交付；UI/UX 不隐藏失败、不修改界面。零兼容 PASS。仅同步受控模型 fixture，Native 领域门禁不变。

### 当前执行所有者测试身份同步（实施前）

`task-provider-budget.test.mjs` 的生产恢复路径使用明确 Work 但旧 fixture 缺 threadId。同步为受控 app-server 的实际根线程 `thr-test`，不通过生产 fallback 补身份。当前 owner/renew/release 合同由集中 harness 严格验证；三个成功/执行失败/预检失败仍验证原持久预算、结算次数和请求限额。五视角均 APPROVE WITH CONDITIONS：架构与技术 A 要求任务/Work/owner 精确同根；技术 B 要求原结算断言不变；产品与 UI/UX 要求只同步验收身份、不新增授权或界面。零兼容 PASS。范围仅此 budget fixture；其他已改文件不并发处理。

同一五视角复核明确区分真实 HTTP 与集中 mock：自有两个真实 Runtime 集成 `work-decision-runtime`、`initial-candidate-failure` 采用当前 harness 的 `nativeExecutionOwners:true`，把 owner 获得/续租/释放直接传到 Native，不能凭集中 mock 当作真实 owner 验收。其他自主 Core 测试由 Root 同步；没有生产回退。

六个项目创建均同步当前四个必需字段，五个 HTTP helper 保留非 JSON 状态与原文本。临时库由当前创建接口建立默认小说 edition，不改生产 DTO。候选链同步当前模型重新组织步骤；最终模型通过实际 Candidate 源分页读取作者原要求。scoped 配置使用当前产品 per-tool 输出额度；workspace reference 明确断言当前 view 与说明，而不是强制旧形状。

| 复核视角 | 结论 | 证据/限制 |
| --- | --- | --- |
| 架构 | APPROVE | 未增加兼容路径；当前项目/来源/工作区契约按真实 Runtime 验证。 |
| 技术 A | APPROVE | 保留任务绑定、作者阅读全文、分页 hash、未采用、独立评估、预算、CAS；当前候选链通过真实 Native owner 获得/释放与两次独立真实拒绝，不由 Host 代写审核。 |
| 技术 B | APPROVE WITH CONDITIONS | 下列原失败与必要复验保留；当前 scoped、rebind、候选链、预算通过，新的真实 owner opt-in 初始失败用例与完整 Node 链由 Root 统一复验。 |
| 产品 | APPROVE WITH CONDITIONS | 测试仍只证明机制；真实 DeepSeek 自主生成验收另记。 |
| UI/UX | APPROVE | 本代理只补 rebind seed 当前 requirements 分页聚合；当前界面与快照由负责代理维护，不隐藏真实失败。 |

当前实际结果：

- 原完整 Node 链 `/tmp/autonomous-v3-node-current-frozen5.log`：169 项，163 通过、6 失败；创建请求缺字段，保留。
- 首次沙箱定向 `/tmp/autonomous-v3-current-project-fixtures-20261008.log`：6 项在 Runtime 监听前退出，未执行业务断言；隔离本机运行获准后重试，不抬超时。
- 完整创建参数的定向 `/tmp/autonomous-v3-current-project-fixtures-escalated-20261008.log`：1/6 通过，实际 Native+MCP 子智能体隔离/不可变候选整合通过；其余五个后继合同或生产错误被保留。
- 作者要求阅读全文同步后的 `/tmp/autonomous-v3-current-source-fixtures-20261008.log`：initial persisted worker failure 同原 Work 自主修正通过；scoped 强回执断言发现生产 MCP structuredContent 用恢复证明覆盖业务结果，另写独立设计评审修复；Workchain 定位原大纲已尝试失效来源并失败，交 Root 修 Host 当前批次准入。
- Workchain 状态诊断 `/tmp/autonomous-v3-work-outline-diagnostic-20261008.log`：原大纲 failed/TaskBindingInvalid，不能伪装为未尝试授权动作；原强动作断言保留。
- rebind seed 的完整 readDecision 已同步聚合每一页 requirements，保留每页 version/sourceDigest 及 current/passed/report 精确断言；负责代理继续验证当前 UI 合同。
- Root 当前规划准入后，方法计划不再被 fixture 当作领域义务；裸 complete 仍要求真实 Native 拒绝，再由唯一同租约模型回合阅读全文并自检。rebind 最终动作后重新读真正原 run 输入与实际 UI 追加原话，不把父步骤的截短调度句子当作完整作者要求；原失败日志由负责代理保留。
- 当前 owner budget fixture 仅补实际 `thr-test` 根身份；`/tmp/autonomous-v3-current-owner-budget-fixture-20261008.log` 12/12 通过，包含成功、执行失败、预检失败与结算失败原限额/结算断言。架构、技术 A/B、产品、UI/UX 实施后复核均 APPROVE，此同步未改生产授权、费用或界面。
- MCP HTTP 业务结果修复后的实际 Core scoped `/tmp/autonomous-v3-mcp-current-core-scoped-20261008.log` 1/1 通过，原 scope、保存、独立评估、建模和原 Run 续章断言保留。rebind 负责代理 `/tmp/autonomous-v3-rebind-current-ui-accepted3.log` 1/1 通过：重新读真正原 run 与 UI 追加原话后，独立 passed 报告和完整读取门禁通过，原失败记录保留。新的真实 Native owner opt-in 与 MCP 字符串诊断增量由最终统一链复验。
- 当前候选链 `/tmp/autonomous-v3-current-native-owner-workchain-final-20261008.log` 1/1 通过（5.54 秒）。真实最新 Runtime owner 获得/释放均直接被调用。负例分别要求 HTTP 400/invalid_request 的原要求来源完整读取与必需 deliveryReview 拒绝，并核对拒绝前后 Task DTO 完全一致、Work.deliveryReview 仍为空；唯一第 6 回合同租约模型读取完整原话并提交当前审核后实际 Work completed/fulfilled。方法计划不替代领域义务、批准批次后来源刷新接力、同一 Work、原预算、未采用和 Host 不代写强断言都保留。先前两次新增 fixture 负例失败是中文词序与任务必为 running 的错误假设，原日志分别 `/tmp/autonomous-v3-current-native-owner-workchain-20261008.log`、`/tmp/autonomous-v3-current-native-owner-workchain2-20261008.log` 保留；改为读取真实错误 JSON 和任务前后精确等值，没有放松 Native 门禁。

零兼容复查 PASS。本代理当前最小 fixture 范围已通过上述定向证据；最新 owner 初始失败用例、最终 MCP bin 和完整 Node 链仍由 Root 统一验收。真实模型文学质量另验，不能由合成响应的机制通过替代。
