# Host Runtime IPC 透传通道拆分

## 实施前

目标：把 `electron/main.mjs` 中只做“IPC形状校验 → Runtime HTTP”的 `runtime:*` 通道移出组装入口，按领域放入独立的 `register*Handlers(ipcMain, request)` 模块；六类索引校验（search/entity/relation/knowledge/snapshot-knowledge/fact）的 18 个同构通道改为表驱动。沿用 `work-catalog.mjs`、`commitment-handlers.mjs` 已有模式。

非目标：不改通道名、参数顺序、校验条件、错误文案、Runtime路径、请求体或幂等键规则；不改 preload、`src/types.ts`、Renderer、Native路由；不动依赖 main 内部状态的通道（`list-work-tasks`、`get-work-task`、`list-generation-runs`、`cancel-planning-review` 等）。

归属：Electron Host（B01、B04）。Host负责受信IPC形状校验与Runtime调用；领域事实、权限和版本冲突仍由Native判定，本改动不新增事实权威、状态或准入。新模块只依赖 Node 内建与 `shared/author-input-budget.mjs`，经 main 注入 `runtimeRequest`，仍通过 main 的执行域包装器注册，`controlDomain`/`executionRequests.wrap` 路径不变。

五视角前审（同一执行者自审）：

| 视角 | 结论及依据 |
| --- | --- |
| 架构 | APPROVE：职责不变，仅降低组装入口体积；依赖方向为 main → handler 模块 → shared，符合B01。 |
| 技术A | APPROVE：校验逐条保留，表驱动只合并字面相同的审计通道；IPC安全边界不变。 |
| 技术B | APPROVE：一次性移动并删除 main 中原实现，无双轨；以现有 handler 合同测试验证经 main 注册后的真实行为。 |
| 产品 | APPROVE：无用户可见变化。 |
| UI/UX | APPROVE：错误文案与状态投影不变。 |

零兼容门禁：不新增兼容路径；被移动的实现从 main 删除。状态判定表不适用：不改变状态、权限或恢复。

验证计划：E1 `architecture:check`；E2 受影响 handler 测试（`agent-configuration`、`host-bindings` 及经 `main-handler-harness` 加载 main 的测试）；E3 `node --check` 与 `verify-preload-api`（通道集合不变）。

## 实施后

实际变更：`main.mjs` 2928 → 约 2650 行；61 个 `runtime:*` 通道移入 [project-artifact-handlers.mjs](../electron/project-artifact-handlers.mjs)（原样搬移）、[knowledge-proposal-handlers.mjs](../electron/knowledge-proposal-handlers.mjs)（原样搬移）与 [index-audit-handlers.mjs](../electron/index-audit-handlers.mjs)（18 个同构通道表驱动，校验条件、错误文案、路径和请求体不变）。main 中原实现已删除，无双轨。偏差：无。

| 编号 | 验证 | 结果 | 证明范围 |
| --- | --- | --- | --- |
| E1 | `npm run architecture:check` | 后审通过，0 诊断 | B01 依赖方向 |
| E2 | `verify-preload-api.mjs` | 后审通过，missing/undeclared 均为空 | 通道集合与 `NovelCodexApi` 一致 |
| E3 | 直接覆盖受影响通道的 7 个测试文件 | 改动前后均 49/51；2 个失败为既有问题（`list-work-tasks` 缺 `tasks`；长讨论预算 9011/9000），不在本次范围 | 经 main 注册与执行域包装后的真实 handler 行为 |
| E4 | 经 `main-handler-harness` 加载 main 的 44 个测试文件 | 改动后 267/297；30 个失败文件在“内联回 main”的对照版本上重跑结果完全相同（101/131，失败集合差集为空） | 本次改动未引入回归；既有 30 个失败为工作区未提交改动遗留，未处理 |

五视角后审（同一执行者自审）：架构 APPROVE（E1，依赖 main → handler 模块 → shared）；技术A APPROVE（校验逐条保留，E3/E4）；技术B APPROVE（一次性切换，E2/E4 对照）；产品 APPROVE（无可见变化）；UI/UX APPROVE（文案与状态投影不变）。零兼容门禁 PASS；B01–B10 边界复核 PASS。

剩余项：既有 30 个 Host 测试失败需单独排查（与本次无关）。
