# 创作运行测试按自主决策合同重建

日期：2026-10-08。仅 `native/story-runtime/tests/authoring_run_tests.rs` 与专用 test helper；不修改生产引擎、权限、预算或共享 fixture。

## 发现与方案

冻结8全组 1/12 通过，原失败保留。旧测试将 brief→outline→plan→正文→知识→承诺→场景评估视为每次必经流水线，而当前版本由 advisory 获取真实 readyActions，再由模型选择下一能力。不能把旧断言加回生产，也不能伪造运行 stage、跳过实际 coordinator 或扩大原额度。

同步开局/保存后 stage 为 coordinate；以真实 HTTP 原任务资料完整分页、模型决策分页、原 actionKey/version/sourceDigest、原生 Task claim/complete、实际 Engine grant 结账及 advance 验证行为。暂停、取消、未知状态、失败重试、陈旧 token、候选目录变更、作者历史、原预算与正文未采用继续严格验证。

三章用当前模型自主选写、独立评估、修订与交付重建，不再断言固定团队和每章自动承诺抽取。旧场景映射、场景反馈与承诺提案的专门领域负例继续由 `planned_scene_mapping_http_tests.rs`、`work_decision_http_tests.rs`、`revision_outcome_tests.rs`、`commitment_extraction_http_tests.rs` 验证；这里保留三章真实保存、首次不合格后修订、来源失效、作者短输入/空反馈不冒充新原话、原额度结账与第四章接续。不得将被删除的流水线计数冒称新的验收证据。

## 实施前五视角

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 直接同步当前架构，不增加旧 pipeline/fallback，不以模型方法计划代替权限 |
| 技术 A | APPROVE WITH CONDITIONS | 原输入全读、真实 scope/source/action identity、每次实际 Attempt 计费，24 次原限额不放宽 |
| 技术 B | APPROVE WITH CONDITIONS | 所有 12 项默认完整执行；领域独立负例仍保留；原失败完整可查 |
| 产品 | APPROVE WITH CONDITIONS | 允许按需省略能力；真实保存、独立评估及交付不是同一状态 |
| UI/UX | APPROVE WITH CONDITIONS | 作者改向/无附言反馈不捏造输入，无 UI 扩张，控制模型测试不冒称实机 |

综合 APPROVE WITH CONDITIONS；零兼容门禁 PASS，无兼容例外。

费用 fixture 仅在可信 Host 验收侧只读 Native 持久 `grant_json`，沿用已有 `settle_task_fixture` 合同，再由实际 Engine 生成 terminal receipt 并走 Native provider-usage 结账。终态 Task 不再通过 live provider-budget 入口重新获取执行许可；该入口正确拒绝，不能为测试放宽。模型的原文/产物阅读仍必须完整 HTTP 来源读取，账本读取不替代正文或作者证据。

## current10d实际失败后的增量前审

首次实际3/12，原日志保留。当前唯一作者来源是带版本标记的完整authorText；用真实run history/current chapter通过generation_author_intent渲染后逐字等值核对，不做字段fallback或子串放宽。裸作者输入仍在实际作者记录中核对。原范围发生外部资料变动时/next正确返回409 AuthoringRunStale；验证原run未变、无新Attempt并允许显式取消，不要求拒绝事务保存虚假blocked状态。同Work advisory接续必须保留原root thread，不能为新Attempt换root。

五视角前审均APPROVE WITH CONDITIONS：架构保留当前唯一来源与Work归属；技术A全source/hash和原作者全文等值；技术B精确409/原状态/原root与不可变失败Attempt，不放宽门禁；产品/UI拒绝诚实且不捏造新输入。零兼容PASS。

## 实施后

current10f 完整 Native 默认全组中的 `authoring_run_tests` 实际 12/12、exit0、22.99秒；完整 workspace 本轮 843 项通过、8 项失败、实际 exit101，因此不能把该组通过写作完整验收成功。证据 `/tmp/autonomous-v3-native-full-current10f-nodebug.stdout.log` 与对应 result.json；前后全树1524项源码摘要一致。

五视角实施后复核：架构确认模型动作选择与领域许可分别负责，不恢复旧固定流水线；技术A确认原范围、24次限额、真实Engine结账和取消/未知负例保留；技术B确认当前默认12项完整执行，旧失败日志保留；产品确认三章保存、独立负评、自主修稿、复评和后续接续有受控模型证据；UI/UX确认作者原话不被伪造。五视角均 PASS（该组自动化），实际应用与真实 DeepSeek 文学效果仍未验收。零兼容门禁 PASS。

同Work报告对后继writer的隐藏断言新增后，该helper与本组须再验，不能沿用本次12/12。四类真实报告的保存与目录回读由 `workspace_revision_reports_http_tests` 负责，生成修稿writer的角色隔离由实际generation helper负责；该证据分工不新增生产权限或强迫普通非生成Work获得writer动作。
