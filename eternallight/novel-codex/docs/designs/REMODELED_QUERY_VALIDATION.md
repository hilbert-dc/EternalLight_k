# 改稿后结构化知识查询验证

## 目标与边界

对应持续创作的“修改正文 → 重新建模 → 后续对话按新来源检索”。项目级
advisory 的基础 Context Pack 不默认装入整个知识图；workingKnowledge=null
不能直接判为缺陷。query_story 负责按问题执行有限查询，并给出来源、候选状态、
证据字典和覆盖信息。本切片验证该显式读取路径，不新增另一套检索或模型调用层。

实施者自检；按用户要求不组织独立评审。没有生产接口/schema 修改、兼容路径、
旧数据迁移、UI 修改或权限扩展。使用当前 schema 的临时数据库与真实 Router，
复用既有长篇 fixture 与请求辅助方法，新增测试放在独立文件。

## 回归场景

1. 原始已建模正文可被有任务能力凭据的 advisory 查询。
2. 作者保存新稿后，同一旧任务的查询失效，不混入新旧来源。
3. 新对话在新稿未建模时得到空知识及不完整覆盖，而不是旧稿事实。
4. 绑定新修订提交建模后，再建 advisory 只取得新稿断言，保留 candidate 状态，
   evidenceRef 能定位到新来源的实际 Unicode 原文。
5. 成功检索不声称语义完整，semanticCompleteness 仍为 unknown。

测试入口：native/story-runtime/tests/query_plan_tests.rs 的
revision_query_checks 模块。该测试不证明模型主动选择正确查询、不证明自由创作
质量，也不覆盖桌面自动派发、采用/撤销和完整 S1–S5 验收。

## 验证结果（2026-09-28）

- query_plan_tests：6 项通过，包含新增跨修订用例。
- query_scope_tests 与 knowledge_conflicts_tests：10 项通过，包含范围遮蔽、
  同名消歧、知识冲突及历史快照保留。
- story-runtime 的 query_plan_tests 严格 Clippy（-D warnings）通过。

以上为真实 Router/SQLite 的确定性测试，不消耗模型额度；真实模型目前只证明
新正文和建模来源元数据消费，结构化查询的模型自主使用仍待单独验证。
