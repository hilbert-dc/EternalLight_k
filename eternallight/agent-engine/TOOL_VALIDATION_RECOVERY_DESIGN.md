# 工具参数失败与可恢复反馈

状态：受信执行器的同轮纠错路径已实施，验收进行中。未委托的工具保留桥接层严格拒绝。

## 当前实现（2026-10-06）

嵌入方可在 ModelBridge / ModelBridgeProcess 指定 `executorValidatedTools`，每项必须包含精确 namespace（可为 null）与 name。BridgeService 校验配置，Exchange 仅从宿主选项注入 codec 私有 context，不从请求或供应商读取此授权。工具身份、tool_choice、JSON 对象、大小、调用 ID 与流提交校验仍在 bridge 完成；只有参数 schema 检查移到受信执行器，调用内容与 ID 均保持原样。

小说工坊先仅对 `novel_workspace.publish_knowledge_batch` 启用。原生 MCP 在发出任何业务 HTTP 请求前按完整原始 schema 校验，返回真实 `isError` 和有界结构化诊断，进入 Codex 原调用历史。三次连续 schema 错误将执行器锁死，host 依据当前 thread/turn、server、tool 的结构化终止码结束原执行；不另开智能体循环，不重置预算。其他工具、权限和未知写入恢复边界未放宽。

这是执行器接入契约；未实现完整校验的工具不能加入名单。下方早期探针记录保留为历史证据；其“待实施”表述不代表本节当前状态。协议回归、实际 Codex 集成和真实供应商验收分开记录，不相互替代。

## 真实证据

2026-09-28 合成作品运行 `2450928693129461`：知识 publish 已成功，
complete_task 的无关 operation 属性造成 schema 拒绝；多次 502 后预算拒绝。
小说层原声明将共享属性池用于不同工具，错误暴露了候选任务专用枚举。

引擎中 responses::validate_call 和 canonical 参数校验返回字符串错误，
bridge_exchange::Failure 将普通字符串默认映射为 502。流式响应已经发头时
failure 直接 abort，调用方只看到断流。仅把非流式 HTTP 改成 400 不足以解决
流式路径；也不能把所有 502 改成不可重试，因为真实网络中断仍需要恢复。

## 已实施的边界修复

小说 MCP 统一按每个处理器的 required 与真实 optional 参数发布 schema。
上下文的位置/查询计划、正文搜索的身份/游标/上限、关系检索的方向/时间/跳数
保留；其他工具不再继承无关属性。删除分散的局部过滤逻辑，不改变 Runtime
授权、来源检查、持久化或任务完成标准。无需旧 schema 兼容或数据迁移。

## 引擎目标设计及实施约束

1. 将可纠正参数错误、未授权工具、重复调用 ID、协议结构损坏、供应商 HTTP
   与网络故障分为类型化失败；禁止依赖错误字符串包含某个英文片段来决定重试。
2. 可纠正错误必须有明确工具身份、原始调用 ID、schema 路径和有限错误说明。
   参数仍须严格校验；未经校验的调用不得下发执行。错误输出不能包含密钥或
   无界参数回显，正常输出及失败输出均受已有单项/总上下文预算约束。
3. 恢复应进入 Codex 可见的调用—拒绝—修正历史；不能在桥接层暗中修改模型
   历史，也不能伪造一条已经执行成功的工具结果。需确定与上游同步友好的执行
   边界接口后实施，不引入第二套 Agent 调度器。
4. 流式与非流式必须共享类型和恢复规则。不能在已发出部分可执行项后重放整轮，
   也不能仅改变 HTTP 状态却保留断流重试。完成事件、usage 和校验失败须有可核对
   记录，未知用量保守记账，不把失败尝试从预算中抹掉。
5. 修正次数有限且使用原 TaskBudget/执行租约；达到上限明确终止或请求作者处理，
   不重置预算。鉴权失败和越权调用不是可自动放宽权限的“参数修复”。

## 待完成的验收

用真实内核与模拟供应商覆盖 Responses/Chat、stream/non-stream：首轮非法参数
无副作用，第二轮收到可见反馈并正确调用；重复非法参数达到上限停止；混合多调用
不会重复执行已提交动作；usage 不漏账；断线仍按网络策略恢复；取消/恢复不绕过
预算和租约。随后进行有界真实模型复验。现有 codec/MCP 测试不替代这些验收。

本次实施者自检，无独立专家评审。尚未改变引擎错误分类或重试行为。

## 工具契约联合验证补充

新增真实 MCP tools/list → Responses codec 请求构造与参数响应校验，覆盖整个
modelKnowledge 暴露工具集合及合法位置/检索可选参数。测试首次发现 queryPlan
与阅读检查点的 pattern 声明不在引擎支持的 schema 子集中，请求构造直接失败。
已改为固定长度字符串与明确说明。Runtime 的 digest_valid、精确基线哈希、
结果 ID 成员校验及阅读报告重算未变，不能仅凭 64 字符通过执行。

修复后 11 项原生 MCP 测试、1 项上述联合测试与 story-mcp 全目标严格 Clippy
通过。此证据不等于参数错误已能自动反馈给模型；恢复路径仍待实现。

另有 semantic_query_tests 3 项通过，覆盖基线绑定、伪造/过期判断拒绝、
作用域隔离及统一上下文预算；未运行真实模型复验。

## 实测的 Codex 工具 schema 压缩风险

2026-09-28 合成作品 `875f1b2cd7a813c7` 中，模型提交缺少事件 evidence 的
publish_knowledge_batch 调用却抵达 Runtime。工具原始 inputSchema 为 6229
字节；Codex `tools/src/json_schema/compaction.rs` 对超过 5000 字节的 schema
依次剥离描述、压缩深层对象/组合。此机制与缺失字段能到 Runtime 的现象一致；
仅用原始 MCP schema 直接调用 Responses codec 的测试不能证明 Codex 中间注册
仍保留完整约束。它不是已证明的唯一原因，仍需从实际桥接请求捕获压缩后的
tool schema 才能闭环确认。

产品 MCP schema 改用顶层 `$defs`/`$ref` 共享证据、实体 ID、时间与事件引用
约束，语义不变，原始长度减为 4812 字节。联合测试从真实 MCP tools/list 读取
schema，验证长度低于压缩门槛，并确认缺失 evidence 仍被 codec 拒绝。
Runtime 始终是最终边界；它把缺证据与缺事件 value 四字段分开反馈，真实
Router 在同一任务上验证拒绝后可以纠正提交。此改动没有放宽身份、来源或
确认权限，也没有替代类型化引擎错误恢复的目标设计。

补充本机模拟端点探针：实际 Codex App Server 启动真实 novel_workspace MCP，
首个转发至 `/responses` 的请求中，publish_knowledge_batch 的 schema 为
4283 字节；顶层必填 `entities/assertions`、`$defs.E` 的
`quote/start/end`、事件 value 的四个必填字段均仍存在。
脱敏结构结果见
`/private/tmp/novel-live-revision.wogLsO/schema-22f827e4283b83b8/result.json`。
该探针不触发模型请求，也不证明历史故障唯一由压缩造成；与一次完整真实
模型合成短章运行通过共同构成当前切片证据。类型化错误恢复、多样本稳定性
和生产桌面联合验证仍未完成。

该本机探针已固化为小说工程的 `codex-tool-schema.test.mjs`，并加入默认
`test:agent`。它不使用真实供应商或真实资格证据，专门防止同步上游 Codex
之后模型可见 MCP schema 的嵌套必填项退化；当前 Agent 集合 160/160 通过。
