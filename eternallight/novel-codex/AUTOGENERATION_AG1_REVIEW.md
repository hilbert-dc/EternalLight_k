# 自动生成 AG-1：试写分支与连续稿实施方案

> 现行覆盖规则：本文涉及旧库备份、迁移或兼容的历史结论均已由 [测试阶段工程策略](TEST_STAGE_ENGINEERING_POLICY.md) 废止。

后续状态（2026-09-12）：本方案仅开始写入 schema v4，连续分支业务与 UI 尚未接通。用户随后转向讨论规划优先路线，见[作者主导分层创作 PRD](/Users/hilbert/private/codex/eternallight/novel-codex/PRODUCT_AUTHOR_DIRECTED_CREATION.md)。下面的“允许实施”是当时方案的历史结论，不覆盖新提案；继续 AG-1 前须先核对新依赖并重新评审，本轮未继续实现。

版本：0.1 · 日期：2026-09-12 · 状态：实施前五角色评审通过，允许实施

关联需求：`PRODUCT_AUTOGENERATION.md` AG-1、G10～G17。

## 1. 背景、目标与范围

AG-0 已能把一次写章绑定到明确章节并验证持久化草稿，但每章仍共享一个全局候选头，后一章也只能承接正式稿。它不能支持“先连续试写三章，再决定是否采用”，也不能隔离两个互相矛盾的方向。

本批目标：作者以当前正式 revision 为固定基线创建一个命名试写分支，选择现有且连续的 1～5 个章节；系统逐章生成，每章完成后以 CAS 推进不可变工作提交，下一章读取本分支此前正文与临时事实；正式稿、正式事实和全局候选头在批量采用前保持不变。作者可为当前工作头创建审阅包，并用一次幂等事务采用依赖闭合的章节前缀。

本批交付：

- `StoryBranch`、`WorkingCommit`、`GenerationSession`、`BranchReview` 持久化对象。
- TaskRun 绑定 branch/session/expectedWorkingHead；迟到或并发写入由工作头 CAS 拒绝。
- 分支任务的 DraftRevision 不写全局 candidate pointer；完成任务时才把最新完整检查点推进为 WorkingCommit。
- Writer Context 先读取当前分支的有效正文与临时事实，再用同一正式基线补齐未覆盖来源；不读取其他分支。
- 连续生成入口按明确章节数组串行执行 1～5 个写作 TaskRun；预算为章节/模型回合硬计数，失败保留已推进工作头。
- 分支审阅与批量采用：只允许从第一笔工作提交开始的章节前缀，一次事务移动正文正式指针、确认对应临时事实、写一个采用回执并只增加一次 project revision。
- 章节页显示分支、连续范围、状态、已保存章节和批量采用入口。

非目标：自动创建正式章节结构、任意章节集合/跨分支合并、A/B 多方案树、自动文学择优、后台无人值守、旧正式版本变化后的自动 rebase。连续生成只针对工程中已存在且章号连续的章节；缺失章节时在模型调用前拒绝并提示作者先建立章节。

## 2. 候选方案与选择

方案 A：继续复用每章全局 candidate pointer，并给候选加 branch 标签。实现少，但两个分支仍会互相覆盖，无法满足 G10/G12，拒绝。

方案 B：复制一套完整作品数据库作为分支。隔离简单，但正式采用、搜索索引、迁移和磁盘成本都产生第二套权威，拒绝。

方案 C（采用）：分支只保存固定正式基线、单调工作头和不可变 WorkingCommit；每个提交引用既有不可变 DraftRevision 与该稿的事实建议。读取时由 Runtime 合成“分支最新提交 + 固定正式基线”，采用时仍进入唯一正式版本系统。

关键取舍：

- 第一批连续模式要求章节已存在。章节身份是正式结构，不在试写任务中偷偷创建；后续可另行设计 branch-local chapter slot。
- 工作头用整数版本而不是可变 draft pointer。TaskRun 创建时固定 expected head，完成时单事务 CAS。
- 每章一个 WorkingCommit，事实仍复用 `fact_proposals`；只有该 DraftRevision 进入分支提交后才成为本分支临时事实。
- 批量采用只支持依赖闭合的提交前缀，避免首批实现不可证明安全的任意组合。

## 3. 领域模型与控制流

