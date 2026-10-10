# 当前工具 schema 的有界正则执行合同

## 变更概览与证据边界

目标：保留 Native 工具声明的 `pattern` 格式约束，让 Engine 对可执行的正则子集使用线性匹配和明确的编译资源上限。初始控制面和按需发现的领域工具使用同一合同，不能通过删掉约束让工具可见。

已证实的失败：`/tmp/autonomous-v3-live-control-bootstrap-tests.log` 的 76 项中 75 项通过；实际 Native MCP 目录原 schema 进入 Engine ModelBridge 的测试返回 502，错误为 blanket `pattern` 拒绝。问题 ID、64 位来源哈希和有 `maxLength:100` 的分页游标均受到影响。这是 raw Native → Bridge 的实际工程证据；不能据此宣称真实 UI 已遇到同一 502。当前 Codex schema DTO 会省略若干约束，本次不修改该 DTO，也不通过省略来掩盖失败。

范围：Engine `schema.rs`、`tool_validation.rs`、`canonical.rs`、engine-core 的显式 `regex-syntax` 依赖及 lock；现有 Engine codec 测试和应用现有 actual-MCP bootstrap 测试内的全 operation 目录编译覆盖。Native schema、角色准入、业务权限、预算、领域流程、UI 均不变。Root 唯一构建和执行测试，本代理仅实现与静态检查。

## 架构责任与当前增量（B01–B10）

责任模块为 Agent Engine 的通用工具协议编译与参数校验，主要适用 B01/B04/B10，保持 B02/B03/B05/B06/B07/B08/B09 的原边界。读取事实权威是当前 Native MCP 的真实 schema 与当前 Engine 工具登记；只写通用校验实现和本合同测试，不读取或写作品正文、作者授权、Work、费用或完成事实。公开契约为实际 MCP `tools/list` → Engine 公开 `chatCompletionsCodec.createRequest`/`parseResponse` 与 `ModelBridge`。测试跨层读取生产目录，Engine 生产代码没有依赖产品实现。

不放邻层的原因：Native 已声明合法领域参数约束，移除 Native 约束会违反 B04；Engine 负责通用 schema 执行，不能让 Core、Host 或模型自行猜测或替代参数约束。Codex DTO 当前约束投影限制明确记录于上文；本轮只证明 raw Native 到真实 Engine 转换/校验的等值，不宣称 Core 模型传输已等值，不改 Core。

Root 实际首轮构建 exit 0，Engine `/tmp/autonomous-v3-bounded-schema-engine-tests.log` 为 89/90：编译膨胀表达式被安全拒绝，但依赖的错误字段路径为空。应用 `/tmp/autonomous-v3-bounded-schema-application-tests.log` 为 75/76：实际全 operation 目录到 `patch_media_document` 的 `exclusiveMinimum` 被拒。这两份失败保留，不作为通过。

同一合同最小增量：每个 pattern 在原声明节点先使用同一线性 validator 独立编译，将资源拒绝精确绑定原 `/.../pattern`；支持当前 Native 已用的 Draft202012 数值排他边界及 revisionOutcome 的 `dependentRequired`、`if/then`（同一条件合同含 `else`）并递归审查其子 schema。没有新增领域评价或媒体功能。当前 Native 源码静态未发现其他未支持关键字；`setComicFormat.format` 是普通属性名，不是 schema format 关键字。实际全目录公共转换器是最终验证，静态搜索不替代它。

| 前审视角 | 本轮增量决定 |
| --- | --- |
| 架构 | APPROVE：Engine 执行 Native 原声明；公开转换/调用契约相同，条件依赖只是标准 schema。 |
| 技术 A | APPROVE WITH CONDITIONS：孤立 pattern 仍走同一限额线性引擎；保留编译膨胀及非法数值、缺失配对、条件不满足负例。 |
| 技术 B | APPROVE WITH CONDITIONS：`if/then/else` 必须递归 normalize；目录原 schema 等值与实际 parse 参数校验同测，不改失败断言。 |
| 产品 | APPROVE：修复工具可执行性，不增加用户输入或写作流程，不扩大权限/额度。 |
| UI/UX | APPROVE：定位工程字段错误，无新增 UI；真实 UI 样本2尚未发送，不宣称完成。 |

Root 验证范围为当前 Engine codec/工具校验/发现回归、实际应用全 operation 目录及 bootstrap/生命周期相关原 76 项、`npm run architecture:check`。本代理不执行构建或测试。

## 执行合同

