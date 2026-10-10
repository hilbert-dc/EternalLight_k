# 空对话与创作任务的 MCP 生命周期

## 实施前合同

真实 UI 在输入作者消息之前，新建空对话即失败：`required MCP novel_workspace` 的初始化要求 `NOVEL_CODEX_TASK_CAPABILITY`，但 Native 此时尚未创建 Task。`createThread` 复用了执行用的 `openThread`，把持久化对话与授权执行混为一次初始化。MCP 拒绝缺少凭证是正确行为。

责任属于 Host Adapter（B02/B03/B05/B08）：使用现行 AppServer `thread/start`、`thread/resume` 的公开 `config` 合同组织生命周期。Engine 不理解小说阶段，Native 仍唯一签发 Task 能力，MCP 鉴权、全局 `required:true`、权限、预算、模型决策不改。

- 创建空对话及无 Task 的对话恢复：仅此请求配置 `mcp_servers.novel_workspace.enabled=false`，不修改配置文件，不产生 Task、作者消息、模型轮次或占位凭证。
- 正式执行：请求明确 `enabled=true`、`required=true`；真实任务 prepare 保持能力校验、独立进程和现行 Task 生命周期。
- 仅供对话管理的 live process 不得直接发送/审阅/压缩执行。必须先绑定真实 Task 进行进程切换；即使 operation/cap 相同也不能复用这个管理进程。并发启动的用途不匹配必须拒绝。
- 持久化实际名称成功后才注册作品对话，失败保留精确 pending creation 并上抛，不增加自动重试。历史仍调用真实 Store，不伪造空历史。

## 五视角预审

| 视角 | 结论 | 检查 |
| --- | --- | --- |
| 架构 | APPROVE | 对话管理不拥有任务权限；公开 RPC 配置属于 Host。模型仍自主决定创作行动。 |
| 技术 A | APPROVE WITH CONDITIONS | 空管理进程不能漏入执行；任务绑定必须重启；必需 MCP 的任何错误仍向上传播。 |
| 技术 B | APPROVE WITH CONDITIONS | 精确参数、并发用途、同凭证重绑定、失败不注册及真实 required-MCP 离线证明。无 Rust 改动或兼容 fallback。 |
| 产品 | APPROVE | 用户可先创建空对话，输入后才建立任务；不新增输入步骤。 |
| UI/UX | APPROVE | 空创建显示准备对话，正式任务仍显示准备作品工具；保留真实错误，不伪称连接/创作成功。 |

## 验收与证据边界

Root 统一执行 Adapter/Conversation 两个完整测试文件。离线真实 CLI 必须配置 `required novel_workspace`，以严格拒绝初始化的 sentinel 确认空创建和持久化回读为零 MCP、零 provider、零 turn；执行启用必须真实触发必需 MCP 且原样拒绝。正式任务权限另沿用已有真实 Native→MCP 受控链，不将 sentinel 当领域能力证明。不读 key/env，不调用供应商，不修改 Engine/runtime/App，不构建。

## 实施后复核

已静态冻结：仅 Adapter 与其两份合同测试改变，Conversation handler、Engine、Codex runtime、App、Native 鉴权及预算无行为改动。Adapter 以精确 live process 身份记录管理用途；同 operation/cap 的 prepare 也不能复用管理进程。无 Task 的普通对话恢复使用同一管理合同；真实任务 prepare 后的启动/恢复显式 required/enabled。审阅/压缩用例同步正式 advisory prepare，保留全部原断言。

新增三个合同用例涵盖零凭证/推理不可用时管理、同凭证进程重绑定、精确恢复参数、零 turn、必需 MCP 失败上抛与不重试。Main 合同同时断言空创建只禁本次 MCP，发送所选对话前恢复必需 MCP，原归属/存储/名称/pending/归档断言保持。

真实离线脚本：`/tmp/autonomous-v3-empty-conversation-required-mcp-offline-20261008.mjs`。全局明确 `enabled=true, required=true` 的 MCP sentinel，空创建/名称持久化/完整历史要求零初始化与零 provider；继而在全新隔离 Native DB 通过当前 HTTP 注册真实 Task 能力 hash（随机凭证不输出、不冒充未存在 Task），限额 1 request/20000 tokens/600 seconds，不发送模型输入，正式 prepare/resume 后必须收到 sentinel 原始必需初始化拒绝。该实际拒绝只证明 Host/CLI 生命周期与错误传播，不能证明文学交付或替代 Native MCP 的领域鉴权验收。

