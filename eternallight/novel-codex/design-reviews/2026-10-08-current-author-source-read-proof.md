# 当前作者要求的完整阅读证明

日期：2026-10-08。以下为五项评审视角，不表示五位独立人员。

## 变更概览

目标：同一主任务已完整阅读当前作者要求后，另一合法阅读上下文缓存同样的要求，不应让交付校验随机拒绝。

代码确证：`task_sources::project_task_sources` 的 policy 包含 `context.target.position`，来源标识包含 policy，而缓存主键允许同任务、同 section、同 hash 存在多个 policy 来源。正常 context 有 position；`revision_outcome_api` 创建报告 context 时没有 target，随后也调用 `project_task_sources`，因而可以缓存第二个当前作者要求来源。现有 `require_author_requirements_read` 使用不带排序的 `query_row` 只检查其中一个来源，可能选到尚未阅读的报告上下文来源，忽略主任务实际完整读过的正常 context 来源。Root 的真实 Core 同一自主链既有失败也有通过，原日志分别保留。

实施范围：只修改当前作者要求阅读 guard，另加当前空库 Repository/SQLite 回归测试；不改来源构造、权限、Root 自主测试、MCP、Host、完整阅读记录、引用证明或完成自审合同。对所有同 task、当前 author hash、精确 section 的来源逐个检查：任一单个来源由主任务完整阅读即可；各来源分别计算完整覆盖，禁止跨来源拼接分页。

验收：用实际 `project_task_sources` 注册正常/报告两种 policy，查询旧 SQL 实际选择的来源，再通过真实读取 API 完整读取另一个来源，旧 guard 必须失败、新 guard 必须通过。分别覆盖 raw 与 parsed 阅读；缺目录、只读目录、子任务读取、旧 hash、其他任务读取、只读后半段，以及跨来源各读部分都仍拒绝。新测试不修改超时、门禁或断言来换取通过。

## 测试阶段零兼容门禁

PASS：不新增 schema、旧字段 fallback、迁移、回填、降级、版本判断或双读双写。多个来源是当前 policy 模型的合法状态；这是当前数据一致性修复，不承担历史格式兼容。测试从当前空库创建项目和任务。

## 实施前五视角评审

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 只修 Native 阅读证明判定；模型仍选择阅读与方法，Native 维护真实当前授权和完整性。来源标识、冻结上下文和引用证明保持不变。 |
| 技术 A | APPROVE WITH CONDITIONS | 只接受同任务、精确 section、当前 hash 的单个来源完整根线程回执；不同 source 的部分记录不能相加，子线程不能补主任务证明。 |
| 技术 B | APPROVE WITH CONDITIONS | 真实 Repository/SQLite/API 先重现旧失败，再验证 raw/parsed 两个正例及所有负例；保留旧失败与构建证据，交 Root 统一验收。 |
| 产品 | APPROVE WITH CONDITIONS | 减少模型完成正常阅读后被迫重复读取或由用户反复重试；不把阅读完成当作文学质量通过或最终任务已交付。 |
| UI/UX | APPROVE WITH CONDITIONS | 不新增界面、确认框或内部术语；真实缺读时仍保留现有准确诊断，合法完整阅读不再得到误拒绝。 |

综合结论：批准上述最小增量，Root 已明确授权 guard 范围。无兼容例外。

## 实施后复核

已完成真实旧失败与最小修复：`/tmp/autonomous-v3-author-source-guard-before-20261008.log` 为 7/9、exit101，两个完整当前主任务阅读正例稳定得到原误拒绝，七个负例通过；`/tmp/autonomous-v3-author-source-guard-after-20261008.log` 为 9/9、exit0。测试通过实际当前空库、来源注册与 raw/parsed 阅读 API 建立回执，没有伪造阅读证明。生产只改变同 task、当前 hash 来源的逐个独立完整性判定；不同 source 的覆盖分别从零开始计算。旧诊断、作者版本、引用校验、子线程权限、缓存与 schema 不变。

相关 Runtime 首轮 `/tmp/autonomous-v3-author-source-current-runtime-contracts-20261008.log` 中 `revision_outcome_tests` 为 33/38、exit101。五个全对象等值断言未纳入当前 AUTHOR 报告的 `issues` 投影，实际状态 200、assessment 与其他报告字段仍正确；这不是阅读 guard 拒绝。原失败保留并交 Root 同步当前合同；该命令因首目标失败而没有执行后两个目标。

最新共享 Native 事件补丁期间，Core 全 lib 首轮 `/tmp/autonomous-v3-author-source-current-core-unit-20261008.log` 在编译阶段 exit101：`work_execution_events.rs:332` 调用尚为 private 的 `work_decision_generation::load_state`（E0603）。已通知 Root 由该增量所有者闭合；本修复不修改该模块。最终当前合同与二进制证据待该编译错误解决后补齐，不能把此前 9/9 等同于当前所有 Native 验收完成。

此前已启动的相关接口验收 `/tmp/autonomous-v3-author-source-task-and-work-runtime-20261008.log` 最终 exit0：`task_sources_http_tests` 5/5（6.50 秒），`work_decision_http_tests` 10/10（19.26 秒）。包含当前来源边界、子线程隔离、Unicode/游标、原稿冻结及工作续接、失败纠正、保存和交付的原强断言。Root 已接手五个报告测试的当前 issues 合同，并要求停止新构建以进行统一全量认证；本增量三文件已冻结，不启动重复 Native 构建。

| 实施后视角 | 结论 | 证据与范围 |
| --- | --- | --- |
| 架构 | APPROVE | 仅当前阅读 guard 更正合法多 policy 来源的判定；来源构造、权限、引用合同、模型决策边界没有改变。 |
| 技术 A | APPROVE | 9 项真实 Repository 回归与 15 项相关 Runtime 接口断言通过；当前 hash、同 Task、单 source 完整覆盖、子线程不记根回执仍保持。 |
| 技术 B | APPROVE WITH CONDITIONS | 旧 7/9 失败完整保留，新 9/9 和相关 15/15 通过；全量 Core/Runtime、同步后的报告合同及当前生产 bin 由 Root 在共享源码冻结后统一认证。 |
| 产品 | APPROVE | 正常完整阅读不再因报告上下文再次注册同要求而被误拒；最终文学质量和整部任务交付仍由原独立评估和自审判定。 |
| UI/UX | APPROVE | 无新增界面；保留真实缺读诊断，消除合法完整阅读后的错误重读要求。 |

零兼容复核 PASS。没有提高超时、删除业务断言、降低门禁或增加许可；没有声称专项真实模型创作已由这些机制测试完成。
