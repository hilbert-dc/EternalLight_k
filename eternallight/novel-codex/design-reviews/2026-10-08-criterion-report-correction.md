# 冻结报告检查项与可纠正反馈

## 责任、事实与公共合同

责任为 Story Core 的冻结报告检查项、通用 typed payload validation，以及 Runtime 结果投影、MCP 声明语义（B01/B03/B04/B05/B06/B07/B10；保持其余规则）。不修改 `codex-rs` Core、Engine、Host、UI、作者输入、模型决策或费用。`assessment_outcomes` 是检查项的唯一权威；模型可作负评、提出遗漏与选择修稿，不能自行增加 `outcomes` ID。篇幅由既有 Native `actualLength` 客观检查，不要求新增文学流程。

真实样本2的报告有3个冻结检查项，却提交第4个模型自造的篇幅 ID；正确门禁只返回 `TaskOutputForbidden/details={}`，缺少纠正依据。提前完成返回 `TaskOutputMissing`，事务未关闭任务。该失败样本保留；本次测试使用独立合成内容，不复制私有稿件或引文。Native recovery ledger 接线由另一负责人实施，未知费用不因本补丁放行。

方案：上下文由同一 `assessment_outcomes` 输出 `criterionContract`（冻结引用、精确 IDs、恰好一次、篇幅独立权威及遗漏原要求入口）。Story Core 新通用 `RepositoryError::PayloadValidation {code,instance_path,details}`；检查项不一致返回来源化 expected/missing/unexpected/duplicate IDs。ID反馈只接受有限 ASCII 合法形式，最多16项，非法值仅报索引；不携带正文、reason、引文、凭据。Runtime 以结构化400投影，不自动删项/补项/改结论，不冒充 MCP 本地未派发证明。MCP schema 保留当前形状，仅明确 id 描述。

## 五视角前审

| 视角 | 结论与条件 |
| --- | --- |
| 架构 | APPROVE：冻结事实归 Native，同一权威构造 context/feedback；通用 typed error 不承载小说流程。 |
| 技术 A | APPROVE WITH CONDITIONS：保留24KB、完整来源阅读、Task身份、CAS、保护、篇幅、原预算/未知结果门禁；拒绝必须无作品/报告/issue/任务终态变化。 |
| 技术 B | APPROVE WITH CONDITIONS：真实合成3项+额外第4项，精确诊断后同Task纠正并保存合法负评、完成；missing/duplicate/foreign/stale及现有全部负例保持，Root唯一执行。 |
| 产品 | APPROVE：一句简单请求不要求作者纠正技术字段；未达篇幅仍未交付，不降低标准。 |
| UI/UX | APPROVE：反馈供模型纠正，技术细节不新增作者交互；真实保存、评估与交付继续区分。 |

零兼容 PASS：当前合同一次性同步，无 fallback、版本分支、迁移、回填、双轨或自动重试。MCP schema 描述改变会影响真实工具 fingerprint，须重建当前 Native/MCP并重新验证模型资格；不把已构建 codex-rs 保真补丁重复改动。

## 实施后复核

待本范围静态冻结与 Root 定向验证；自动化、受控provider和真实样本分别记录，不将本修复或样本2的已保存短稿记为完整创作交付。

### 定向验收发现的测试投影错误

联合 cargo 实际 exit101：新增 Router/SQLite 回执3/3通过，原报告目标39/40；新增正向纠正用例在 `checks.as_array()` 失败。公开决策状态本来就是有界摘要：`checks` 转为 `checkCounts`，零遗漏计数省略，篇幅上下限在顶层 `chapterAcceptance`。保存的完整报告仍全等且已有独立测试负评通过，不是生产丢报告。本次只同步该新增测试的读取位置，严格验证3项全部 supported、完整报告遗漏清单为空、当前实际篇幅未达标、750冻结下限、notMet、canFinish=false；所有身份、状态、副作用、同Task纠正断言保持。原失败日志保留，不修改生产投影或标准。

五视角前审：架构 PASS（读取既有公开摘要）；技术A PASS（原报告与客观篇幅双证据）；技术B PASS（40项全目标复验，3项回执原证据保留）；产品 PASS（仍拒绝短正文交付）；UI/UX PASS（无界面改动）。独立只读复核确认相同根因与修正范围。零兼容 PASS，无生产行为变化。

### 实际定向验证及五视角后审

Native构建exit0；Core完整lib265/265、真实Native→Core→Engine目录35/35、真实Core与受控provider自主修订链1/1，分别实际exit0。Runtime报告目标fixture纠正后完整40/40实际exit0；新增进程内生产Router/SQLite回执3/3实际通过。原39/40 exit101日志保留。架构332文件/7crate、0诊断；冻结生产代码未再变化，只有上述测试读取修正。证据日志均在当前实施验收记录列明。

后审：架构 PASS（唯一Native事实权威，未改codex-rs）；技术A PASS（集合/权限/来源/预算/确定与未知写入正负证据）；技术B PASS（实际当前生产目录及完整报告目标，不靠简化fixture）；产品 PASS（纠正报告不使短稿达标）；UI/UX PASS（没有新增技术参数或作者输入）。独立只读审查未发现必须修复项。真实样本3仅通过应用发送一次短输入，实际创作交付仍须观察，不由自动化结果代替。
