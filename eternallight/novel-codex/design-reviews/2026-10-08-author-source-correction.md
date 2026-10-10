# 作者原话来源引用的可纠正拒绝

## 责任与合同（实施前）

实机第 2 章检查任务 `task-ee3f013053ef4239bc216eb551129206` 的两次报告提交均为真实 Native rejected，未保存报告。冻结 writer/chapter/draft/original/knowledge 引用、四个标准 ID 和十二处正文证据均正确；唯一遗漏条目的 authorSourceChecks 却引用 `/operationContext/revisionOutcomeInput` 的 sourceId/hash，而不是已经完整读取的 `/authorRequirementsSource`。`validate_uncovered` 调用共用 `validate_author_requirement_quote`，原同 Work / 作者 section SQL 拒绝该引用，返回泛化 TaskOutputForbidden。技术证据见 `/tmp/autonomous-v3-current6-chapter2-report-technical-evidence.json` 和 terminal/extended-proof 元数据；不复制正文、引文或凭据。

本次归 Story Core 的来源合同与可纠正错误，以及 MCP 的同一来源说明。适用 B01、B03–B06、B10：Native 仍唯一拥有领域权限、作者来源、事务与交付事实；MCP 仅传输规则和真实错误；Runtime 复用既有 PayloadValidation 错误响应和 prewrite rejected 证明，不新增接口或结果判定。B02/B07/B08/B09 不新增文学流水线、方法、界面或内核行为。

共用来源查询仅在原“同项目、同 Work、作者 section 的真实来源”查询无结果时给出 `author_requirement_source_not_admitted` 的结构化错误。错误仅包含固定期望 section 与说明，不回显任意输入，不透露给定 ID 是否存在于其他 Work，不返回其他 Work 的来源、摘要、原话或权限。原 hash、当前作者要求、真实引用、主任务实际读取及来源新鲜度检查全部保留。

相对字段路径为 `/sourceId`。报告调用方在 `/uncoveredRequirements` 下附带遗漏条目与 authorSourceCheck 的索引、相对字段路径，供模型定位；不扩充通用 RepositoryError 类型、不改变其他来源调用方路径。MCP 共用 author_quote 说明要求 sourceId/hash 来自当前真正的 authorRequirementsSource，quote 为其 authorText 原话；模型解释、chapterAcceptance、任务说明、规划和检查输入不因此升级为作者原要求。

模型决定该意见是否属于作者遗漏、冻结检查标准或自身建议，以及后续如何创作。工程不自动替换引用、删除遗漏、改结论、重写草稿、扩大授权或清零用量。报告纠正后可以保存，但非空遗漏仍阻止检查通过与交付；空遗漏路径原样保留。新反馈不要求作者输入内部技术参数。

## 五视角前审与零兼容

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构专家 | APPROVE | 修改共用 Native 来源验证与 MCP 声明，复用既有错误/恢复公开合同；无第二权限或文学决策层。 |
| 技术专家 A | APPROVE WITH CONDITIONS | 无结果时统一拒绝且零其他 Work 信息泄露；全部原来源/hash/current/quote/read guards 保留；明确预写拒绝仍有精确 ledger。 |
| 技术专家 B | APPROVE WITH CONDITIONS | 真实 Router/SQLite 合同覆盖错误 section、未知来源、其他 Work 来源，以及同当前任务修正来源后保存、非空遗漏不通过；Root 唯一构建/运行验证。 |
| 产品经理 | APPROVE | 应用得到能自主核对的反馈；不让作者指导技术纠错，也不替模型决定文学建议是否保留。 |
| UI/UX 设计师 | APPROVE | 不新增界面；错误供工具调用者定位，用户主线仍为真实进展与成果。 |

综合结论：按条件实施。无兼容例外、版本分支、迁移、回填、fallback、双读写或自动恢复。只调整当前来源拒绝的说明，不降低任何交付标准。

## 验证计划及边界

在现有真实 Runtime 报告 HTTP 合同中添加来源拒绝与同任务纠正用例：确定 rejected 可查询、作品/任务/预算不变；纠正来源后原候选保存 committed，非空遗漏仍 notMet/不能交付。已有空遗漏、错误 hash/current/quote/read 与鉴权负例保持。MCP schema 描述变更由实际生产工具目录贯通检查验证，不使用简化工具 fixture 取代。

