# 重规划 fixture 的完整原生决策分页

日期：2026-10-08。五项为评审视角，不代表五位独立人员。

## 原失败与当前合同

冻结 8 `generation_replan_tests` 实际 1/2，`replan_uses_new_sources_without_reopening_attempts_or_refunding_budget` 原第 210 行未在首个 readyActions 页找到 generateBrief，unwrap 中断。日志保留 `/tmp/autonomous-v3-native-group1-current8/generation_replan_tests.stdout.log`，默认完整断言，未过滤。

`native/story-core/src/work_decision_status.rs:205–244` 根据 requirements/actions/receipts 总页数签发并校验当前 Work、decision Task、version 与实际证据摘要绑定的 opaque cursor；`:309–339` 每页 readyActions 最多两项，coverage.availableActions 是全量数量。因此第一页没有目标能力不是授权丢失，测试必须真实读完目录，模型再选用具体原生 actionKey。固定操作仅为本 fixture 模拟模型的确定选择，不实现生产词语规则。

`native/story-core/src/task_provider_budget.rs:92–173` 要求运行中实际 attempt、相同 Work 和未结算原授权；测试在完成前读取真实 grant，Engine 测试预算复制该 budgetId/maxRequests/maxTokens。原作者 8 attempts、256 requests、4000000 tokens、600 秒不变。重规划不得退还消耗、重开 terminal attempt 或赋予新作者身份。

## 范围与验收

仅 `native/story-runtime/tests/generation_replan_tests.rs` 与本独立评审。生产、shared helper、主实施记录冻结；不构建、不调用外部模型。

- 复制每页实际 nextCursor 请求后续页；拒绝重复 cursor，Work/thread/version/sourceDigest/总动作数与页序必须一致。
- 聚合完整 readyActions；总数等于 coverage.availableActions，actionKey 唯一；仅从实际目录选择 generateBrief，并使用该 key、当前版本与 sourceDigest 提交。
- 当前 actual Native provider grant 与原作者全额范围精确比较，再交本地 Engine 生成真实 terminal receipt；不以缺失字段生成无限预算。
- 保留重规划和 rebind 的原完整身份、原文、预算、费用、历史、来源变化、CAS、候选权限、已用 attempt、取消与耗尽拒绝断言。

## 实施前五视角

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | Native 签发动作和 cursor 是权威；测试模拟模型选用，不在生产固定流水线。 |
| 技术 A | APPROVE WITH CONDITIONS | 分页必须同一精确 Work/Task/thread/version/digest，不拼接旧页或猜 cursor；预算来自实际 grant。 |
| 技术 B | APPROVE WITH CONDITIONS | 原 1/2 和日志保留，聚合全量且完整性/唯一性明确验证；所有重规划保护断言保持。 |
| 产品 | APPROVE | 最新作者方向可重规划，原费用、身份和已有作品保持可追溯。 |
| UI/UX | APPROVE | 仅测试目录阅读，不新增用户操作、固定关键词或技术界面。 |

零兼容门禁 PASS：没有 fallback、迁移、双读双写、限额或超时提升、权限扩大。综合结论为允许本 fixture 同步；实际通过证据须由集中构建后的当前二进制提供。

## 实施后复核

实际增量位于 `generation_replan_tests.rs:194–277`：运行中读取并精确检查真实 grant；复制全部 Native cursor 请求，核对 Work/thread/version/sourceDigest/页序/actionOffset/总数/hasMore；最终聚合数严格等于 coverage.availableActions 且 actionKey 全部唯一。提交仍使用真实目录中的 generateBrief key 和当前版本、sourceDigest。`:298–304` Engine 测试 budget 复制该 grant 的实际 id/maxRequests/maxTokens；没有新增预算或作者批准。原重规划身份、来源、记录、收费、预算、终态 Task、取消/耗尽拒绝断言保留。

| 视角 | 后审结论 | 证据与边界 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 单一 Native 目录与原授权，没有生产固定决策规则；当前二进制执行待主任务集中构建。 |
| 技术 A | APPROVE WITH CONDITIONS | cursor 原样复制并核对页 fence、数量和 actionKey 唯一；真实 grant 保持256/4000000/600秒。 |
| 技术 B | APPROVE WITH CONDITIONS | 原 1/2 日志保留，原强重规划和权限断言保持；未过滤或降低门禁。 |
| 产品 | APPROVE | 新作者方向变更不会把预算、旧 attempt 或身份重置。 |
| UI/UX | APPROVE | 无界面或额外用户步骤。 |

当前限定文件 rustfmt actual exit 0、`git diff --check` actual exit 0。未构建、未宣称 2/2 已通过。零兼容门禁复查 PASS，没有 fallback、版本判断、迁移、OS/超时改动或权限扩大。剩余项为当前二进制默认完整两项实际验证。
