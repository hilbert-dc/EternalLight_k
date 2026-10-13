# 小说工具契约评审与纠错反馈优化

日期：2026-10-10。范围：本工作区小说 MCP 工具声明、发现、参数拒绝与结果传输；对照本地 Codex 工具实现。此记录是实施者五视角自审，不冒称独立模型评审。

## 评审发现与借鉴范围

| 维度 | 当前实现与判断 | 本轮处理 |
| --- | --- | --- |
| 职责与调用上下文 | Codex `ToolInvocation` 将会话、调用身份、取消与模型参数分开；本工程已有受信 `_meta` 与 Native 能力绑定，但模型仍复制项目/任务/修订封套 | 保留现有身份与 CAS；未来简化封套须同步来源、摘要、恢复与公开合同，不能直接删除校验 |
| 按需发现 | Codex tool_search 返回可加载的真实工具；本工程已有 live admitted set 与 toolDiscovery，未发现需要重建发现机制的证据 | 保留；修正角色读取与目录查询的互斥规则未进入 schema 的缺口 |
| 声明与执行一致 | `discover_creative_capabilities` 的 query 与 roleId 同时给出能通过 schema，执行才返回非结构化错误 | P2：将已有互斥规则写入同一公开 schema，进入统一零执行纠错路径 |
| 参数可纠正性 | Codex apply_patch 返回具体解析/应用错误；本工程通用 schema 拒绝有位置，但 type/enum/边界不返回实际期望值 | P2：从执行所用 schema 返回有界 expected；不回显拒绝值，不代模型修参数 |
| 联合类型诊断 | 当前仅能按 enum 选择 anyOf 分支；对象/null 联合及 oneOf/const 会退化为笼统 constraint，隐藏真正缺失字段 | P2：只在判别字段或 JSON 类型唯一确定分支时下钻；歧义保持，不猜创作意图 |
| 写作操作粒度 | Codex apply_patch 的局部操作范式值得借鉴；当前正文保存要求完整正文或场景组，范围由 Native 保留校验 | 本轮不另建写入协议；局部 patch 需独立验证重复锚点、Unicode、保护、来源和幂等，不能把工具易用性当成授权 |
| 输出、状态与恢复 | 已区分业务结果、recoveryOperation、保存/采用/交付；已有分页与错误回执测试 | 沿真实 MCP 和 Core/Engine 链验证新增反馈不被回执覆盖，不改领域状态 |

上述问题来自源码审查；以新增回归保留改前失败，不把历史创作失败直接归因于本次发现。工具数量、schema 大小及回归数量不代表文学质量。

对照来源：[Codex 调用上下文](../../../codex-rs/core/src/tools/context.rs)、[工具发现声明](../../../codex-rs/core/src/tools/handlers/tool_search_spec.rs)、[工具发现校验](../../../codex-rs/core/src/tools/handlers/tool_search.rs)、[局部补丁与可纠正错误](../../../codex-rs/core/src/tools/handlers/apply_patch.rs)。借鉴的是声明/执行一致、错误帮助下一次行动、上下文与业务参数分离；不复制 Codex 的文件写入权限或模型专属 freeform 协议。

## 实施前责任与契约

- 责任：`native/story-mcp`，适用 B01/B03/B04/B05/B06/B10。MCP 拥有参数声明与派发前反馈；Native 仍是授权、作品、版本、费用与完成事实权威；Core/Engine 仅作为现有传输消费者。
- 公开链：真实 `tools/list` inputSchema → Core/Engine 转换 → `tools/call` → ArgumentValidation → Runtime；失败通过 MCP `content`/`structuredContent` 返回。不更改工具名称、领域数据、UI、预算、重试上限或正文。
- 新增 `expected` 仅包含失败 schema 关键字与原声明值，限制序列化字节；路径与分支选择均取自静态 schema。不返回作者原文、输入值或凭据，不生成代填参数。
- 联合类型始终先由完整 schema 判断是否合法，分支选择只改拒绝后的诊断。仅唯一类型/判别分支可下钻，多个匹配保持原联合错误。
- 互斥规则使用已有支持的 `not`/`required`，删除执行分支的重复规则，保留一套当前合同；角色 ID 和 cursor 的资源有效性仍由目录检查。
- 模型自主选择查询、阅读、修改与方法。没有新增文学流程、批准或自动恢复。
- 影响调用方：所有 MCP 工具的 schema 参数错误；能力发现 query/roleId 非法组合。成功参数与保存载荷保持。

| 输入事实 | 结果 | 副作用及下一步 |
| --- | --- | --- |
| query 与 roleId 同时提供 | schema 拒绝，返回具体互斥条件 | 零 Runtime 派发；模型选择一种原有模式 |
| 类型/枚举/数值边界错误 | 精确路径及声明期望 | 零派发；在原任务内纠正，不改授权 |
| 唯一对象分支缺字段 | 指向该分支的缺字段 | 不替模型选择值或新增来源 |
| 联合分支歧义 | 保留整体拒绝 | 不猜测业务分支 |
| 身份错误、纠错耗尽、领域拒绝 | 保留原拒绝与生命周期 | 不放宽权限、不清零费用、不恢复旧执行 |
| 修正后的合法调用 | 原合同执行 | 真实回执决定是否保存/完成 |

## 五视角前审