| 后审视角 | 静态结论 | 复核 |
| --- | --- | --- |
| 架构 | PASS | 管理/授权执行边界由 Host 正式公开 RPC 表达，领域权限仍属 Native/MCP。 |
| 技术 A | PASS | 管理进程拒绝执行、重绑定换进程、并发用途不匹配拒绝，初始化失败上抛。 |
| 技术 B | PASS | 参数及正负用例同步，旧历史不伪造，未新增 fallback、自动重试、Rust 改动。 |
| 产品 | PASS | 空对话无需提前创建 Task；正式创作依然真实建 Task 后执行。 |
| UI/UX | PASS | 对话准备与工具准备文案精确；保存失败及必需工具错误仍真实显示。 |

静态 `node --check`（Adapter、两测试文件、离线脚本）和 scoped `git diff --check` 均实际 exit 0。Root 业务实跑、实机重新创建和真实模型闭环仍待验收。旧无 MCP 的历史离线成功、37/37 历史合同通过及本轮真实 UI 创建失败均保留，均不能单独证明本新增生命周期已通过。

### 当前实跑及增量边界

Root 当前两个完整合同文件实际 **40/40、exit 0**。真实 required-MCP 离线样本 `/tmp/autonomous-v3-current3-empty-conversation-offline.json` 初次 **exit 2**：四次实际 Store 探测确认空名称保存及完整历史 `0 turns` 成功、provider 为 0、空管理 MCP 初始化为 0；真实 Native Task 注册后正式恢复也确实产生 RPC `-32603` 和 sentinel 初始化拒绝。错误来自脚本把现行 `required MCP servers failed to initialize: novel_workspace:` 写成不同连续字符串的正则，最后归类为 setup failure。这份原失败保留，不将部分执行当脚本通过。

脚本只修 expected-rejection 检查：当前 Core 必需服务器标识和 sentinel `-32603` 原因必须同时匹配，真实 RPC 错误严格只有一条且 code 相同；所有零 provider/turn、真实 Task、限额、进程轮换及拒绝后不伪 ready 断言保持。修改后等待 Root 实跑。

另当前 Core 输出测试只声明通用 `output_fixture` MCP，没有 Native operation，暴露 Adapter 不能对所有通用执行都注入未声明的领域服务器配置。此增量须明确基于实际 Task 绑定的配置责任，不能靠改通用 fixture 名称伪装领域服务器、删除 required 或捕获 invalid-transport 当成功；方案与执行证据另补录。

### 通用执行/领域 Task 增量预审

Root 已批准最小职责修正：只有明确 `conversationOnly` 或 `activeOperation` 实际任务绑定状态才写领域 MCP override；无 Task 的通用执行保留调用方通过现行 compiler 明确提供的服务器配置。不是探测缺失后降级，也不捕获失败决定配置。真正 Task 仍强制 `novel_workspace.enabled=true/required=true`，当前配置缺 transport 则必须 fatal，不回退通用执行。

| 视角 | 预审 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE | 通用执行不越权创建领域工具配置；领域 Task 则必须恢复其唯一 Native MCP 权限。 |
| 技术 A | APPROVE | 管理进程隔离不变；Task 的 invalid transport/初始化错误仍 fatal。 |
| 技术 B | APPROVE | 新增通用启动→正式 Task 的精确 config 正反对比与零自动 retry；不改通用工具 namespace。 |
| 产品 | APPROVE | 普通产品对话仍先管理后建 Task；不增加交互或授权。 |
| UI/UX | APPROVE | 无新 UI；真实失败仍显示，状态含义不扩大。 |

增量已静态冻结。唯一生产变化为 config 生成条件；新对比用例强制通用请求不注入领域配置，正式 Task 必需配置完整，缺失 transport 必须上抛且 process/thread 清空、零 turn、一次恢复、不转通用重试。五视角静态后审均 PASS：模块归属、管理进程隔离、Task 权限和失败不变，正负接口一次同步，无兼容/fallback。四个 `node --check` 和 scoped `git diff --check` 实际 exit 0。前一版 40/40 保留为历史阶段证据；当前完整 41 项合同、2 项实际 Core 输出及修正后的 required-MCP 离线脚本均等待 Root 唯一实跑，当前实机/真实模型闭环仍 pending。
