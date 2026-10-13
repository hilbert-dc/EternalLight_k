# 当前任务来源定位

## 前审

证据：第31章写手用了4页 index 才定位历史章节；前置决策曾把旧任务目录 sourceId 传入 workspace，得到 task_source_not_in_catalog 后才纠正。正文阅读14章且无重复区间，不能据此认定正文浪费。问题分类：接口设计缺口，目录身份是已知任务事实却要求模型重复传递，精确 revision 不能直接筛选。

方案：read_source_content 的 workspace 视图允许省略 sourceId，由 Native 在已认证任务下登记/绑定当前目录；显式旧/外来 sourceId 仍拒绝，绝不重绑定任意正文引用。增加 workspace-only revisionId 精确筛选，仅与现有可见目录取交集，不按任意数据库 revision 读取。章节/objectKind/query 筛选仍有效，游标绑定全部筛选。正文仍需返回的不可变 sourceId，目录不会计入已读。read_context 的 workspace.read 返回无 sourceId 的可执行导航。工具 schema、MCP、HTTP、Native和测试一次同步；不增加提示词行为约束，不改Core/Engine、不改作者标准/预算、无需迁移。

责任：B01/B03/B04/B06/B10。Native拥有任务权限/修订/来源事实；Runtime认证与参数传递；MCP呈现短指南和可执行请求；模型决定读哪章。旧编号拒绝保留原结构化恢复，不复制作品事实。

| 状态 | 结果 |
| --- | --- |
| workspace无sourceId，有效当前任务 | Native绑定当前目录，按过滤条件只列授权项 |
| workspace显式外来/旧sourceId | 拒绝，不复用旧cursor |
| 非workspace缺sourceId/附定位filter | schema/服务端拒绝 |
| 不可见revision/未来章/外项目 | 目录不返回，不产生阅读证据 |
| cursor更换筛选条件 | 拒绝stale cursor |
| 只定位没有读原文 | 不满足正文阅读/核验条件 |

五视角前审，同一执行者自审：架构APPROVE（Native绑定）；技术A/B APPROVE WITH CONDITIONS（权限、过滤游标、HTTP/MCP真实链路回归）；产品APPROVE（降低导航成本，必要正文不削减）；UI/UX APPROVE（无需用户处理内部编号）。零兼容门禁：无fallback/迁移/双写；省略sourceId是当前工具合同，不接受旧正文身份。

验证计划：Native现有workspace隔离/范围/历史回执测试，新精确revision与无目录ID测试；MCP schema与请求路由及导航测试；HTTP当前任务自动绑定、外任务引用拒绝；架构检查；重建Native并在安全维护边界打包部署。真实节省留待原任务后续完整样本，不另启测量生成。

回归发现并同步修正的旧fixture：原有task_workspace两项测试假设parsed正文页上限6500字节、650次短句必分页，但现行pack_parsed_page已使用63KiB（此前token优化）。保留目录6500上限不动，正文断言改引用唯一生产常量；作者要求fixture改为超过两页预算，仍验证首段不够、子阅读不能充当根阅读、只有完整根读取才通过。不是放宽权限或修改分页生产行为。首次结果12通过/2失败原因记入本记录。

## 实施验证与后审

Native14项全部通过（新自动绑定/精确revision，以及原权限、POV、跨任务/Work、保存版本、取消、根/子阅读边界）；MCP4项workspace与1项导航测试通过；HTTP task_sources_http_tests 5项全部通过，包括无sourceId查询、revision精确筛选、不合法来源/模式拒绝、未读证据、Unicode和外来游标。架构369文件/7 crate无诊断，cargo check及Native双二进制build通过。Host schema测试因自身环境条件skipped，不计通过。

本机测试启动曾停在dyld初始位置，原进程采样留于/tmp/source-test-sample.txt；测试未执行时未记通过。停止两次挂起的验证进程后在允许本地执行环境重跑，系统签名验证完成后执行成功；未改变系统安全设置或正在运行的创作应用。真实模型未用于回归测试。

证据日志 `.release/source-locator-validation/`。架构/技术A/产品五视角后审APPROVE；技术B与UI/UX APPROVE WITH CONDITIONS：发布门禁与原任务恢复待完成。生产数据、正文及累计账本不变，不报告整体token节约比例。

补充传输回归：MCP task_source_tools_tests 18项、tool_contract_review 6项通过，涵盖实际请求参数绑定、schema、结构化纠错与回执。与Native14、navigation1、HTTP5合计44项相关测试通过（先前workspace子集不重复计数）。

维护边界：等待第34章完整正文保存、writer任务completed后操作；随后UI确认第34章当前稿检查通过。点击停止按钮未立即观察到持久停止，未据此宣称已暂停；使用正常退出应用完成维护关闭，并通过进程检查确认退出，未直接改写数据库。待新包门禁后继续原运行。

## 部署与真实使用

2026-10-12T05:19:44.667Z 发布报告passed（release/package-report.json），启动新包。沿原对话提交维护后的原100章续接，不创建新范围或重置费用。系统确认新进程pid5167持有NoIdleSleepAssertion，屏幕休眠不受阻。

05:21UTC后实际模型调用已出现 `read_source_content({view:"workspace"})`、`{chapterNumber:35,view:"workspace"}`、`{chapterNumber:34,view:"workspace"}`，均不携带sourceId，未收到来源错误。这证明自动绑定/章节定位在真实模型链路可用，不证明完全消除所有导航或整章成本下降。第34章现稿及完整核验报告仍在，引用核对全部匹配；当时结章计数仍33，后续收尾接续中，不冒称完成34/100。

部署后审五视角均APPROVE：职责和事实权威未变化；44相关回归、发布门禁及真实定位调用闭合实施验证。继续创作的长程收益需完整可比样本。定时跟进已按用户要求删除，本次仅手动验证。

05:24UTC左右进一步确认：原运行version539、running、completedChapters=34、blockedReason=null；task-bb979cefc33846d088a3fa50ee744975正在后续决策。无需再次暂停/重发，恢复闭合。

仍存在的独立观察：维护后会话级内部触发 task-7dca7a3e694a5546148cbe025eac3709 曾对已经running的原运行调用request_generation_handoff，被TaskBindingInvalid拒绝，随后完成本轮；原运行之后依正式执行流程收尾第34章并启动下一决策。这是非来源定位的冗余交接，不能据本次来源接口改进声称所有协调浪费已消失。本次没有为其另起模型调用或删除历史失败。
