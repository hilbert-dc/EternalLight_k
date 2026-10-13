# 已收到终态用量后的流中断结算

## 变更与责任（实施前）

真实单章样本 3 的 writer 与最终 planner 分别有一次 Core 流中断重试；8 / 15 条独立 `token_usage_record` 的总和分别与 Engine known 220,066 / 550,180 一致。原 37,579 / 74,946 unknown 保留。现有证据不能证明这两次中断在 Engine 内已收到供应商 usage，也不能区分上游网络与转发故障。

静态确定的通用缺口是：ChatStream 可以先缓存供应商 usage，`bridge_exchange.rs` 后续 `next_provider_event` 或 `HttpEvent::Error` 提前退出时，不经过 EOF 后的 `decoder.reported_usage()`。原退出清理会将整个预留计为 unknown。本次只修这一请求内部的结算路径，不据此重写历史账本。

责任模块是 Agent Engine，适用 B01/B04–B06/B10。供应商终态用量是请求计量证据，Engine 原 `TaskBudget` / `PersistentBudget` 是唯一执行账本权威；Native Work 账本不在本次修改范围。公开 Bridge、预算 reserve/settle、身份与执行 lease 契约保持不变。Core、Host、Native、Renderer、模型创作决定不承担供应商结算。

只在同一 Exchange 暂存已被现有 decoder 接受的 Chat 终态 usage：同帧存在非空 `finish_reason`，或已有该 finish 标记后收到明确 `choices: []` 的 usage-only 帧。普通 delta 携带 usage，即使此前已有 finish，也不升级为终态总量；无 usage 不推算为 0。原始 total、存在的 input/output 数值经现有 `reported_usage` 的非负、安全整数校验，避免归一器将非法 total 回退为 input+output。退出时使用暂存证据调用原结算；已有 settled 则不重复。没有证据仍按原已发送 unknown / 未发送 not-sent。

本次没有新接口、schema、存储表、观测平台、迁移、回填、旧字段 fallback 或自动重试。供应商流依然失败，工具不得释放，取消不得绕过；保留用量不代表任务成功、作品已保存或费用全部核定。原身份、权限、预算上限、deadline、字节上限和未发送退款条件保持。

## 零兼容与五视角预审

| 视角 | 结论 | 核对 |
| --- | --- | --- |
| 架构专家 | APPROVE WITH CONDITIONS | 只在 Engine 原 Exchange / settle 边界保留当前请求的证据，不让 Host 或 Native推断费用，不跨请求共享缓存。 |
| 技术专家 A | APPROVE WITH CONDITIONS | 只保留 decoder 接受的终态、安全整数 usage；缺失、非法与中途 usage 仍 unknown；取消已发请求不退款，每个 reservation 一次结算。 |
| 技术专家 B | APPROVE WITH CONDITIONS | 在现有实际 HTTP Bridge 测试中覆盖合法终态 usage→transport error，以及无 usage、中途 usage、非法 total 和已发取消的负例。Root 唯一构建和测试。 |
| 产品经理 | APPROVE | 修复已获得计量证据被丢弃的工程缺口；保留样本 3 的 112,525 unknown，不宣称它已可追回或长篇任务已完成。 |
| UI/UX 设计师 | APPROVE | 无界面改动，不追加用户技术输入，不把已知用量包装成流成功或交付成功。 |

综合决定：按条件窄修。独立只读预审指出“任意 usage 帧不等于终态用量”和“非法 total 归一回退”两项，均纳入实施条件；无兼容例外。

## 预定验收

- 两种实际 Engine Bridge 入口（原生库与独立进程）经真实本地 HTTP：明确终态合法 usage 已被 decoder 接受，随后 socket 运输错误，流仍失败，原 reservation 只结算一次、known 精确、unknown 为 0。
- Root补充同一终态证据的usage-only形状正例：先finish，再choices为空的合法usage，再文本marker；marker仅用作有序解码的观察门，普通后续delta不升级用量。与同帧finish正例共同覆盖生产判断的两个分支，不新增生产行为。
- 缺少 usage、非终态 usage、finish 后普通 delta 的 usage、非法原始 total 后同样断流：known 为 0，unknown 等于本次真实预留，requests 不退款。
- 已发送请求取消且缺少终态 usage：取消结束后 requests 保持 1、unknown 保留、reserved 清零，不释放失败流工具。
- 现有 stream terminal、预算拒绝、原生取消/未解决结算与进程预算合同保持；构建、架构门禁由 Root 实际执行。受控 HTTP 故障不证明当前 DeepSeek 样本中曾收到 usage。

## 实施后静态复核

实际变更只有一个生产文件 `agent-engine/native/engine-core/src/bridge_exchange.rs`：每个 Exchange 的 finish 状态与暂存用量互相独立；raw total、存在的 prompt/completion counts 用原 `reported_usage` 严格校验；只有同帧 finish 或此前 finish 后明确 `choices: []` 的帧可以产生候选。两处 SSE 消费点均在 decoder 成功接受帧之后才保留候选；后续运输错误或取消的原退出结算使用该证据。`settled` 与原 reservation 防重复，已发送与未发送的原 outcome 不变。未改 ChatStream、codec、预算存储、上限、权限或任何领域流程。

