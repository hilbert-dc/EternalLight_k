# 工具契约四项优化实施评审

2026-10-10，实施者五视角自审。承接 tool-contract-review，范围为小说 MCP、Runtime、Story Core、Host 恢复消费者、当前技能与测试；不修改通用 Codex Core，不创建新的 agent loop。

## 实施前边界与公开合同

1. `save_draft_checkpoint` 增加与 payload/delegatedCandidateId 互斥的 patch。来源是任务冻结正文与原 CAS，或本任务已保存且与当前请求 CAS 相同的检查点；编辑为 scene group 内 Unicode scalar [start,end) + oldText/newText。Native 验证来源、原文、重叠和保护，确定性展开后复用候选事务、CAS、幂等及恢复账本；不是模型补全文本或另一个保存系统。
2. MCP 模型参数移除固定 projectId/taskId/threadId。Runtime 按进程持有的 capability 解析唯一有效任务，MCP 缓存不可变绑定并验证受信调用元数据；每个业务请求仍验证当前凭证/生命周期。保留 expectedRevision 与候选 CAS。模型不能覆写身份。摘要对业务参数计算，恢复身份与账本/Host binding 对齐，所有消费者同步。
3. tools/list 提供精炼用途与必要参数约束。完整操作说明通过 read_tool_guide 按需读取，来源仍是同一工具声明；技能保存跨工具的长期规则。指南不授予未准入工具，不替模型选创作步骤。
4. 原文入口统一 read_source_content，view=content/raw/index/workspace；移除 read_task_source 公开名。沿现有不可变来源和游标约束，返回可直接调用的 nextRead/read 请求。目录不充当阅读证明，原文/解析分页继续由 Native 记录真实阅读覆盖。

事实权威：Story Core 的 SQLite；Runtime 持有权限和领域协调；MCP 只声明、传输与展示；Host 只绑定生命周期与恢复；Renderer 仅投影当前工具回执、资料视图与精确重试身份。适用 B01–B10。同步接口/schema/提示/注册表/测试，不保留旧入口或旧模型参数 fallback。当前工作区其他人的媒体/编辑器变更不得覆盖。

| 状态/输入 | 行为 |
| --- | --- |
| 模型提供固定身份 | 零业务派发，按当前 schema 拒绝 |
| 凭证过期、歧义、非绑定线程 | 拒绝，不猜任务、不扩大范围 |
| patch 来源不符、原文不符、范围重叠、受保护内容 | 拒绝，不部分写入 |
| patch 合法 | 保存完整不可变候选，复用既有事务/回执/恢复 |
| 相同请求重试 | 原幂等回执；冲突仍拒绝 |
| 目录或页未读完 | 明示下一次调用，不推断全文已读 |
| 暂无指南/工具未准入 | 拒绝，不扩展准入集合 |

## 五视角前审

- 架构：有条件通过。领域 patch/身份权威留在 Native，MCP 不接数据库，Host 不解释正文。
- 技术 A：有条件通过。精确 Unicode、重复文本定位、保护与 CAS；恢复摘要必须与真实 Core 工具参数一致。
- 技术 B：有条件通过。新增安全反例、真实 MCP/Runtime 与 Core 传输回归；迁移全部当前消费者。
- 产品：通过。减少重复身份和全文回传，作者流程/授权不增加步骤；不以工具测试声称文学效果。
- UI/UX：通过。指南和分页为 agent 可行动反馈，不向作者暴露内部纠错操作。

## 验证计划

定向 Native patch/身份测试；MCP 全量；实际目录/codec/Core 纠错链；Host 恢复测试；架构门禁和 diff 检查。隔离 fixture 数据，不使用作者作品/外部模型。后审记录实际执行结果及未覆盖范围。


## 实施后复核

四项均已实现：`draft_patch.rs` 是领域展开器，Runtime 复用既有候选事务；`tool_binding_api.rs` 从能力凭证解析任务，`tool_binding.rs` 负责运输注入；`tool_guidance.rs` 从原始声明分页提供详细说明；`source_navigation.rs` 把 Native 游标转换成下一次可调用请求。Host 的恢复摘要与证据验证同步采用模型业务参数，Renderer 的重试身份包含视图与 workspace 筛选条件。当前技能、角色源、注册表、生成检查、测试及现行架构说明已同步。

- patch 完整保留元数据与未修改正文；精确 Unicode scalar 范围、oldText、非重叠、锁定、范围、CAS 在领域链上校验。已拒绝的原文锚点错误写入恢复账本；未知存储结果不冒充拒绝。
- 重试展开不读取已推进的当前任务目标。原请求使用不可变冻结来源，连续修改使用本任务自己的已保存检查点；二者均须与请求 CAS 对应。
- 模型覆写固定身份在业务派发前拒绝。凭证与根线程决定任务，子线程仍沿既有受信委派检查。缺失执行元数据返回错误，不导致 MCP panic。
- 已确认的完成回执在上下文压缩丢失版本时仍可回报终态，不重新执行已结束任务。
- 指南只覆盖当前准入工具，不授予能力。原始 HTTP/解析 HTTP 路由是内部表示实现，公开只有 read_source_content，没有旧公开名称 alias。
- 目录不算阅读证明；nextRead 保留来源、版本、视图、筛选和精确游标。界面不把另一个视图或筛选的成功误认作失败页恢复。

### 实际验证

使用 Node 22、隔离 `CARGO_TARGET_DIR=/tmp/novel-tool-contract-target` 和临时数据库。回环测试在允许监听本机端口的执行环境完成；首次沙箱端口拒绝未计入产品故障或通过结果。测试未调用外部模型或作者作品。

