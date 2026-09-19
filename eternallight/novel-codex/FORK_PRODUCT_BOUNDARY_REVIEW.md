# Novel Codex Fork 产品边界设计评审

版本：0.1 · 2026-09-19
状态：实施完成，带一个外部配置条件通过

## 变更概览

- 目标：保持完整 Codex Git 历史和直接合并上游 `main` 的能力，同时将 `eternallight/novel-codex` 建设为独立构建、测试、打包、发布、认证和数据边界的新产品。
- 非目标：拆除 Codex 上游历史、复制一份失去来源关系的引擎代码、自动解决上游冲突、允许产品读取 Codex/ChatGPT 桌面状态。
- 影响模块：仓库远程约定、上游同步流程、产品 package/build 配置、独立 CI、边界验证脚本、架构文档。
- 验收标准：可以从 `upstream/main` 创建同步分支并直接 merge；产品差异默认限制在批准路径；macOS/Windows 可构建独立安装包；发布包携带固定摘要的引擎；产品运行边界继续与 Codex 桌面版隔离。

## 测试阶段零兼容门禁

- [x] 不保留独立仓库与 Fork 仓库双轨方案。
- [x] 不引入 submodule、subtree 或复制源码的第二套引擎来源。
- [x] 同步、构建、测试和文档一次性采用 Fork 单仓库结构。
- [x] 不改变当前 Runtime schema、IPC 或产品数据。

## 角色评审

### 架构专家

- 结论：`APPROVE WITH CONDITIONS`
- 模块边界：Codex 根目录是只读上游基线；Novel Codex 产品差异集中在 `eternallight/novel-codex`，根级只允许产品专用 CI 等少量集成文件。
- 条件：`origin` 必须是产品 Fork，`upstream` 必须是 OpenAI Codex；严格检查不得允许二者指向同一地址。
- 是否存在无授权兼容层：否。

### 技术专家 A

- 结论：`APPROVE WITH CONDITIONS`
- 可靠性与安全：同步必须从干净工作树开始，在独立 `codex-sync/*` 分支 merge，并在进入产品主分支前执行产品回归。
- 条件：上游合并脚本不得隐藏冲突、不得 reset 或自动覆盖用户修改。

### 技术专家 B

- 结论：`APPROVE WITH CONDITIONS`
- 实现与测试：提供可测试的 remote/差异路径验证；CI 以子项目为工作目录，不污染上游 Codex 发布流程。
- 条件：发布 Runtime 必须来自同一提交构建的 Codex CLI，并继续做版本与 SHA-256 校验。

### 产品经理

- 结论：`APPROVE`
- 用户价值：Novel Codex 保持独立品牌、安装包、账号和数据；“使用 Codex 内核”只属于技术实现，不转化为 Codex 桌面依赖。
- 范围：上游同步是研发能力，不在最终用户界面暴露 Git 或 Codex 仓库概念。

### UI/UX 设计师

- 结论：`APPROVE`
- 流程：本次不改变产品 UI；安装包名称、应用名称和系统数据目录必须继续显示 Novel Codex。
- 可访问性：无新增界面。

## 综合决策

- 最终结论：`APPROVE WITH CONDITIONS`
- P0/P1 阻断项：Fork 远程角色校验、同步分支门禁、独立发布配置和 CI 必须同时落地。
- 获得明确授权的兼容例外：无。
- 实施范围：Fork 边界脚本、同步脚本、产品打包配置、产品 CI、架构与操作文档。

## 实施后复核

- 实际变更：官方远程已改名为 `upstream`；新增 Fork 角色与产品差异路径验证、干净工作树同步脚本、独立产品 CI、macOS/Windows 打包配置、同提交引擎暂存与摘要校验、独立应用图标及操作文档。
- 与方案的偏差：产品 Fork URL 未提供，因此没有创建可写 `origin`；`repo:check:strict` 会持续阻止发布门禁通过，直到 `origin` 配置完成。没有为此增加临时 fallback。
- 构建与测试证据：`npm run verify` 通过（Runtime 116/116、Agent 72/72）；Electron UI 3/3；`just fmt` 通过；workflow YAML 解析通过；`electron-builder --dir` 成功生成 `Novel Codex.app`，确认 `CFBundleIdentifier=com.eternallight.novelcodex`、产品名称、图标和私有 Runtime 资源。
- 零兼容门禁复查：`PASS`。只有 Fork 单仓库方案；没有 submodule、subtree、复制内核或独立源码仓库双轨。
- 架构复核：`APPROVE WITH CONDITIONS`，远程角色边界、同步分支和差异路径门禁符合方案；待配置可写产品 `origin`。
- 技术复核：`APPROVE`，同步脚本不 reset、不 stash、不覆盖，冲突保留为标准 merge 状态；发布内核从同提交构建并做版本和摘要检查。
- 产品与 UX 复核：`APPROVE`，安装包、应用 ID、品牌、数据和模型边界独立，上游 Git 概念未进入产品 UI。
- 剩余项：提供产品 Fork URL 后执行 `git remote add origin <URL>`、让本地 `main` 跟踪 `origin/main`，并运行 `npm run repo:check:strict`。正式分发还需要配置 Apple/Windows 签名证书，这属于发布凭据而不是代码阻断项。
