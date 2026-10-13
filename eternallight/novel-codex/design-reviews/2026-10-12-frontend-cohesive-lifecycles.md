# 前端应用生命周期与按需依赖

## 责任与合同

Renderer只管理会话目录、工作区导航与编辑生命周期，Native仍是版本、权限与保存事实权威。公开IPC/HTTP不改。将App的会话目录读写与错误/归档模式合并reducer；以语义方法接收已注册线程及当前项目事件，列表读回使用owner代次隔离。章节编辑中的冲突恢复、规划反馈等按独立操作责任抽离，保留脏草稿、CAS与已提交后读回状态。主组件保留跨区域协调，不以搬文件或行数代替解耦。

首屏大包实际依赖检查发现author-input-budget静态导入双token编码表，而Renderer同步作者校验只需字符/UTF-8字节。与合同任务协调将纯作者校验和需要编码表的独立token统计分离；不降低校验、不异步放行未校验输入。其他页面按需加载须保留加载状态及当前编辑挂载，不能因Suspense丢输入。

## 状态判定

| 事实 | 行为 | 保护 |
| --- | --- | --- |
| 当前项目目录读回 | 原子替换目录、结束loading | 最新请求owner匹配 |
| 切项目/作者/API后迟到列表或写回 | 不更新新区域 | owner代次，含A→B→A |
| 会话仍running/waiting/submitting | 拒绝归档，说明原因 | 运行权威不移入目录 |
| 编辑冲突 | 保留本地/远端选择、真实保存回执 | 当前editor owner、dirty及写锁 |
| 首次打开按需页面 | 只呈现加载状态 | 既有未保存导航确认仍在外层 |

## 五视角前审（单执行者自审）

- 架构 APPROVE WITH CONDITIONS：B01/B03，目录投影与UI编辑分域，公开合同不复制，删除原内联实现。
- 技术A APPROVE WITH CONDITIONS：B06，异步读写、重复操作及ABA必须验证；真实预算不变。
- 技术B APPROVE WITH CONDITIONS：定向真实App/Creative测试、tsc、architecture、build；不能仅测纯函数或统计行数。
- 产品 APPROVE WITH CONDITIONS：当前创作、回执恢复、来源与运行归属保持，无新增模型动作或确认步骤。
- UI/UX APPROVE WITH CONDITIONS：加载期间保持作者可理解状态，当前编辑/输入不卸载，导航与可访问性保持。

验证计划：多对话/title/history、恢复/发送、章节冲突与发布/规划受影响路径；结构门禁、生产构建记录主chunk与分包实际体积。完整UI由主任务统一执行。本轮无历史兼容例外，不移除安全校验或扩大权限。

## 章节/规划编辑生命周期补充前审（主代理）

`useWorkspaceEditor` 统一编辑基线、所属身份、本地未保存正文、远端三方合并、历史基线读取与保存回执。既有 `useCreativeSave` 改为该模型内部适配；View 消费编辑、保存、重新读取、冲突解决命令和只读状态，不持有基线/存储/保存恢复 refs。评审选择由既有评审状态拥有，通过明确读取/清理回调连接；发布决定仍单独授权，不因保存而采用。与App执行hook由不同代理分文件实施。

五视角前审：架构 APPROVE WITH CONDITIONS（消除View对内部refs的依赖，不引入第二份正式事实）；技术A/B APPROVE WITH CONDITIONS（保留owner/ABA、dirty、保护场景、原保存身份及未知读回；真实章节编辑/合并/创建/发布/改稿UI回归）；产品/UI APPROVE WITH CONDITIONS（当前界面、作者输入与恢复操作保持；不新增确认，不把未保存文本升级正式内容）。实现后补实际测试，不能以行数或将整组件搬到单hook代替边界完成。

### 编辑器实施与独立审查（2026-10-13）

