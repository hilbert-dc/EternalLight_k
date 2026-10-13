# 实机对话历史读取：只读诊断与最小修复前审

## 变更概览

- 目标：确认 QA 隔离环境指定线程的真实历史存储合同，避免将本地历史读取错误误判为模型连接或创作失败。
- 诊断范围：`/private/tmp/novel-autonomous-v3-live-final13-20261008/codex-home` 与其 SQLite；指定线程 `01a119cb-22cb-7c93-a6ed-7ef311fed52a`；当前 Electron、Agent Engine 配置编译器、Rust CLI 的读取源码。
- 非目标：不改生产代码、数据库、线程模式或 CLI；不运行构建、模型、UI；不读取密钥、环境变量内容、正文或历史 item_json。
- 当前状态：只读诊断完成，修复建议尚未实施。自动化 868/201 的此前结果不能证明这个实机入口可用。

## 实际存储证据

只读快照：`/tmp/autonomous-v3-live-thread-history-metadata-readonly-20261008.json`。数据库以 `mode=ro` 与 `PRAGMA query_only=ON` 打开，只查询该线程的身份、模式、路径、时间、归档状态与数量。

1. `prepareIsolatedStorage` 使用 `profile/codex-home`、`profile/codex-home/sqlite`；`createCodexEnvironment` 向 CLI 明确提供 `CODEX_HOME` 与 `CODEX_SQLITE_HOME`。当前生成配置没有 `sqlite_home` 或 `experimental_thread_store` 覆盖。
2. `sqlite/state_5.sqlite` 实际存在；该线程 metadata 实际存在，`history_mode=paginated`、未归档，cwd 和 rollout_path 均属于该 QA 环境。
3. metadata 的创建时间为 Unix `1791434105`，即 `2026-10-08T04:35:05Z`；更新时间 `1791434113`。对应 SessionMeta 的 id 与 history_mode 一致。
4. `sqlite/thread_history_1.sqlite` 中该线程有 **1 个 turn、10 个 item**。本次未读取这些 item 的内容，也未将数量当作文学交付证据。
5. 当前指定线程没有可确证的 state DB 缺失、metadata 缺失或 Legacy 模式问题。不能由现存数据倒推错误发生时的进程 StateDbHandle、线程身份和持久化状态。
6. 原 `/tmp/autonomous-v3-app-live-final13.log:8` 错误没有 threadId 和自身时间戳。前一条键盘日志 `12:30:40` 不能证明错误早于指定线程创建，不能据此建立错误与当前线程的因果关联。

## 当前调用与 Store 合同

- `src/App.tsx` 新建对话成功后选择它；选中变化会调用 `agent.readConversation`。
- `electron/conversation-handlers.mjs` 先验证 Runtime 的 project/thread 归属，再读取 Engine 历史。`main.mjs` 的 readHistory 调用 `codexAdapter.readThread`。
- `electron/codex-adapter.mjs:493` 无论新空对话还是已持久化历史，固定请求 `thread/read` 且 `includeTurns=true`。
- 当前 Engine 协议对 Paginated 线程明确推荐 metadata-only `thread/read`，再通过 `thread/turns/list`、`thread/items/list` 分页。metadata-only 读取新空线程有正式测试，返回预计算 path；**path 非 null 并不证明文件或持久化历史存在**。
- Rust CLI 的 `list_turns` 已实现。Paginated 的 full-history hydration 分支确实会调用它。LocalThreadStore 的分页准入要求实际 state DB、该 thread metadata、Paginated 模式；缺任一则返回 Unsupported，并在 AppServer 映射为原错误 `list_turns is not supported yet`。
- 因此已确证的 caller 缺口是：新空对话没有独立的 metadata-only 展示路径，仍要求完整持久化历史。它是原错误的合理候选，**尚未获得错误对应线程与错误时存储状态的证据，不能写成已唯一证明的根因**。

## 与“模型连接尚不可用”的关系

