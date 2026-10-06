# 模块内修改记录评审

日期：2026-10-05。状态：完成。五角色 PRE 均为 `APPROVE WITH CONDITIONS`；最终五角色 POST 均为 `APPROVE`，本轮无剩余 P0/P1。最终源码、五个唯一 UI 用例、实机只读证据、构建、格式化和产品差异检查已收口。

本记录遵循 `AGENTS.md`、`TEST_STAGE_ENGINEERING_POLICY.md` 与 `DESIGN_REVIEW_TEMPLATE.md`。评审者为 AI 产品/UX 代理，不代表外部公司产品经理。

## 变更概览

- 用户目标：修改记录属于当前创作模块，不应成为作者需要单独理解的全局管理页面。
- 目标范围：删除全局侧栏入口、独立 `diffs` 页面及四模块选择器；当前模块用轻量入口打开本地只读浮层。
- 影响模块：App 导航、CreativeWorkspaceView、模块记录浮层、PlanningVersionsPanel/PlanningRevisionPreview、章节对照读取与相关类型/调用方/fixture。
- 非目标：历史回滚、撤销、任意历史检索、模型摘要、自动保存/采用、角色或世界规则的伪历史；不改 Runtime/API/CAS/写入权限。
- 前阶段已完成证据留在 `PLANNING_VERSION_UX_REVIEW.md`。其批准不代替本阶段五角色 PRE。

## 产品决策与最小实现

1. 修改记录在章节、作品方向、全书大纲/当前卷纲、近章规划内查看。入口语义指向当前模块，不提供全局模块切换。
2. 查看只打开 local dialog，不导航、不保存、不丢弃。原编辑 DOM 保持挂载，内容、选区、滚动、检查决定及未发送对话草稿不被重置。
3. 浮层关闭就是返回编辑。保留明确关闭按钮和 Escape，焦点归还仍 connected 且属于当前 scope 的触发点，使用 preventScroll；项目/页面变化导致自动关闭时不把焦点送回旧页面。
4. 未保存修改存在时仍允许只读查看。明确比较的是已保存内容，不包含未保存修改；来源指定、取消与锁定动作禁用，并说明先保存内容后才能改变设置。门禁使用实时 dirty/running 状态，不仅使用打开浮层时的快照。
5. 原来源设置 API、同步单飞、精确回执核对与未知结果只读恢复保留。模型运行期间沿原写操作门禁，不因浮层而增加执行能力。
6. 项目、模块、章节或卷精确身份变化时卸载浮层并关闭。卷身份包含目录 volumeId 及当前唯一节点绑定，不能按标题匹配；异步读取及回执仍核对原 owner，不能在新 scope 首帧展示旧内容/错误。
7. 章节读取 exact projectId/chapterId 的 `getDiffs`。没有章节时不伪造修改记录，章名/正文不能从其他章节借用。
8. 大纲与卷纲属于同一份 outline authority。卷浮层只预览精确唯一节点及下级，不展示其他卷；来源设置与锁定作用于整份全书大纲，按钮附近明确说明该范围。不能称独立卷版本或独立卷采用。
9. 来源缺失、重复或失效时明确说明不可核对，不改用全书/同名节点掩盖错误。没有对应历史 API 的角色、世界等页面不增加空记录入口。

## 测试阶段零兼容门禁

- [x] PRE 不新增旧路由兼容、旧字段 fallback、双读双写、升级迁移、回填或降级。
- [x] 全局 `diffs` 入口/页面/选择器直接删除，不保留第二条旧导航路径。
- [x] 实施后一次性同步当前 ViewId、导航/对话 focus 调用方、组件、类型、fixture 和现行文档；没有引用已删除路由的隐藏分支。
- [x] 实施后验证当前版本 dirty/运行/锁定门禁、数据一致性、失败恢复、单飞及跨 scope 隔离。
- 获得明确授权的兼容例外：无。

## 五角色 PRE

### 架构专家

