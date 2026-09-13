# 自动生成 AG-0 落地方案与五角色评审

> 现行覆盖规则：本文涉及旧库备份、迁移或兼容的历史结论均已由 [测试阶段工程策略](TEST_STAGE_ENGINEERING_POLICY.md) 废止。

版本：0.2 · 日期：2026-09-12 · 状态：AG-0 已实施；五角色复核通过，保留发布验收项

关联需求：[PRODUCT_AUTOGENERATION.md](PRODUCT_AUTOGENERATION.md) AG-0、G01～G09、G20、G22～G23。

## 1. 本批范围

目标：把当前“一段自然语言发送给一次 Codex 回合”收敛为可验证的单章生成任务。作者从中央章节发起生成时，系统显式绑定目标章节、任务用途和阅读位置；Agent 保存有效草稿后任务才能完成；失败或取消保留检查点；审阅针对候选合并结果运行确定性检查。

本批交付：

- 结构化 `CreativeTaskTarget`，由 UI → preload → Main → Runtime 固定章节与场景位置；自然语言不再决定写到哪个章节。
- TaskRun 保存 `purpose`、目标、要求产物、阶段和完成状态；Runtime 拒绝跨用途 Context 请求。
- Writer Context 返回相关正式/候选前文原文及精确修订来源，并以 UTF-8 字节硬预算截断，显式报告遗漏。
- `complete_task` 验证写章任务至少产出一个属于目标章节的有效 DraftRevision；主进程不再把“模型回合结束”直接视为完成。
- 检查点标记完整度；取消/失败后工作区能显示已保存输出和恢复提示。
- 候选审阅检查实际待采用草稿的场景组、正文一致性、锁与事实证据锚点；语义检查未执行时清楚标为未检查。
- 中央章节页提供“生成当前章”的显式入口与任务状态。

非目标：试写分支、连续多章、关键选择多方案、批量采用、审美偏好、向量检索、云端后台执行。它们分别属于 AG-1～AG-4。

## 2. 推荐架构与数据流

沿用现有边界：React 只提交目标与意图；Electron Main 创建 TaskRun 并调 Codex；Runtime 是任务、候选与完成状态的权威；Agent 仅通过 candidate MCP 保存候选；正式采用仍由 Author Gateway 发起。

```text
章节页 target + intent
  → Main 校验 target 属于当前 project
  → Runtime create_task(purpose=writer, requiredOutput=draft, targetChapter)
  → Context(task binding + reading position + byte budget)
  → Codex turn → MCP save_draft_checkpoint(task binding + target validation)
  → MCP complete_task(summary)
  → Runtime 验证 revision/project/chapter/TaskRun outputs
  → ready_for_review
```

TaskRun 状态扩展为 `queued/running/waiting/ready_for_review/failed/cancelled`。`completed` 仅保留迁移兼容读取并在迁移中转换，新的写章任务以 `ready_for_review` 表示交付结构完成。阶段以 `preparing/writing/checking` 等字符串记录，但首批不建设复杂调度器。

Context 字节预算是序列化响应的硬上限。先保留硬约束和当前章计划，再加入最近的前文章节；每段带 chapter/draft/passsage 来源。事实仍受既有条目上限约束，并在统一 omissions 中报告。Writer 不读取未来章正文。

## 3. 接口与数据调整

- 数据库版本递增；`task_runs` 新增 `purpose`、`target_json`、`required_output_kind`、`stage`、`result_summary`。
- `task_checkpoints` 新增 `completeness = partial | sceneComplete | chapterComplete`。
- 创建任务请求新增结构化 `purpose` 和 `target`；Runtime 验证 `purpose` 与 kind 的允许组合、目标章归属和输入 revision。
- candidate Context 的 `purpose` 必须等于 TaskRun purpose；author 调用可显式选择用途。
- 保存 Draft 时，candidate 只能写目标章节；章节完成任务不得动态创建另一个章节。
- 完成任务只接受由任务自身写出的持久化 output refs；Runtime 从数据库重新计算，忽略伪造的外部引用。
- Agent IPC 新增 `generateChapter(projectId, request)`；保留 `sendPrompt` 供自由协作，但正文按钮使用结构化入口。

迁移对已有数据库只加列，不删除数据；现有 queued/running 等任务保持可读。字段默认值必须让旧测试 fixture 可迁移。

## 4. 错误、恢复、安全与性能

