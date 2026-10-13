# 候选版本参数的可纠正拒绝与精确保存证明

## 变更概览与模块归属

第 2 章原 Work 的正常 UI Retry 已恢复检查任务。模型首次遗漏 payload.writerTaskId，被工具 schema 拒绝；纠正后两次 submit_revision_outcome 把 expectedCandidateRevisionId 传成字符串 `"null"`，而该任务冻结的报告候选 head 是 JSON null。当前正文、作者来源、报告引用及输入绑定有效。Native 在 validate_task_write 的候选参数比较处正确拒绝，但错误被表达为 TaskInputStale，且保存事务未把这一明确的写前拒绝记入精确操作证明，两次调用保留 accepted。实际技术证据由 Root 保存于 `/tmp/autonomous-v3-current8-chapter2-report-argument-evidence.json`；不复制私有正文、作者原话、模型提示或凭据。

责任属于 Native Story Core 的通用候选任务保存合同（B03/B04/B06），不是模型文学判断、Host 工作组织或 Renderer 状态推断。仅改 tasks.rs、sqlite_repository.rs、recovery_operations.rs 的现有私有路径及既有 HTTP 合同测试。B01/B10 要求唯一 kind→tool 定义、真实故障对应回归和实际应用再验。Root 独占构建、测试、应用和真实模型。

## 目标合同与实施前方案

validate_task_write 保持 validate_active_task、真实输入 current、范围/输出类型、同一目标 object 全部现有校验。仅在这些已经通过后，调用方 expectedCandidateRevisionId 与 Native 冻结值或该任务最后一次成功保存版本不同，返回 PayloadValidation，reason 为 candidate_expectation_mismatch，instancePath 为 /expectedCandidateRevisionId。字段详情只包含已授权 outputObjectId、真实 expectedCandidateRevisionId 的 JSON string/null 值及固定纠错说明；不回显任意请求文本、不检索其他对象、不自动替换参数、不特判 null 一词。

save_candidate 的同一个 Immediate 事务在所有领域写入前，仅识别上述构造器明确拥有的新拒绝理由，使用既有精确 project/task/tool/call/request 绑定保存 rejected。候选 kind→tool 与成功 commit_candidate 复用一个私有定义，不在 Host、MCP 或调用方复制事实。原 draft InvalidContent 的确定拒绝保持。其他错误尤其 Storage/Corrupt，以及真实 TaskInputStale、版本变化或未知写入，继续保留现有保护；没有一般错误⇒rejected 的推断。提交拒绝证明失败仍返回存储错误并保留未知状态。

公开 HTTP 参数 schema 不变；现有 PayloadValidation 传输生成 invalid_output_payload 和字段详情，所有候选保存调用方同时使用这个通用 Native guard。MCP 已要求复制冻结 CAS 的 JSON 类型，模型可以据真实诊断自行重新选择参数并用新的调用身份提交。工程不决定检查结论、不重写正文、不建立固定重试文学流程。保存候选、采用和最终交付仍分开。

历史两条 accepted 不回填、不清除；这次 forward 修复不能追认旧调用结局。作者授权、原 Work、保存第 2 章、累计费用和预算账本均不修改。原任务当前的未知状态仍需 Root 依既有公开恢复合同处理；任何实际再验未完成不得宣布闭环。

## 五视角前审

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构专家 | APPROVE | Native 负责领域 CAS 与事务证明；模型纠正参数，Host/Engine/Renderer 不猜测成功或未知写入。只复用现有私有映射，没有新增公共恢复 API。 |
| 技术专家 A | APPROVE WITH CONDITIONS | 结构化详情只能在身份、输入 current 与 outputObjectId 校验后产生；在任何领域写入前的同一事务，只关闭确证的新理由；Storage/Corrupt 不得关闭。 |
| 技术专家 B | APPROVE WITH CONDITIONS | 既有真实 HTTP fixture 复现字符串 null 和其他错误 ID、输入正文 ID 混用、已有非空报告版本、同任务纠正；通用 brief 路径同步，真正存储故障仍 accepted。真实输入/跨身份拒绝继续覆盖。 |
| 产品经理 | APPROVE | 让应用模型自行纠正保存参数，作者不用承担技术指令；保持正文及费用事实，不把报告保存当创作目标完成。 |
| UI/UX 设计师 | APPROVE | 无新 UI；现有错误和执行证明能表达可定位参数拒绝，界面仍只投影真实回执和结果。 |

