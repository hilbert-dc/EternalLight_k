# 承诺候选提取的原 Work 交付闭环 fixture

日期：2026-10-08；仅 commitment_extraction_http_tests.rs，生产不改，不构建。冻结9 原失败日志保持：子 Task complete 200，Work running 与旧 completed 断言不符。

## 当前原因与方案

保存 candidateProposalBatch 和子 Task completed 并不关闭作者整个 Work。当前 refresh_work_task 对未结 work_decisions.finished=0 保持 running；final decision 必须独立读取原作者要求，核对真实已保存报告并写 deliveryReview。原 fixture 仅完成提取子 Task，未结真实子费用账本，也未执行最终交付决策。

保留子 Task completed/真实报告输出、所有历史报告/候选/跨项目权限/晚写拒绝。先明确证明原 Work 仍待核对，再用既有 settle_task_fixture 创建真实零请求 Engine receipt 并 Native charge；通过 Author /work-tasks/{id}/decision 准入、原 thread/原 Work 新 advisory Attempt、当前 taskSources 目录实际全文分页原作者要求、真实保存报告证据 deliveryReview、complete，再真实结账。最终仍严格断言 Work completed、deliveryReport 已核对且无未结 Attempt。不得 SQL 更新finished、换作者消息/预算、把running当通过。

## 五职责实施前评审

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 当前 Work/真实决策入口，没有第二状态机或新授权 |
| 技术A | APPROVE WITH CONDITIONS | 真实 Native 输出关系/完整原文/hash/使用账本；所有原负例保留 |
| 技术B | APPROVE WITH CONDITIONS | 完整1用例重测、旧失败留存、Root唯一构建；明确费用与执行终态不同 |
| 产品 | APPROVE WITH CONDITIONS | 候选提案只完成提取与保存，不能宣布已采用或全书目标完成 |
| UI/UX | APPROVE WITH CONDITIONS | 无 UI或作者新操作，最终可理解的交付依据由模型生成 |

零兼容门禁 PASS；无迁移、回填、SQL权威伪造或授权放宽。综合 APPROVE WITH CONDITIONS。后审待实际当前完整用例。

## current10b 实测后的增量预审

完整1项在 `/decision` 返回真实 step 后，创建新 Attempt 缺少 `target.resumeStepId=step.id`，Native 正确拒绝 TaskBindingInvalid。按当前 `work_decision_http_tests::claim` 合同带实际 step ID 以及 step.inputRevision，保留原 Work/root thread/作者原话和实际终态断言。不得把 target 原 JSON 当执行票据，不能凭已有预算创建新的根 Work。

五视角：架构 APPROVE（真实同 Work Attempt 绑定）；技术A APPROVE（Native 铸造 step 身份，原权限不放宽）；技术B APPROVE WITH CONDITIONS（完整1项实测，原失败留存）；产品 APPROVE（最终交付仍须实际完成）；UI/UX APPROVE（无UI变化）。零兼容 PASS。静态检查不是运行通过。

## 当前实测后审

Root current10d 完整 `commitment_extraction_http_tests` 实际 exit0、1/1、0 ignored/0 filtered，日志 `/tmp/autonomous-v3-authoring-current10d-nodebug/commitment_extraction_http_tests.stdout.log`（10.46s）。真实子 Task 保存和 completed、费用结账、Native `/decision` 原 Work/root 新 Attempt、实际原作者要求阅读、final deliveryReview和费用结账后 Work completed 严格通过；原报告历史版本、授权/范围/幂等/候选不自动采用和晚写负例仍在同一完整案例。五视角后审 APPROVE。这里只闭合本领域原 Work 交付测试；不宣称新增四报告 workspace HTTP或全量通过。