```text
Author startContinuous(existing contiguous chapters, intent, 1..5)
  → Runtime create branch + session(base project revision, head=0)
  → for target in session.targets:
      Main create writer TaskRun(branch, session, expectedHead)
      → Context(formal baseline + same-branch commits only)
      → Agent checkpoints + fact proposals
      → complete_task validates latest complete draft
      → CAS branch head → immutable WorkingCommit
      → session nextIndex++
  → session readyForReview / incomplete

Author prepareBranchReview(head, prefixCount)
  → deterministic checks + exact commit/draft/fact manifest
Author acceptBranchReview(idempotencyKey, current project revision)
  → one BEGIN IMMEDIATE transaction
  → accepted draft pointers + passages + temporal facts + receipt
  → project revision +1 exactly once
```

不变量：

1. Branch 的 `base_project_revision` 创建后不变；正式 revision 变化后旧分支可读，但不能继续推进或采用。
2. WorkingCommit 只能从当前 head 追加；`UNIQUE(branch_id, head_version)` 防止双推进。
3. 分支 DraftRevision 不进入 `chapter_workspace.candidate_revision_id`。
4. 一个 session 的 targets 必须同作品、章号严格连续、无重复且长度 1～5。
5. Context 由 TaskRun 推导 branch/head，candidate 不能自行指定其他分支。
6. 分支事实仅来自当前 head 可达提交所引用的 draft；相同事实键以较新分支提交覆盖正式事实。
7. BranchReview 固定 head 和前缀 manifest；head 或 project revision 变化即 stale。
8. 批量采用全部写入或全部回滚；幂等键重试返回同一 receipt。

## 4. 文件、接口和 Schema

Schema v4：

- 新表 `story_branches(id, project_id, name, base_project_revision, head_version, status, created_at, updated_at)`。
- 新表 `branch_working_commits(id, branch_id, project_id, head_version, parent_commit_id, chapter_id, draft_revision_id, created_at)`。
- 新表 `generation_sessions(id, branch_id, project_id, targets_json, intent, next_index, status, max_model_turns, turns_used, error, created_at, updated_at)`。
- 新表 `branch_reviews(id, branch_id, project_id, head_version, project_revision, prefix_count, manifest_json, checks_json, status, created_at)`。
- `task_runs` 增加 `branch_id/session_id/expected_working_head`。

模块：

- 新增 `runtime/creative_workspace/branches.py`：创建/读取分支与会话、工作头推进、上下文投影、审阅与批量采用。
- `generation.py`：任务创建/完成接入 branch CAS，不复制分支规则。
- `service.py`：分支任务保存不移动全局候选；Context 合并分支投影；workspace 返回分支摘要。
- `server.py`：新增 author-only branch/session/review/accept 路由；task 创建透传绑定字段。
- Electron 新增连续生成 contract 与 IPC；Main 串行驱动 session targets，失败时保留 Runtime 状态。
- preload/types/UI 增加分支列表、1～5 章范围、连续生成和批量采用。

接口：

- `POST /projects/{id}/generation-sessions`：`name,intent,targets,maxModelTurns`，创建 branch+session。
- `POST /projects/{id}/branches/{branchId}/reviews`：`headVersion,prefixCount`。
- `POST /projects/{id}/branch-reviews/{reviewId}/accept`：`expectedRevision,idempotencyKey`。
- Task create 新增 `branchId,sessionId,expectedWorkingHead`。
- `agent:generate-continuous(projectId,{chapterIds,intent,branchName})`。

## 5. 错误、恢复、安全与性能

- 非连续/跨作品/超过 5 章：422，模型调用前失败。
- branch head 已变化：409，保留新旧提交，调用方重新读取；不覆盖。
- 正式 revision 已变化：409 `branch_base_stale`，分支只读，提示新建分支或后续 rebase。
- 中途模型失败：session=`incomplete`，`nextIndex` 指向首个未完成章；已完成 WorkingCommit 可读和审阅。
- 应用重启：workspace 返回 session/nextIndex；首批恢复由再次发起剩余范围创建新 session/任务，不复活无法确认的在途模型回合。
- candidate MCP 不增加 accept 能力；批量采用仅 author token 路由和窄 IPC。
- Context 仍受现有总字节预算；分支摘录替换同章正式摘录，不叠加无界正文。
- 工作提交/审阅 manifest 使用外键、唯一约束与最多 5 章限制；列表投影有固定上限。
- 测试数据库直接按当前 schema 重建，不提供旧数据库备份或迁移路径。

## 6. UI 状态与无障碍

