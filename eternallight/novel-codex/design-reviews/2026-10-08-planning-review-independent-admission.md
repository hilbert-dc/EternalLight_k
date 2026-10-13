# 规划评审登记与真实派发的独立准入 fixture

2026-10-08，仅 task_planning_review_http_tests.rs 单个失败用例；不生产变更、不构建。原冻结8完整5/6日志保持。

## 当前根因与方案

旧测试要求缺 executeGoals 时登记失败，实际 registrationOnly:true 仅登记。当前 model_work_plan::action_ready 明确方法计划既不授予也不撤销执行权，只校核已声明满足的方法目标是否仍有真实证据。不能把 executeGoals 再变成新增权限来源。

改为严格证明缺方法计划仍可登记，未派发任何子 Task/Attempt/grant，复用原作者 Work/消息/预算。源 Task 活跃且费用未结时真实 seat/tasks 拒绝且事务回滚、整个 Work/Task/Attempt/grant/authorMessage/authorization 全同；模型补计划后仍不能绕过该门禁。源 Task 完成但费用未结仍拒绝，真实 Engine零用量 receipt和Nativeprovider-usage结账后，同原 Work/原授权真实席位才可派发。重复登记/原选定大纲/root位置等断言保持，其余5个完整负例不动。

## 五职责预审

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 方法为参考，作者与费用准入独立；不新增固定计划流水线 |
| 技术A | APPROVE WITH CONDITIONS | 真实 seat派发失败必须零状态变化，结账后同 Work原预算成功 |
| 技术B | APPROVE WITH CONDITIONS | Root唯一编译后完整6项；旧失败不忽略/filter，无SQL权威伪造 |
| 产品 | APPROVE WITH CONDITIONS | 登记仅表示安排评审，非已执行或结果完成 |
| UI/UX | APPROVE WITH CONDITIONS | 不增加UI或技术字段输入，计划不能阻碍正常模型选择方法 |

综合 APPROVE WITH CONDITIONS；零兼容 PASS，无迁移、回填、旧executeGoals兼容门禁或保护放宽。后审待当前完整6项真实退出。

## 当前实测后审

Root current10b 完整 `task_planning_review_http_tests` 实际 exit0、6/6、0 ignored/0 filtered，日志 `/tmp/autonomous-v3-fixtures-current10b-nodebug/task_planning_review_http_tests.stdout.log`，二进制/profile 见同目录 manifest。真实 registrationOnly 不派发、活跃/未结账来源不能派发、实际结账后同 Work/预算派发、原 scope/作者/root/评审关闭和恢复负例均通过。五视角后审 APPROVE；没有通过方法计划新增执行权限或声称文学评审结论正确。