- 结论：`APPROVE WITH CONDITIONS`（本阶段批准由根代理汇总，不引用前阶段代替）。
- 条件：模块内浮层只负责现有 API 的阅读/操作入口，不新增来源状态或第二执行器；删除路由的调用方完整切换，卷纲保持单 outline authority。

### 技术专家 A

- 结论：`APPROVE WITH CONDITIONS`（codex_pm；根代理本阶段汇总）。
- 条件：scope incarnation、每 await 后 owner 核对、实时 dirty/running 门禁、原 CAS/回执核对、未知写入只读恢复与跨作品/卷迟到隔离。

### 技术专家 B

- 结论：`APPROVE WITH CONDITIONS`（cursor_pm；根代理本阶段汇总）。
- 条件：真实 Renderer 验收、原编辑 DOM/选区/滚动保留、键盘焦点和两主题窄宽布局；删除全局页面及调用方一次性同步，无双轨。

### 产品经理

- 结论：`APPROVE WITH CONDITIONS`。
- 用户价值：作者在当前章节或规划内容旁完成核对，不需要离开创作并再次选择模块。当前范围提供已保存内容阅读，不能借“记录”名称宣称新增了完整历史或撤销。
- 条件：查看绝不触发保存/模型/采用；脏内容与保存内容区分，卷设置整份大纲范围准确，无 API 的页面不造假入口；原检查与失败核验能力没有被删除。

### UI/UX 设计师

- 结论：`APPROVE WITH CONDITIONS`。
- 条件：local modal 正确隔离背景交互而保持其 DOM；默认轻量状态和内容入口，设置仍按需披露；错误与恢复入口可见。关闭返回触发点不抢编辑滚动，自动 scope 关闭不抢新页面焦点。窄屏只有可见浮层的一个阅读滚动层，关闭按钮与键盘路径可达。

## 本阶段验收旅程

- 章节有未保存修改：打开记录不弹离开确认，不调用保存/模型，显示 exact 章已保存对照；关闭后原正文、选区、外层滚动与草稿保留。
- 规划 dirty：可读内容、对照和来源设置说明，但 select/clear/lock 均不能提交。保存、采用检查仍沿编辑页既有入口。
- 卷纲：同名卷也只匹配精确 volumeId/node；只展示本卷节点及下级，设置处说明作用于全书大纲，不伪造独立卷修订。
- pending 读取或操作时换项目/模块/章/卷：浮层关闭，迟到回包不重开、不显示旧错误、不更新新 scope；相同 A→B→A 也不接受旧 incarnation 回包。
- 来源错误/缺失/重复：可读原因与重新核对，不回退到别的节点、不自动写或重试未知操作。
- 键盘打开、Tab/Escape/关闭、触发点焦点返回及 330/740 亮暗布局通过；编辑 DOM 不卸载，长内容无额外嵌套滚动。
- App 无全局修改记录侧栏入口/独立页面/模块 picker，当前类型及导航调用方没有旧路由残留；角色/世界等无伪记录入口。

## 实施后复核

- 实际变更：新增 `ModuleRecordsDialog` 与局部样式；App 删除全局入口、独立页面/选择器及对应 ViewId，模块入口打开本地浮层；原编辑器与对话仍挂载。规划面板/预览增加精确 outlineNodeId 范围，其读取与写入权限沿用原 API。
- 与 PRE 偏差：当前主链未发现扩大业务权限或另建版本状态；对照依然是完整已保存内容，不是增量 diff 或历史回滚。
- 自动化、实机及构建证据：分项如下，不引用前阶段测试作为本阶段通过证明。
- 零兼容门禁源码复查：`PASS`，未见旧路由 fallback、双轨、迁移或 schema/API 扩展。
- 五角色 POST：全部 `APPROVE`。本记录者完成产品经理/UI/UX 最终批准，根代理汇总架构与技术 A/B 的最终批准；本轮无剩余 P0/P1。

### PM/UI/UX 冻结源码检查