| 检查 | 实际结果 |
| --- | --- |
| Story Core library | 284/284 通过，含 Unicode patch 与保护反例 |
| MCP 完整测试 | 111/111 通过，含身份注入、伪造拒绝、指南分页、缺失元数据、完成回执及来源导航 |
| Runtime library | 15/15 通过 |
| Runtime context_scope | 14/14 通过，含真实 patch/CAS/重复提交与能力绑定 HTTP |
| Runtime recovery_ledger | 7/7 通过 |
| Runtime task_sources | 5/5 通过，含冻结来源、范围及异源游标拒绝 |
| Host、Adapter、Runtime 配置、恢复协议、角色、handoff、展示及输出策略 | 150/150 通过 |
| 真实 Core 查询纠错与进程崩溃账本恢复 | 3 个 Core 纠错 + 14 个恢复用例通过 |
| 真实 MCP/Runtime 委派候选与 patch | 通过，原文不符回执 rejected，合法 patch committed，原请求重放同一回执，锁定正文不变 |
| 真实 Core 长章纠错 | 2/2 通过：MCP schema 纠错及输出上限纠错；4500 汉字完整保存并读回，原任务/线程/预算不变 |
| 真实 Core 原始规则资料分页 | 通过，3 页完整拼接、冻结来源保留、候选保存不等于作者采用 |
| 受控 provider 的真实 Engine 委派读取 | 通过 |
| 真实 Core 限定范围修改、建模与后续 GenerationRun | 通过，完整流程约 80 秒 |
| 生产 Host 自主修复失败评估并交付 | 通过，真实 Core/Bridge/MCP/Native 与单一持久预算；脚本 provider，无第二次作者推动 |
| 构建 | Native bins、TypeScript、Vite 通过 |
| 架构 | 361 个文件、7 个 crate，0 diagnostics；门禁测试 16/16 |
| 生成资源、差异 | managed-agent-roles --check、git diff --check 通过 |

对同一完整 53 工具声明的可复现测量：说明文字 39419 → 6178 UTF-8 bytes（减少 84.3%）；完整声明 147954 → 104330 bytes（减少 29.5%）。这个结果证明输入体积下降，不等同于真实模型质量、token 用量或调用成功率的提升。

验收中同步修正了测试调用方：不再携带固定身份；伪 Runtime 实现绑定查询；不同任务使用独立凭证；跟随当前作品创建 DTO、Core 的 MCP 文本封装和恢复证明，并对有真实 usage 回执的三个请求全部计费。没有降低生产 schema、预算、权限或输出上限以通过测试。

### 扩展验收的限制

后续状态：下列记录保留为本轮历史。修复入口六项失败中包含真实的嵌套 target 引用比较缺陷，不能全部归为 fixture；该缺陷及生命周期 fixture 已修复，18/18 通过。知识纠错八项已在严格构建校验通过后实际执行，8/8 通过。新增诊断、正文锚点及效率验收详见 [后续落实记录](2026-10-10-tool-reliability-and-evaluation.md)。

本轮还运行了超出四项契约的流程测试，不能据此声称全仓库或产品端到端已全绿：

- revision-repair-handlers 的 6 个完整 Host 生命周期 fixture 失败，涉及修复 target、并发运行状态及任务控制身份；本轮该生产模块只替换资料读取提示，未修改这些生命周期逻辑。
- autonomous-creation-engine 最初在测试 harness 复制 ModelBridge 实例后丢失原型方法；fixture 改为给执行域保留实际实例后，完整自主修复和交付通过（约 30 秒）。生产 Host 未因该 fixture 问题改动。
- knowledge-correction 的 8 个受发布清单保护的用例在本机 Core SHA-256/平台身份校验处拒绝，未执行工具链。未放松支持清单或伪造构建身份。
- scoped-revision 最初因重复凭证和过小的测试 schema 接收上限失败；同步为独立能力凭证与生产相同的 40000-byte 上限后，限定范围修改、建模、评估及 GenerationRun 续作完整通过。

上述扩展检查不替代已完成的精确 patch、身份、分页、指南、恢复及真实 Core 保存验收。受控 provider 的生产 Host 自主闭环已通过；无真实 provider 文学效果评估，无桌面作者交互验收。

### 五视角后审

- 架构：APPROVE。B01–B10 边界保留：Native 是事实/权限唯一权威，MCP 只负责契约和运输，Host 管理绑定与恢复，Renderer 投影回执。未修改通用 Codex Core，也未新增 agent loop。
- 技术 A：APPROVE。原文锚定、Unicode、范围、锁定、CAS、幂等及恢复证明有真实 Native/MCP 证据；终态压缩和缺元数据反例已补足。
- 技术 B：APPROVE WITH CONDITIONS。四项合同验收通过；全应用自主流程仍受上述扩展验收限制，不能作为完整发布验收。
- 产品：APPROVE。减少重复输入与全文回传，作者授权和采用流程不增加步骤；模型仍自主选择工具和方法。
- UI/UX：APPROVE。局部保存显示真实章节与版本回执，指南保持内部操作；不同分页视图不互相掩盖失败。未新增页面布局。

零兼容门禁 PASS：无旧公开工具 alias、旧模型身份字段 fallback、双写或迁移。全部数据变更仅在隔离测试库；既有其他任务的媒体、编辑器和生命周期改动保留。四项实现无已知 P0/P1；扩展验收未通过项如上，不作全产品发布就绪声明。
