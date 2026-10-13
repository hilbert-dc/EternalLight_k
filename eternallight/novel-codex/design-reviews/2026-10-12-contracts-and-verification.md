# Runtime 合同与分级验证、文档归档

## 变更与责任边界（前审）

2026-10-12，执行者：同一实施代理五视角自审。适用 B01/B04/B09/B10。
Runtime 拥有 HTTP 请求和响应 DTO，Rust serde 为唯一线格式权威；ts-rs 从同一类型生成 Shared TypeScript 类型，Renderer 消费生成类型。动态 JSON 仍由领域校验，不冒充已生成的领域 schema。生成一致性与真实 HTTP 序列化分别验证；不会仅凭 preload 方法名宣称载荷一致。
构建脚本负责按受影响边界选择验证；不改变产品权限、预算、写入事务、恢复状态或 UI 行为。默认完整 verify 保留，PR 选择采用保守依赖闭包，未知生产路径回落完整验证。Nightly 完整套件不会替代权限和跨层合同 PR 门禁。归档仅移动已证实完成/被替代的记录，保留引用和证据，不按日期判断规范有效性。

## 验证计划

- E1：生成类型漂移检查、请求 optional/null/default 与响应/错误真实 HTTP 序列化合同测试、Renderer 类型检查。
- E2：路径选择器正负例，覆盖 core => runtime/authoring/mcp/Host、shared => 双端、未知路径 => full；验证完整套件去重后仍覆盖所有 native crate。
- E3：architecture:check；文档引用完整性；临时评估报告忽略，评审证据仍追踪。

## 五视角前审

| 视角 | 前审结论及依据 | 后审 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS：Rust 唯一合同，Shared 仅生成类型，E1 | APPROVE：E1 生成与消费边界、E3 架构检查通过 |
| 技术A | APPROVE WITH CONDITIONS：生成检查须阻止独立漂移；未知路径保守 full，E1/E2 | APPROVE：E1 最终315文件零漂移、E2全部通过 |
| 技术B | APPROVE WITH CONDITIONS：真实 HTTP 而非同源文本匹配，E1 | APPROVE：E1 真实 HTTP/请求语义回归通过 |
| 产品 | APPROVE：减少重复校验耗时，保留关键验证与历史证据，E2/E3 | APPROVE：E2/E3 保留完整回归和证据，未宣称测得加速比 |
| UI/UX | APPROVE：无新增交互，现有载荷一次性同步，E1 | APPROVE：视图按真实消费字段投影，未伪造时间戳；tsc 通过 |

零兼容前审：PASS。不新增旧合同 fallback、迁移、双轨；不删除作者资料。

## 实施证据

- E1（静态/生成）：Core/Authoring 公开 serde DTO 和 Runtime 库内具名 serde DTO 313 个根 DTO 全部纳入 `ts-rs` 导出；覆盖门禁防止新增类型遗漏。实际生成 315 个类型文件，并全量 TypeScript 类型检查通过。启动入口的进程凭据不是 HTTP DTO，不纳入该公开合同。JSON 动态值保留为 `JsonValue`；业务约束仍由领域层检查。门禁拒绝未评审的自定义 serde 序列化/反序列化转换；当前显式 nullable 读取器按原样保留。TypeScript AST 只将整数类型的 `BigIntKeyword` 映射到 JSON number，字符串字面量及字段名不受影响。
- E1（消费方）：`listProjects` 使用严格生成回执；作者界面 `ProjectSummary` 只投影实际显示的 id/title/logline，不把不同工作区投影缺失的时间戳伪造出来。`RevisionPredecessor`、`ExecutionScopeRequest`、`ChapterPlanRebindRequest` 删除手写副本并使用生成类型。`tsc --noEmit` 通过。统一 Native 回归已通过 Runtime lib 144/144（含 optional/default、显式 nullable 授权限额、enum）与 wire_contract_tests 3/3（真实项目/错误 HTTP、动态错误细节及 BudgetLedger 自定义读取往返/未知字段拒绝）。
- E2（受控脚本）：`npm run test:verification` 12/12 通过，覆盖 core/shared 依赖闭包、未知可执行路径 full、文档-only 不启动编译器、真实临时 Git 仓库从嵌套 cwd 发现 untracked 文件、类型字面量保持、Host 类型检查遗漏/抑制负例。所有 Git 枚举在仓库根执行；产品 CI 文件改动也触发完整检查。
- E2（CI 配置）：`test:story-core`/`test:runtime` 分别指定包，完整 `verify` 仅运行一次 `test:native --workspace`。现有产品 CI 接入 PR affected、main/manual/nightly 完整验证，保留独立架构正负例、Fork strict、发布、委派和评估门禁。Nightly 为 UTC 18:23（上海次日 02:23）。这是配置和本地选择器证据；尚未声称远端 GitHub Actions 已执行。
- E3（静态）：`architecture:check` 扫描 715 个生产文件、7 个 crate，零诊断。文档位置与相对链接门禁通过：20 份有完成/替代依据的历史评审归档；另 125 份专题设计及评审只调整组织位置，保留原生命周期状态；根部保留 9 个规范/日常指南入口。原已删除 Python 源码的 26 处死链接显式标为历史路径。旧源码清单、散列和证据内容不改写，映射可追踪。
- E3（产物）：`.evaluation-reports/` 与 `native/*/bindings/` 忽略；持久评审证据和 `shared/generated` 继续追踪。默认 cargo 导出不会覆盖版本化生成目录。

