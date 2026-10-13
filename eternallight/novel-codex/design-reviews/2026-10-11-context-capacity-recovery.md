# 长篇创作的上下文容量与恢复

## 目标与前审

把“200 万字作品的前 100 章”作为持续工作，而不是一次模型响应。正文、作者要求、修订与保存回执以 Native 为权威；模型决定组织与读取范围。Core 管理工作上下文与摘要，Engine 管理完整请求准入和真实用量，Host 管理原任务生命周期，Renderer 显示实际完成与阻塞。没有新增小说流程、提高额度、截断作者指令或重置任务。

原故障：已保存第一章后父会话请求被本地桥拒绝为 HTTP 413；旧应用重复注入约 13k 字符工作规则，Core 91814 的统计尚未达到 95846 压缩阈值，桥又将确定性拒绝作为断流重试。失败请求的精确编码计数未留存，不能把历史估算当成它的实测值。源码与运行包不一致，运行包报告为 unverified。

分类：两者兼有。规则生命周期、统计口径与错误恢复合同存在设计缺口；旧打包产物没有包含源码已有修正属于交付缺口。

## 目标合同与本次实施范围

1. 当前规则用 additionalContext 更新，不反复伪装成作者历史；原始作者要求和真实交互保留。当前源码已有实现。
2. Core 的提前压缩阈值只作预警。Engine 编码器检查连接容量，桥检查包含预留空间的路由容量；两者共享容量校验与诊断结构，不共享不同含义的上限。输出空间继续扣除。当前源码已有 60% 提前阈值，本次不再凭经验调整比例。
3. 本次补齐 `contextCapacity`：boundary（connection/route）、estimatedInputTokens、inputLimitTokens、excessTokens。只传固定枚举和非负整数，不记录请求正文、密钥或完整历史。计数是保守估算，不是供应商计费。HTTP 错误和 response observation 传相同结构；Host 只白名单投影到诊断。
4. 本地超限在派发和预算预留之前拒绝，400 / context_input_budget_exceeded / retryable=false。Core 已有错误码映射。重发必须缩减上下文；供应商错误文本不能冒充本地决定。
5. 理想恢复：Core 负责摘要；Host 仅调用公开能力并核对原 work/lease/budget、取消与保存状态后接续。当前 Core 普通回合超限会标记 context full 并结束回合；不能把 guardian 特有的回合内压缩分支宣称为一般自动恢复。本次不修改 Core，也不在 Host 新造模型循环；无人推动的首次硬超限恢复仍需独立实现和实际应用验收。
6. 作品长期记忆必须可追溯至原文和版本；摘要是可丢弃上下文，不是作品事实。100 章交付以实际保存正文及领域验收为准；本次不声称完成创作或文学质量验收。

适用 B01/B03/B05/B06/B08/B09/B10。公共边界为 Engine HTTP 与 observation，现有 Core 只消费 code/retryable，无需改协议映射。没有更改权限、采用/发布、来源版本、工具 schema、预算所有权或持久数据库；未增加兼容分支。新增容量模块归 Engine，因为只有编码及路由层拥有实际估算和对应准入限额。

| 状态/权威证据 | 行为 | 副作用/后续 | 验证 |
| --- | --- | --- | --- |
| Engine 估算等于或小于上限 | 允许继续准入 | 原预算计量 | E1/E2 |
| 连接或路由容量超限 | 终止原样重试，传结构化原因 | 不派发、不预留；缩减后重新准入 | E1/E2 |
| 下游供应商相同文本或损坏诊断 | 不伪装成本地容量决定 | 保留供应商/一般错误分类 | E1 |
| Core 提前压缩 | 保留同一线程、规则与工具合同 | 摘要请求计入原预算 | E3 |
| 普通回合首次硬超限 | 结构化失败 | 不声称已有回合内自动恢复 | 待独立闭环 |
| 暂停/取消/未知写入 | 原生命周期合同 | 不因容量变化绕过保护 | 本次不改 |

## 验证计划和五视角前审

同一执行者五视角自审。架构 APPROVE：Engine 拥有校验、Core 拥有摘要；技术 A APPROVE WITH CONDITIONS：E1 验证严格解析/不误分类；技术 B APPROVE WITH CONDITIONS：E2 真实 HTTP、E3 实际 Core；产品 APPROVE WITH CONDITIONS：明确构建与 100 章未验收；UI/UX APPROVE：仅诊断投影不添加虚假自动恢复状态。

| 证据 | 计划 | 实际结果 |
| --- | --- | --- |
| E1 | Rust 容量边界、错误分类与编码回归 | `cargo test --manifest-path eternallight/agent-engine/native/engine-core/Cargo.toml --lib`：45 通过 |
| E2 | Engine 实际 HTTP 连接/路由超限，数值诊断、零派发/零预留；Host 白名单 | `bridge.test.mjs` + `contracts.test.mjs`：53 通过；含 Responses、Chat Completions、Messages 六种拒绝组合及 Responses 原预算缩小后成功 |
| E3 | 实际 Core + Engine + MCP 压缩、规则保留及原预算 | `autonomous-context.integration.mjs` + `portable-discovery-compaction.integration.mjs`：4 通过；明确区分自动提前压缩与硬超限后测试显式调用压缩 |
| E4 | architecture:check，定向源码测试 | 无诊断（365 文件、7 crate）；`codex-runtime.test.mjs` + `model-failure-diagnostic.test.mjs`：39 通过；`git diff --check` 通过 |