`codex-adapter.mjs` 的 RPC error 分支只拒绝对应请求，不调用 fail、不改变 configurationError/configured。App 的历史读取 catch 只设置对话读取错误。输入区 agentAvailable 使用 Runtime online、agentStatus.configured 和配置保存状态；提交 catch 仅将 `MODEL_CONNECTION_UNAVAILABLE:` 明确前缀当作模型资格失效。当前 `list_turns` 错误不匹配该前缀。

Root 已进一步定位 Renderer 的实际合同问题：`App.tsx:2122` 将 `agentAvailable && !activeConversationDetail.loading && !activeConversationDetail.error && ...` 合并传给 Composer 的 agentAvailable；Composer 又把这个复合 false 解释为“模型连接尚不可用”。因此 **对话读取失败可以导致该误导文案，路径是复合 availability 参数，不是 configured 被该 RPC 错误改写**。Root 指定另一代理修通用不可发送原因接口，本任务不修改该共享代码。

## 最小修复建议与验收条件

1. 先为本地历史请求记录有界、无内容的 request method、threadId、当前读取意图、错误 code；匹配该次错误的线程与阶段。禁止记录正文、环境变量或 token。
2. caller 使用当前协议：刚创建、尚无实际回合开始/保存回执的对话走明确 metadata-only 展示；真实回合存在后读取当前有界历史。判定必须来自实际创建/回合身份回执，不通过 preview 空字符串、path 非 null、文件名或错误文案猜测；未知状态继续报告原错误。
3. 正式历史路径按当前分页合同读取最近有界 turn/item，保留原 project/thread 校验、真实 turnId 对应的作者原话覆盖、回合 phase 与时序。不得捕获 Unsupported 后返回空历史、切换 Legacy、重建线程、插入替代作者消息或绕过 Store 准入。
4. 验收包含真实 CLI 的新空对话 metadata 读取、首个实际回合后历史读取、进程重启后同线程读取；模拟供应商可用于本地自动化，不为此重复发送真实作者输入。断言同 thread/project、原作者只一次、实际历史内容/phase、无额外模型请求和无资格状态变化。尚需采集原错误的对应请求再决定是否存在 Engine 存储初始化/投影问题。
5. 若错误对应的是已经实际持久化的 Paginated 线程，须核查同一 CLI 进程的隔离 SQLite 身份与真实 Store 状态；必要的 Rust Core 修复先向 Root 报告，当前任务不实施大重建。

## 测试阶段零兼容门禁

- [x] 仅当前协议、当前 Store 和 fresh QA schema；未新增版本判断、fallback、迁移、回填、双读或 Legacy 降级。
- [x] 不将吞错/空历史当作修复，不将真实历史读取失败归类成供应商资格失败。
- [x] 仍要求原 project/thread/作者身份与真实保存证据。
- [x] 本次未改代码或数据库；当前只读证据不能替代修复后的真实入口验收。

## 五视角前审

### 架构专家

`APPROVE WITH CONDITIONS`。历史 metadata、历史正文和供应商资格是不同合同；优先修当前 caller 的读取意图，保留 Store 对真实持久化历史的权限与身份校验。错误对应身份尚缺，不批准猜测性的存储重建。

### 技术专家 A

`APPROVE WITH CONDITIONS`。只读 SQLite 证明当前指定线程存储完整；需要错误时 thread/process/storage 证据才能进一步归因。新增诊断只能记录有界身份元数据，不记录正文或秘密。

### 技术专家 B

`APPROVE WITH CONDITIONS`。metadata-only 与当前分页读取需一次同步 Host、Renderer 契约和真实 CLI fixture；禁止 catch 后降级。新空、真实首回合与重启回读必须全部通过，纯 mock 历史不足。

### 产品经理

`APPROVE WITH CONDITIONS`。新空对话应展示可输入状态；本地历史故障应提示读取失败，不迫使作者重新配置模型或重复输入。不能把咨询/规划缺写权限的另一问题与此错误合并认证。