## 后审状态

架构、产品、UI/UX：APPROVE，职责和消费边界保持；技术 A/B：APPROVE。最终 `npm run check:runtime-contracts` 完整通过：313具名serde类型，315生成文件全量TypeScript检查及零漂移，真实HTTP合同3/3再次通过。零兼容复查 PASS；未加入旧格式分支、迁移或双轨，未删除作者数据。文档重组仅按证据归档，未知状态不冒称过期。

已检查 ts-rs 的已知 parser warnings：`deny_unknown_fields` 不改变输出字段；显式 Option 读取器只改变缺字段准入；`ProviderBudget` 的 `BudgetLedger` 转换复制相同字段并运行预算一致性校验。保留编译器告警，不全局屏蔽；生成门禁对现有 serde 属性和这几个变换明确列单，未知变换直接失败并要求合同映射及往返验证。

补充合同闭环：完整 Native 发现 executor-validated-tools.json 原52项与编译目录54项失配。原始 native 快照中 MCP main.rs 与当前完全相同，缺项来自既有基线。生成器调用已编译 MCP 的 tools/list 与现有静态资源发现分页协议，得到54项并排序写入；工具名不在脚本重复维护。生成检查接入完整和受影响验证，Native bundled_names_match_every_compiled_tool_once 仍对照完整 tool_specs 保证无遗漏。故障注入测试在正常启动校验后安装 trigger 并复用连接池，或在故障断言后移除临时 trigger，产品 schema 校验未放松。

消费边界补齐：除原先4类，另24个手写类型已改生成别名或明确投影。包括6类审计作业响应、TaskFinalReply、AuthorReadingProgress、修订决定请求/响应/枚举、NarrativeCommitment/证据/来源、创作意图/章节交付及DesiredOutcome等。ExecutionScopePreview.targets、ConditionEvaluationJob.result仍为Native动态JSON的UI解释，以Omit只覆盖动态字段；GenerationFeedbackDecision.choice为经领域约束的UI细分，Host steer命令为生成MessageTrigger的显式扩展。CreativeWorkspace、CharacterKnowledge、规划候选等Value拼装响应仍未转换成具名Rust DTO，不宣称这些已有完整生成合同。追加后 tsc --noEmit、verification12例及文档145项检查通过。

追加集成验证：生成类型消费后，获批沙箱外 Electron frontend-boundaries-ui 1/1通过，覆盖评审选择、生成确认及章节创建的异步范围与回执；沙箱内首次Electron SIGABRT由正常宿主启动复核排除。

计数来源核对：初次生成309/311，最终检查313/315。逐项比较两次export_bindings日志，增加的是媒体域 ScreenProduction、ScreenForm、ScreenExpression、ScreenDeliverable 四类；它们及其生成文件由共享工作区中的其他变更带入，本代理最终 --check 没有写入任何生成文件，已将该并发变化报告主代理。

故障注入与工具名单最终实测：recovery_ledger_http_tests 8/8、story_event_http_tests 4/4（含203源条件扫描与失败回滚）、bundled_names_match_every_compiled_tool_once 1/1 全部通过；对应日志位于本次本机验证输出，不把测试fixture的trigger当作受支持生产schema。

## UI 集成收口补充前审

只读历史既有 hook 在导航重构后失去 App 入口，导致运行中不能保持当前输入框查看旧记录。恢复到当前对话管理菜单，复用 `useThreadHistoryDetail` 和 `ConversationReader`，只调用 `readThread`；独立可关闭的阅读层不得切换对话、恢复任务、取消归档、提交消息或泄漏旧项目迟到结果。主代理确认属于保持既有能力。架构/技术A/技术B/产品/UI 五视角前审均 APPROVE WITH CONDITIONS：保留原 App 的权限、只读、项目及选择 ABA、原 composer DOM/草稿/选择区、运行事件及焦点回归断言后关闭条件。

其余 fixture 更新只同步现行合同：WorkStep.outputRefs 必填；附加精确来源不自动编造作者指令；重挂载恢复未保存正文，需作者显式恢复保存文本后才能发起改稿。

UI补充后审：三项实际Electron回归全部通过，最终tsc通过；五视角条件关闭为APPROVE。具体现行源码证据与保留断言见[前端收口记录](2026-10-12-frontend-boundaries.md)。
