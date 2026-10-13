# Rust 引擎迁移整体验收

日期：2026-09-23。环境：本地 macOS arm64，当前未提交工作区。

## 结论

本地自动化验收通过。首次应用回归发现 token 用量线协议类型错误，修复后重新执行所有受影响的 Rust、引擎及应用接入测试，全部通过。没有修改或放宽失败测试的断言。

| 检查 | 结果 |
| --- | --- |
| 独立 Rust workspace `just test` | 45/45，通过 |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | 通过，无警告 |
| 引擎 Node 回归 | 179/179，通过 |
| 真实 Codex Responses 原生/便携委派集成 | 2/2，通过 |
| Novel Codex Agent 接入回归 | 112/112，通过 |
| Electron UI 回归 | 5/5，通过 |
| 发布门禁测试 | 7/7，通过 |
| 小说 Python Runtime 回归 | 通过 |
| TypeScript 检查、Vite 生产构建 | 通过 |
| 上游分叉边界检查 | 通过，strictReady=true |

`npm run verify` 首次执行完成了构建、Runtime、引擎、委派与发布门禁，但在 Agent 接入阶段发现三个失败；修复后分别重跑 `just test`、引擎全套测试和 `npm run test:agent`，并完成 Clippy 检查。UI 是额外执行的独立套件。测试完成后按仓库要求执行最终格式化，不在格式化后重复测试。

## 验收中修复的问题

Chat Completions、Messages、Generate Content 转换产生的 Responses 完成事件将 token 用量序列化为 `20.0` 这样的浮点数。上游 Codex 的 `ResponseCompletedUsage` 及缓存/推理 token 明细要求整数，导致 `failed to parse ResponseCompleted: invalid number`，并触发流重连。

修复位置：`native/engine-core/src/canonical.rs`。统一用量计数输出为整数；无效计数不输出可信 usage。新增 `native/engine-core/tests/usage_wire_tests.rs`，使用整数类型消费者直接反序列化原生输出，覆盖输入、输出、总量和缓存明细，避免 JS Number 转换掩盖差异。现有真实内核集成测试同时验证模型→MCP→模型完整循环。

## 测试环境与限制

- Electron UI 首次在沙箱内启动时 SIGABRT；获准在沙箱外运行同一套件后 5/5 通过，没有更改 UI 实现或测试断言。
- 真实 Codex 集成使用本地受控供应商响应。它验证实际内核执行、子 Agent 创建、消息传递、等待、定向路由、MCP 权限边界和取消，不代表 DeepSeek 在线认证通过。
- 本次未运行 Windows/Linux 实机打包和安装验证，未执行新的 DeepSeek 在线付费认证。
- Vite 提示主 JS 包约 4 MB，超过建议分包阈值；Python 3.14 测试出现未关闭 SQLite 连接的 ResourceWarning。两者没有使对应检查失败，尚待各自模块专项处理。
- 引擎仍通过 Node-API 接入 Node/Electron；宿主回调、Promise 和测试夹具保留 JavaScript，不是无 Node 依赖的独立 Rust 可执行产品。

本轮未提交或推送代码；上游 codex-rs 未加入迁移逻辑。
