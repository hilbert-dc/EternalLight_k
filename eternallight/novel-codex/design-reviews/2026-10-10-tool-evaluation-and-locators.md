# 工具对照评测、失败证据与精确定位

## 前审

责任：Story Core 从授权不可变来源定位原文；Runtime 验证查询形状和读者；MCP 暴露 locate 视图并统一绑定错误；Renderer 的分页重试身份包括定位条件。定位只返回精确匹配，不选择修改目标，不增加完整阅读证明或写权限。现有范围、锁定、CAS 和账本不变（B01/B03/B04/B07）。

评测由 scripts 驱动隔离作品。确定性工具回放增加逐案例失败/超时记录、原子增量报告和 CI 阈值；真实模型入口复用生产 Core、Engine、MCP、Runtime，不实现第二模型循环。分别记录真实 provider 用量、首次工具成功、纠错及保存正文保留率；未知用量保留 null。真实模型运行显式提供连接资格证据和额度，缺少条件保存阻断报告，不伪造结果（B02/B05/B06/B10）。

| 状态 | 行为 | 副作用/下一步 |
| --- | --- | --- |
| 原文唯一/多处/不存在 | 返回全部匹配的分页锚点或空页；标记仅匹配覆盖 | 不选择目标、不保存、不冒充读完 |
| 外部来源/异查询游标 | 拒绝 | 不扩大来源权限 |
| 可读但锁定/范围外 | 定位可读内容 | 不授权写入，保存仍拒绝 |
| 绑定内容/操作/root 不符 | MCP 结构化本地拒绝 | 无业务派发 |
| 单样本异常/超时 | 记录阶段及安全错误码，继续独立样本 | 不重试未知写入，清理进程 |
| 缺模型资格/凭据/预算 | 阻断真实模型运行并输出报告 | 不调用模型、不伪报零费用 |

五视角前审：架构 APPROVE；技术 A APPROVE WITH CONDITIONS（Unicode、多匹配、权限和游标反例）；技术 B APPROVE WITH CONDITIONS（报告断言失败仍落盘、缺数据门禁失败、真实 Core 边界验收）；产品 APPROVE（减少片段重传且可量化）；UI/UX APPROVE（定位参数内部使用，正常流程不增加确认）。零兼容：同步当前协议，无旧工具 alias 或字段 fallback；不修改 Codex Core。

## 后审

四项代码已实施，真实外部模型的收益证据仍受当前连接资格阻断，不能宣称已取得。

### 实际实现

1. `evaluate-live-novel-tools.mjs` 使用生产 Core 的单次 turn、ModelBridge、PersistentTaskBudget、MCP 与 Runtime。短/长正文分别比较完整 payload 与局部 patch，重复批次交替组序；从真实候选读回正文及元数据，核对其他文字、组顺序和锁定结尾。单独统计首次保存成功、保存纠错、全部工具错误和实际 Engine 用量回执。未知费用不记零，也不把保存成功当成任务完成。
2. `evaluate-novel-tools.mjs` 和 `tool-evaluation-report.mjs` 在启动、每个样本开始/结束及终态原子落盘。失败留在分母，独立样本继续；超时无输出单独计数，不伪造响应。中途硬终止至少留下 running 样本及此前完整结果，不能据此推断未写入。CI 验证样本身份/数量、100% 正文保护和成功、已知纠错闭环及长文请求 token 比值 ≤ 0.5；缺样本或重复样本也失败。该阈值宽于实测收益，留出分词及随机 ID 差异；不使用易受系统负载影响的延迟门禁。workflow 配置 always 上传已有报告，远端 CI 尚未运行。
3. MCP 的本地绑定内容、operation/root、执行身份、直接子线程和 root-only 拒绝统一返回 `tool_binding_rejected`，保留 `origin=novelMcp`、phase、稳定原因、字段和下一步。Runtime HTTP 拒绝仍保留原始诊断，未将网络失败或未知保存结果改称确定性拒绝。
4. `read_source_content(view=locate)` 在 Native 授权的场景组正文中定位短原文，返回每个精确匹配的不可变锚点，包括重复、重叠、emoji 与锁定标记。分页继续保持 quote/groupId；换查询、来源或任务不能复用游标。定位不认证完整阅读、不选择修改位置、不提供写授权。Renderer 重试身份同步包含定位参数，workflow 指令说明如何消费。