## 后审

实际变更：新增 Engine 容量诊断模块，替换 Responses/portable canonical/路由层各自的裸字符串拒绝；HTTP 和 response observation 使用同一数字诊断，编码前建立请求关联；Host 严格白名单投影计数。既有规则拆分、60% 阈值与 Core 错误码映射没有重写，增加实际 Core 回归验证。

实际结果：E1–E4 共 141 项通过（不重复计数重跑），Engine release 原生模块已重建。首次沙箱执行无法绑定本机端口；在允许本地回环监听的隔离环境重跑通过。测试开发中修正了协议名称、压缩发生在下一回合的检查时机，以及从真实终态通知读取结构化错误，未放宽生产准入。硬超限测试使用约 89k 的保守输入估算超过连接 55808 上限，确认只拒绝一次、Core 终态为 contextWindowExceeded、服务商请求和账本不增；随后由测试显式压缩再发较小上下文，通过原线程和原预算断言。这不是应用无人推动自动恢复证据。

构建身份（本次工作区有预存未提交修改，不能仅用 HEAD 代表源码）：
- 实际 Core `codex-rs/target/debug/codex` SHA256：`894c544e75fa2859da09da9f86c4cab79ea87a40690183205b1bddacfd26cfb4`；本次未改/重建 Core。
- 新 Engine `native/bin/darwin-arm64/engine.node` SHA256：`840eb99a09a4d69175862fa030562e8850f59b9730b70d0cd604421b80455750`。
- `.app` 未重新打包、未替换运行中的旧产物；作者项目数据库没有修改。

五视角后审（同一执行者自审）：架构 APPROVE（E1/E4，职责不变）；技术 A APPROVE（E1/E2，边界严格、零派发、原账本）；技术 B APPROVE（E2/E3，真实 HTTP 和 Core 覆盖当前改动）；产品 APPROVE WITH CONDITIONS（完整长任务恢复和 100 章实际交付仍未验收）；UI/UX APPROVE（诊断只投影实际计数，没有添加自动恢复状态）。条件不阻止源码诊断改进，但阻止宣称长篇创作故障完全解决。

零兼容后审 PASS：没有双读、迁移、旧字段 fallback；真实作者资料不参与测试。B01–B10 受影响边界复核 PASS：Native 权限/保存不变，Engine 负责准入和计量，Core 负责压缩，Host 没有增加模型循环。原故障中的无诊断和原样重试在受控边界闭合；实际应用打包和原作品创作仍未闭合。

## 后续完整闭环的实施验收条件

- 普通回合硬超限应由 Core 提供保留当前动作的通用压缩恢复；按 B09 先同步完整上游，只有仍未解决才维护最小通用补丁。不得在 Host 复制模型循环。
- 一次模型步骤仅在压缩确有容量进展后重试；当前规则或必要输入本身放不下时，明确返回不可压缩原因，不能无限重试或静默删作者约束。
- Host 只在原 Work/线程/授权/累计预算下调度公开恢复操作；暂停取消、未知写入与未结算费用按原权威状态处理。UI 显示“整理上下文”必须有实际执行事件，失败必须可读取。
- 从已验证源码重建应用，核对包内 Core/Engine/catalog 和 Host 的身份；在原失败作品上验证恢复，并以实际保存的第 1–100 章及领域验收确认交付。模型的完成回复不算产物。

这些项尚未实施/验收，本次不把诊断与受控压缩测试记为全任务完成。

## 第二阶段：继续完成通用硬超限恢复（实施前）

用户要求继续。恢复属于 Core 的模型步骤循环，不在 Host 伪造新作者回合。B09 已于 2026-10-11 拉取官方 https://github.com/openai/codex main，目标 `cc7ba33601286a751477832b01de7b6f8de0dd9c`；先完整合入新增 16 提交并重建复验，再决定最小本地补丁。合入前 Codex 路径没有未提交改动，保留产品现有工作区改动。

方案：普通模型步骤遭遇 ContextWindowExceeded 时调用现有 Core 摘要能力，原步骤至多恢复一次；成功模型步骤才刷新恢复机会。压缩失败、取消、额度拒绝立即按原合同结束；摘要无效且仍超限时不得无限重试。TokenBudget 的历史重置模式保持拒绝边界，Guardian 原有保护不削弱。原线程、当前 turn 和工具保存回执不重建，所有模型请求继续走 Engine 原租约与账本。

前审五视角自审：架构 APPROVE（B09 完整同步优先）；技术 A/B APPROVE WITH CONDITIONS（必须验证真实超限成功恢复、无进展终止及取消）；产品/UI APPROVE WITH CONDITIONS（没有实际成功前不报告完成、不增加虚假恢复状态）。下一步需记录上游合入后原故障复验、Core 补丁登记、构建身份、Fork 门禁及受影响集成结果。

