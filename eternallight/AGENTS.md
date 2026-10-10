# 创作产品工作区规则

对 `novel-codex` 与 `agent-engine` 的代码、配置、接口和UI改动，先阅读并遵守 [架构规则](ARCHITECTURE_RULES.md)。该文件是模块职责和跨层边界的唯一规则正文，B01–B10适用于新增和修改文件。

- 改动前说明责任模块、事实权威、公开契约、受影响调用方及验证；执行产品现有五视角前后评审，一个连贯改动使用一份记录，精简写法见[评审模板](novel-codex/DESIGN_REVIEW_TEMPLATE.md)。
- 模型决定创作组织；工程保证身份、权限、契约、事务、用量及恢复；不得互相代替。
- 生产代码验证运行 `npm --prefix novel-codex run architecture:check`（从本目录）；测试按影响范围选择，不能以测试数量代替交付。
- 不把现存越界写成豁免。规则变更与边界例外必须显式评审，不隐式扩大任何权限，也不自动增加用户确认。
- 当前测试阶段遵循 [零兼容策略](novel-codex/TEST_STAGE_ENGINEERING_POLICY.md)；产品对Codex Core的改动继续走既有上游补丁登记和Fork门禁。