已亲读 App、ModuleRecordsDialog、CreativeWorkspaceView、PlanningReadingView/Header、PlanningVersionsPanel/Preview、章节对照读取及对应 CSS：

- 查看入口直接打开 local dialog，没有 requestNavigation/save/model；编辑器和 composer 不卸载。dirty 状态明确已保存对照不含编辑区内容，实时 blocked/dirty 禁止来源设置写入。
- dialog 使用原生 showModal，初始聚焦关闭按钮，提供 aria-label、Escape 与明确关闭操作。只有用户主动关闭才归还 connected opener 且 preventScroll；自动 scope 关闭避免归还旧触发点。
- App 的 project/API/view/chapter/volume/source 匹配决定是否挂载浮层，requestId 为每次打开建立新的 incarnation；applyNavigation/reset 清除浮层。旧 panel/read effect owner 和序号隔离保持。
- 章节沿 exact project/chapter getDiffs。各份规划保存内容独立验证节点 id/clientKey 唯一，不因为历史份缺卷而改用全书/同名节点；当前份定位失败外露 alert 并禁设置。
- 卷浮层说明只查看本卷，后续依据与锁定作用整份大纲。原 CAS、同步单飞、回执核对与未知结果只读重读未改变；采用检查仍留编辑页。
- 全局侧栏/独立页面/四模块 picker 已移除；角色、世界等没有伪历史入口。浮层窄屏对照改单列，原生 modal 负责背景交互隔离；布局实际结果以本阶段测试/实机为准。

卷范围边界已修并亲读最终三处源码：Workspace 原样传 requestedOutlineNodeId，不因节点缺失/重复退为 null；App 与 source.nodeId 的比较明确规范为空 null；完全未关联的独立卷由 Dialog 显示 alert，不挂载 Panel，也不改用全书记录。原反馈中的无反馈/失去卷 scope 风险已消除。技术 A 将该范围问题按 P1 收口，最终独立卷 Renderer 反例通过；本记录不把初始反馈优先级当作尚存风险的依据。

### 本阶段已收到证据

- 根代理报告集成类型检查 `PASS`。
- 规划 Panel/Preview 作者报告新 scoped Electron 专项 1/1、类型检查 `PASS`，并亲审三个 scope 快照及亮暗宽度快照；其架构/技术 A 分模块 POST `APPROVE`。这不是整个 App 实机验收。
- 根代理首次集成构建通过。卷边界与 modal 样式修正后的最终构建、Renderer、实机及五角色证据见下文；没有以前次构建代替最终源码验收。
- 本记录者只读源码及编写本评审文档，没有运行测试、操作 GUI 或调用模型。

### 根代理实机只读验收补证

根代理操作真正 Electron（开发服务 127.0.0.1:5173），使用同测试作品“雾港钟声”；本记录者未点击实机：

- 侧栏无全局修改记录入口。
- 章节本地浮层绑定 exact 章标题并发起 getDiffs；原测试数据返回 ChapterNotFound。明确错误与重新读取入口可见，Escape 返回原触发点。此项证明本地范围、失败反馈和焦点路径，不是正常章节对照读取成功。
- 全书大纲浮层显示当前内容已保存、尚未采用；写作设置默认关闭，关闭后焦点回原入口。
- 正文“查看卷纲”后打开本卷浮层，标题、卷内容及单元下级准确，不混全书主线；写作设置作用整份大纲的说明外露。
- 实机本轮操作零模型请求、零作品写入。截图发现原树/卡片样式仅在 main-panel 生效，根代理已扩展共享选择器支持 modal；这一修正不改变 API 或来源。
- 根代理最终实际 Electron 再核截图：精确本卷范围、共享树/卡片布局正常，无默认 ol 数字或 dd 缩进；关闭浮层焦点正确返回。根代理亲审窄暗色图通过。

### 最终验证证据