| 视角 | 结论及条件 |
| --- | --- |
| 架构 | APPROVE：改动留在 MCP 参数边界，无 Core/Engine/Native 权威迁移 |
| 技术 A | APPROVE WITH CONDITIONS：保留完整 schema 门禁、诊断限长脱敏、歧义零猜测与零派发证据 |
| 技术 B | APPROVE WITH CONDITIONS：新增回归先失败，再验证真实目录/转换/调用/输出，保留现有相邻拒绝测试 |
| 产品 | APPROVE：减少可纠正参数的反复试错，不增加作者操作或承诺文学效果 |
| UI/UX | APPROVE：不改作者界面，内部 schema 纠错仍由 agent 处理 |

零兼容门禁：无历史版本分支、迁移、fallback、双读双写或删除作者资料；同步当前 schema、实现、测试与此记录。其他任务正在修改的媒体文件保持。

## 验证计划与结果

计划：新增真实工具 schema/MCP 回归先复现失败；完整 story-mcp 测试；构建 MCP；真实 Core+Engine+MCP 的受控纠错测试及真实目录 codec 验证；`architecture:check`；范围与差异检查。无外部模型调用、真实作品修改或发布。

实施中证据：

- 新增 5 组回归在改前为 0/5，分别复现缺少 expected、能力发现 schema 接受非法组合、对象/null 联合隐藏缺字段、oneOf/const 定位笼统及超长期望无省略标记。实现后首次定向验证为 5/5。
- 后审补充了无 type / 仅引用的联合分支反例：这类分支可能接受同种 JSON 类型，不能因缺少显式 type 就被忽略并误称唯一匹配；已加保守诊断和回归。
- 首次完整 MCP 测试 65/104，39 个 HTTP 用例因沙箱禁止本地监听返回 `Operation not permitted`；这不是产品行为通过或失败证据。原失败保留在本次工具输出中，随后在允许 loopback 的隔离环境重跑。
- 首次 Node 验证发现新增 fixture 错误地要求 modelKnowledge 工具集开放协调者能力发现。已改为 advisory 独立用例，并明确断言 modelKnowledge 不暴露该工具，未放宽生产权限。同轮 Node 26.7.0 的测试子进程出现 `InternalCallbackScope::Close` 原生断言；后续使用本机已有 Node 22 运行测试，不更改系统默认或产品依赖。

- 集成回归初次扩展时沿用了原查询测试的 Bridge 配置，未注册生产 `executorValidatedTools`，严格参数被 Engine 提前拒绝。已改为导入 Host 同一生产策略，校验仍由真实 MCP 执行；没有放宽生产 schema 或在测试手写工具白名单。随后按实际 Core `Wall time/Output` 包装解析 JSON，未改变模型输出。这两次 fixture 失败均保留在本次工具输出。

最终验证：

| 检查 | 结果及证明范围 |
| --- | --- |
| `cargo test --manifest-path native/Cargo.toml -p novel-codex-story-mcp` | 104/104，通过真实 MCP 调用及隔离 loopback Runtime 接收器；涵盖新增诊断、既有身份/范围/耗尽/回执拒绝 |
| `cargo build --manifest-path native/Cargo.toml -p novel-codex-story-mcp --bin novel-workspace-mcp` | 当前生产 MCP 编译通过 |
| Node 22：`knowledge-schema-integration.test.mjs`、`query-correction-engine.integration.test.mjs` | 5/5，无跳过；真实目录及 Responses codec 2 项，真实 Core/Engine/MCP 纠错 3 项 |
| `read_work_issues` 新增传输场景 | 第一调用 limit=0，Runtime 零接收；下一模型请求收到 `/limit` 和 `expected.minimum=1`；同任务修正后仅一次合法读取；三次模型请求、15 个 fixture tokens、0 unknown，原线程与回合保持 |
| `npm run architecture:check` | 359 文件、7 crates、0 diagnostics |
| 格式与差异 | 两个改动 Rust 文件 rustfmt 检查、Node 语法检查、git diff --check 通过 |

这里的 provider 与 HTTP 业务接收器为受控 fixture。没有启动真实作者应用、使用外部模型或修改真实作品；不能据此声称文学质量、token 节省比例、历史局部改稿问题或完整发布已验收。

源码与构建身份：本轮 6 个源码/测试文件逐项摘要存于 `/tmp/novel-tool-contract-review-2026-10-10.json`；集合 SHA-256 `23a3f9d7936b8798dc17d4903990ec6e7f6b117f33bc352d26a7c416419435bc`，MCP 二进制 SHA-256 `6d5b70d1eb2ec18d3ca64c58e69ad0928190f8865c981e1e8377ffd38531316f`。工作区其他任务的媒体修改未纳入本轮成果或身份。

## 五视角后审

| 视角 | 结论与依据 |
| --- | --- |
| 架构 | PASS：仅 MCP 声明/诊断与对应测试，无内核、领域数据或权限迁移 |
| 技术 A | PASS：完整 schema 始终先校验；明确分支才下钻；无 type 分支保持歧义；有界期望不回显拒绝内容；原身份/耗尽拒绝仍通过 |
| 技术 B | PASS：改前失败与改后通过具备对应回归，目录、codec、实际 MCP 和下一轮模型输入均验证；fixture 已使用生产策略 |
| 产品 | PASS（本轮工具契约范围）：模型获得准确纠正依据，无新增作者步骤；真实模型收益待另行评估 |
| UI/UX | PASS：无作者界面变更，未将内部参数纠错转交作者 |

零兼容门禁及受影响 B01–B10：PASS。没有旧字段 fallback、双轨、迁移、作者资料删除或校验放宽；未提交、推送或发布。

后续独立优化候选：精简工具描述中的重复流程说明、将可靠调用身份从模型业务参数中进一步分离、提供具备原文锚点和版本保护的小说局部 patch。这些涉及更广公开合同，尚未实现，本次未将其算作已完成。