### UI/UX 设计师

`APPROVE WITH CONDITIONS`。维持现有简洁对话界面，将有效新空对话与读取失败明确区分；无需暴露 SQLite、RPC 或线程 ID。排错元数据保留在日志。

## 综合决策与本次后审

- 决策：允许按上述当前 caller 合同准备最小补丁；**唯一错误根因仍待请求身份与发生时状态证据，不宣称已修复**。
- 实际变更：仅本独立评审与无内容的 `/tmp` 只读证据快照。
- 构建/测试：未执行；不存在新的通过声明。
- 五视角后审：只读边界、证据限定、零兼容与产品错误分类均 `PASS`；生产修复和真实 CLI/UI 回读验收尚未执行。
- 兼容例外：无。原 QA 数据和日志保留，未改变原作者记录或费用。

## 离线 fresh-thread 定位脚本前审（Root 执行）

脚本仅放 `/tmp/autonomous-v3-fresh-thread-history-offline-20261008.mjs`，不加入生产或现有测试链。它先核对已 verified workspace CLI 的 SHA `31514a5685220cf5a5677a63bf35e4ed4b0564988f21fe118d6369f5770b6906` 与当前 supported-cli 清单，建立独立新临时 profile，使用实际 Adapter 的 createThread/readThread 与同一 CLI。允许 RPC 仅 initialize、thread/start、thread/read；发送 initialized 通知，禁止任何 turn/start、turn/steer、LLM 调用或真实作者输入。

配置无 key、无 MCP、禁 api_key_model_discovery；provider 只指向本地拒绝请求的 HTTP 哨兵，任何访问只记录 method/path 并返回失败，不记录 body、不实现模型响应，最终必须为零请求。子进程环境使用固定白名单值和该新 CODEX_HOME/SQLITE_HOME，不继承真实 provider 凭据。各阶段仅以只读 SQLite 保存该线程的 metadata/mode/turn/item 数量。

顺序：真实 createThread 回执 → SQLite 快照 → metadata-only read(false) → SQLite 快照 → 当前 Adapter readThread(true) → SQLite 快照。无 turn、无文本输入；错误不吞掉，精确记录 RPC method/id、错误 code/message 与退出状态，结果若 Unsupported 必须保留。已有 QA profile 仅作为上面的只读证据，不供此脚本写入。

- 架构专家：`APPROVE`。真实 caller/当前 Store 复现，不添加兼容或生产分支；诊断结果才决定后续修复。
- 技术专家 A：`APPROVE`。新 profile、固定无秘密子进程环境、拒绝式本地 HTTP 哨兵、RPC allowlist 和实际 SHA 能限定无模型/无外发。
- 技术专家 B：`APPROVE`。metadata-before/full-after 与实际 SQLite 阶段证据可区分未物化空对话、现存历史以及 Store 初始化问题；不伪造 TaskOutput/metadata、不改 Legacy。
- 产品经理：`APPROVE`。诊断不会让用户重复输入，不把临时脚本通过当作创作交付；当前连接文案改动归其他 owner。
- UI/UX 设计师：`APPROVE`。不占用实机 UI，不添加产品复杂度；仅产生工程诊断证据。
- 本阶段后审：脚本待 Root 实际运行；本代理只静态检查，不执行 CLI、模型或构建。当前未宣称错误已复现或已修复。

## Root 实际离线复现：缺失线程 metadata 行

Root 已实际运行上面的原脚本，actual exit **1**，stderr 空；原记录 `/tmp/autonomous-v3-fresh-thread-history-offline-20261008.stdout.json` 与临时目录 `autonomous-v3-fresh-history-offline-KI0kFu/result.json` 保留。CLI 前后 SHA 均为上述 `31514a...`，providerRequests 为 **0**，没有模型回合或作者输入。

精确新线程：`01a119db-1cb1-7f53-97df-761b6463359f`。

