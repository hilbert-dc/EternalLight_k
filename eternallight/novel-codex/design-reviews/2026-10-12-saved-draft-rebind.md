# 检查点恢复保留已保存正文

## 前审
目标：消除完整正文保存后执行失败，经 rebind 丢失核验入口而重复出稿的 token 浪费。原第41章 draft-eaddfe62398248faac752e2331b19cb6 已保存，但 rebind 将 task-9564570f35694449ac328f40d791692f 对应步骤 superseded；动作投影仅提供 finishChapter/generateChapter，前者实际 TaskOutputMissing。原运行已自然完成第41章，不回滚或重做测量。

根因：实现缺陷。普通重试已支持保存回执恢复，检查点刷新却无条件废弃失败步骤；动作投影把“没有待核验事项”误当“已通过核验”。不需要增加提示词。

Native（B01/B04/B06/B10）在既有 rebind 事务中用当前输入、当前候选头、完整保存回执判定成果可复用；同作者意图才复用。Authoring 继续拥有 CAS、作者意图和预算准入；Host 仅调用公开恢复接口。账本、错误、原稿、验收及作者范围不变；不把提供商失败改成计费成功。模型继续选择核验或有实际理由的改写。finishChapter 必须满足与执行端相同的持久核验条件。

| 事实 | 结果 | 验证 |
|---|---|---|
| 同意图 rebind、有效完整稿、已结算 | 原任务 ready_for_review，原步骤保留，核验原修订 | HTTP 重启恢复并最终交付同修订，无新增 writer |
| 部分稿/无稿/输入过期 | 不标记完整恢复，保持既有重新规划 | HTTP 对照 |
| 新作者意图 | 仍按新意图规划，不恢复旧任务为完成 | HTTP 对照 |
| 未知写入/未结算/过期 CAS | 保持既有拒绝及原事务 | 既有与新增负例 |
| 未通过核验 | 不提供 finishChapter | HTTP 动作列表 |

五视角自审（非独立代理）：架构 APPROVE（复用 Native 保存事实）；技术A APPROVE WITH CONDITIONS（事务及负例验证）；技术B APPROVE WITH CONDITIONS（公开 HTTP 重现）；产品 APPROVE（减少无效重写且不降文学标准）；UI/UX APPROVE（复用现有恢复操作，动作与执行一致）。零兼容前审 PASS：无迁移、双轨或历史 fallback。

验证计划：新增 HTTP rebind 完整/部分/作者更改对照，保留错误与预算，实际完成原修订；运行连续恢复回归与 architecture:check，构建并在安全暂停后更新应用。无额外付费模型测量，不报章节节省百分比。后审待实际结果。

## 实施与后审

实际修改：authoring_runs 在同意图 rebind 的原事务中，先调用既有 recoverable_output 检查原稿来源、当前候选头与完整保存回执；有效稿恢复 ready_for_review 并保留原步骤，否则继续原重新规划逻辑。work_decision_generation 使用执行端 revision_outcome::verify_chapter 的同一判断，只为确实通过持久核验的稿提供 finishChapter。没有改写提示词或降低验收。

E1（公开 HTTP）：continuous_recovery_tests 最终16项全部通过，含原稿恢复后原修订交付、无额外 writer、原账本/错误保留、重启、部分稿、新作者意图、未知写入 rebind 原子拒绝、作者编辑及原有暂停路径。E2（Host合同）：semantic-work-continuation 9项通过。E3（静态/构建）：architecture:check 无诊断（369文件、7 crates），Runtime/MCP 构建通过；已有未使用字段/测试方法警告保留。证据与本次源码摘要：`.release/saved-draft-rebind-validation/`。

实际应用再验尚未完成：CUA 读取应用返回 noWindowsAvailable，重新连接 timeoutReached，无法通过原界面安全暂停并替换应用。未终止运行任务、未替换正在运行的应用、未直接改生产数据库、未增加付费模型测量。因此不能宣称生产运行已使用修复或已实现某个 token 节省比例。第41章先前重复写作成本仍计入原账本。

五视角后审：架构 APPROVE（领域判定一致）；技术A APPROVE（E1事务、恢复与拒绝边界）；技术B APPROVE WITH CONDITIONS（E1/E2/E3通过，待应用更新与自然路径观察）；产品 APPROVE WITH CONDITIONS（原标准与成果保留，待生产生效）；UI/UX APPROVE WITH CONDITIONS（接口动作已一致，应用窗口不可访问导致再验未闭合）。零兼容门禁 PASS；未新增schema/迁移/fallback。B01/B04/B06/B10 代码边界 PASS，交付闭环未完成。

剩余：应用窗口恢复可访问后，经本步结束暂停安装修复，再恢复原运行；只观察自然后续步骤，不人为制造失败或重复章节来测量。

## 应用更新闭环（2026-10-12 15:14，Asia/Shanghai）

用户确认窗口已打开后，重置 CUA 会话成功恢复操作；先前 noWindowsAvailable 不代表用户未打开窗口。通过原对话“本步结束后暂停”安全暂停，Native 原运行与 Work 均 paused，所有 queued/running/waiting 尝试为零。第42章此前自然完成核验，completedChapters=42。

本次包于 07:10:06Z 完成，六项检查全部 passed；包内 Runtime SHA-256 与经过测试的构建一致，详见 package-report.json、binary-hashes.json。通过正常退出/启动更新原应用，再从“重试当前任务”接续原 Work；恢复后 Native status=running、version=644，advisory task-7007b738d9be4ce794de6f706d239f75 已排队，UI 显示执行中。第42章修订 draft-b6bcd3a0aa304c9aa3431843c3a02086、2596非空白字符及正文 SHA-256 均与更新前完全相同，未重复生成正文。证据 before-update.json、after-update.json、resumed.json。

后审更新：技术B、产品、UI/UX 的安装与原任务恢复条件关闭，APPROVE。完整稿失败后 rebind 的精确故障由公开 HTTP 回归覆盖；本次生产更新时写作成果已完成，不能称为生产上再次触发并验证该故障，也不据此计算节省比例。后续仅观察自然执行，不新建定时任务或付费测量。