- 架构/技术 A 已给整个集成 POST `APPROVE`：全局 route/ViewId/selector 移除、模块精确 scope 与 requestId、编辑器挂载、dirty 禁设置和失联卷处理均核对。
- 技术 A 早期 scoped Electron 1/1、类型与差异检查通过，日志 `/tmp/novel-module-records-volume-kind.log`。源额外核对节点必须唯一且 kind 为 volume，同 ID 类型变化不能冒充卷；四个范围反例快照已审。该次结果不与最终同一专项重复计数。
- 最终实际 native dialog scoped 专项 1/1、零跳过，进程 17819 exit 0，日志 `/tmp/novel-module-records-modal-ui-final.log`。技术 A 亲审亮/暗 × 330/740 四张图片，确认本卷范围、无横溢出、树和卡片共享样式正常。
- 实际 App viewport 专项 1/1、零跳过，进程 65175：54 个页面布局组合（9 页 × 6 尺寸/主题组合）及章节浮层几何、对比度 ≥4.5、末行可达、只读重试与迟到跨项目隔离。
- 章节与规划整合两个用例 2/2、零跳过，进程 38141；项目入口 1/1、零跳过，进程 4021，日志 `/tmp/novel-module-records-project-final-ui.log`。其他整合日志为 `/tmp/novel-module-records-final-three-ui.log`、`/tmp/novel-module-records-final-ui.log`。
- 上述最终共五个唯一 UI 用例全部通过，不将先前同一专项的重复运行累加。技术 B 已亲审快照并给最终 POST `APPROVE`。
- 行为证据：dirty 卷/章保留原 DOM/选区/草稿，关闭回精确触发点；独立/失联卷不回退全书，换卷/章关闭；模块记录不新增作品写入或模型调用。原业务旅程仍保留显式写入/检查/采用断言，不把其计数误说成全测试零写入；本地 mock 对话不称实模。
- 最终生产构建进程 41855 exit 0，日志 `/tmp/novel-module-records-build-final.log`；`just fmt` 进程 28230 exit 0；产品范围 `git diff --check` 通过。格式化后未重跑测试。

### 已修复的验收首失败

- 卷请求 null 导致失联范围无反馈/可能误用全书，已按前述精确身份路径修复并覆盖。
- modal 起初未获得 main-panel 专用树/卡片样式，生产共享选择器已同步；独立 fixture 也补齐真实 dialog 容器及生产 planning-reading.css，最终图片与回归通过。
- 项目入口原 fixture 漏掉当前近章规划需要的 getIdentityCatalog，按当前接口补齐后通过；不是生产旧格式兼容或为了过测试降低门禁。
- 实机旧测试章节返回 ChapterNotFound，仍按真实错误记录；没有修改作品以伪造正常读取成功。

## 五角色最终 POST

| 角色 | 结论 | 最终依据 |
| --- | --- | --- |
| 架构专家 | `APPROVE` | codex_pm 与根代理最终汇总：模块浮层复用单 Runtime authority，旧路由完全删除，卷仍整份大纲来源与设置范围。 |
| 技术专家 A（codex_pm） | `APPROVE` | 精确 scope/owner/CAS/未知回执只读核对、唯一 volume 节点、最终 native modal 专项和四图亲审通过。 |
| 技术专家 B（cursor_pm） | `APPROVE` | 四个整合用例零跳过、实际 App 布局与交互/错误/迟到隔离、快照及差异检查通过。 |
| 产品经理 | `APPROVE` | 本记录者源码检查结合明确分开的自动化/根代理实机证据：记录属于当前模块，查看不保存、不采用、不调用模型，无剩余 P0/P1。 |
| UI/UX 设计师 | `APPROVE` | 原编辑器保留、dirty 可读禁写、精确卷范围、异常可见、键盘焦点返回和两主题窄宽实际布局通过。 |

综合决策：`APPROVE`，本阶段完成，零兼容门禁 `PASS`。正常章节 getDiffs 实机成功不在本次已有证据之列，不以错误路径验收冒充；没有新增或宣称完整历史浏览、撤销、回滚、自动确认、模型输出成功或文学质量提升。