- 目标章节不存在或不属于作品：422，不启动模型。
- task purpose 越权：401/409，不返回更高信息范围。
- Codex 回合结束但没有草稿：任务标 failed，错误为 `required_output_missing`；已有检查点保留。
- 取消后迟到 MCP：现有 task 结束校验拒绝写入。
- 正式 revision 改变：现有 stale task 校验拒绝写入，提示基于最新版本重试。
- Context 超预算：保持 JSON 有效，删除低优先级原文并返回 omissions；硬约束放不下则 413，不静默继续。
- Renderer 不获得 Runtime token、数据库路径或 provider key。所有 IPC 继续二次校验 sender。
- 章节原文按最近优先，首批最多返回 6 个片段；单段与总响应均有限制，防止超长注入。

## 5. 测试与验收

- Runtime：指定第三章不会写第十章；purpose 越权；Writer 只见允许前文；字节预算；有效/无效 complete；取消后写入；检查点完整度；候选合并检查。
- MCP：新字段 schema、complete 不信任模型 refs、错误结构。
- Electron：结构化请求创建正确任务，工作包使用显式章节；无产物回合返回失败；自由聊天行为保持。
- UI/构建：章节页按钮传正确 target；生成中、部分完成、失败、待审阅文案；键盘焦点和禁用状态。
- 命令：`npm run test:runtime`、`npm run test:agent`、`npm run build`。

## 6. 五角色独立评审

以下是同一模型在五个隔离视角下的工程评审，不代表外部真人意见。

### 架构专家

结论：`APPROVE WITH CONDITIONS`

- P1：不能在 Electron Main 复制任务状态机；状态与完成验证必须由 Runtime 掌握。责任：Runtime 实施。
- P1：结构化 target 必须贯穿 IPC、TaskRun、Context、MCP 保存，不能只拼进提示词。责任：全链路。
- P2：当前 `service.py` 已超过 900 行，本功能应放入新的 `generation.py`，仅复用既有底层函数。责任：Runtime 实施。

处理：方案已明确 Runtime 权威、新模块及全链路 target。

### 技术专家 A：可靠性、安全、性能

结论：`APPROVE WITH CONDITIONS`

- P1：模型提供的 outputRefs 不可信；完成时必须查询任务自产出的 revision。责任：Runtime。
- P1：candidate 不能通过 `read_context(purpose=planner)` 绕过 Writer 信息边界。责任：Context。
- P1：Context 预算必须包含序列化后的正文，超限不可产生无界 MCP 返回。责任：Context/MCP。
- P2：迟到写入、重复 complete 和 stale revision 需要测试。责任：测试。

处理：全部写入接口与验收矩阵。

### 技术专家 B：实现、迁移、测试

结论：`APPROVE WITH CONDITIONS`

- P1：迁移增加非空字段需要默认值或显式回填，保证现有数据库可打开。责任：Schema。
- P1：目标章校验必须使用 chapter ID 与 project ID，不能只信 chapterNumber。责任：Runtime。
- P2：旧 `sendPrompt` 测试仍需通过，结构化生成应是新入口，避免把所有聊天命令一次性改造成复杂路由。责任：Electron。

处理：新增列带默认值；目标校验双键；保留自由协作入口。

### 产品经理

结论：`APPROVE WITH CONDITIONS`

- P1：作者必须从当前章节直接发起，不再依赖模型理解“第三章”。责任：UI/IPC。
- P1：完成、失败、部分完成和待审阅需要不同状态；“模型回复了”不能显示成“草稿完成”。责任：Runtime/UI。
- P2：本批必须明确没有交付连续多章，避免范围与宣传错位。责任：文档/界面。

处理：结构化入口、状态和阶段范围已列为交付项。

### UI/UX 设计师

结论：`APPROVE WITH CONDITIONS`

- P1：入口应在正文上下文中展示目标章，按钮禁用原因可见。责任：章节 UI。
- P1：生成中仍允许作者打开已保存检查点，失败不得遮蔽正文。责任：章节 UI。
- P2：状态文本不能只用颜色；按钮需原生键盘可用，加载区使用 `aria-live`。责任：UI。

处理：本批 UI 验收包含目标、部分完成、失败、可访问状态。

## 7. 门禁结论

五个角色均为 `APPROVE WITH CONDITIONS`，全部 P1 已转化为本方案的必做实现与测试项，没有未解决的产品取舍。允许开始 AG-0 实施。任何新增试写分支、批量采用或审美记忆的代码均视为实质偏离，需要新一轮对应评审。