### 第二阶段实施后证据

- 官方目标 `cc7ba33601286a751477832b01de7b6f8de0dd9c` 完整合并为 `c51b76f853509e104b69ae415b343cda6fee20b5`，祖先检查通过。无冲突，未提交产品文件与上游变更无重叠，全部保留。
- 官方完整合入后的构建身份 `580c3bfdf29017d969a58162b33133c6c5a8cf33f7d8005954a30b9365b6778d`；实际 Engine/Core 硬超限用例失败于 compactions=0，再确认上游未解决原问题，随后实施通用补丁。
- `core/src/session/turn.rs` 将原 Guardian 专用的有界摘要恢复扩展至普通模型步骤；仍排除 TokenBudget 重置模式，仍仅对真正 Guardian 写其状态。每步一次，成功采样才重新获得恢复机会。Host 未增加作者输入、任务或模型循环。
- 新补丁 `bounded-hard-context-recovery-v1` 已登记路径、符号、Rust 回归、目标基线及精确差异摘要，`repo:check:strict` 通过。
- 修复后的 Core BuildIdentity：`16a9358ffa7c5b8cd275d915f126391c5dc4829885f54c12fadb61ac03833f30`；二进制 SHA256 `952af09d2eb0e027bbc99951db5ba9b5eea89aa4827c0341c4c560443bf2e961`。开发支持清单同步真实身份，实际 resolveCodexCli 选择当前 workspace-build 且 reason=null。
- 实际 Core/Engine 集成 6 项通过：上下文显式/提前自动/硬超限自动压缩、原回合恢复、压缩中取消、MCP 已发现 schema 及新 lease 再发现。旧 overflow 用例的“必须失败”断言按新生产合同更新为自动成功；仍保留实际超限、同回合、原预算与只有摘要/接续派发断言，没有弱化原失败触发。
- Core Rust 成功恢复和无进展终止 2 项实际执行通过；相邻 Guardian 容量保护 12 项通过。初次继承网络禁用标记被测试宏跳过，不计为通过；允许本机 HTTP 的运行移除该测试跳过标记后，默认栈发生溢出，使用测试进程 RUST_MIN_STACK=16777216 后通过（上游 CI 同样显式设置更大测试栈）。没有修改应用容量或保护限制。
- Host/运行时/终态生命周期 46 项通过；前端与 Host 类型构建通过；领域 Runtime/MCP 重建通过；`git diff --check` 通过。取消适配器按既有合同返回 interrupted，测试已按实际终态校验，未要求它必须抛异常。

五视角后审：架构 APPROVE（Core 原循环通用修正、B09 完整更新）；技术 A APPROVE（恢复有界、取消及 Guardian 拒绝边界）；技术 B APPROVE（真实公开协议和构建身份）；产品/UI APPROVE WITH CONDITIONS（受控恢复已闭合，打包与原作品付费模型验收仍未完成）。零兼容 PASS。第二阶段证据覆盖并取代上文“普通回合硬超限恢复尚未实施”的状态，不将受控测试冒称 100 章创作交付。

### 编译输出路径整理复核

根据用户提醒复核 `2026-10-10-build-output-layout.md`：目录整理已生效，`.release/resources` 是已组装资源，`.release/package` 是待验应用，`release` 是通过后唯一交付出口；Rust 三个独立 workspace 仍各用 target，native/bin 是模块加载目录。14 项目录与失败保留测试再次通过。

该次整理的 E3 只归一化旧应用，明确标记 unverified；E4 已记录同一媒体组件缺失，未生成新应用。本次 package:dir 再次在 staging 前置失败，`.release/package-report.json` 记录失败，原 release 保留。因此没有证据表明新媒体产物被搬到了其他输出目录。媒体来源变量是外部输入，不是被删除的 RELEASE_OUTPUT/RELEASE_RESOURCES 输出覆盖变量。

现有 `media-runtime-bundle.mjs` 只有接收、验证、签名和复制；MEDIA_DELIVERY.md 明确它不下载、不自动复制 Homebrew、不生成占位包。工作区和原应用没有找到合规的 media-runtime-manifest.json。本机 PATH 虽有 FFmpeg/FFprobe，却不证明存在携带字体、许可证、来源摘要和完整相对依赖的发布 bundle。真正缺口是便携媒体组件的生产/接入流程尚未闭合，不应通过回退旧目录或跳过发布校验解决。用户要求分析该路径整理，未在此容量补丁中扩写 FFmpeg 构建供应链。


后续交付补记（2026-10-11）：媒体生产链缺口已由 [便携媒体构建评审](2026-10-11-portable-media-build.md) 关闭。新 `release/mac-arm64/小说工坊.app` 已完成资源、Core 身份、真实媒体、前端构建及隔离应用启动验证，并通过 `verify:package`。先前缺少媒体输入的打包阻断已解除；100 章创作尚未完成，不以工程验收替代。
