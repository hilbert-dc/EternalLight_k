# Novel Codex

Novel Codex 是以小说、漫画与动画持续创作为目标的独立桌面产品。它在完整 Codex Fork 单仓库中开发，以便直接同步上游 Agent 内核；产品运行时不依赖 Codex 桌面应用的登录、配置、会话、审批或模型额度。

所有代码改动必须遵守 [创作工程架构规则 B01–B10](../ARCHITECTURE_RULES.md)：模块职责、事实权威、工具契约、模型决策空间及验证标准。规则通过工作区/产品AGENTS、[改动评审模板](DESIGN_REVIEW_TEMPLATE.md)、内部依赖检查和产品CI执行；依赖检查通过不代表真实创作已经验收。

总体技术方案见 [独立创作应用理想技术方案](IDEAL_ARCHITECTURE.md)：围绕现有 Agent Engine、Rust Runtime 和 SQLite，管理作品与素材、可编辑制作结构、原作读取、按需语义分析及可靠执行。作者可从创意、已有作品或局部问题进入，在授权范围内生成、调整并继续，无需预先完成全书或全片建模。该目标方案尚未全面实施。

创作准备与跨媒介改编统一方案见[主 agent 与改编设计](design-reviews/2026-10-12-creation-intake-and-book-planning-design.md)，实际落地及验证见[实施记录](design-reviews/2026-10-12-creation-implementation.md)。已接入按任务深度准备的主指令、Default 可选提问、作者回答的持久来源与独立投递状态，以及任务内冻结来源变化查询、改编用途和说明。它们不等于全自动长篇、完整改编依赖调度或电影成片已验收。

对话中的启动、停止、补充要求、临时插问、跨对话执行与恢复见 [对话任务启动停止与临时切换设计](docs/designs/CONVERSATION_TASK_CONTROL_DESIGN.md)。方案复用现有 Work 与 Engine，定义目标合同、实施顺序和验收矩阵；尚未实施，不表示当前已支持多任务并发。

多媒介扩展目标见 [小说漫画与动画创作理想架构](docs/designs/MULTIMODAL_CREATION_ARCHITECTURE.md)：同一应用支持独立创建小说、漫画和动画，并可建立跨媒介改编关系。文档定义共同底座、专属编辑器、素材与版本、可靠生成及分阶段验收；部分能力已有实现，当前交付与未验收范围以 [多媒介实施记录](docs/designs/MULTIMODAL_CREATION_IMPLEMENTATION.md) 为准。

后续投入顺序见 [独立创作应用演进路线](docs/designs/CODEX_CREATIVE_EVOLUTION_ROADMAP.md)：先明确按操作准备与交付条件，验证小说修改接续，再推进漫画和动画的真实制作、局部编辑与交付。必要方法和外部服务随任务接入，Codex 插件暂不排期；建模、上下文与协作投入通过对照验证，不替代既有架构和验收记录。

产品以“调整后仍能准确、可靠地继续”为核心目标。“一句创意生成三章”只是其中一个测试场景。小说沿 [持续创作验收计划](docs/designs/CONTINUOUS_CREATION_ACCEPTANCE.md) 与 S0–S5、C01–C10 验证；漫画和动画沿 [多媒介实施记录](docs/designs/MULTIMODAL_CREATION_IMPLEMENTATION.md) 与 M0–M4 验证，不以章节数、媒体文件数量或自动测试数量宣称整体完成。

数据与行为细则见 [对话式创作与全局故事知识推理架构](docs/designs/STORY_SYSTEM_ARCHITECTURE.md)，覆盖关系、演进、证据、全局检索、实时建模与一致性。

创作技法与专家能力的来源、接入边界、实施顺序和验收见 [EternalLight 创作能力吸收与接入方案](docs/designs/CREATIVE_CAPABILITY_ADOPTION_DESIGN.md)，不直接复制源工程的运行框架。

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

