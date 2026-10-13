# 对话阅读与输入体验 · 2026-10-06

## PRE
目标：修复检查器内容已折叠但外框仍占245px的空间错误，改善对话阅读和输入。纯Renderer，不修改任务、权限、预算、模型上下文、历史及候选授权。

实施：顶栏放创作对话与任务细节开关；检查器hidden保留DOM和表单，关闭时不占空间/不显示sash。输入按实际高度2–6行增长。统一14px/1.7消息排版、中性用户泡、助手无框和弱化重复标签。长用户原话及建议按实际溢出提供4行预览与原生details完整内容，稳定身份key，保留原文、提案目标/操作/状态。助手正文和已有成功产物说明门禁不变。

架构专家/技术A APPROVE WITH CONDITIONS（原绑定/门禁保留、同一textarea、全文/精确操作）；技术B APPROVE WITH CONDITIONS（hidden释放空间、焦点、窄屏/停止按钮可达、真实App回归）；产品经理/UIUX APPROVE WITH CONDITIONS（实际溢出才提示展开、决策可见、无新增控制墙）。条件纳入实现，无阻断。

零兼容门禁：无版本判断/迁移/回填/fallback/双轨；当前接口无变化；同步生产/fixture/快照；保留当前并发/安全/可访问性；无兼容例外。

验证计划：actualApp viewport追加对话几何/亮暗/窄宽/输入选区/Stop/展开；presentation阅读保护、delegated任务状态、历史只读与删章后对话回归；构建和最终格式化。

## POST
架构专家、技术A、技术B、产品经理及UI/UX均APPROVE，无P0/P1。源码审阅发现展开记忆恢复时可能缺少收起提示，已由原生toggle同步open状态修复；原始文本和操作身份未变。

验证：构建通过。六个Electron测试文件均通过：workspace-viewport、conversation-presentation、delegated-conversation、thread-history-view、chapter-deletion、action-proposal。viewport包含6组宽度/主题的54页面检查，以及每组actualApp对话几何、48–144px输入、选区/滚动、IME、Stop、完整原文、展开状态保留；模型请求0，作品写入0。已逐读新增快照。历史夹具同步当前目录summary入口；提案夹具补齐当前listThreads接口并保留错误输出，未改生产门禁。

实机：原用户项目保持第3章选中及原数据，HMR已加载新版。底部折叠检查器不再占245px，输入卡片收紧，提案原文4行预览且批准操作始终可达。本轮是Renderer验证，未发起新的真实模型请求，不宣称已达到其他产品的完整体验水平。

零兼容门禁复查PASS：无版本判断、fallback、迁移、回填或双轨；当前接口/schema未改；原读写和并发边界不变。没有兼容例外。剩余非阻断项：历史模型回复里的技术术语属于生成内容质量，本轮未重写用户历史。

## 第二轮 PRE：回复操作与执行状态
目标：修复被标题样式隐藏的回复复制入口，为当前、历史和保存回复统一底部操作和明确反馈；执行摘要增加真实状态标记。Architecture APPROVE，TechA/TechB/PM/UX APPROVE WITH CONDITIONS。条件：原始Markdown与首尾空白不变；身份、文本及单次请求隔离迟到反馈；生成说明唯一复制且保留折叠守卫；截断标明复制已显示内容；失败就地可见；仅running动画并遵守reduced-motion，无秒级播报。纯Renderer，无模型调用、Host/权限或上下文改变。零兼容门禁PASS，无例外。测试覆盖复制成功/失败/缺失/迟到、各回复路径、展开与阅读保护、状态矩阵与减少动画。

第二轮排版追加PRE：实机发现Markdown的grid gap与段落margin叠加，五角色均批准将回复直属Markdown容器切为block并只重置直属首尾margin；保留列表、引用、代码与表格语义及横向滚动，其他页面不变。

第二轮键盘追加PRE：测试发现native disabled在复制期间会丢失键盘焦点。五角色批准改aria-disabled+同步owner/ticket guard，防重复且不主动focus；finally只清当前ticket。专项继续强断言焦点保留，不放宽预期。

## 第二轮 POST
五角色源码复核通过，无P0/P1。实施：新增统一回复底部操作组件与折叠说明操作边界，复制原始Markdown（含空白），本轮/历史/保存回复共用；部分保存明确标记范围。原文身份与单次请求隔离成功/失败反馈，pending期间保留焦点且同步拒绝重复激活。执行摘要添加真实状态标记与减少动画支持。回复Markdown不再叠加grid gap。

验证：build通过；7个Electron测试文件均通过（reply-actions、workspace-viewport、conversation-presentation、delegated-conversation、thread-history-view、chapter-deletion、action-proposal）。专项覆盖剪贴板拒绝/缺失、身份/文字/ABA/卸载后的迟到结果、同tick重复点击、焦点、折叠唯一入口、截断/缺失/读取失败、五状态与真实reduced-motion；actualApp六布局/主题×九页面，当前回复进入历史后仍精确复制。全部为本地fixture、零provider请求及作品写入。新增及调整快照已审阅，330/740宽亮暗视觉截图已检查，实机原项目HMR确认底部按钮与紧凑排版加载。

与PRE唯一实现差异：状态标记使用aria-hidden配现有可访问状态文字，无额外aria-busy或秒级播报。零兼容门禁PASS，无新增API/迁移/fallback/双轨，原任务/候选授权/错误和阅读机制不变。本轮未调用真实模型或改写历史内容。

## 第三轮 PRE：输入与模型菜单
五角色批准纯Renderer ComposerModelPicker提取与键盘/焦点改进。条件：保原连接和disabled门禁，highlight不选择/验证；Escape选择后回触发器，Tab/外点/忙碌关闭不抢焦点；连接id保持焦点、名单变化不选失效对象；窄栏列表限高独立滚动；输入focus快捷键提示匹配发送/追加/阻塞真实能力，IME/提交逻辑不改。无Host/API/schema/模型调用改动，零兼容PASS。测试扩展真实App连接fixture与viewport输入回归。

## 第三轮 POST
五角色源码审阅APPROVE，无P0/P1。模型菜单独立组件保持原连接配置与验证回调，补齐方向键/Home/End、Escape返焦点、原生Tab退出、外点关闭和当前项语义；Shift+Tab回入口后可再次用方向键进入菜单。名单变化按稳定id处理焦点，禁用关闭不抢焦点，长列表在窄栏独立滚动。输入获得焦点时才显示与真实发送/追加/阻塞条件一致的快捷键说明。

验证：build通过；5个Electron测试文件共6项测试通过，零skip（model-connection两项、workspace-viewport、conversation-presentation、app-author-input、app-agent-availability各一项）。保留原验证/配置次数与恢复流程；原生键盘、连接删除/空列表/禁用、900×600窄窗口与300px对话栏独立滚动均通过，viewport保留54页面检查。快照已逐项审阅。测试驱动使用隔离的真实可见窗口验证CSS焦点与原生按键，并按实际初始窗口尺寸恢复；没有放宽界面断言。实机HMR验证当前模型标记、Escape返焦点与输入提示，未选择新模型或改写作品。

零兼容门禁PASS：无Host/API/schema/迁移/fallback/双轨变化；原IME、发送与任务权限保持。本轮全部为Renderer与本地fixture验证，没有新增真实provider调用。最终按仓库要求运行just fmt。
