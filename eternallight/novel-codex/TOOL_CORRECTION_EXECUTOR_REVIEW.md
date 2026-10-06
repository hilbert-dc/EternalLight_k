> 当前覆盖已扩展到全部 29 个 bundled 工具；以下单工具描述是首阶段记录。最终当前契约、范围与测试见 [CURRENT_REVISION_CONTRACT_REVIEW.md](CURRENT_REVISION_CONTRACT_REVIEW.md)。

# 受信执行器参数纠错评审与验收

日期：2026-10-06。角色：架构专家与技术专家 A。范围：Codex 同轮工具参数纠错、执行器校验边界、纠错次数上限与 host 终止。

## 架构结论

采用宿主显式配置的 `executorValidatedTools` 是本次合适的最小改动。名单只启用 `novel_workspace.publish_knowledge_batch` 的精确身份及 Codex 实际注册形态。桥接层保留工具授权、tool choice、JSON 解析、大小、调用 ID 和流提交校验，仅把已委托工具的 JSON Schema 校验交由原生 MCP 在业务 HTTP 之前完成。

MCP 使用原始完整 schema 校验并返回真实 `isError` 与有界 `structuredContent`，让 Codex 在原始调用 ID 对应的历史中看到拒绝并纠正。三次连续 schema 错误后，MCP 先锁死后续业务调用，host 再根据当前 thread/turn 与可信 server/tool 的结构化终止码结束执行。前者堵住同一模型批次中后续有效写入抢先执行的竞态；后者复用已有 managed turn 的进程退出与上层用量结算路径。

不能仅依赖 MCP 文字提示终止，也不能把所有工具加入名单、取消 Runtime 鉴权、为纠错重置预算或在 bridge 增加第二套智能体循环。

## 已实现的 host 处理

- `electron/tool-validation-recovery.mjs` 只识别当前执行中 `novel_workspace.publish_knowledge_batch` 的 `item/completed`、`status: failed` 与精确终止码。
- `electron/codex-adapter.mjs` 复用 `fail()`，由 `executeManagedTurn` 关闭真实 app-server。普通 schema 错误仍留给 Codex 同轮处理。
- 普通错误文字、其他服务器/工具、旧 thread/turn 和未达到上限的错误均不触发这一终止逻辑。

## 验证结果

### 宿主与原生 MCP

- 原 adapter 回归 24 项通过。
- `electron/tests/tool-validation-recovery.test.mjs` 3 项通过，覆盖身份绑定、普通错误不终止、达到上限后真实 fixture 子进程退出且不启动替代回合。
- 原生 MCP 全套 35 项通过，包含 `correction_limit_blocks_later_valid_writes_and_reads_before_runtime_dispatch`：同一 RuntimeClient 缺少 `value` 三次后，后续合法 publish 与 read_context 都返回终止码，loopback 接收器记录零业务 HTTP 连接。日志 `/tmp/novel-executor-mcp-tests.log`。

### 实际 Codex 集成

测试：`electron/tests/knowledge-correction-engine.integration.test.mjs`。

使用实际打包 Codex、原生 bridge、原生 MCP 和原 TaskBudget；供应商与 Runtime HTTP 接收器是受控 fixture。该验收证明执行协议闭环，不代表 DeepSeek 真实模型的自主纠错能力，也不替代 Runtime 持久化业务验收。

最终重跑结果：**4 通过，0 失败，0 跳过**，总耗时约 3.12 秒。日志 `/tmp/novel-knowledge-correction-engine-final.log`。

| 协议 | 场景 | 已验证结果 |
| --- | --- | --- |
| Responses stream | 缺字段后纠正 | 首次零 HTTP；下次模型输入含原调用 ID 与完整必填字段提示；纠正后仅一次 publish |
| Responses stream | 连续三次缺字段 | 三次真实 MCP 错误；达到上限后 host 关闭 app-server；零 HTTP |
| Chat Completions stream | 缺字段后纠正 | 与 Responses 相同，同一 Codex 回合完成 |
| Chat Completions stream | 连续三次缺字段 | 与 Responses 相同，终止码实际经 Codex 事件传递 |

每例断言只有一个原始会话与一次 turn/start；原预算记录 3 次供应商请求、15 个 fixture 用量 token、0 未知用量、0 未结算预留，不分配新预算。纠正后的工具调用与错误输出保留在真实 Codex rollout。

首次运行的第一个场景因新版原生程序冷启动导致 thread/start 超过 90 秒，未到供应商交互阶段；其余 3 项通过。待启动完成后重跑全部 4 项通过。未修改生产启动上限，未把首次超时隐藏为成功。

运行方式：

```sh
PATH=/opt/homebrew/opt/node@22/bin:$PATH \
NOVEL_CODEX_TEST_RESOURCES=/Users/hilbert/private/codex/eternallight/novel-codex/.release/resources \
node --test electron/tests/knowledge-correction-engine.integration.test.mjs
```

`NOVEL_CODEX_TEST_MCP` 可指定独立构建的 MCP；上述最终通过使用标准 `native/target/debug/novel-workspace-mcp`，没有使用未生成的临时二进制。

## 验收边界

非流式 codec 与桥接协议回归由主任务单独记录。以上 4 例不代表真实 DeepSeek 自动纠错能力，不宣称整个任务无限自动恢复；其他未委托工具仍保持原桥接层拒绝机制。推广名单前，每个执行器都必须具备等价的完整前置校验、授权、无副作用拒绝和有界停止机制。

## 真实 DeepSeek 局部改稿验收补充

主任务在独立应用界面发起实际 `deepseek-flash` 创作后，本评审者以 SQLite `mode=ro` 独立核对持久化结果，没有修改数据库。

- Work：`work-task-4662992a6ecc786b17002287543df4db`，最终 completed，error 为 null。
- 三个步骤为 `advisory → generateChapter → advisory`，均使用 `deepseek-flash` 且 completed；没有 `modelKnowledge` 步骤。
- 当前正文 `draft-b4b12ccb4f534f71b0caa6b301f47ce6` 的直接父版本为 `draft-4d604a334c0d4867bf8cf151215e97bb`。
- 精确断言成立：`old.body.replace("节奏均匀得像被谁拧着开关。", "明灭的节奏均匀，像有人在拨动开关。") === new.body`。旧句和新句各自只有一次匹配；正文从 863 到 867 个非空白字符（含标点）。
- 标题、场景元信息、场景顺序与锁定、规划引用等保持原样。变化仅为正文、场景中对应正文及内部版本号。
- 新正文版本没有 modeling batch；当前章没有建模 outbox；项目没有活动任务。作品仍只有一章《无人寄出的信》，状态为 draft，未发布。

该真实模型样本验证了“局部措辞修改按需完成，不强制启动设定建模”的创作流程。本次没有主动制造 DeepSeek 参数错误，因此不把它表述为真实模型纠错稳定性测试。