综合决策：按上述条件实施。独立只读静态方案复核 PASS。零兼容门禁 PASS：无迁移、回填、旧字段 fallback、双轨、降级、版本分支或兼容例外。

## 故障与验证映射

| 实际失败 / 责任模块 | 必要回归 | 证据边界与再验 |
| --- | --- | --- |
| 当前报告 task 的正确输入下，字符串 null 错误 CAS 被误称输入 stale；Native 通用 validate_task_write。 | 既有 report_prewrite_recovery_http_tests 增加候选参数回归，所有错误 CAS 得到同一字段拒绝和 JSON null；同 task、新调用、正确 null 保存得到 committed；后续版本只能使用该任务同报告对象的真实回执版本。 | 真实 HTTP/SQLite 证明工程合同，不能代替模型自行纠错。Root 通过现有应用路径观察真实模型读取字段反馈并交付当前报告。 |
| 同一事务的确定性 CAS 拒绝缺操作证明，留下 accepted；Native save_candidate 与 recovery_operations。 | 验证 domain heads/revisions/outputs/issues/task/budget 不变，只有精确调用 rejected；纠正调用 committed，旧拒绝不变；实际持久化失败仍 accepted。 | 旧生产 accepted 原样保留，不依据此次代码反推旧证明。Root 对受影响实际恢复路径和累计用量验证。 |
| 所有候选调用方共用该 guard；通用 brief 保存已保存后 null CAS 也应确定拒绝。 | 既有 recovery_ledger_http_tests 同步明确 CAS 拒绝、授权内真实非空版本诊断、正确非空 CAS 保存，以及通过数据库 trigger 制造真实 Storage accepted 未知负例。 | 保留跨 root/capability/replay/terminal 读取边界；不使用可纠正参数错模拟未知写入。 |

## 实施后复核

实际生产变更限三个文件：tasks.rs 在正确任务、真实输入 current 和同一 outputObjectId 通过后构造 candidate_expectation_mismatch；sqlite_repository.rs 在保存前的同一个 Immediate 事务只对该明确理由和原 draft InvalidContent 调用 reject_operation；recovery_operations.rs 将原 kind→tool 映射抽为私有 candidate_tool，成功提交与确证拒绝共用。没有其他错误分类、读取、权限、正文或预算变更。公开 schema、Runtime、Host、Engine、Renderer 均不新增分支或重复状态。

现有 report_prewrite_recovery_http_tests 新增 `candidate_expectation_rejection_is_exact_and_same_report_task_can_correct`，用真实已授权独立报告任务覆盖字符串 null、任意 ID、真实输入稿 ID 混用；相同安全字段拒绝、精确 rejected、无领域变化；正确 JSON null 保存、已有非空版本的错误 null 诊断及按真实回执在同 task 新调用保存；原拒绝证明不可改写；wrong thread/tool/task/project 拒绝不泄露授权报告对象或 head；实际正文变化仍 TaskInputStale。已有 storage failure、capability、wrong writer、冻结 criterion 和作者来源负例继续保留。

现有 recovery_ledger_http_tests 的通用 brief 用例不再把确定 CAS 错误当未知：验证候选参数拒绝、授权内非空 head、同 task 正确非空 CAS 成功；另以实际持久化 trigger 故障验证 accepted 未知、事务无部分领域写入、terminal 不能关闭未知。原成功证明、不同身份/请求 hash、restart、幂等回放和分页仍使用真实公开接口。

