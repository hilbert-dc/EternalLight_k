# 修改记录入口与位置评审

日期：2026-10-06。状态：完成。五角色 PRE 均为 `APPROVE WITH CONDITIONS`，全部在实施前获得；最终五角色 POST 均为 `APPROVE`，无剩余 P0/P1。本结论来自本轮独立评审与证据，不沿用前轮批准。构建、规定格式化与最终范围差异检查通过。

## 目标与当前证据

用户反馈设计体验差、位置不合适。本轮重新评估入口和阅读重心，不把前轮正确的来源/权限边界等同于布局体验已满足。

PRE 时源码显示：修改记录入口分别位于规划阅读动作末尾、规划 Header 末尾和章节工具条；`ModuleRecordsDialog` 使用 showModal、最大 880px 宽和 35% 黑色遮罩。记录虽绑定模块，查看时仍形成强中央任务，默认说明/状态比作者内容更突出。

目标：入口可预期、记录处于当前内容旁、默认先读内容，需要时才对照或改变设置。非目标：API/schema、来源权限、模型调用、自动保存/采用、历史回滚或角色/世界的伪历史。

## 已确认的最小方案

- 记录入口使用图标与“修改记录”标签，aria-label/title 可读。规划入口经 portal 放入 App maincontent-header 右侧，由 Workspace 提供精确内部节点；章节工具条采用同一表达。删除 Reader/Header/brief 普通重复入口，每个模块同一时刻只出现一个主记录入口。
- 原生 `popover="auto"`，非模态 `role="dialog"`，不写 aria-modal，不 trap Tab、不 inert 背景。主区右上贴边，最大宽 440px；不占用右侧对话、不重排原编辑区，不保留中央大遮罩。
- ResizeObserver 和 window resize/scroll 跟随真实主区边界。主区较窄或 viewport≤640 时同一组件采用安全 sheet 布局，边缘至少 12px，不横溢；不是额外一套数据/交互路径。
- 头部紧凑且 sticky，保留明确关闭，刷新改图标且有可读名称；body 独立单层滚动。默认当前已保存内容预览展开，比较按需打开，写作设置排最后。移除占首屏的泛系统说明，dirty、错误、实际来源不同和作用范围解释不隐藏。
- 背景点击自然 light-dismiss，目标自然获焦；明确 Close 或 panel 内 Escape 归还精确且仍 connected 的 opener。Tab 到背景编辑后 Escape 关闭记录但保留已有背景编辑焦点，不主动抢回入口。scope 变化卸载、尺寸调整或自动关闭不抢新页面/背景目标焦点。
- 原编辑 DOM、输入、选区和滚动保持。查看与定位不触发保存、丢弃、模型或采用；重定位不重挂编辑器，也不触发重新读取。

## 必须确认的交互边界

1. 非模态必须允许 Tab 到背景及继续编辑。开启初焦点明确；Escape/关闭只收起记录。panel 内 Escape/明确 Close 还入口，背景编辑中的 Escape 保留背景焦点；light-dismiss 与明确关闭须区分，不能所有 toggle closed 都归还入口而抢走作者刚点击的正文焦点。
2. 继续使用 project/API/view/chapter/volume/node/request incarnation 的精确范围；换范围关闭，迟到读取/错误不重开或串入新范围。
3. dirty 允许读取已保存内容，对照明确不含未保存修改；select/clear/lock 沿实时 dirty/running/CAS 门禁，不能因非模态编辑导致旧按钮获得权限。
4. 卷只读精确节点及下级，设置仍作用整份大纲，范围明示；失联/重复节点与未关联卷不回退全书或同名内容。
5. 锁定、未知操作回执、读取错误与只读重试保留可见处理入口。没有历史 API 的模块不增加虚构空历史入口。

## 五角色 PRE

| 角色 | 当前结论 |
| --- | --- |
| 架构专家 | `APPROVE WITH CONDITIONS`，根代理确认本阶段实施前已获得；只改展示与原 API 入口，来源和权限不变。 |
| 技术专家 A | `APPROVE WITH CONDITIONS`，根代理确认本阶段实施前已获得；焦点关闭原因、scope/request incarnation 与原回执/CAS 边界保留。 |
| 技术专家 B | `APPROVE WITH CONDITIONS`，根代理确认本阶段实施前已获得；真实 Renderer 覆盖定位、键盘、迟到范围与窄宽主题布局。 |
| 产品经理 | `APPROVE WITH CONDITIONS`：模块唯一入口，内容先行，比较与设置分层，保存/采用/来源权限不变。 |
| UI/UX 设计师 | `APPROVE WITH CONDITIONS`：非模态背景编辑与 light-dismiss 焦点正确、真实边界贴边/窄区安全、sticky 关闭及单层阅读滚动。 |

