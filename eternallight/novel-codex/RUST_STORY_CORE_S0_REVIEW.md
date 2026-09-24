# Rust Story Core S0 设计与实施复核（历史阶段记录）

> 本文记录 S0 初期门禁，不再代表当前状态。生产迁移、Rust HTTP/MCP、Electron 切换和 Python 删除均已完成；当前状态见 [理想架构](IDEAL_ARCHITECTURE.md#9-本次交付状态)。

日期：2026-09-24 · 范围：首个可审阅切片。本文为单实施者按项目模板执行的角色视角检查，不代表召开了多人评审会议。

## 变更概览

- 目标：建立独立 Rust Story Core，先承接稳定身份、不可变修订、快照、分支 CAS、幂等提交和补偿式撤销。
- 非目标：本切片不接 SQLite、Node/Electron、MCP 或 UI，不切换 Python 生产写入，不迁移知识图、自动创作或创作方法。
- 影响模块：`native/story-core`、验证脚本及理想架构文档。
- 当前验收：Rust 测试和 Clippy 通过；不得据此宣布 S0 或领域迁移完成。

## 测试阶段零兼容门禁

- [x] 未新增旧字段 fallback、双读双写、升级、回填、降级或 deprecated 路径。
- [x] 新 crate 不依赖旧 Python schema 或调用方。
- [x] 尚未切换接口，因此没有需要保留的旧 Rust API。
- [x] 数据一致性通过 stale snapshot、parent revision、跨作品和幂等冲突测试。
- [x] 跨平台只使用标准库；后续 SQLite/Node 绑定需另行验证 macOS 与 Windows。

## 角色视角复核

### 架构专家

- 结论：`APPROVE WITH CONDITIONS`
- Story Core 与通用 agent-engine 独立，依赖方向正确；后续持久化和 authoring 仍留在产品域。

## S1 进展（2026-09-24）

- `SqliteStoryRepository` 已将项目创建、初始分支、不可变修订、快照成员、分支 CAS、提交回执、幂等重放和补偿撤销放入 SQLite 单事务。
- 修订正文以规范 JSON 随修订持久化，不再只有摘要。
- 新增独立 `novel-codex-story-runtime` Rust 可执行程序，采用与现有 Runtime 相同的回环地址、stdin 凭据和 Bearer author 边界；当前已覆盖健康检查及项目创建/列表。
- 候选产物写入已经进入 Rust：候选头 CAS、分支快照推进、不可变内容、请求摘要与精确幂等回执在单事务内完成；Rust HTTP 已覆盖工作区读取与 brief/outline/plan 作者候选保存。
- Rust 已覆盖草稿检查点、章节与草稿原子创建、审阅、正式采用和仅最新提交可撤销；HTTP 端到端测试覆盖 `创建作品 → 候选 → 审阅 → 采用 → 撤销 → 工作区回读` 以及草稿回读。
- Rust Runtime 尚未达到 HTTP/MCP 契约全覆盖，因此 Electron 仍使用 Python Runtime。这是迁移门禁，不是长期双轨设计。
- 切换条件：现有 Runtime 契约测试移植并全部通过、MCP 写入回执保持原子性、发行脚本打包 Rust 二进制、Electron 默认启动 Rust、随后删除已替换 Python 模块。
- 条件：生产切换前必须加入 SQLite 仓储和唯一宿主绑定，禁止 Python/Rust 双权威。

### 技术专家 A

- 结论：`APPROVE WITH CONDITIONS`
- 先完整校验后提交内存状态，错误不推进 head；幂等键绑定 request digest。
- 条件：当前是内存参考实现，不具备崩溃恢复；SQLite 事务和故障注入是下一切片阻断项。

### 技术专家 B

- 结论：`APPROVE WITH CONDITIONS`
- 公共 API 小，模块均低于 500 行；集成测试覆盖主要状态转换。
- 条件：只允许撤销当前最新提交是明确的 S0 限制；支持任意历史撤销前必须建模后续依赖冲突。

### 产品经理

- 结论：`APPROVE`
- 本切片不改变用户体验，但建立对话改稿、采纳和撤销所需的可靠版本基础。
- 不宣称已带来小说生成质量提升。

### UI/UX 设计师

- 结论：`APPROVE WITH CONDITIONS`
- 本切片无界面变化。
- 条件：接入时需分别展示工作稿、已确认版本、过期结果和撤销回执，不能暴露内部 ID 作为主要文案。

## 综合决策

- 最终结论：批准 S0 领域状态机进入工程；阻止将其描述为生产切换完成。
- P0/P1 阻断项：生产接入前完成 SQLite 原子仓储、Node 宿主边界、现有调用方切换和故障恢复测试。
- 兼容例外：无。
- 变更规模：首个复杂逻辑切片集中在一个 500 行以下实现模块；测试和类型定义独立。后续持久化另做切片。

## 实施后复核

- 实际变更：新增无第三方依赖的 Rust workspace/crate；加入 npm 验证入口和五项集成测试。
- 与方案偏差：尚未接通生产“草稿候选 → 采用 → 撤销”路径，属于计划中的后续 S0 切片。
- 验证：`cargo test --manifest-path native/Cargo.toml`、`cargo clippy --manifest-path native/Cargo.toml --workspace --all-targets -- -D warnings`。
- 零兼容门禁：`PASS`。
- 剩余项：SQLite 仓储、宿主绑定、调用方切换、故障注入和双平台验证。
