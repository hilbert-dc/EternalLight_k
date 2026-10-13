# 插问期间保留父任务后续动作

## 前审

真实故障依据：task-live221 的父 advisory 在 open 插问期间两次 request_candidate_task 被 TaskNotWritable 拒绝，最后 completed/discussion_ended，插问返回后没有原改写。责任属于 Native 的动作登记与 attempt 准入边界（B03/B05/B06）；Host 不重新解释作者原话或另建 Work。

方案：将现有暂停屏障校验分为动作登记与执行准入。显式 pauseAfterStep 仍禁止登记；临时插问仅禁止原步骤派发新 attempt，允许有效的父执行按原权限保存提案/已授权队列。task_actions 使用实际所属 Work，禁止按 successor taskId 猜 Work 身份。普通提案、精确批准和有限 scope 不扩大授权；只读插问仍拒绝候选提案；预算、CAS、输入版本及当前 intent 检查不变。prepare_work_decision 和 attach_attempt 保留完整插问屏障。generation 决策登记同步采用登记屏障。

状态判定：父 attempt + open 插问可登记，未授权保持 proposed，已授权可 queued，但不可创建 attempt；插问 answered 未 return 同样不可派发；return 后按原 Work 的预算和版本创建 attempt；pause/cancel/只读插问不能借此授予写入。

五视角同一执行者自审：架构 APPROVE；技术A APPROVE WITH CONDITIONS（拒绝边界与事务回归）；技术B APPROVE WITH CONDITIONS（普通/生成登记及公开创建路径验证）；产品 APPROVE（保留原目标，不把暂停误报成权限缺失）；UI/UX APPROVE（沿用真实提案/队列投影，不解析回复）。零兼容 PASS，无 schema 变更。

验证计划：Native 插问登记、批准、暂停、只读与 return 前后 attempt 准入；已有动作绑定/Work 决策回归；生产 Host 插问集成；architecture:check。真实模型结果单独记录，不能用本地通过回写 221 失败。

前审补充：普通动作队列的业务前驱排除临时插问步骤；插问全局派发屏障单独管理。否则用户跳过未回答插问时，superseded 的临时步骤会成为永远无法满足的业务依赖。新增“未回答直接返回”回归验证保留父步骤依赖、仍用原预算。

工具说明补充（B04，五视角自审批准）：221–222 的 Chapter position is required 已定位为请求遗漏 chapterNumber；Runtime 的现行必填合同不变，将 generateChapter 的 chapterId/正数 chapterNumber 要求写入 MCP 短说明，使模型无需等拒绝后再发现这一必需参数。完整指南与领域校验仍在，不能宣称这一文字修正已经证明真实调用效率改善。

## 已实施与证据

- Story Core 新增 assert_can_register：显式 pauseAfterStep 拒绝不变；临时插问不阻止父任务登记/批准队列。普通候选与 Work 决策共用此登记检查。
- attach_attempt、prepare_work_decision 保留 assert_can_schedule，原目标只能在插问关闭、原来源/授权/预算全部满足后派发。
- 普通候选登记从实际 attempt→step→Work 持久关系查所属 Work，续接任务不再猜 work-{attemptId}。
- 普通动作依赖链跳过临时插问，避免 unanswered/skipped 问题变成永远无法完成的业务前驱。
- MCP 短说明补齐 generateChapter 必需 chapterId/chapterNumber。未放宽 Runtime 的现行参数校验，也未声称已实测减少 token。
- 生产 Host + 当前 Native 的普通/生成插问集成 2/2；新增真实候选 HTTP 登记，并验证 returned 后提案仍为 proposed、pendingActionIds 保留、deliveryStatus 不是 discussion_ended。
- JS 生命周期、插问、事件回归 42/42；动作绑定 14/14；交付状态 5/5；最新 Runtime bins 构建通过；architecture:check 通过。
- 扩展 Native 组首次 8/9：唯一失败为新测试写死 step-root，但预算根任务实际返回 task-* 身份。已将断言改为实际 root，不改生产逻辑，最终结果待补。保留失败日志。

本轮无付费模型请求。此前 221–222 的失败结果不回写。尚需真实模型确认当前改动后的连续改写交付；指南重复阅读的全链路成本以及改要求后的复杂工具纠错仍不能凭这一局部修复宣称解决。现有生成场景集成验证往返；本轮新增登记回归主要覆盖普通候选，生成决策沿同一登记检查并通过已有决策交付回归，未声称真实生成全流程通过。

## 最终验证与后审

最终 Native 插问 9/9、暂停控制 12/12 通过；加上动作绑定 14/14、交付 5/5、JS 42/42、Host/Native 2/2，本轮相关回归均通过。Runtime bins 构建与 architecture:check（364 文件、7 crates、0 diagnostics）通过。源码指纹及完整日志见 [验证结果](evidence/2026-10-10-interlude225-results.json)。初次断言错误保留于 interlude224-native.log；最终 interlude225-native.log 不覆盖原失败。编译保留已有 unused field 警告。

五视角后审：架构 APPROVE（Native 唯一事实与调度权威）；技术A APPROVE（原预算、只读、暂停、幂等及跳过插问的拒绝/恢复边界通过）；技术B APPROVE（公开 HTTP + 生产 Host 验证原提案保留，源码/构建一致）；产品 APPROVE WITH CONDITIONS（本地根因修复通过，真实模型完整交付仍未复验）；UI/UX APPROVE WITH CONDITIONS（现有状态投影不再丢失提案，但无视觉/人工 UI 验收）。零兼容 PASS，未引入 schema/迁移/双轨。未提交、未推送。