| 阶段 | state DB | 该线程 metadata | history DB | turn/item |
| --- | --- | --- | --- | --- |
| thread/start 后 | 存在 | null | 不存在 | 未建表 |
| metadata-only read(false) 后 | 存在 | null | 不存在 | 未建表 |
| read(true) 后 | 存在 | null | 已创建 | 0 / 0 |

thread/start 与 metadata-only 正式回执均为同一 id、Paginated、Idle、0 turns；完整读取实际返回 RPC `-32601`、`list_turns is not supported yet`。所以这个确定性 fresh-thread 样本不是缺 state DB 或错误模式，而是 **在 state DB 仍无该线程 metadata 行时进入了分页历史准入**。read(true) 内部 persist/投影创建了 history DB，但没有补成真实 metadata 保存合同；不能把空表或成功 metadata-only 返回当作完整历史已保存。

这证明当前实际 caller 会进入同类失败路径；原实机日志仍无 threadId，不能据此声称已认证原错误和该样本同一线程。

## 当前正式 API 的最小修复方案与增量五视角前审

当前 `ThreadStartParams` 的 historyMode、ephemeral 等字段没有显式“保存新空对话”承诺。默认已是非临时 Paginated；重复这些参数不会解决本样本的 metadata 行缺失。无需先改 Rust Core。

现有 LocalThreadStore `update_thread_metadata.rs` 的正式合同明确：**Naming saves a new thread even before its first turn**。真实 Paginated name patch 会先 persist live recorder，再实际保存缺失的 state metadata；不是修改 Unsupported 的准入规则。App 新对话注册本来就有明确的真实 title `新对话`，却只调用 thread/start，没有将该实际创建属性同步保存到 Engine。

最小方案：仅显式新对话创建由其 caller 提供真实 title；在同一 thread/start 回执身份上通过现有 `thread/name/set` 保存该 name，成功后再完成 Native 对话登记与 UI 发布。不得使用伪 author message、空 turn、任意 Daybreak/project patch 作为持久化技巧，也不得在读取失败后补 name 作为 fallback。若 name 保存失败，创建尚未完成，应保留已创建身份供当前 pending-creation 重试，不能重新 start 导致重复线程。实际 fork/已有历史按其原当前合同处理，不为规避这个问题改模式或重建。

验证变体只准备于 `/tmp/autonomous-v3-fresh-thread-history-named-offline-20261008.mjs`：独立新 profile，真实 createThread → 保存 UI 当前真实 name `新对话` → 只读 SQLite → read(false) → read(true)。要求 metadata 的真实同 id/Paginated 与 name、0 turn/0 item、provider 请求 0、无 turn/start，并保留原失败脚本与日志。Root 执行前本代理不声明该变体已通过。

- 架构专家：`APPROVE WITH CONDITIONS`。沿当前正式 naming/save 合同完成新对话创建，不改 Store 或权限、预算、历史模式；只在真实创建意图下保存实际名称。
- 技术专家 A：`APPROVE WITH CONDITIONS`。要求同 thread 身份保存，失败保留 pending 身份，真实 SQLite 行和完整 read 成功双证明；不允许通过 RPC catch/fallback 伪造空历史。
- 技术专家 B：`APPROVE WITH CONDITIONS`。先运行该零模型命名变体；通过后同步 creation caller/Adapter fixture，并验证保存失败、登记失败重试无重复线程。现阶段不需 Rust 重建。
- 产品经理：`APPROVE WITH CONDITIONS`。新对话应实际保存后再展示可继续状态；保存的是既有产品 title，不新增用户输入或无意义设置。
- UI/UX 设计师：`APPROVE WITH CONDITIONS`。不新增 UI，保留空对话体验；创建保存失败显示对应原因，不能冒充模型连接故障。通用 availability 文案由 Root 的其他 owner 完成。
- 本阶段静态后审：原 fresh 样本实际失败证据已确定；生产未改，命名变体只准备未执行。Rust 改动范围当前为 **无**；若现行命名合同实际测试仍失败，再报告实际 Store 写入问题并重新限定范围。