- `useWorkspaceEditor` 持有基线、历史、恢复记录、脏状态和保存收据；`useCreativeSave` 作为内部保存适配，View 只调用修改标题/场景/保护状态、保存、重新读取与解决冲突等命令。公开属性移到 `creative-workspace-props.ts`，模型不反向依赖视图实现。
- 独立 Host 代理指出并复核两处跨会话缺口：旧分页响应可能污染新作品；旧冲突处理的写锁可能在切换后残留。分页成功/失败均检查当前 owner；活动采用独立 operation 身份，切换生命周期清旧锁，旧 finally 只能释放自己的 operation，不能清除新写入。
- 新增真实 React/Electron `editor-lifecycle-ui`，验证 A 项目迟到分页不污染 B、旧活动不能释放 B 写锁，以及同 API A→B→A 不复用旧 owner；1/1 通过并纳入完整 `test:ui`。
- 首轮原有章节/发布/改稿/规划回归 16 项中 15 项通过；规划重试发现 App 保存回调过早读取，已由会话模块修复并定向通过。并行编辑期间第二轮遇到 ConversationPanel 中间态导入缺失，不能把该次执行写为成功。最终完整 UI 由聚合 verify 验收。
- 原始响应允许缺失的规划 payload 时，展示不解引用 null；保存近章规划须有真实大纲 payload，不凭空补造规划内容。缺失版本号由展示模块明确表达，不伪造版本 0。

架构、产品、UI/UX后审 APPROVE；技术A认可 owner/operation 隔离及独立负例，技术B待最终聚合验证结果关闭。未将后端未知写入降格为可自动重发。

## 执行边界与布局补充（2026-10-13）

执行状态由 conversation/useExecutionSession 私有持有，useConversationExecution 暴露当前运行、重试、问题、scope 与 handoff 的读取投影，以及切换/离开会话、记录提示、配置回执、候选执行完成、恢复原任务等语义命令。App 不再访问 execution 的可写 ref 或状态 setter。协议订阅、发送/排队、恢复和生成通过 conversation 内部适配器取得同一实例的私有状态；WeakMap 按 controller 实例隔离，不使用“当前执行”全局单例。公开 scope 只用于跨区域迟到回执判定，不能写回内部 scope。

ConversationPanel 下沉搜索容器、阅读区域引用、inspector 开关与标签状态；领域内容通过状态、阅读、输入与检查区域组装。会话切换没有给整个面板加 key，因此 composer 与只读历史覆盖层保持原挂载关系。目录、媒体导航、规划草稿各自有 reducer/owner，未声称所有 hook 提取都能减少渲染。

最终审查发现并修复两处提取时的语义偏差：规划保存命令必须在发送时读取子编辑器最近注册的 handler，不能取上次父渲染快照；恢复原任务必须先核对 recovered owner，再建立 execution scope。前者由真实规划保存失败重试测试验证，后者新增无效来源不得改变 scope 的负例。未放松原有来源、dirty、CAS、输入接受与停止归属判断。

同源生成揭示的空值消费同步收口：章节来源缺失显示“章节来源尚未提供”，缺失版本不显示 vnull 或编造版本号，规划预览缺 payload 显示内容未提供；规划/目录只读取实际存在字段。Creative editor 生命周期由主任务独立实施及记录，本记录不冒领其验证。

## 五视角后审（单执行者自审）

- 架构 APPROVE：App 的执行可写状态收至私有生命周期；订阅、提交、恢复适配、目录、规划草稿、媒体导航、面板有明确职责。公开 HTTP/IPC 与预算合同未复制演进。
- 技术A APPROVE：保留 owner/epoch、ABA、后台执行恢复、停止回执及作者输入接受语义；新增 controller 跨会话恢复、切项目失效、无效恢复不重置 scope、懒加载校验 ABA 与版次缺失显示检查。
- 技术B APPROVE：最终 tsc、production build、architecture 通过；真实受影响 UI 11 个独立测试全部通过，恢复顺序补丁后相关 2 个再次通过。完整 UI 由主任务统一执行，不能将这里的定向结果表述成完整套件。
- 产品 APPROVE：不新增模型调用或自动采用；未知写入/保存失败保留作者内容，原任务恢复与候选审阅不变。生成合同中的缺失值没有被伪造为有效版本/来源。
- UI/UX APPROVE：只读历史保留当前 composer；搜索/检查状态留在对话区域；首次需引用预算时明确等待校验，未校验引用不能发送。

