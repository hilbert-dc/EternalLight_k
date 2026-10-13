# 知识建模 MCP 对象契约修复

2026-09-28；实施者自检，按作者要求不组织独立专家评审。

## 问题与目标

真实模型能改稿，但建模调用提交 entities/assertions 的 JSON 字符串数组，Runtime
要求对象并拒绝。MCP 原 schema 未声明 items；Codex 的
`tools/src/json_schema.rs::ensure_default_children_for_schema_types` 会为缺少 items
的数组填入字符串元素。模型收到的工具契约与领域实际契约因此不一致。
后续参数校验失败导致桥接中断，重试最终耗尽预算。原始证据见
[真实创作验证记录](LIVE_CREATIVE_VALIDATION_2026_09_28.md)。

修复目标是公开当前合法结构，不让模型猜测对象形状，不解析旧字符串格式，
不改 Codex 内核，不放松 Runtime 权限或来源校验。

## 实施

- 在 story-mcp 独立 `knowledge_schema` 模块声明实体对象、单个证据对象和断言对象。
- 明确别名、参与者、前置引用、效果引用等数组元素，避免嵌套数组再次被补成字符串。
- 断言区分通用断言/关系/认知、事件、属性状态三类形状；事件声明参与者与引用结构，
  属性状态声明类型、基数与值结构。通用断言 value 保持 JSON 值，不擅自缩窄领域能力。
- 发布工具只包含自身必需参数，不混入其他操作的 payload、summary 或 idempotencyKey。
- schema 是形状指引，不是领域权威：字节限额、Unicode 原文证据、实体角色、前后时间、
  引用当前性、属性值与类型匹配仍在 Runtime 事务校验。此次不宣称 schema 与所有领域
  语义一一等价，也没有新增提交权限。

## 验证及非目标

新增自动集成回归启动真实 MCP 二进制，读取 tools/list，交给引擎 Responses codec
构造请求并校验响应：对象、事件和属性状态通过；字符串候选及数组 evidence 拒绝。
该回归已纳入默认 `test:agent`。另运行原 MCP 传输与权限测试，真实模型复验沿独立
测试目录执行，结果单独记录，不把 codec 测试冒充完整 Codex/小说验收。

没有 schema 数据迁移、旧接口、双轨或兼容代码；未改变 UI、数据库、引擎校验政策。
模型仍可能输出错误参数。本次不解决所有 schema 失败的可恢复反馈、其他工具的
完整形状声明、跨平台验收或文学质量；这些不能因一个真实模型用例通过而取消。

## 复验反馈与第二项修正

仅修复 schema 后，真实模型已发出对象候选，未再产生原 schema 断流，预算中的
unknownTokens 为 0；但模型给 evidence 加入来源字段，仍被领域拒绝。原提示要求
“每个 claim 保留批次、来源修订”，容易被理解成向载荷添加元数据。
现明确这些身份由 Runtime 根据冻结批次关联；entity/assertion 不自加批次、来源
修订或断言 ID，evidence 只含 quote/start/end。

Runtime 证据错误也拆开：额外字段明确报告允许的字段和批次绑定原则，字符锚点
不匹配继续走原有错误。没有丢弃字段或自动改正内容。新增 Router/SQLite 回归验证
拒绝错误形状后，同一任务可以正确提交，未把失败当成完成。

验证：MCP 11 项（本地监听须沙箱外）、JS 契约与 handoff 10 项、Runtime 局部改稿
与证据修正 5 项均通过；story-mcp、story-core、story-runtime 全目标严格 Clippy 通过。
真实模型最终结果仍以单独验证记录为准。