## 命名变体实际通过与最小生产补丁前审

Root 实际运行命名变体，`/tmp/autonomous-v3-fresh-thread-history-named-offline-20261008.stdout.json` actual exit **0**，同一线程 `01a119e0-b254-7ea2-b91e-431aad7c445e` 在真实 name save 后出现 state metadata，mode=Paginated，history DB 的 turn/item 均为 0。metadata-only 和完整 read 都成功，实际 name 为 `新对话`。provider 请求 0、无 turn/start、CLI 前后 SHA 不变。原失败样本与日志保留。这是正式 API 的离线当前合同验证，未声明真实模型创作完成。

Root 已批准最小生产范围：

- `electron/codex-adapter.mjs` 新增 `saveThreadName(threadId, name)`，走当前 name 保存 RPC，再 metadata-only read 验证同 id 和实际 name；非法名称、保存错误或身份/name 核验错误均直接失败。
- `electron/conversation-handlers.mjs` 使用已有实际 registration.title，且在 pendingCreations 已保存原线程身份之后执行保存，完成后才 Native 登记/发布。仅显式新对话，原 fork 与 openThread/model-turn 合同不变。
- 失败重试继续使用 pendingCreations 的同一 threadId；不创建替代对话，不调用模型，不增加作者消息或预算。失败后的原创建身份仍可被当前下一次创建请求重试。
- 仅上述两文件及各自 focused tests、独立评审；不改 App、main、Runtime policy 或 Rust，不引入空历史分支或错误 catch 降级。

五视角批准此具体范围：架构专家 `APPROVE`（正式保存与 Native 注册边界）；技术专家 A `APPROVE`（身份、名称与错误强验证）；技术专家 B `APPROVE`（真实离线旧失败/新通过已在相同 CLI 确证，补同 id 重试/错误传播）；产品经理 `APPROVE`（有效空对话先保存再展示）；UI/UX `APPROVE`（不新增 UI，失败原因准确，由已有创建错误展示）。所有视角维持无兼容、无 Legacy/no fallback、无伪作者消息的门禁。

生产补丁的 focused tests 与实际创建/回读仍由 Root 执行；实施前的离线 API 样本不能替代补丁后的实际测试。

## 最小生产补丁静态后审与冻结

- 实际代码：`saveThreadName` 使用正式 name 保存 RPC，并通过 metadata-only 回读核实同 id/实际 name；它不消费历史，不返回虚构空历史。当前新对话 handler 在已有 pending identity 保存之后调用此方法，成功后才 Native 登记。原 createThread/openThread/fork、model-turn、资格/预算/权限均未改。
- 错误行为：名称非法、任务活动、Native 保存失败、回读身份/name 不匹配均失败上抛；保存/核验/登记失败都保留现有 pending creation 身份，由作者的显式下一次创建重试使用同 id；没有自动重试。
- focused tests：`electron/tests/codex-adapter.test.mjs` 增补保存的精确 RPC 顺序、合法名称、身份/name 错配、保存失败以及活动回合拒绝；`electron/tests/conversation-handlers.test.mjs` 增补保存失败/核验失败不注册、同 id 重试仅一次 thread/start、无 Task/turn/start，原归属/归档/分支/登记失败测试保留。
- 新实际生产方法离线变体：`/tmp/autonomous-v3-fresh-thread-history-production-offline-20261008.mjs` 直接调用新 `adapter.saveThreadName`，其余真实同 CLI、新 profile、0 provider/0 model turn、真实 metadata/name/full-read 断言保留。只准备，待 Root 执行。
- 五视角静态复核：架构专家 `PASS`（当前正式保存/注册顺序）；技术专家 A `PASS`（强验证与同身份错误重试）；技术专家 B `PASS`（精确小范围及正负 focused tests，实际执行待 Root）；产品经理 `PASS`（实际空对话可保存，错误不假装成功）；UI/UX `PASS`（无新增 UI，当前创建失败正常展示）。
- 验证边界：旧离线样本 actual exit1 与命名 API 样本 actual exit0 均保留；补丁后的真实测试尚未运行。本代理不构建、不执行测试、不调用模型。静态语法/差异检查另记冻结清单，不作为业务通过。
- 零兼容门禁 `PASS`：无版本/fallback/Legacy/吞错；无空 turn/伪作者/新预算/新权限/新 Engine 状态机或 Rust 改动。