同一个现有实际 HTTP 测试文件增加两种 Bridge × 六种情形：合法终态 usage、missing、nonterminal、postfinish-delta、非法 raw total、已发送 cancelled。先读取客户端的同帧文本 delta 证明 decoder 已接受帧，再破坏供应商 socket 或取消；不用 sleep 猜处理进度。断流必须真的导致 HTTP 读取拒绝，不能伪造成功结束；原 reservation 一次结算、已发不退款、reserved 清零、重复取消不重复计费均有断言。既有六个失败终态工具不释放的用例保留。

| 实施后视角 | 静态结论 | 核对及证据边界 |
| --- | --- | --- |
| 架构专家 | PASS（待实际验收） | 只改 Engine 同一请求的原结算路径，公开 Bridge/lease/预算合同与依赖不变；无第二事实权威。 |
| 技术专家 A | PASS（待实际验收） | 已接受的明确终态安全整数证据才保留；缺失、普通 delta 与非法 total 仍 unknown，取消未绕过，历史账本未碰。 |
| 技术专家 B | PASS（待实际验收） | 实际 HTTP + 两种真实 Bridge 入口，客户端 marker 形成因果门；源码静态检查不代替 Root 编译/测试。 |
| 产品经理 | PASS（待实际验收） | 只避免已收到的合法终态计量被丢弃；样本 3 原 112,525 unknown 不恢复，不把此修复说成真实样本已追回费用。 |
| UI/UX 设计师 | PASS（待实际验收） | 无 UI 或作者输入变化，流失败仍失败；Root 的其他 UI 验证不能替代 Engine 合同。 |

独立只读复核最终 PASS：已排除中途 usage、非法 total 归一以及 finish 后普通 delta 的三项反例。零兼容门禁静态 PASS；B01–B10 边界静态 PASS；定向 `git diff --check` exit 0。未启动构建、测试、应用或模型，也未读取凭据或修改实际账本。实际结果待 Root 填入，尚不声明构建或合同通过。

冻结生产/测试 SHA-256：

- `agent-engine/native/engine-core/src/bridge_exchange.rs`：`0a3d15d54ee5d7780b5e6ecc6fb2e07deae29c32784ca475cb19452e0fe9857d`。
- `agent-engine/tests/bridge-stream-terminal.test.mjs`：`125252de55713086d3573548464f372d5e8a15226e4536651054041961c209af`。

Root 最小实际入口（按顺序）：

1. 在 `agent-engine` 执行 `npm run build:native`。现有脚本执行当前 workspace `cargo build --locked --release --target-dir target`，构建 `eternallight-engine-core` / `eternallight-node-binding` 并更新实际 `native/bin/<platform>-<arch>/engine.node`；不改 flags 或降低断言。
2. 在 `agent-engine` 执行 `node --test tests/bridge-stream-terminal.test.mjs tests/bridge-budget-failure.test.mjs tests/native-bridge-lifecycle.test.mjs tests/process-budget-runtime.test.mjs`。需本地 HTTP 权限；受控供应商不发送私有小说或调用付费模型。
3. 在 `novel-codex` 执行 `npm run architecture:check`，并核对冻结源码后再记录结果。若已有应用进程加载旧 Native Engine，Root 决定其正常生命周期重启；本补丁不自行操作应用。

未新增诊断平台、持久表、迁移、回填或原稿修改。受控合同只能证明当前 Engine 在确已收到合法终态 usage 时保留它，不能证明过去任何无证据的请求 usage。

## Root实际验证

`npm run build:native`实际exit0、9.88秒（`/tmp/autonomous-v3-current6-stream-error-engine-build.log`）。生产源码仍与冻结SHA一致；Root仅给现有HTTP测试补充usage-only正例，现在两种Bridge各7种断流/取消情形，另保留原6种失败终态。

首次受限环境测试无法监听本机端口，实际exit1，日志 `/tmp/autonomous-v3-current6-stream-error-engine-tests.log` 保留。经自动审批允许测试127.0.0.1后，同一4文件实际33/33、零失败/取消/跳过、exit0（`/tmp/autonomous-v3-current6-stream-error-engine-tests-localhost.log`）；架构检查332文件/7crate/0诊断、exit0（`/tmp/autonomous-v3-current6-stream-error-architecture.log`）。源码与实际engine.node身份见 `/tmp/autonomous-v3-current6-stream-error-root-source-proof.json`。

Root后审：五视角条件均由上述合同和边界证据满足；取消和断流仍失败，已派发请求不退款，终态合法用量精确结算一次，缺失/中途/非法仍unknown。原样本112,525未知用量不变。此为当前Engine受控合同通过，不证明DeepSeek历史请求用量可追回，也不等同真实恢复验收完成。