实施后在本文件追加实际变更、测试结果、偏差与剩余风险。

## 8. 实际实施结果

本次按评审范围完成 AG-0，没有引入 StoryBranch、连续多章、批量采用或审美记忆。

- 当前 Schema 为 TaskRun 提供 purpose、target、required output、stage、delivery status 与 summary，并为检查点提供完整度；测试库直接按当前 schema 重建。
- 新增 `generation.py` 承担任务目标、用途/产物权限、取消和完成契约；Runtime 只采信任务自产出的持久化 DraftRevision。
- 新增 `context_limits.py`，以完整 JSON 的 UTF-8 字节数实施 4,000～64,000 字节硬边界，并报告截断项。
- Context、草稿保存、ReviewBundle 与 MCP 全链路校验用途、目标章、证据场景和检查点完整度；语义审阅未执行时是非阻塞“未检查”，不是伪通过。
- Electron 增加结构化 `generateChapter`，按作品/用途/章节隔离会话；恢复任务时重建同一目标绑定。
- React 章节页增加目标明确的生成卡片、创作意图、任务交付状态；正文、场景保护和事实表单保持在同一主列。
- Novel Workflow Skill 已同步新的 context 字节预算、来源引用、检查点完整度和 complete 契约。

实现与原设计有一项有意偏差：为安全迁移保留既有 `task_runs.status = completed`，另以 `delivery_status = ready_for_review` 表达可交付状态，而没有直接修改旧 CHECK 约束。UI 与 API 以 delivery status 判断“可审阅”，语义仍满足评审要求。

## 9. 验证结果

| 验证 | 结果 |
| --- | --- |
| Python 语法检查 | 通过：server、MCP、schema、service、generation、context limits |
| `npm run test:runtime` | 通过：32/32；覆盖显式第三章目标、partial 不可完成、用途/产物越权、来源引用、字节预算、证据锚点与迁移 |
| `npm run test:agent` | 通过：25/25；覆盖结构化章节契约、MCP complete 请求与既有 App Server 行为 |
| `npm run build` | 通过：TypeScript 无错误，Vite 生产构建成功（31 modules） |
| macOS Electron 视觉走查 | 通过：1318×768 下目标章、意图、按钮可读；正文主列宽度修正后，正文和后续表单可正常滚动 |

Skill Creator 的自动快速校验脚本因本机缺少 `yaml` Python 模块未能运行；没有为此临时安装依赖。已人工核对 Skill frontmatter 与名称，并由 Agent fixture、MCP 测试和应用构建覆盖实际加载路径。这不等价于补齐该外部校验器。

## 10. 实施后五角色复核

### 架构专家：PASS

生成状态权威仍在 Runtime；target 贯穿 UI、IPC、TaskRun、Context 和候选写入；新增逻辑已从原 900+ 行 service 抽入独立模块。未发现 P0/P1。

### 技术专家 A：PASS WITH RELEASE CONDITIONS

模型引用不可信、purpose 越权、上下文无界和无草稿伪完成均已有服务端约束与测试。真实模型故障恢复和进程重启中的重复执行仍需发布旅程验证，不阻塞 AG-0 本地切片。

### 技术专家 B：PASS

Schema 新列有默认值并保留升级备份；目标用 chapter ID + project ID 校验；自由 `sendPrompt` 保留。测试和构建通过，未发现 P0/P1。

### 产品经理：PASS WITH SCOPE NOTE

作者从当前章节发起、目标可见，交付状态不再由聊天回复冒充。本批只能称为“可靠单章生成基础”，不能称为连续长篇自动创作。

### UI/UX 设计师：PASS

按钮是原生可聚焦控件，状态带文字并使用 `aria-live`，模型不可用/脏编辑有禁用原因。视觉走查中发现的正文窄列已改为统一主列并复核通过。

## 11. 剩余风险与下一门禁

- 未使用真实模型完成“意图 → MCP 保存 → 候选可审阅”的整条旅程，不能据自动化测试宣称文学质量或真实生成成功率。
- 未做 Windows 实机、窗口重启恢复、长正文极限数据和辅助技术完整键盘旅程。
- AG-1～AG-4 全部未实施。尤其没有分支工作头、临时事实、连续 1～5 章或批量采用；进入 AG-1 前必须重新完成对应五角色评审。
- 父仓库中 `eternallight/` 当前为未跟踪内容，需要纳入版本管理后才具备可复现提交边界。
