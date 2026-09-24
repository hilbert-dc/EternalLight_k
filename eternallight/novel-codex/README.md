# Novel Codex

Novel Codex 是面向长篇小说创作的独立桌面产品。它在完整 Codex Fork 单仓库中开发，以便直接同步上游 Agent 内核；产品运行时不依赖 Codex 桌面应用的登录、配置、会话、审批或模型额度。

总体技术方案统一为 [Novel Codex 理想架构](IDEAL_ARCHITECTURE.md)：围绕现有独立 Agent Engine 重构小说领域层，形成故事数据与版本、知识服务、创作能力、创作编排、作者交互五层。系统自动生成小说，作者通过对话指导和调整；该方案尚未全面实施。

数据与行为细则见 [对话式创作与全局故事知识推理架构](STORY_SYSTEM_ARCHITECTURE.md)，覆盖关系、演进、证据、全局检索、实时建模与一致性。

创作技法与专家能力的来源、接入边界、实施顺序和验收见 [EternalLight 创作能力吸收与接入方案](CREATIVE_CAPABILITY_ADOPTION_DESIGN.md)，不直接复制源工程的运行框架。

小说领域运行时已经迁移为独立 Rust Story Core：SQLite 是唯一事实源，Rust HTTP 宿主和 Rust MCP sidecar 承担生产调用，Electron 不再依赖 Python 领域实现。设计与验证边界见 [理想架构](IDEAL_ARCHITECTURE.md)。

## 本地开发

```bash
npm install
npm run build:codex-runtime
npm run refresh:model-policy
npm run dev
```

前两条构建并记录当前 `codex-rs` 源码对应的 BuildIdentity；源码、锁文件、工具链或二进制变化后必须重新执行，旧产物不会被开发启动器接受。

## 验证

```bash
npm run verify
npm run test:ui
npm run qualify:model -- <connection-id> --profile directed-delegation-v1
```

`repo:check` 验证产品提交没有污染 Codex 上游实现路径。发布和保护分支应使用 `repo:check:strict`，要求 `origin` 指向产品 Fork、`upstream` 指向 OpenAI Codex。

`qualify:model` 只执行真实协议、Codex Core 与多 Agent 场景并生成诊断报告，不改动发布策略。确认所有场景通过后，发布人员才运行 `npm run certify:model -- <connection-id> --profile <profile-id>` 写入认证与策略摘要；任一场景失败都禁止发布。

## 独立安装包

发布前先从当前 Git 提交构建 `codex-cli`，再把固定版本和摘要写入产品资源：

```bash
NOVEL_CODEX_CARGO_PROFILE=release npm run build:codex-runtime
rustup target add <target-triple>
cargo build --manifest-path native/Cargo.toml --release --bins --target <target-triple>
NOVEL_CODEX_RELEASE_CLI=../../codex-rs/target/release/codex \
NOVEL_CODEX_RELEASE_RUNTIME_SERVER="$PWD/native/target/<target-triple>/release/novel-codex-story-runtime" \
NOVEL_CODEX_RELEASE_MCP_SERVER="$PWD/native/target/<target-triple>/release/novel-workspace-mcp" \
NOVEL_CODEX_RELEASE_RESOURCES="$PWD/.release/resources" \
npm run package:mac
```

Windows 在对应主机上用 `NOVEL_CODEX_CARGO_PROFILE=release npm run build:codex-runtime` 生成该平台 BuildIdentity，再使用三个 `.exe` 路径和 `npm run package:win`。所有 `package:*` 命令都会重新执行 staging；缺少匹配当前源码的 BuildIdentity、CLI、Creative Runtime、MCP、Skills、角色、上游指令或完整性清单时立即失败。GitHub Actions 的 `novel-codex-ci` 手动任务仅在完整质量门禁通过后，才会在 macOS 与 Windows 分别执行这一流程。

上游远程配置和直接 merge 流程见 [UPSTREAM_SYNC.md](UPSTREAM_SYNC.md)，运行架构见 [ARCHITECTURE.md](ARCHITECTURE.md)。

当前引擎实施依据：[多模型 Codex 引擎基础层技术方案](../agent-engine/ENGINE_FOUNDATION_TECHNICAL_DESIGN.md)，优先建设模型适配和通用执行能力。问题分析及上游同步背景见 [多模型准入与 Codex 上游持续合入重构方案](MODEL_ADMISSION_AND_UPSTREAM_REFACTOR_PLAN.md)。
