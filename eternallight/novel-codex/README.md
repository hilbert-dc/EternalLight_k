# Novel Codex

Novel Codex 是以小说、漫画与动画持续创作为目标的独立桌面产品。它在完整 Codex Fork 单仓库中开发，以便直接同步上游 Agent 内核；产品运行时不依赖 Codex 桌面应用的登录、配置、会话、审批或模型额度。

所有代码改动必须遵守 [创作工程架构规则 B01–B10](../ARCHITECTURE_RULES.md)：模块职责、事实权威、工具契约、模型决策空间及验证标准。规则通过工作区/产品AGENTS、[改动评审模板](DESIGN_REVIEW_TEMPLATE.md)、内部依赖检查和产品CI执行；依赖检查通过不代表真实创作已经验收。

总体技术方案见 [独立创作应用理想技术方案](IDEAL_ARCHITECTURE.md)：围绕现有 Agent Engine、Rust Runtime 和 SQLite，管理作品与素材、可编辑制作结构、原作读取、按需语义分析及可靠执行。作者可从创意、已有作品或局部问题进入，在授权范围内生成、调整并继续，无需预先完成全书或全片建模。该目标方案尚未全面实施。

对话中的启动、停止、补充要求、临时插问、跨对话执行与恢复见 [对话任务启动停止与临时切换设计](CONVERSATION_TASK_CONTROL_DESIGN.md)。方案复用现有 Work 与 Engine，定义目标合同、实施顺序和验收矩阵；尚未实施，不表示当前已支持多任务并发。

多媒介扩展目标见 [小说漫画与动画创作理想架构](MULTIMODAL_CREATION_ARCHITECTURE.md)：同一应用支持独立创建小说、漫画和动画，并可建立跨媒介改编关系。文档定义共同底座、专属编辑器、素材与版本、可靠生成及分阶段验收；部分能力已有实现，当前交付与未验收范围以 [多媒介实施记录](MULTIMODAL_CREATION_IMPLEMENTATION.md) 为准。

后续投入顺序见 [独立创作应用演进路线](CODEX_CREATIVE_EVOLUTION_ROADMAP.md)：先明确按操作准备与交付条件，验证小说修改接续，再推进漫画和动画的真实制作、局部编辑与交付。必要方法和外部服务随任务接入，Codex 插件暂不排期；建模、上下文与协作投入通过对照验证，不替代既有架构和验收记录。

产品以“调整后仍能准确、可靠地继续”为核心目标。“一句创意生成三章”只是其中一个测试场景。小说沿 [持续创作验收计划](CONTINUOUS_CREATION_ACCEPTANCE.md) 与 S0–S5、C01–C10 验证；漫画和动画沿 [多媒介实施记录](MULTIMODAL_CREATION_IMPLEMENTATION.md) 与 M0–M4 验证，不以章节数、媒体文件数量或自动测试数量宣称整体完成。

数据与行为细则见 [对话式创作与全局故事知识推理架构](STORY_SYSTEM_ARCHITECTURE.md)，覆盖关系、演进、证据、全局检索、实时建模与一致性。

创作技法与专家能力的来源、接入边界、实施顺序和验收见 [EternalLight 创作能力吸收与接入方案](CREATIVE_CAPABILITY_ADOPTION_DESIGN.md)，不直接复制源工程的运行框架。

小说领域运行时已经迁移为独立 Rust Story Core：SQLite 是唯一事实源，Rust HTTP 宿主和 Rust MCP sidecar 承担生产调用，Electron 不再依赖 Python 领域实现。设计与验证边界见 [理想架构](IDEAL_ARCHITECTURE.md)。

## 选中内容与智能体讨论

作品内的简报、大纲、章纲、章节正文、人物、世界规则、伏笔、时间线及版本阅读内容，都可划选后点击“加入对话”，或按 `⌘/Ctrl+L`。阅读区与编辑框使用同一入口，受保护的正文也可以引用。

引用会显示在对话输入框上方，可展开原文、移除或添加多段，不覆盖已经输入的问题。每轮最多 8 段，总计 12,000 字符，并受字节与 token 预算限制；超限会提示重新选择，不截断原文。来源、版本与未保存状态随引用发送；跨作品草稿隔离，失败重试仍使用原始引用。

未保存的编辑内容无需先保存即可讨论。这类对话只读，不生成候选修改或后续创作，不覆盖本地编辑；要应用修改，应先保存再明确提出修改要求。此功能覆盖工程内可见的作品文本，不是操作系统任意文件或二进制文件浏览器。

## 本地开发

```bash
npm install
npm run build:codex-runtime
npm run refresh:model-policy
npm run dev
```

前两条构建并记录当前 `codex-rs` 源码对应的 BuildIdentity；源码、锁文件、工具链或二进制变化后必须重新执行，旧产物不会被开发启动器接受。

## 验证

日常修改先根据影响范围运行边界检查与相关验证；共享合同和发布再执行完整回归，避免重复运行未受影响的全量检查。

```bash
npm run architecture:check
npm run test:architecture
```

`architecture:check`扫描当前生产JS/TS依赖及Cargo生产依赖，包含新增文件、类型导入与可静态解析的加载；不执行模型请求，不修改作品库。语义职责通过评审和对应合同测试检查。

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
