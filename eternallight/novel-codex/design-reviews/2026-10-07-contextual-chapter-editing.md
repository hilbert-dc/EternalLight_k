# 章节操作回归正文与设置

## 变更概览

- 目标：删除「手动工具」总入口，正文以阅读和编辑为主；场景操作就近按需出现；章节关联归入页头章节设置。
- 非目标：不修改实际作品，不移除模型与 Runtime 的分析、事实建议或场景引用能力，不改正式发布契约。
- 影响模块：ChapterDocumentView、CreativeWorkspaceView、useChapterPublication、章节样式、当前 fixture 和测试。
- 验收：无模型时仍能编辑、保护、添加、移动、删除、保存、发布及查看版本；保护段落的正文和序号约束保留；自然对话与选区引用仍可用；技术输入不会暗中阻挡保存。

## 测试阶段零兼容门禁

- [x] 不新增历史版本判断、旧字段 fallback、双轨、迁移、回填或 deprecated 入口。
- [x] 一次性清理无入口的状态、组件接口、调用链、fixture 与现行文档。
- [x] 当前来源校验、并发阻断、保存回执与正式确认仍验证。
- [x] 使用隔离 fixture，不修改用户作品。
- 获得明确授权的兼容例外：无。

## 实施前五角色评审

### 架构专家 — APPROVE WITH CONDITIONS

场景行为归 ChapterDocumentView，章节关联归 CreativeWorkspaceView；删除 tools/toolsDirty/sceneDiscussion 注入接口。复用现有 DirectoryMenu 的按需菜单、定位和键盘处理，避免新增工具箱。已核实 writerPreview 常量没有消费处，整条作者侧预览读取链可删除；保留任务自身真实执行上下文与后端预览 API。

### 技术专家 A — APPROVE WITH CONDITIONS

完整删除技术事实输入的八个状态、本地快照字段、保存校验与手工 proposal 拼装。保留模型事实提案及 factDecisions、noConfirmedChanges、依赖处理和正式发布来源校验。发布无模型时仍需明确确认仅发布正文。

### 技术专家 B — APPROVE WITH CONDITIONS

同步 useChapterPublication 唯一生产调用方与 fixture。移除旧专用场景讨论入口测试，改验新菜单的保护、焦点、任务运行阻断和项目隔离；已有自然语言、选区与真实来源测试使用当前入口。无旧行为兼容分支。

### 产品经理 — APPROVE WITH CONDITIONS

移除重复分析/讨论入口、伏笔分析启动按钮和生成记录复本；已有目录与任务结果可达。保存、发布、正式确认、版本对照与章节关联保留。无需再建「高级工具」入口。

### UI/UX 设计师 — APPROVE WITH CONDITIONS

场景操作用有可访问名称的省略号菜单，键盘和触摸可达；复用菜单的 Escape/方向键/Tab 和视口定位。移动后聚焦原场景；删除后聚焦相邻正文，删除最后一段聚焦添加；添加后聚焦新正文。保护状态在正文旁可见，移动须检查两段，删除须检查后续保护段以保留序号。切换章节清除菜单。章节设置只容纳章纲关联和必要回执。

## 综合决策

- 最终结论：APPROVE WITH CONDITIONS；以上条件作为实施验收。
- P0/P1 阻断项：无。
- 实施范围：本记录列出的当前 UI、死状态、测试及文档。

## 实施后复核

- 实际变更：删除手动工具总入口及 tools/toolsDirty/sceneDiscussion 接口；场景操作复用 DirectoryMenu，保护状态就近显示，保留相邻/后续保护约束和添加/移动/删除后的焦点。章节设置移入页头，只保留章纲关联、重绑与真实回执。无章纲的导航明确标为「打开近章规划」。
- 删除旧作者侧预览、重复生成记录、重复伏笔分析/场景讨论和影响分析面板；对应专用组件、死状态、样式与旧测试删除。技术事实输入状态、浏览器快照字段、保存校验和手工 proposal 构造一并删除。后端模型分析、事实提案、假设及场景引用 API 保留。
- 与方案的偏差：使用已有 DirectoryMenu 代替原生 details 场景菜单，复用完整键盘和视口定位能力；不新增菜单基础设施。经消费搜索确认预览没有业务消费者，整条背景读取链删除。
- 架构专家：APPROVE；边界归对应对象，生产无旧入口、旧面板引用或兼容路径。
- 技术专家 A：APPROVE；保存 CAS、保存回执重读、发布真实来源/批次/事实/任务校验及正式作者决定保留。无模型时可显式确认仅发布正文，新事实不再通过技术表单输入。
- 技术专家 B：APPROVE；类型和全部调用方同步，当前 fixture 与测试一次性更新。测试 HTML 明确 UTF-8；视口切换等待实际尺寸生效，避免截图线程的迟到 resize 关闭菜单。
- 产品经理：APPROVE；章节分析与讨论由自然对话提出，资料通过目录/任务结果访问；编辑、保存、发布、正式确认及版本查看仍可达。
- UI/UX 设计师：APPROVE；设置弹层已修正为工具栏定位并限制宽度，外点关闭只在焦点残留隐藏内容时回 summary；窄屏、深浅色、键盘、运行禁用和删除最后一段均验证。
- 构建：`npm run build` 通过；存在工程原有 bundle 大小提示。
- 测试：7 项 creative-product-logic Node 用例、8 项隔离 Electron 界面用例通过。界面覆盖新场景菜单、来源替换/重试、8 类文档选区、真实 App 唯一输入框、workspace 保存边界、任务结果发布、章节书签/版本/正式事实确认及规划指定视角。未调用真实模型或修改用户作品。
- 测试证据：`/tmp/contextual-chapter-build.log`、`/tmp/contextual-chapter-logic-tests.log`、`/tmp/contextual-chapter-planning-tests.log`；workspace/App/task-results 3 个通过项在 `/tmp/chapter-context-verified.log`（其初次并发章节用例失败已修正），章节最终通过在 `/tmp/chapter-document-verified.log`。场景与选区的 3 项测试由 scene_menu_regressions 完成。
- 视觉证据：`/tmp/contextual-scene-editing-ui/` 的 330/740 深浅菜单，`/tmp/chapter-context-screens/` 的 330/980 设置和窄屏菜单截图，已目视核对。
- 零兼容门禁复查：PASS；定向 `git diff --check` 通过。
- 剩余项：无本次范围内未完成项。
