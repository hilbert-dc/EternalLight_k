# Host 自主创作与上下文分离评审

日期：2026-10-07。范围：Electron Host、当前 Codex App Server 协议、Node 旅程测试。下列是五种审查视角，不是五位独立人员。

## 变更概览

目标：一次作者目标后，由同一 Work 中的协调模型安排起草、独立评估、读回失败报告、定向修订、复评及接续。Host 只执行 Native 已登记步骤，预算、权限、未知写入和完成门禁保持 Native 所有。

当前输入将方法、任务绑定和工作规则混合成巨大 user 消息；继续决策恢复同一 Engine 历史，在压缩中保留内部输入而遗忘原文。改用当前 turn/start.additionalContext：固定产品规则为 application，冻结方法、作者背景、候选指令及任务绑定为 untrusted。根对话 input 保持作者原文；内部步骤仅短触发，协调仍恢复同一 Engine 线程，保持 Native 对 task/thread 的严格绑定。writer 与独立 critic 按已有任务机制开启新 Engine 会话。模型从 read_work_plan、read_work_status、read_context 当前源/持久返回范围继续，摘要不构成已读证据。

独立 critic 开启新会话，仅凭冻结合同、当前保存稿及必要原稿/前文逐项判断。未满足/证据不足需要保存问题并由协调模型调整方法，在原章授权内修订与重评；不可把保存报告、writer 声明或 canFinish 当通过。

非目标：上游 Codex 内核改动、故事关键词分流、额外自动预算、新根 Work、旧接口适配或 schema 迁移。

## 零兼容门禁

PASS：单一 {input,additionalContext} 当前 turn 契约，同步生产调用、fixture 和测试；没有旧格式 fallback、双读双写、回填、迁移或降级。应用参数依赖当前嵌入式 App Server additionalContext 能力，缺失时明确失败，不退回巨大 user 消息。

## 五视角实施前审查

|视角|结论|条件|
|---|---|---|
|架构|APPROVE WITH CONDITIONS|决策仍由模型且 Native 持久登记；内部触发不能冒充新用户输入或另建 Work；写作/评估独立上下文。|
|技术 A|APPROVE WITH CONDITIONS|author/source/方法保持 untrusted，固定规则可 application；绑定不可被模型文本提升权限；合计输入预算仍 9000 token/38000 bytes，未知写入 fail closed。|
|技术 B|APPROVE WITH CONDITIONS|所有生产入口和非UI tests 一次切换；脚本 bad draft→failed assessment→targeted repair→pass→next chapter 必须实际测试，包含 fresh session 和 original Work 归属。|
|产品|APPROVE WITH CONDITIONS|一次作者输入后无需外部逐条故事纠错；缺证据和无进展真实停止，不能以重写次数或保存成功算完成。|
|UI/UX|APPROVE WITH CONDITIONS|内部触发/任务包不显示成重复作者输入；原会话继续显示简洁进展与真实结果；详情才暴露工具记录。|

## 实施后复核

当前协议代码证据：Codex `codex-rs/core/src/event_mapping.rs::parse_user_message` 对 additionalContext 的 untrusted fragment 返回 None；`compact.rs::collect_annotated_user_messages` 使用同一映射，因此不会将任务包压缩为原作者输入。这使协调保持同线程成为可行方案，无须更换协调线程或放宽 Native 身份。本轮不改上游 Codex。

|视角|实施后结论|核验|
|---|---|---|
|架构|APPROVE|一次作者目标后，生产 main、真实 Core/Bridge/MCP、真实 Native 和持久预算在一个 Work 完成坏稿→新会话独立负评→协调模型实际读分页报告与引文→模型登记定向修订→新会话独立正评→Native fulfilled。Host 不提交故事判断或交付评估。|
|技术 A|APPROVE|新 turn 契约把原作者、方法与来源保持为 untrusted；真实压缩旅程保留同线程，旧任务片段不成为作者输入。9000 tokens/38000 bytes 总预算和原 Work 额度不变；完整闭环仅一次初始额度且无增额。未知写入仍需真实 ledger；read_revision_outcome 属只读恢复，pause_generation_run 不属只读。|
|技术 B|APPROVE|当前接口同步，所有 writer 必须冻结 chapterAcceptance，独立 report 目录 textDelivered=false 须继续分页读取。直接章节生成先模型 advisory 再 writer/critic。普通与 generation 暂停/取消统一 Native Work 单事务 CAS，未用 Run.version 替代 Work.version。已提交暂停在 task 创建到 Host bind 的间隙也阻止首次 inference。|
|产品|APPROVE|完整闭环中模型读 unmet 精确证据并在持久计划记录改法，自主修复后才交付，无第二次作者纠错或新根预算。无进展、证据不足及未知写的失败门禁保留。|
|UI/UX|APPROVE WITH CONDITIONS|Host 内部输入为短触发并明确没有新增作者要求；执行包移出作者消息。真实 Core 压缩后只保留一次原作者，模型暂停只按真实已提交回执收口所属 lease。实际 UI 和 DeepSeek 文学效果仍由根任务验收。|