日常修改执行 `npm run verify:affected -- --base <比较提交>`；加 `--dry-run` 可查看受影响文件及实际门禁。它合并提交差异、已暂存、未暂存和未追踪文件，Core 变化覆盖全部 Native 依赖和关键 Host 控制，Shared 变化同时覆盖双端，未知生产路径回落完整验证。PR CI 使用这一入口并始终保留架构正负例、Fork、发布、委派和评估门禁；main、手动运行及每日 02:23（上海）nightly 执行完整 `verify:nightly`。

`test:story-core` 和 `test:runtime` 各测对应 crate；`test:native` 一次覆盖整个 Native workspace。完整 `verify` 不再重复运行相同的 workspace 套件。

HTTP 具名 serde DTO 的生成类型位于 `shared/generated`。修改 Rust DTO 后执行 `npm run generate:runtime-contracts`；`check:runtime-contracts` 重新生成到临时目录，检查遗漏和漂移，并运行真实 HTTP 合同回归。动态 `serde_json::Value` 仍按 JSON 值表达，其领域合法性由 Core 校验；生成不替代运行时输入验证。

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
npm run build:media-runtime
NOVEL_CODEX_RELEASE_CLI=../../codex-rs/target/release/codex \
NOVEL_CODEX_RELEASE_RUNTIME_SERVER="$PWD/native/target/<target-triple>/release/novel-codex-story-runtime" \
NOVEL_CODEX_RELEASE_MCP_SERVER="$PWD/native/target/<target-triple>/release/novel-workspace-mcp" \
npm run package:mac
```

`build:media-runtime` 当前支持本机 macOS，要求 Xcode 命令行工具、make 和 pkg-config；源码、字体和许可证按 `scripts/media-runtime-sources.json` 的固定 URL 与 SHA256 获取。合格组件输出到 `.release/media-runtime/<platform>-<arch>`，打包默认读取该目录；失败保留上次合格组件，报告位于 `.release/media-build-report.json`。其他平台或外部构建须显式提供 `NOVEL_CODEX_RELEASE_MEDIA_RUNTIME`，仍接受相同发布校验。详见 [媒体交付](MEDIA_DELIVERY.md)。

Windows 在对应主机上用 `NOVEL_CODEX_CARGO_PROFILE=release npm run build:codex-runtime` 生成该平台 BuildIdentity，再使用三个 `.exe` 路径和 `npm run package:win`。所有 `package:*` 命令都会重新执行 staging；缺少匹配当前源码的 BuildIdentity、CLI、Creative Runtime、MCP、Skills、角色、上游指令或完整性清单时立即失败。GitHub Actions 的 `novel-codex-ci` 手动任务仅在完整质量门禁通过后，才会在 macOS 与 Windows 分别执行这一流程。

上游远程配置和直接 merge 流程见 [UPSTREAM_SYNC.md](UPSTREAM_SYNC.md)，运行架构见 [ARCHITECTURE.md](ARCHITECTURE.md)。

当前引擎实施依据：[多模型 Codex 引擎基础层技术方案](../agent-engine/ENGINE_FOUNDATION_TECHNICAL_DESIGN.md)，优先建设模型适配和通用执行能力。问题分析及上游同步背景见 [多模型准入与 Codex 上游持续合入重构方案](docs/designs/MODEL_ADMISSION_AND_UPSTREAM_REFACTOR_PLAN.md)。

## 构建产物目录

最终应用和安装包只从 `release/` 交付。运行 `npm run package:dir`、`package:mac` 或 `package:win` 后，脚本在 `.release/package` 中打包并执行应用启动检查，通过后替换 `release/`。失败保留上一份交付；本次运行阶段和失败原因写入 `.release/package-report.json`。不要将 `.release` 中的候选当作已验证交付。

| 目录 | 职责 |
| --- | --- |
| `release/` | 唯一交付出口，只保留最近一次成功打包；`package-report.json`记录检查结果、来源和逐文件摘要 |
| `.release/` | 可重新生成的资源、打包候选和运行报告；资源每次从空目录生成，通过后替换，不叠加旧资源 |
| `dist/` | Vite前端产物，由前端构建重建 |
| `native/target/`、`../agent-engine/target/`、`../../codex-rs/target/` | 三个独立Rust工作区的编译和增量缓存，保留各自配置与现有产物发现路径 |
| `../agent-engine/native/bin/` | 构建脚本生成的原生模块加载位置 |

`npm run verify:package`核对当前`release/`的内容是否仍匹配通过打包验证时的摘要；报告不代表完整质量认证。验证状态不用目录名表达，不再创建`release-fresh`、`release-ready`、`release-verified`或`release-conversation`。输出和暂存路径固定，`NOVEL_CODEX_RELEASE_OUTPUT`及`NOVEL_CODEX_RELEASE_RESOURCES`已移除。

同时启动第二次打包会被锁拒绝。进程异常终止时，先确认锁文件中记录的PID已经结束，再移除`.release/package.lock`（资源构建对应`.release/resources.lock`）重试；替换过程中保留的上一份产物会在下次构建时恢复或清理。正常失败自动释放锁。

需要历史版本时，将通过验证的完整`release/`连同报告归档到项目之外的制品存储，以版本号和提交编号命名；日常构建不自动复制多份应用。

### 模型容量与用量原则

Token 用于完成作者任务，不以固定小额度替代质量要求。Engine 按已核实连接能力校验包含历史、工具、输入和输出预留的完整请求；Shared/Host/Native 不再另设 7,000/9,000 token 准入。作者输入仍有字符/字节保护，Host 完整回合为 256 KiB 传输保护，Native 工具结果按转义后的实际字节校验；按需分页与原文定位保留，必需正文不得悄悄裁剪。Core 工具回传容量与单次生成输出额度分离。

当前 DeepSeek 容量在 2026-10-11 经官方 `/models` 核对为 context 1,048,576 / output 393,216 token，并纳入连接目录及资格指纹。最大容量不是消耗目标；不为了省 token 重复失败、遗漏约束或降低交付标准。累计用量、未知费用与请求预留继续记录；作者明确设置的总预算仍强制执行，恢复沿用原 Work。减少浪费依靠按需读取、去除重复表示、复用回执和有依据的恢复，不盲目重试。

实现与验证：[统一容量评审](design-reviews/2026-10-11-unified-capacity.md)。

离线核查重复上下文可运行 `node scripts/context-return-audit.mjs .release/context-return-audit.json --from <ISO时间> --to <ISO时间> <rollout.jsonl> ...`。时间范围为左开右闭，省略时间则分析整个日志。报告区分相同参数调用、相同返回文本、验收标准字段，以及同回合/压缩后/跨回合/未知边界；各统计存在重叠，不得累加或换算成token节费。日志只读，不调用模型，报告不保存正文或参数。供应商累计用量另由 `scripts/context-efficiency-report.mjs` 分析。设计与实际样本：[上下文返回审计](design-reviews/2026-10-11-context-return-audit.md)。

## 文档入口与历史记录

当前规范从 [架构规则](../ARCHITECTURE_RULES.md)、[理想架构](IDEAL_ARCHITECTURE.md)、[测试阶段策略](TEST_STAGE_ENGINEERING_POLICY.md)及[评审模板](DESIGN_REVIEW_TEMPLATE.md)开始阅读。已完成或明确被替代的阶段记录见[归档索引](archive/reviews/README.md)，归档依据逐项登记；未确认完成的专题方案进入 [docs 设计索引](docs/README.md)，评审进入 [design-reviews](design-reviews/README.md)，保留原状态并记录旧路径映射。`design-reviews/evidence` 保存可引用证据；`.evaluation-reports` 仅为本地/CI临时输出，CI通过 artifact 保留失败报告。

MCP 的执行器校验工具名单由已编译 Native 资源目录生成：`npm run generate:executor-tools` 更新，`npm run check:executor-tools` 检查漂移。导出使用真实 `tools/list` 与静态资源发现分页；Rust 测试继续对照完整 `tool_specs()`，防止分页漏项或新工具漏登记。