## 零兼容与验收草案

- PRE 方案不含旧字段 fallback、双轨、迁移、回填；一次性删除旧散落入口及中央弹窗呈现，不同时保留两套体验。
- 实施前完成本轮五角色 PRE；界面稿、测试和文档同步，底层来源与权限不变。
- 实际截图验证入口对齐、内容优先、关闭可达、亮暗与窄宽布局、不挡对话或正文关键编辑位置。
- Renderer 验证只读零新增写/模型、dirty 原值/DOM/选区/滚动、键盘/鼠标关闭、scope ABA 与迟到错误隔离、非模态继续编辑时实时禁设置；外点关闭后正文自然获焦、Tab 能到背景、panel 内 Escape/Close 精确还焦、背景 Escape 不抢焦；主区滚动/resize/sash 缩窄与亮暗布局均验证。
- 本轮证据分项记录在下文；前轮证据仅证明此前范围，不替代本轮位置体验验收。

## 最终源码 PM/UI/UX POST

结论：产品经理、UI/UX 均 `APPROVE`，无剩余 P0/P1。键盘、scope、布局及真实操作证据已齐，默认内容视觉条件已解除。

- `ModuleRecordsPopover` 替换中央 Dialog，原生 auto popover、非模态 dialog role，没有 aria-modal、focus trap、背景 inert 或灰色遮罩；保持原 project/view/chapter/volume/node/request owner 与读取/写入边界。
- `ModuleRecordsButton` 同一图标/标签及可访问性名称。Workspace 对规划向 main Header portal，章节在原工具条显示，两者互斥；Reader/Header 普通记录入口删除，实际来源不同/锁定等异常解释入口保留。
- 已保存内容直接可读，比较按需，写作设置位于后层；刷新为有可读名称的图标。dirty 提示仍明确只读保存内容，实时 blocked 禁止设置，原 CAS/回执核对不变。
- 440px 上限按主区边界计算；窄主区/窗口用同一组件安全 sheet，ResizeObserver 与 resize/scroll 更新位置。popover Header 与独立 body 阅读滚动分离，关闭始终留在头部；不重排或卸载原编辑器。
- close 与原生 light-dismiss 分开：明确关闭/panel 内 Escape 可还 exact connected opener，外点关闭或背景 Escape 保留目标焦点。源变化卸载不把焦点送回旧页面。

键盘边界已明确并验收：Tab 已到背景编辑后 Escape 属于原生关闭，保留已有背景编辑焦点；panel 内 Escape 和明确 Close 才归入口，符合非模态不抢编辑焦点的目标。最终 Renderer 分开覆盖两条路径，真实背景任务细节 Escape 也经根代理操作确认；不扩 API 或另建焦点状态机。

根代理报告额外范围风险已窄修：Workspace 的 requestedNode 变化通知 App 关闭旧节点记录；同入口再次点击为 toggle-close，root close 使用 requestId guard，旧关闭事件不能关掉新面板。该项属于本阶段精确 scope 与 incarnation 可靠性，最终 Renderer 回归通过。

## 根代理实机证据

- 根代理实际 CUA 确认全书大纲唯一入口在顶部标题右侧、导出左侧；打开面板宽 440px、贴主编辑列右边，不进入对话栏，没有灰遮罩。
- 根代理实际确认 saved preview 立即可读、Close 头部独立固定、body 有界滚动。
- 根代理点击右侧创作指令一次，面板 light-dismiss 后完全消失，AX 焦点正确在 composer。该项证明真实背景目标获焦，不是合成事件推断。
- 根代理实际打开“第一卷 停摆之钟”记录，label 精确为本卷，仅显示卷及下级单元，没有 book 全书主线。
- 根代理实际鼠标再次点击同一记录入口：面板关闭，焦点在原入口，入口 toggle 行为通过。
- 根代理实际从初始 Close 焦点按 ShiftTab 到背景任务细节，popover 仍打开；随后 Escape 关闭且焦点保留背景任务细节，符合背景焦点契约。该项不是面板内 Escape 的替代证据，两个场景分别验收。
- 本任务纯 UI，当前模型连接不可用，不宣称新模型或保存产物验证。记录者没有操作主窗口、执行 UI 测试或改源码/测试。
- 本轮 Renderer/native Electron 与亮暗窄宽默认视觉结果已补到下文。
- 架构/技术 A 已独立给本阶段最终源码 POST `APPROVE`，无剩余 P0/P1：requestedNode 变化关闭、入口 toggle、旧关闭 requestId 隔离及分场景 Escape 焦点契约已核对；该角色未新增测试执行。技术 B 与 PM/UI/UX 最终批准见文末。
- 根代理报告最终 `npm run build`、产品 `git diff --check` 均通过。首轮五项 UI 的旧断言失败及最终结果如实保留在下文。