Root 执行定向 Runtime、Core 来源读取与 MCP/目录合同、原 Native 构建、architecture:check；本子任务只实施与静态复核，不构建/测试/操作应用或模型。实际第 2 章失败保留，正常重试只使用既有授权、当前草稿和累计账本；是否能最终纠正与交付须由 Root 实机确认。

## 实施后复核

生产变更只涉及三个既有函数：共用来源查询无结果时返回结构化拒绝；报告调用方为该拒绝增加遗漏条目/引用索引；共用 MCP author_quote 描述区分作者原话与模型解释。全部原权限、当前来源、hash、精确引文和主任务实际读取检查保持。`/sourceId` 是单个作者引用项内部的相对路径；报告公开错误的 `/uncoveredRequirements`、`relativeInstancePath` 与两个索引共同定位该项；其他共用调用方沿原错误传播，不新增动态路径或参数。

现有 `report_prewrite_recovery_http_tests.rs` 增加真实 Router/SQLite 用例，不用模拟工具结果：错误检查输入、未知来源和其他 Work 的作者来源产生相同安全错误；错误不回显其他 Work 或本 Work 的作者内容；仅拒绝恢复记录变化，作品/任务/预算保持；同当前任务使用真实已读作者来源重新提交，候选报告保存且有 committed 回执；原 rejected 回执不可变。该报告所有冻结标准均 supported、客观字数合格，但非空作者遗漏仍 notMet、不能交付。第二用例验证同一有效报告空遗漏时 passed；它覆盖共用空遗漏路径，实际首章初始生成仍由 Root 既有实机验收证据负责。

| 视角 | 静态后审 | 依据与限制 |
| --- | --- | --- |
| 架构专家 | APPROVE | 仅 Native 来源事实与 MCP 声明，无新的权限层、决策流水线或状态推断；来源引用和文学观点的权威保持分离。 |
| 技术专家 A | APPROVE | SQL 同项目/同 Work/作者 section 查询及 hash/current/quote/read guards 未改；无结果统一错误，不泄露来源存在性或其他 Work 内容。 |
| 技术专家 B | APPROVE WITH CONDITIONS | 现有 HTTP 合同覆盖原失败、拒绝账本、同任务纠正和交付边界；子任务未执行构建/测试，Root 需运行冻结文件的真实合同并实机重试。 |
| 产品经理 | APPROVE | 模型能据真实作者输入自主重估遗漏，不要求作者补内部参数，不自动删遗漏、换来源或改变文学结论。 |
| UI/UX 设计师 | APPROVE | 无新界面或调试信息进入主阅读流，结果是否保存和能否交付仍以既有真实回执为准。 |

静态 `git diff --check` 已通过。Root 已告知三个生产文件构建成功、生产目录 35/35 和架构检查通过；这不代替新增 HTTP 合同与实际模型纠正的验证。实施文件冻结后，本子任务不再修改生产文件。综合结论：静态批准，实际完成与交付仅以 Root 的真实合同和原第 2 章检查任务实机证据为准。

## Root实际验证

当前Native构建exit0（1分15秒）；Core全lib265/265、exit0；Runtime实际Router/SQLite鉴权合同5/5、exit0，含同任务纠正来源后保存及非空遗漏仍notMet。MCP首轮受限环境59通过/36监听失败、exit101保留，全部失败为本机监听PermissionDenied；允许受控127.0.0.1后同源码95/95、exit0，未削弱任何合同。实际目录35/35、exit0，架构332文件/7crate/0诊断、exit0。日志前缀 `/tmp/autonomous-v3-current7-author-source-`，冻结源码与实际Native可执行身份见 `/tmp/autonomous-v3-current7-author-source-build-proof.json`。

测试静态复核在HTTP编译排队时纠正了空遗漏摘要断言：当前summary省略0计数字段，测试验证真实持久report空数组、summary字段缺省且passed；生产未改变。最终测试SHA为b50331978d2c35367f8b6528d9cea27af60e2c8cd5213ca404b117f4e971f031。实际5/5使用该文件，没有以旧fixture或新字段回填通过。

Root五视角后审的工程合同条件满足；独立静态复核PASS。真实模型是否自主纠正与交付仍待原第2章任务恢复验收，不由受控HTTP或保存报告的事实替代。重启前无活动Task/预留，第1章原未知112,525保留，第2章新增Work无未知用量；沿原profile正常重启，未清账、未重生成正文、未重新输入作者要求。