### 验证

Node 22；隔离 Native target；临时作品和数据库；未修改通用 Codex Core，未改动真实作者作品。

| 检查 | 实际证据 |
| --- | --- |
| MCP 完整契约 | 113/113，通过 HTTP/RPC 和本地缓存绑定拒绝；最终指南改动另复验 6 项绑定/声明测试 |
| Runtime context_scope / task_sources | 14/14、5/5；精确定位锚点真正用于保存，越权来源/错误游标拒绝，定位不增加全文读回覆盖 |
| 定位分页增强 | 700 次匹配的连续页、Unicode 位置和换查询复用游标拒绝，单独复跑通过 |
| Native 编辑锚点 | 2/2，含重叠命中、锁定和伪来源封装 |
| 报告、门禁、真实模型预检 | 5/5，失败/启动异常报告落盘、失败分母、缺失与重复样本、token 回退、秘密不进入报告 |
| 界面重试身份 | 1/1，content/raw/workspace/locate 与不同 quote/groupId 互不掩盖错误 |
| 模型评测驱动的真实 Core 集成 | 4 个样本全部通过：真实 Core + Bridge + MCP + Native + 持久预算；仅 provider 为本地脚本，报告类型明确 controlledProviderToolComparison |
| 构建与门禁 | Native bins、TypeScript、Vite、生成资源检查、git diff --check 通过；architecture:check 361 文件/7 crate/0 diagnostics |

集成开发中发现并修正了评测脚本的当前协议适配问题：作品创建 DTO 必填字段、turn/start 的 input/additionalContext 分离、Bridge executorValidatedTools 配置，以及测试 provider 对 MCP JSON 数组和 Core `Wall time/Output` 文本封装的解码。修正后从同一入口完整执行四个样本并验证真实用量，不依赖跳过或放松生产校验。前期失败日志保存在本次运行的 `/tmp/novel-live-eval-core-*.log`，不作为模型行为样本。

### 收益与边界

[确定性工具回放报告](evidence/2026-10-10-tool-locator-efficiency.json)：长/短正文各三次，全文/patch 合计 12 个有效样本、6 个独立故意错误纠正样本全部通过。报告包含执行时 Native 二进制哈希，未用本地分词量代替 provider usage。

- 三个长文保存请求合计：cl100k_base **19858 → 436（减少 97.8%）**；o200k_base **13375 → 429（减少 96.8%）**。
- 所有有效样本的请求与规范化结果合计，cl100k_base 为 **82348 → 67876（减少 17.6%）**。额外定位读取有成本，不能把保存请求收益当成全流程收益。
- 延迟原样记录；本机同时运行其他任务，不据此宣称速度改进。两种路径都保留完整原文读取，节省来自精确编辑参数。

[真实模型预检报告](evidence/2026-10-10-live-tool-qualification.json)保存 `blocked / qualification / task_qualification_unavailable`：已尝试使用当前应用的 DeepSeek 连接证据，经现行资格门禁拒绝，执行样本数 0，providerUsage 为 null。未伪造证据、绕过资格或把脚本 provider 的成功率写成真实模型收益。真实连接验证恢复后，入口可以按下面配置直接执行；本轮不包含重新发布连接资格。

运行：

```sh
npm run evaluate:tools -- .evaluation-reports/tools.json 3 --check
npm run test:tool-evaluation
npm run evaluate:tools:live -- /path/to/config.json .evaluation-reports/live-tools.json
```

配置字段见 `scripts/tool-evaluation-live.example.json`。路径指向当前应用的资格证据和凭据作用域文件，配置不存 API key。例子为 4 个样本，每样本最多 10 次请求、50000 token 和 90 秒模型执行时间；增加 repeats 会相应增加总额度。启动/清理时间另计。真实模型报告不是文学质量认证，也不会更新产品资格清单、采用或发布候选。

五视角后审：架构 APPROVE（B01–B10，无第二模型循环）；技术 A APPROVE（源权限、Unicode、游标与错误传输实测）；技术 B APPROVE WITH CONDITIONS（真实外部模型比较仍 blocked，远端 CI 未运行）；产品 APPROVE（精确编辑与可量化回归）；UI/UX APPROVE（原流程不增加作者技术输入，诊断和重试身份准确）。零兼容门禁 PASS；保留其他任务已有修改。代码实施、受控链路验收与真实模型收益明确分开。