最终证据：`/tmp/host-autonomous-real-engine-final.log` 64/64 通过，含 `autonomous-creation-engine.integration.mjs` 的完整工程闭环与 `autonomous-context.integration.mjs` 的真实压缩旅程；`/tmp/host-autonomous-directed-last.log` 64/64 通过，含不同 Work 不误停、真实 Native 回执 fresh 重查后仅中断所属 lease、统一 Work CAS、只读报告恢复和四种创建/bind 间控制竞态。`/tmp/host-autonomous-policy-broad-v2.log` 33/33、`/tmp/host-autonomous-protocols-final.log` 18/18 通过。前述批次包含重叠测试，不相加作唯一测试总数。

## 最终 Native 契约的广泛回归

本轮继续以 `NOVEL_CODEX_NATIVE_DIR=/tmp/autonomous-native-target/debug` 显式选择最终 bins5，使用真实 Core/Bridge/MCP/Native 和隔离的临时数据库。测试构建选择器只允许两种现行 Native 名称与绝对目录；不存在程序仍直接失败，不选择另一个协议或补旧字段。`read_work_status` 按当前 1 requirement / 2 actions / 2 receipts 分页，脚本模型沿 `nextCursor` 读完全部状态再选择动作。状态仅提供 `checkCounts` 与当前结论；模型须通过 `read_revision_outcome` 的目录引用实际分页读取完整原因与原文引证。

完整自主坏稿修复旅程在最终 Native 上通过，见 `/tmp/host-engine-final-native-current-v7.log` 的 `one author goal autonomously repairs a failed independent assessment before delivery`。它实际保存未满足稿，新会话独立报告 unmet，协调模型读完整分页报告、保存改法并登记原范围定向修订，再由新会话独立报告 supported，最后由 Native fulfilled；没有第二次作者纠错、Host 故事情节判断、额外 Work 或预算增加。批次中的另一项旧流程数组漏列新增的续写独立评估，真实续写与交付断言已经通过，随后同步当前数组；`/tmp/host-scoped-final-current-native-v8.log` 单独复核 1/1 通过，涵盖修订、建模、继任连续 Work、下一章首写及独立验收、反馈、真实交付计量与读取历史回执不增用量。

广泛回归首轮：`/tmp/host-creative-journey-broad-final.log` 为 85/86，`/tmp/host-agent-broad-final.log` 为 435/437。共同失败为上述流程数组遗漏，已由当前 Native 定向复核收口；Agent 另一个失败为三组本地 CPU 密集测试同时运行时的 2 秒性能断言。保持原阈值单独运行，`/tmp/host-author-input-perf-isolated-final.log` 4/4 通过，确认完整 Unicode 作者原文没有截断。上述两项均通过定向复核，因此两组的全部用例已完成验收；通过项不重复运行、不把重叠批次相加，也不将首轮失败日志称为全绿。生产 Host/harness 最终定向 `/tmp/host-harness-final-regression.log` 54/54 通过。

广泛同步只修当前 fixture/caller：移除已废止 `read_context.purpose` 输入；保留 Native 的可信用途派生；补齐冻结 Work.goal/step.instruction、mandatory chapterAcceptance 和所有验收目标；完成回执按当前 Runtime DTO 判读；恢复使用持久任务、原预算和现行 handoff 形状。修正测试 harness 的 GET provider-budget 意外将任务 terminalize 的错误，恢复 queued 原断言。未增加生产 fallback、未降低通过标准。

实际并发旅程也发现新的 Native 来源返回 journal 在 DEFERRED 读事务中升级写入会触发 SQLITE_BUSY。Native 负责人改为预先 BEGIN IMMEDIATE，读取与真实返回范围 journal 原子提交；没有重试或放宽证据校验。最终 bins5 的并发回归与完整自主旅程均验证了该修复。

职责边界代码：`main.mjs::forwardAgentEvent` 调用 `interruptCommittedGenerationPause`，先重新读取 Native 已提交 Run/Work 归属和状态才中断当前 lease；`main.mjs::bindWorkFlowAttempt` 对已经 paused/cancelled 的真实 Work 阻止派发，不 PATCH 或 cancel 覆盖其状态；`agent:control-work-task` 经单一 `/work-tasks/:id/control` Work CAS 返回匹配 id/project/thread/status/新版本回执。模型语义仍由模型的 pause_generation_run 决策，不检测“暂停”或“继续”关键词。

边界：脚本 provider 只能证明工程机制，不证明真实 DeepSeek 文学判断或本书目标达成；实际模型创作仍由根任务在真实应用中验收。