## 最终验证证据

- `/tmp/frontend-panel-ui2.txt`：7/7，通过停止竞态、历史恢复、失败领域任务恢复、章节统一 composer、会话呈现、规划先保存再发送、只读历史。
- `/tmp/frontend-facade-final-ui.txt`：4/4，通过跨页任务控制及迟到回执、完整动作名称搜索、生命周期边界负例、多会话草稿/来源/运行隔离。
- `/tmp/frontend-resume-order-ui.txt`：最后恢复来源顺序变更后 2/2 复测通过（与上述 11 个有重叠，不累加为 13）。
- `/tmp/frontend-final-types.txt`：`npx tsc --noEmit` 无诊断。
- `/tmp/frontend-final-architecture.txt`：1,404 文件、7 crates、0 诊断。
- `/tmp/frontend-final-build.txt`：生产构建通过。主 JS 1,185.29 kB / gzip 362.29 kB；原首屏约 4,699.72 kB / gzip 2,041.73 kB。编码表随 discussion-excerpts 按需加载（3,421.17 kB / gzip 1,650.83 kB），媒体 99.40 kB、设置 4.49 kB、诊断 10.56 kB 分包。

体积结果说明首次不需引用校验/媒体页面时的主包减少，不代表总安装包同比减少或已测量交互延迟。Vite 对大于 500 kB 的 chunk 仍有提示；完整编码表是严格 token 预算校验的真实依赖，本轮未为压数字降低校验。顶层仍负责跨区域导航与组合，行数不是验收标准；本次 App 为约 1,200 行，没有另造一个等规模的单体 hook。

作者隔离补充：唯一生产入口 App 使用 `key={author.email}` 挂载 CreativeWorkspaceView，切换作者会卸载整个编辑生命周期；恢复记录另以 authorKey 命名空间隔离。本 hook 的项目/API owner 不能替代该作者挂载边界，新增调用方须保持这个约束。独立审查已核对生产路径。

## 设置页就绪等待补充前审（2026-10-13）

完整 task-controls 回归发现 workspace-context 夹具点击设置后仅等待固定 40ms，未等按需加载的设置页挂载便查找退出按钮。产品由局部 Suspense 展示加载状态，此时缺按钮符合真实边界。测试改为监听 DOM，直到实际退出按钮出现再操作；不预加载组件、不增加固定休眠、不修改测试进程超时，保留退出/卸载释放上下文与媒体脏稿作用域全部断言。

五视角自审：架构 APPROVE（现行异步 UI 边界）；技术A APPROVE（基于真实 DOM 就绪，不猜加载耗时）；技术B APPROVE（单文件真实 Electron 定向复测）；产品 APPROVE（退出语义不改）；UI/UX APPROVE（保留按需加载状态）。

该补充后审五视角均 APPROVE：只改夹具真实 DOM 就绪等待，产品未改。`node --test electron/tests/workspace-context-ui.test.mjs` 1/1 通过，日志 `/tmp/workspace-context-lazy-ready.log`；退出登录、卸载、加载中项目不得选中以及媒体未保存编辑作用域断言全部执行通过。目标 diff whitespace 检查通过。

同类 `app-author-input-ui` 夹具也在首次打开 lazy 设置页后固定一 tick 点击尚未存在的退出按钮。按上述五视角边界，仅改为原有 `until(() => button("退出登录"))`，不改变等待预算或产品；作者跨项目草稿隔离、退出后当前草稿清理、再次登录保留其他作品草稿与创建幂等断言全部保留。单独 1/1 通过 `/tmp/app-author-input-lazy-ready.log`；最终与两个真实 Runtime 旅程合并 3/3 通过 `/tmp/condition-rebind-author-final.log`。后审五视角 APPROVE。

## 最终聚合复测（2026-10-13）

完整 `test:ui` 117/117 通过，日志 `/tmp/optimization-final-stage-test-ui.log`；任务控制138/138、真实桌面5/5及构建通过。编辑生命周期技术B的最终集成条件关闭，五视角后审全部APPROVE。精确编码预算保持不变；上述结果不宣称全树渲染隔离或未经测量的交互帧率收益。
