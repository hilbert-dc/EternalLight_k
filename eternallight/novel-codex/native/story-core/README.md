# Story Core 存储边界

Story Core 是作品事实、当前版本、授权、来源、预算和事务的唯一权威。目录按领域组织；SQLite 是共同的存储适配器，不是让调用方默认获得全部业务方法的服务接口。

## 生命周期与能力

- `StoryDatabase` 启动时校验并初始化当前 schema，随后借出有限数量的连接。归还时清理未完成事务；每次借用创建新的调用身份上下文。
- `SqliteStoryRepository` 是一次借用的连接/事务上下文。固有公开接口仅用于创建上下文与取得已有借用式 store。
- 领域用例在各自模块的窄 `*Repository` trait 中声明。调用方显式导入所用能力，例如 `TasksRepository`、`WorkBudgetAuthorizationRepository`、`ChapterHistoryRepository`；不建立导入所有能力的 prelude。
- 已有 `CandidateStore`、`ReviewStore`、`WorkExecutionStore`、`TaskIntentStore`、`ConversationStore`、`QualityStore` 继续通过借用限制操作范围，不能再加回同名的固有业务方法作为捷径。
- 跨域用例显式导入所用能力，并保持原事务边界。不能因为接口拆分而把一个原子操作拆成多次连接借用，也不能在 Host/Runtime 复制领域决定。

Runtime 的 `repository_executor` 在异步准入后运行阻塞工作。不要在 Tokio worker 上打开或调用同步 SQLite，不要持有数据库借用等待网络。

## 新增与修改

优先在现有用例所属能力上增加必要操作；只有实际独立职责才新增能力。实现直接持有领域校验与事务，避免委托到另一个隐藏的全能力对象。响应的稳定传输字段使用具名 serde DTO，从 Rust 生成 TS；任意作者内容与协议封套分开。

`repository_capabilities_tests` 扫描实际 Rust AST，拒绝在连接上下文上重新增加公开固有业务方法。它只验证静态边界；授权、来源、CAS、回滚、取消及恢复仍需对应单元和 HTTP 集成验证。

目录和能力拆分不等于 crate 级独立编译。保持跨域事务正确性，不为未经测量的编译收益人为切开同一事务。
