# 章节创作入口收敛评审

状态：本轮完成；五角色 PRE 与 POST 全部通过，无剩余 P0/P1。Electron UI 17/17、Node 3/3、其他合同 8/8、构建与规定格式化 PASS；零兼容门禁 PASS。2026-10-06。

## 目标与源码证据

章节承载标题与正文，右侧对话组织创作；已有章节页手动新增统一从作品目录进入。移除章节工具栏常驻“新建章节”和“起草本章／讨论本章”，空正文只保留轻量“让 AI 起草”。零章作品仍保留现有首次创建表单与目录，不在本轮重做欢迎流程。不改变保存、发布、作者决定或写权限。

- `CreativeWorkspaceView.tsx` 的 `prepareChapterWriting` 已携带精确 project/chapter 身份；空章默认指令保留标题，App 只在输入为空时预填。点击不是执行授权，不触发保存或模型。
- `App.tsx` 普通发送已附当前章节 ID 与章号的 `ConversationFocus`。导航上下文不等于写权限；已有精确来源不能被“本章”覆盖，未保存正文仍遵循当前发送门禁。
- 手动目录创建通过 `requestedChapterCreation` 打开现有 `StandaloneChapterForm`，保留卷 ID、原子创建与未知回执只读恢复。删工具栏按钮不能删此处理链。
- 普通 advisory 提示明确禁止为不存在的新章虚构 `generateChapter` ID。真正建章在 `generation_materialization.rs` 的 GenerationRun 事务中，校验计划、范围、源与预算。
- 实施前 `conversationContinuationEntry` 位于作品概况及章节设置内；本轮已移到精确匹配的当前回复后。仍绑定作品 revision、snapshot、sourceDigest，打开原续写许可确认。“自然要求写下一章即可直接自动建章”不是本轮能力，不能以按钮精简宣称完成。

## 最小实施取舍

1. 删除工具栏的创建与 AI 入口，保留修改记录／导出。目录右键、悬停／键盘／触屏创建及跨页 dirty 导航沿原链；已有章节页创建表单仅承接目录请求，零章首次 starter 保留。
2. `ChapterDocumentView` 按正文是否有非空文字显示一个原生按钮“让 AI 起草”，复用现 preparation；保留标题与正文输入 DOM。视觉空正文与首写权限分开：既有 `emptyChapter` 仍决定首写许可，不因清空已保存正文而放宽。已有正文无重复讨论按钮，直接在主对话表达。已有输入不覆盖、不自动发送；更改预填指令仍沿普通 advisory，而非关键词分流。
3. 将已绑定原始对话的续写许可入口从章节设置与作品概况移到 `WorkTaskPanel` 的 `conversation.renderCurrent` 包装内、匹配本轮回复之后，仅一份低优先级“用这次对话续写新章节”。不在 portal 底部混入其他任务，仅实际 matching conversation 存在且 App 原 exact prompt 核对通过时呈现。复用现范围与预算确认及 GenerationRun；不让作者先手建空章。点击仅打开原 Permit，不自动建章，原 source/revision/dirty/model 门禁保留；advisory 说明同步指向“本轮对话后的续写入口”。若要求无需许可入口的直接自然新章委托，应另审 typed Runtime 目标，不在本轮借假 ID、关键词或自动批准实现。

空作品没有当前章节，不能显示“起草本章”或伪造 chapter focus。首次创建表单与目录可手动建章；对话可讨论构想并进入既有新章许可流程。首次入门不是已有章节的重复工具栏入口，独立欢迎页为非目标。许可失败、来源过期或模型不可用须保留输入并显示真实原因，不创建占位来绕过。

## 五角色 PRE

| 角色 | 本轮结论 | 条件 |
| --- | --- | --- |
| 架构专家 | APPROVE WITH CONDITIONS（根代理汇总） | 新章沿权威任务建章，导航不授写权 |
| 技术专家 A | APPROVE WITH CONDITIONS（本轮报告） | 单飞、精确 scope、回执与异步隔离 |
| 技术专家 B | APPROVE WITH CONDITIONS（根代理汇总） | 当前接口与 fixture 一次同步、无双轨 |
| 产品经理 | APPROVE | 根代理范围满足：复用原 Permit，无手动空章前置；不假称直接自动委托已完成 |
| UI/UX | APPROVE | 一份对话后按需许可、轻量空正文 CTA、草稿保留与键盘可达，验收仍需实施证据 |

