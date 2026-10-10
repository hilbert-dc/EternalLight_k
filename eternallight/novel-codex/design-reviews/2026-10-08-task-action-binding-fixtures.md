# 当前行动绑定测试合同同步

日期：2026-10-08；范围仅为 `native/story-core/tests/task_action_binding_tests.rs`。

## 变更概览

冻结5统一验收的该文件有14项，其中10项通过、4项失败。原始日志 `/tmp/autonomous-v3-unified-current-frozen5.log` 保留。两项章节测试没有提供当前必需的 `chapterAcceptance`，在测试目标之前就被正确拒绝；另外两项规划测试在简报保存后直接执行旧大纲步骤，但3.0会把从未尝试且来源已变化的原规划步骤置为 `superseded`，交回同一 Work 的模型决策，原作者行动和许可保持不变。

目标：用当前合同建立有效前置条件，继续验证精确行动/权限绑定、冻结来源与顺序产物。不得修改生产代码、降低权限或作者约束、跳过失败测试，也不新增兼容路径。

实施方案：两个章节目标显式填写候选要求及原作者没有指定篇幅的验收范围；两项规划测试先断言旧步骤不可直接执行，再经真实 `prepare_work_decision`、`select_work_action`、决策完成及精确预算回执结清，以原 Step、原行动/许可、原作者意图和原对话执行当前来源。保留未完成 sibling、外部修改、篡改目标/作用域、伪造来源、未获批准与事务失败等负例。补充决策不能改换对话、模型选择不能换成外来来源或未批准操作的断言。

## 五视角预审

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构专家 | APPROVE WITH CONDITIONS | 只同步测试到单一 Native Work/Step/Attempt 合同，不重建第二流程，不修改生产行为 |
| 技术专家A | APPROVE WITH CONDITIONS | 原许可、作者、对话、步骤身份与冻结来源持续断言；所有拒绝和事务回滚断言保留 |
| 技术专家B | APPROVE WITH CONDITIONS | 使用公开真实仓储接口完成模型决策与预算结清；不直接改数据库来绕过 superseded/权限 |
| 产品经理 | APPROVE WITH CONDITIONS | 已保存简报成为模型重新决策的输入；原计划不能被机械直接执行，也不能扩大已批准目标 |
| UI/UX设计师 | APPROVE WITH CONDITIONS | 仅仓储测试，无 UI 改动；保存、选择、执行与作者采用的边界保持 |

五视角表示审查角度，并非五位独立评审者。综合结论：按上述条件实施；无兼容例外。

## 零兼容门禁

PASS：不新增版本判断、fallback、双读写、迁移、回填或旧测试行为分支；隔离内存库按当前 schema 初始化。生产源码和接口保持不变。

## 实施后复核

实际变更仅为当前测试文件：两个章节目标显式携带 `chapterAcceptance`；两个规划用例继续验证未完成 sibling 不可消费及原冻结输入不被重写，并先确认旧直接 claim 被拒绝，再经真实同 Work 模型决策选择原大纲 Step。新增断言覆盖原 Step/Work/作者意图/对话、原行动及完整许可证据不变、后继捕获真实简报来源、选择幂等，以及换对话、外来来源、未批准正文操作被拒绝。有限预算使用16次请求、10,000 tokens；决策仅记合成的一次请求/一个 token，按实际 Native grant 精确结清，未发送服务商请求。

验证命令：`cargo test --manifest-path native/Cargo.toml -p novel-codex-story-core --test task_action_binding_tests`。定向日志 `/tmp/autonomous-v3-task-action-binding-current.log`：14/14通过，0 failed、0 ignored、0 filtered；业务测试0.65秒。工具收到的进程实际 exit code 为0。现有 unused-field 编译警告保留。原失败日志仍为冻结5的10通过/4失败，不改写为绿色。

| 视角 | 实施后结论 | 复核 |
| --- | --- | --- |
| 架构专家 | PASS | 生产流程未改；真实 Native 决策重新启用原未尝试 Step，原作者许可独立保存 |
| 技术专家A | PASS | 精确拒绝、事务原子性、持久化篡改、外部来源与未完成 sibling 等原负例全部通过；新增换对话和越界选择负例通过 |
| 技术专家B | PASS | 当前14项全通过且无跳过；未直接改数据库，未增加 fallback/迁移或弱化断言 |
| 产品经理 | PASS（机制） | 来源刷新交回模型选择，旧目标不机械执行；只认证合成仓储行为，不认证真实模型文学质量 |
| UI/UX设计师 | PASS（范围） | 无界面改动；不把登记、重新选择或候选视为作者采用或作品完成 |

与方案无偏差。零兼容门禁复查：PASS。当前文件可交回统一验收；其他 Native/整链结果及真实模型创作仍由 Root 分别验收，不用本14项代替。