## 最终行为与视觉验收

- 根代理报告首批 chapter、planning-workspace、project-entry、workspace-viewport 四项通过；planning-versions 失败是旧断言匹配已删除的 summary，按默认可见真实 payload 更新断言后，planning-versions 与 project-entry 定向重跑 2/2、零跳过。五个唯一 UI 用例行为均通过，不累加重复运行。
- project-entry 新增真实原生 Return 再点同入口关闭、零额外读取；局部 node-second 记录切回全书时关闭且不还旧 opener，零新增模型/写入。
- Panel 原生 pointer 外点、panel 内 Escape、Close、背景 Escape 和 caret 保留均通过。viewport 原 54 个页面组合及亮暗窗口、compact 几何和对比度通过。
- 根代理亲看亮暗图，theme、边界、紧凑尺寸、关闭可达且无横溢。但默认截图的 fixture 把所有 details 都折叠了，错误隐藏了生产中应默认展开的保存内容。真实本卷已证明 focused node 默认展开，问题属于截图准备，不是新增生产缺陷。
- fixture 已修正为仅折叠 Panel 直接子级 comparison/reference/settings；以 innerText 和 dd.checkVisibility 断言当前已保存 purpose 默认可见。最终 planning-versions 重跑 1/1、零跳过，4.24 秒，日志 `/tmp/novel-records-popover-default-final-ui.log`。该修正没有 src 变化，也不重复累加唯一用例数。
- 12 张最终截图位于 `/tmp/novel-records-popover-ui-20261006`。根代理亲审更新 dark330/light740：保存节点默认展开、body 滚动、Close 固定、亮暗字色和边界正确，无横溢。之前人工折叠错误的默认图弃用，不作为最终验收证据。
- 根代理最终 `just fmt` exit 0：首次沙箱 uv cache 权限不足，本地提权重跑通过，没有源码问题。格式化后未重跑测试，最终 src/tests/doc 范围 `git diff --check` 通过。

## 五角色最终 POST 与收口

| 角色 | 结论 | 本轮依据 |
| --- | --- | --- |
| 架构专家 | `APPROVE` | 原权限/来源/API 复用，非模态只改入口与呈现，精确内部节点和 request incarnation 关闭链已核对。 |
| 技术专家 A | `APPROVE` | 源码复核：CAS/unknown 只读恢复、dirty/busy 门禁、旧关闭隔离及不同 Escape 焦点语义无扩大写入权限。 |
| 技术专家 B | `APPROVE` | 本轮五个唯一 Renderer 用例通过，原生焦点/入口/卷范围/几何与默认可见内容断言、最终截图已复核。 |
| 产品经理 | `APPROVE` | 本记录者源码检查与本轮独立证据：唯一模块主入口、内容默认先行、设置按需、没有新增历史/自动保存/模型/采用能力。 |
| UI/UX 设计师 | `APPROVE` | 440px 贴编辑列、无遮罩、外点目标焦点、两种 Escape、Close、单层阅读及亮暗窄宽默认内容符合约定。 |

零兼容门禁 `PASS`：旧中央 Dialog 和普通散落入口一次性替换，没有旧行为双轨、fallback、迁移、schema/API 或模型权限变化。

综合决策：`APPROVE`，无剩余 P0/P1。源码、测试、实机、默认视觉、构建、格式化与范围差异检查全部收口。记录者只修改此文档，未运行模型、GUI 或测试，不以本轮 UI 证据宣称文学质量、完整历史或模型可靠性改善。