综合：五角色 PRE 齐备后已实施。若跳过新章权威链却宣称支持“写下一章”，仍为产品 P1 阻断。

## 本轮验收

- [x] 已有正文：无顶部新建／AI 按钮；直接讨论附正确章节，模拟 IPC/UI 验证纯问题零写与原改稿／发布边界。
- [x] 空章：点击 AI 只预填、聚焦对话，零模型／保存；已有输入及选区保留，作者可手写。
- [x] 目录：根章与卷内章精确创建，dirty 取消不创建，未知回执重读不重复创建；目录与 viewport 回归验证入口可达。
- [x] 下一章许可：真实 React + mock IPC 验证匹配原对话、目标、来源水位与范围确认后的准确 intent/scope handoff；未确认不调用执行，不自动发布／确认设定。没有验证真实模型新章生成或保存。
- [x] 项目／章节切换、重复点击、晚回包、取消／失败：不串章、不覆盖下一条输入；同 prompt 新发送失败不能复用旧许可。
- [x] 空作品、不可用模型、来源过期及任务运行状态均明确；亮暗窄宽与可访问名称通过。

自动化渲染回归、根代理真实 GUI 操作与真实模型交付分别记录，不能互相替代。PM/UX 本人只读源码；以下 GUI 由根代理实际操作。本轮模型连接验证不可用，零模型调用，未验证新章真实模型交付。

## 零兼容门禁

- [x] 方案无旧字段 fallback、迁移、回填、双读双写或 deprecated 路径。
- [x] 不重写模型历史，不自动保存／采用／确认，不扩预算，不猜章节 ID。
- [x] 实施后同步生产、fixture、文档并复核当前数据一致性与错误恢复。门禁 PASS，无兼容例外。

## POST

源码核验：顶部仅切章、修改记录、导出；正文空白处一次 CTA，既有首写与标题核验不放宽。App 接受新发送即清旧 continuation；工作区只在当前 completed、无 error、exact prompt 与三水位匹配时呈现当前回复后续写许可，避免同文新失败误用旧对话。目录请求、保存发布与源回执链不变。

根代理 GUI：Electron `127.0.0.1:5173` 的规划验收作品第 4 章，CTA 点击聚焦 composer 且保留已有输入；临时 8 字正文让 CTA 消失，清空后恢复，同一 textarea 节点 113 保持，零保存／模型；恢复 0 字同步状态，保存／发布 disabled。目录菜单仍有新章／新卷／整理，Escape 关闭未创建。临时定位与输入已还原。构建 PASS。

| 角色 | 当前 POST |
| --- | --- |
| 架构专家 | APPROVE（本轮源码报告） |
| 技术专家 A | APPROVE（本轮源码报告），无 P0/P1 |
| 技术专家 B | APPROVE（最终源码／自动化报告），无 P0/P1 |
| 产品经理 | APPROVE，入口收敛与许可交接闭环通过，未扩大创作权限 |
| UI/UX | APPROVE，空态、草稿／编辑 DOM、精准上下文与可达性通过 |

最终验证：六目标核心 Electron UI 13/13（定向复验 12 项加未变手工流程 1 项），目录／viewport 4/4，共 17/17，零 skip；`domain-task-prompt` Node 3/3，根代理其他合同 8/8。核心复验日志 `/tmp/novel-chapter-entry-recheck-ui.log`。快照已逐项审阅，最终 build PASS、`just fmt` exit 0 PASS；首次 sandbox 的 UV cache 权限受限后，在本地 cache 重跑成功，没有源码错误或自动审批拒绝。源码与测试冻结，fmt 后未重跑测试。

收口核对：CTA 可访问名称“让 AI 起草本章”包含可见文本。零章首次 starter 保留现表单与目录，范围限于已有章节的重复工具栏入口。无剩余 P0/P1；本轮纯 UI 与合同交接完成，不要求额外模型调用，也不将 mock intent handoff 写成真实新章交付。不继承模块记录阶段的批准。