| 视角 | 静态后审 | 依据与限制 |
| --- | --- | --- |
| 架构专家 | APPROVE | 唯一 Native CAS 与保存事务权威，私有工具映射共用，无新增公共恢复能力或跨层文学流程。 |
| 技术专家 A | APPROVE | 字段诊断不先于身份/真实输入校验；只明确新理由可在任何领域写入前关闭 exact call；Storage/Corrupt、历史 accepted 和真正来源 stale 不变。 |
| 技术专家 B | APPROVE WITH CONDITIONS | 故障与报告/通用候选 HTTP 回归对应；Root 独占构建和测试，实际退出结果及当前编译源码身份待 Root 登记。 |
| 产品经理 | APPROVE WITH CONDITIONS | forward 修复让模型自行纠正技术参数，原草稿与费用不动；原第 2 章 Work 的历史 accepted 无既有合法清账入口，因此当前应用交付仍 blocked，不能以新合同替代该闭环。 |
| UI/UX 设计师 | APPROVE | 没有新增界面或要求作者提供内部参数；现有状态仍投影确证回执，未知不会被误报完成。 |

生产独立只读静态复核 PASS，无 must-fix；`git diff --check` 静态通过。零兼容及 B01–B10 边界复查 PASS。生产冻结 SHA256：tasks.rs 为 `5eab73ecdb64c6cef515e1edfc36328c7200e06285eba783a886730e83a84a42`；sqlite_repository.rs 为 `53a0320d2b780c4063df9a9eccde52f0e5d20b6e59d6cab3ce46243c179ea5cc`；recovery_operations.rs 为 `5362218f14a06cc81c9507a924b4949c257fa5a0568592e26ddca2c6a118336e`。

两个测试文件独立只读静态复核也 PASS，无 must-fix。最终测试 SHA256：report_prewrite_recovery_http_tests.rs 为 `9ff4b8b0dd506b8e20dc4a06d83c8a9293d8e1dc341c2cf86bf0e033ead3a66f`；recovery_ledger_http_tests.rs 为 `c8fab957fb96d49237a4b250fbaf89312f0da6f56f572b713428c3e544440353`。这仅确认 fixture、真实合同路径、身份/证明与静态实现一致，尚不声称运行通过。

最小当前合同命令：`cargo test --offline --locked --manifest-path native/Cargo.toml -p novel-codex-story-runtime --test report_prewrite_recovery_http_tests --test recovery_ledger_http_tests`。编写/静态审查不等于测试通过；Root 已核对冻结生产并构建，实际结果待登记。本子任务未运行构建、测试、应用、模型或操作生产数据库。

真实再验尚 blocked：两条历史 accepted 必须原样保留，不自动替它们回填 rejected，也不重建原 Work 绕过未知保护。原第 2 章不能据此次 forward 修复宣布完成。受影响应用路径未来是否能从明确新拒绝自行纠正，并生成真实可打开结果，仍需 Root 的实际模型与保存证据；不让作者追加内部参数来替应用验收。

## Root实际构建与合同验证

三生产文件、两测试文件SHA均与上述冻结版本完全相同。Native build:story-runtime实际exit0（8分02秒，开发debug/overflow策略未变）；Core完整lib266/266实际exit0，0失败/忽略/过滤；两完整Runtime目标实际exit0，recovery_ledger_http_tests为2/2、report_prewrite_recovery_http_tests为6/6，0失败/忽略/过滤；architecture检查332文件、7crate、0diagnostic，实际exit0。日志为 `/tmp/autonomous-v3-current9-candidate-expectation-native-build.log`、`-core-tests.log`、`-runtime-tests.log`、`-architecture.log`；源码/产物/日志身份汇总见 `/tmp/autonomous-v3-current9-candidate-expectation-build-proof.json`。技术B的构建与合同条件已满足，不冒称整体真实创作闭环。

Root确认原profile无活动Task/provider预留后，正常停止本次隔离实例（exit143）并启动新构建。实际UI仍可读第2章1100字正文、显示正文已保存/待独立检查；第1章仍已保存且检查通过。两次纯只读快照 `/tmp/autonomous-v3-current9-before-restart.json` 与 `/tmp/autonomous-v3-current9-after-restart.json` 证明任务、生成运行、章/版本、独立报告、作者记录、Native及Engine费用、操作账本逐项不变；历史两条accepted及所有unknown用量均保留。没有新作者消息、新Work、新模型调用或回填。

证据结论：本轮通用合同修复和对应回归通过，原模型路径的旧未知结果仍阻断。新字段反馈使模型自行纠正并交付的实际场景尚未验收；不点击原任务Retry、代改报告或清除账本获取绿色。产品与UI实际应用交付条件仍未完成。