## 补丁后实际验证与唯一当前 fixture 同步

Root 实际执行生产方法离线脚本，`/tmp/autonomous-v3-fresh-thread-history-production-offline-20261008.stdout.json` actual exit **0**。实际新线程 `01a119eb-3bd9-7902-9fed-9e4eb6e51030` 经新 `saveThreadName` 保存并强核验后，完整 read 成功返回同 id、0 turns；实际 name 为 `新对话`、Paginated metadata 存在，provider 请求 0，无模型回合，CLI 前后 SHA 仍为 `31514a...`。这证明当前生产方法调用正式保存合同后可读，不是吞掉原错误或人工构造历史。

Root 两个完整 focused 文件第一次运行：`/tmp/autonomous-v3-live-history-contract-tests.log` actual exit **1**，37 tests / 36 pass / 1 fail / 0 cancelled / 0 skipped / 0 todo。唯一失败为原 preload 参数快照缺少当前 `sendPrompt` 的最后一个 mediaInput 默认 null；新 name-save 与同 id 重试、原归属/归档/分支/登记失败均已实际通过。

当前真实 `preload.cjs:146` 和 `src/types.ts:878` 明确含第六个 mediaInput 参数；普通文本调用默认 null。只把本 owned fixture 的严格完整参数数组增加该尾部 null，不修改调用方、权限或媒体能力，不删除严格 deepEqual，也不增加旧格式分支。原失败日志保留。

此单行 fixture 同步的五视角前审：架构专家 `APPROVE`（真实当前 IPC 一致）；技术专家 A `APPROVE`（仍严格核对每个参数，无权限变化）；技术专家 B `APPROVE`（仅末尾 default null，同步后重跑原两完整文件）；产品经理 `APPROVE`（无产品行为变化）；UI/UX `APPROVE`（无界面变化）。静态后审均 `PASS`；完整 37 项重跑结果仍待 Root，不能把 36/37 或离线 0 当作完整实机创作通过。

## 最终当前合同验收：PASS

Root 对原两个完整 focused 文件再次实际执行，`/tmp/autonomous-v3-live-history-contract-tests2.log` actual exit **0**，**37/37 pass，0 fail / 0 cancelled / 0 skipped / 0 todo**。直接使用新生产 `saveThreadName` 的真实 CLI 离线样本 actual exit0 亦已核对。生产源码未因唯一 IPC fixture 同步改变；本次补录不改源码、不追加测试。

五视角实施后结论均为 `PASS`：架构专家确认正式名称保存→身份/name 核验→Native 注册顺序；技术专家 A 确认保存失败/核验失败强拒绝与同 id 显式重试；技术专家 B 确认实际真实 CLI 零模型保存/完整回读及原两完整文件 37/37；产品经理确认新空对话已实际保存，失败不会发布或产生重复线程；UI/UX 确认无需新增 UI，创建错误由既有路径呈现，availability 文案改动仍由其他 owner 独立验收。

原离线 fresh 未命名样本 exit1、其 metadata=null/原 RPC -32601、原 focused 36/37 exit1 及全部日志继续保留。最终 PASS 仅认证这个最小新对话持久化与历史读取合同及其安全/错误重试；不声明另一创作提案权限问题、真实 DeepSeek 章节生成或全应用闭环已通过。
