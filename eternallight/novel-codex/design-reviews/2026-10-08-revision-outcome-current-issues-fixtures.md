# 当前修订报告 issues 测试合同同步

日期：2026-10-08

## 变更概览

当前 AUTHOR 修订效果报告包含稳定问题摘要 issues。五个旧完整报告断言缺少该字段，实际 HTTP38项中33通过、5失败；原日志保留在 /tmp/autonomous-v3-author-source-current-runtime-contracts-20261008.log。只同步测试的明确预期，不修改生产投影、读取证明、任务权限或报告保存门禁。

## 零兼容门禁

PASS：当前 schema 与 HTTP 合同；不新增旧字段过滤、fallback、回填或迁移。不删除完整对象相等断言。

## 五视角预审

| 视角 | 结论 | 实施约束 |
| --- | --- | --- |
| 架构专家 | APPROVE | 测试预期独立写明问题摘要，不调用生产投影产生期望 |
| 技术专家 A | APPROVE | 保留授权、CAS、重放、账本及当前/失效报告所有断言 |
| 技术专家 B | APPROVE | 完整报告新增 issues，明确 id/status/summary/currentDraftRevisionId/观察次数/分页 |
| 产品经理 | APPROVE | 新稿和历史报告均反映实际当前稿问题状态；不把保存当成解决 |
| UI/UX 设计师 | APPROVE | 当前问题摘要字段完整，观察详情保持按需读取合同 |

## 实施后复核

当前重新编译的38项先执行37通过、1失败，原日志 `/tmp/autonomous-v3-native-group3-current8/revision_outcome_tests.stdout.log` 保留。唯一遗漏为作者后续改稿后的历史报告：state已预期stale，但issues仍预期旧稿open。已按预审要求补充明确的新稿revision与stale问题预期，仍保留旧assessment内容不变及完整对象相等断言；未改变生产。五视角复核均维持原批准条件；当前只完成rustfmt解析，最终38项复验待统一构建，不记通过。

当前8组fixture重新编译后的定向结果：`/tmp/autonomous-v3-native-current9-fixtures/revision_outcome_tests.stdout.log`，38/38，实际exit0，43.05s；入口及完整Native test源在frozen9中，运行器记录入口和二进制hash均不变。五视角实施后复核PASS（本次当前issues合同fixture），全对象相等、实际原文和历史assessment、当前revision/stale问题字段未放宽。其后新增workspace产物回读生产改动须重新全量验收，本38项不替代后续当前二进制结果。原33/38及37/38失败继续保留。