章节生成卡增加模式切换：单章 / 连续试写。连续模式显示分支名、从当前章起可用的连续章节数量和 1～5 数量选择；没有第二章时仍可生成 1 章，但明确提示需要先建立后续章节。

分支卡显示：名称、固定基线、工作头、`已完成 x/y`、状态、章节标题列表。运行中禁用重复发起；失败/部分完成不遮蔽已保存稿。ready 分支提供“准备审阅”；审阅通过后显示“采用前 N 章”，按钮文案明确会一次推进正式版本。

状态不只使用颜色；动态状态 `aria-live=polite`；表单使用原生 label/select/button；脏编辑时禁止开始或采用并说明原因。键盘可完成模式、范围、发起和采用。

## 7. 测试与验收

- G10：两分支同章相反事实，各自 Context 只见本分支。
- G11：三章逐次提交，第三章 Context 包含前两章分支稿；正式指针/正式事实不变。
- G12：同一 expected head 的两个任务只有一个 complete 成功。
- G13：批量采用一次 revision；相同幂等键返回同 receipt；故障事务无半成功。
- G14：review 只接受 1..N 工作提交前缀，跳章拒绝。
- G15：正式 revision 变化后旧分支推进/采用拒绝。
- G16：取消/完成后的迟到调用拒绝；session 从 nextIndex 恢复。
- G17：中途失败显示 incomplete 和已完成数量。
- 回归：AG-0 Runtime、Agent、build 全通过；macOS Electron 视觉走查。

## 8. 五角色独立评审

以下为同一模型在五个明确隔离视角下的评审，不代表外部真人参与。

### 架构专家

结论：`APPROVE WITH CONDITIONS`

- P1：不得为分支复制第二套正式 Canon；WorkingCommit 只能引用不可变修订，正式采用仍走唯一 project revision。责任：branches/runtime。
- P1：全局 candidate pointer 不能承载分支稿。责任：draft save。
- P2：连续调度留在 Main，但 session/head 权威必须在 Runtime。责任：Main/Runtime。

处理：采用稀疏分支投影；分支保存不改全局 candidate；Main 仅串行执行 Runtime 已固定的 targets。

### 技术专家 A：可靠性、安全、性能

结论：`APPROVE WITH CONDITIONS`

- P1：推进必须校验 task/session/branch/head/project revision，并在一个事务中完成。责任：generation/branches。
- P1：Context 不得信任调用方 branchId；只能从 candidate TaskRun 推导。责任：Context。
- P1：批量采用必须 author-only、幂等、单事务。责任：server/branches。
- P2：分支正文替换同章正式摘录，避免预算翻倍。责任：Context。

处理：全部写入不变量和测试矩阵。

### 技术专家 B：实现、迁移、测试

结论：`APPROVE WITH CONDITIONS`

- P1：Session targets 必须是稳定 chapter ID，并在创建时验证连续顺序。责任：branches。
- P1：任务完成重复调用必须幂等；两个任务争用 head 必须确定性冲突。责任：generation tests。
- P2：批量采用复用现有 passage/diff 与 temporal fact 语义，避免 UI/搜索投影漂移。责任：branches。

处理：接口只接受 chapter IDs；完成返回已持久化 commit；采用同步 passage/diff/事实与索引水位。

### 产品经理

结论：`APPROVE WITH CONDITIONS`

- P1：产品必须说清“连续试写”不等于自动采用。责任：UI/文档。
- P1：中途失败仍应显示已完成章节，不把整个 session 说成失败无产物。责任：workspace/UI。
- P2：首批只处理已存在章节必须显式提示。责任：UI。

处理：状态分别使用 working/incomplete/ready/accepted；分支卡显示完成数和正式版本未改变说明。

### UI/UX 设计师

结论：`APPROVE WITH CONDITIONS`

- P1：连续模式不能挤压正文；高级选项置于生成卡内部折叠/紧凑区域。责任：UI。
- P1：采用操作必须显示章数和一次性正式变更含义。责任：UI。
- P2：动态进度带文字与 aria-live；只有一章时解释范围限制。责任：UI。

处理：沿用正文主列，在生成卡内增加紧凑模式控件；分支摘要置于其后，不改变正文宽度。

## 9. 门禁结论

五个角色均为 `APPROVE WITH CONDITIONS`；所有 P1 已转为明确实现约束和自动化/视觉验收，没有待用户裁决的重大取舍。允许实施本文件定义的 AG-1 最小切片。若实施中需要自动创建章节、任意跨分支合并或后台运行，必须暂停并重新评审。