- 模式原文最多 1024 UTF-8 字节，解析嵌套最多 48 层。必须为字符串、能被 regular-expression 语法解析，且具有同节点 `maxLength`（最多 40000 字符）或有限 HIR 最大匹配长度（最多 40000 字节）的证明。未经证明的无界表达式仍拒绝，原 `^(a+)+$` 无界负例保留。
- `jsonschema 0.33` 自带 `PatternOptions::regex()`，使用线性 `regex` 引擎；编译大小与 DFA 缓存各最多 1 MiB。没有 fancy/backtracking fallback，不支持的反向引用、lookaround、损坏表达式必须拒绝。
- `schema::compile_validator` 为唯一 options 构造入口；转换时的编译、普通 schema 校验和实际 tool-call 校验都调用该入口。保留原 40000 字节 schema 上限与 48 层 schema 深度。
- 拒绝诊断携带声明中的工具 namespace/name 与 JSON Pointer 字段路径，不包含作者输入、参数值或凭据。schema 原声明保持等值；实际非法 ID/hash/cursor 仍在调用校验时拒绝。
- 实际 MCP 全 operation 目录逐个通过公共 codec 编译，验证领域工具的延迟加载并未藏住不可执行 schema；bootstrap 正/负角色权限证明继续走实际 Bridge 与原累计预算。

## 五视角实施前评审

1. 架构专家：`APPROVE`。格式约束由 Native 声明，Engine 负责通用执行能力；显式共享 validator 消除编译与执行分叉。不加入文学阶段或词路由。
2. 技术专家 A：`APPROVE WITH CONDITIONS`。线性正则引擎、模式/语法/编译/缓存/匹配语言或输入上限全部有界；不允许降级到回溯引擎。Root 必须执行正/负校验和原恶意无界负例。
3. 技术专家 B：`APPROVE WITH CONDITIONS`。三处 validator 必须统一；实际目录原 schema 等值和全操作覆盖不能改为简化 fixture。保留错误日志，待实际构建/测试后才能声明通过。
4. 产品经理：`APPROVE`。模型可以在现有权限内使用控制面与按需选择领域能力，输入无需增加技术参数或人工流程指导；修复不会授予额外写作、发布或采用权。
5. UI/UX 设计师：`APPROVE`。不增加 UI；工程诊断能定位真正有问题的工具字段。实际应用体验与短篇创作是否完成由后续真实 UI 样本验证，编译测试不能替代。

## 零兼容门禁

`PASS`：当前版本一次性使用同一有界 validator；无版本判断、旧字段 fallback、双轨、迁移、回填、降级或 deprecated 路径。Native 当前 schema 保持原语义并非历史兼容。没有兼容例外。

## 实施后复核

实际变更与五视角后审：

| 视角 | 静态复核 | 结论 |
| --- | --- | --- |
| 架构 | `convert`、`schema.validate`、实际 `tool_validation.validate` 共用 `compile_validator`；Native schema 原文不变；只有控制入口参与 bootstrap，领域方法仍延迟。 | APPROVE WITH CONDITIONS |
| 技术 A | 显式线性引擎、1024 字节模式、48 层 parser、40000 的输入字符或有限匹配字节证明、1 MiB 编译/DFA；无回溯 fallback。有限 HIR 证明是最大匹配长度，不把它冒称整个输入长度上限。 | APPROVE WITH CONDITIONS |
| 技术 B | 原恶意无界负例保留，ID/hash/cursor 的正例与非法参数、缺失边界、损坏模式、lookaround/backref、编译膨胀负例加入现有 codec 测试；14 个实际 operation 的每个目录 schema 经公共 codec 编译并等值比较，bootstrap 仍只两次 Bridge 请求及原累计预算。 | APPROVE WITH CONDITIONS |
| 产品 | 模型保留自主工具选择，不新增章节流程、词识别路由、授权或花费额度。 | APPROVE |
| UI/UX | 无新增交互；schema 拒绝包含工具和声明路径而不含调用参数值；实际 UI 体验留给原短句样本。 | APPROVE WITH CONDITIONS |

零兼容复查 `PASS`。无方案偏差；所有条件均为 Root 执行本轮构建与必要回归，代理没有调用外部服务商。保留原 75/76 失败证据；编译/测试未出真实终态前，不宣称全量测试、真实 DeepSeek 创作或文学质量通过。

当前标准关键字增量后审：架构/技术 A/技术 B/UI 结论 `APPROVE WITH CONDITIONS`，产品 `APPROVE`。`pattern` 在 normalize 的实际节点先用同一 `compile_validator` 独立编译，资源限制失败精确为原 `/pattern`，未删除原膨胀负例且断言收紧到完整字段。数值排他边界由 Draft202012 校验实际值，`dependentRequired` 保留成对依赖；`if/then/else` 子 schema 全部递归 normalize，条件内无界 pattern 仍拒绝。新增正/负断言放在原 codec 测试，目录生产 schema 等值测试不简化。B01/B04/B10 责任和公开边界如上；无新产物权威、权限、费用或业务循环。静态语法与 diff 检查通过；本轮新的构建/真实回归均由 Root 执行且当前待完成。前轮 89/90 与 75/76 只作为失败诊断，不冒用为当前通过。
