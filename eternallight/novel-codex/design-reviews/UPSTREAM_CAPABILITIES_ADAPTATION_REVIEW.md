# Codex main 能力适配设计与评审

日期：2026-10-10。官方仓库：https://github.com/openai/codex，目标 main `4bad6d78e`；完整合并，无冲突。适用架构规则 B01、B04–B10。

## 理想方案与实施边界

1. **恢复证据**：Host 从 `thread/items/read` 按 thread/turn/call 精确取得原调用，不下载整段历史。结果仍核对服务器、工具、原身份、修订、幂等键、请求哈希和原有 128 KiB 证据上限；Native 决定是否允许恢复。读取失败不得执行推理或重试写入。
2. **协作上下文**：Engine 从冻结 SessionPlan 生成配置。获得协作能力的会话开启完整 fork 前缀保留及子模型默认上下文；每条 route 的真实输入容量、费用和权限仍由 Bridge 核对。父历史仅作为模型上下文，不转授父审批。
3. **工具结果保留**：Host 和通用 Engine 透传 `turn/start.toolOutputs`；`retain` 必须显式指定，默认不保留。仅受信调用方提交真实工具结果，保留不代表领域保存、作者授权或任务完成。普通 MCP 返回仍走原工具循环，不重复注入。
4. **已读状态**：Codex 是唯一权威。Host 提供带 observed revision 的 read/unread 操作，禁止冲突自动刷新重试；状态作为对话元数据返回，不能决定 Work 完成或恢复。作者资料访问仍通过当前项目归属校验。UI 自动标记必须在实际展示相应 revision 后实施，读取历史本身不产生标记副作用。
5. **消息板**：采用上游真实权限目录和结构化拒绝，不在 Host 模拟消息板或扩大角色权限。模型自主决定使用，不强制固定协作流程。
6. **传输**：继续使用已部署的受控 App-server/Bridge。gRPC-over-stdio 属于独立传输资格工作，需要 code-mode-host 随包构建、取消/退出/启动故障验证后再启用，不以开关代替完整生命周期接入。沙箱、代理和输出排空修复通过完整上游构建取得。

## 责任与公开合同

Engine 配置编译器拥有通用执行配置；Host 拥有 App-server 生命周期、项目归属和协议绑定；Codex 拥有历史及已读状态；Native 拥有作品、任务、权限、预算和恢复准入。Renderer 不接触原始恢复参数。现行接口一次同步，不为旧二进制保留整段历史 fallback。

| 输入事实 | 合法结果 | 副作用/下一步 | 回归 |
| --- | --- | --- | --- |
| 找到唯一同轮调用且哈希一致 | 返回原证据 | 仅供 Native 核对，零推理 | 精确查询正常路径 |
| 缺失/重复/异轮/异项目/截断/哈希变化 | 拒绝证据 | 不保存、不推理、不扩大权限 | 参数和响应拒绝矩阵 |
| 未获得协作能力 | 协作配置关闭 | 不额外开放工具 | 冻结计划配置测试 |
| 明确保留工具输出 | 按 Core 合同传递 | 模型历史保留，领域事实不变 | 出站 RPC 断言 |
| 已读 revision 已过期 | 原错误返回调用方 | 不自动覆盖新消息 | revision 冲突测试 |

## 五视角前审

| 视角 | 结论与依据 |
| --- | --- |
| 架构 | APPROVE：依赖沿公开合同，恢复证据与领域准入分离，无第二份历史/已读账本。 |
| 技术 A | APPROVE：身份、哈希、字节上限保留；读状态 CAS 冲突不重试；取消不被保留开关绕过。 |
| 技术 B | APPROVE WITH CONDITIONS：同步测试/fixture，重建 Engine 与 Codex 并核对构建身份，运行 Fork 与架构门禁。 |
| 产品 | APPROVE：长篇任务恢复和跨模型协作受益，不给作者增加技术参数或文学流程。 |
| UI/UX | APPROVE：不把历史读取当作已读；已有恢复错误仍保留真实原因；自动已读/未读视觉入口另需实际展示验收。 |

零兼容门禁：无历史版本判断、双读双写、旧字段 fallback 或资料重建；保留现行身份、修订、哈希、用量与权限校验。无授权兼容例外。

## 验证与后审

完整合并提交：`7e4445a1a`，已核对目标 `4bad6d78e` 是 HEAD 的祖先。拉取首次遇到连接重置，重试成功。没有新增产品侧 Core 补丁；工作区原有改动保留。

实际实施：精确恢复查询及全部原身份/修订/哈希/字节校验；Engine 协作配置；Host/Engine 显式工具结果保留合同；已读状态投影与经过 Native 对话归属核对的 IPC。调用方可通过 `agent.updateThreadReadState(projectId, threadId, { operation, expectedRevision })` 明确操作。没有把历史读取自动转换为已读，也没有新增 UI 未读标记入口。消息板使用合入后的上游实现；gRPC-over-stdio 仍未启用，传输资格与随包构建属于独立实施阶段。

构建：Engine release 成功；Codex debug 构建成功，BuildIdentity `e6e8fc88f806065fe1325c18085a22d943c963a22804d5dd9b2c30baba185a70`，二进制位于 `codex-rs/target/debug/codex`，身份文件位于 `codex-runtime/build-identities/darwin-arm64.json`。测试使用本次构建。

验证结果：

- Engine 执行、配置、工具结果合同：47 项通过。
- Host 适配与恢复合同：52 项通过，含真实 Runtime/MCP 进程重启后的保存核对。沙箱内 Runtime 无法监听，允许本地监听后通过。
- preload/实际 Host 对话合同：9 项通过，含异项目已读更新零派发。
- 实际 Core→Bridge→MCP 受控模型循环：通过。首次模拟服务缺少用量导致 `task_provider_usage_unknown`，补齐测试服务的明确用量帧后通过，生产预算保护未调整。
- 新增 `electron/tests/upstream-history.integration.mjs`：在全新隔离目录启动真实 Core，不执行模型推理；验证已读/未读 CAS、旧 revision 拒绝、空精确查询及 Host 恢复拒绝，全部通过。测试退出等待真实进程结束再清理其专用目录。
- TypeScript `tsc -b` 通过；最终 `architecture:check` 扫描 359 个生产文件、7 个 crate，无诊断；相关改动 whitespace 检查通过。
- `repo:check` 的 Fork 门禁未通过：既有 `api_bridge.rs`、对应测试、`mcp_tool_call/conversation_history.rs` 缺少登记；既有发布目录非 ASCII 路径被 git 引号表示后也被误判。此次没有修改这些补丁或降低门禁，未将全量发布标记为可交付。

五视角后审：架构、产品、UI/UX 通过；技术 A 的身份、拒绝、取消、版本冲突与费用保护通过；技术 B 的本次构建与受影响合同通过，全量 Fork/发布门禁仍有上述既有阻断。零兼容门禁 PASS，无双轨或旧历史 fallback。静态、合同及受控服务证明当前传输与拒绝，不证明真实供应商表现、文学质量、自动未读视觉交互或新传输资格；未调用付费模型、未重新打包 Electron、未推送远端。
