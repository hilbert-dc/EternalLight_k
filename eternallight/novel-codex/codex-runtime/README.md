# Novel Agent Engine 私有内核

该目录定义 Novel Agent Engine 的固定协议内核、技能和 MCP 资源边界。Codex App Server 仅作为私有实现依赖，不是产品身份；这里不放置 ChatGPT 桌面版认证或普通 Codex CLI 状态。

## CLI 来源

- 开发：`codex-runtime/bin/codex` 或当前仓库 `codex-rs/target/debug/codex`；两者都必须在 `supported-cli.json` 中登记当前平台／架构的 SHA-256、BuildIdentity 和精确 `--version` 输出。
- 打包：`resources/codex-runtime/bin/codex`；Windows 为 `codex.exe`。
- 发布构建先用 `NOVEL_CODEX_CARGO_PROFILE=release npm run build:codex-runtime` 从当前源码生成 BuildIdentity，再设置 `NOVEL_CODEX_RELEASE_CLI`、两个 Runtime sidecar 和 `NOVEL_CODEX_RELEASE_MEDIA_RUNTIME` 运行 staging（固定写入 `.release/resources`）。源码摘要、二进制摘要或目标平台不匹配时 fail-closed。
- `supported-cli.json` 固定可用来源、协议、版本、逐平台摘要、BuildIdentity 和模型目录指纹；打包态不接受显式路径或开发构建。
- 启动器不从 PATH、ChatGPT.app 或系统级安装查找 CLI。
- 应用使用单实例锁后才启动 Runtime 和回收孤儿任务；隔离 CODEX_HOME 的 config 与模型目录在每次启动时原子重写，旧配置不能恢复动态工具或远端目录。
- 配置、线程、日志和 SQLite 状态位于 Electron `userData/codex-home`；该目录不读取或复用 Codex/ChatGPT 登录态。

## 创作基础提示

`engine-prompt.md`是本产品基础提示的唯一正文。开发Host、发布staging、模型认证和真实创作探针均读取此文件，发布时逐字复制到resources/codex-runtime/engine-prompt.md。它定义通用创作身份、信任边界、自主工作和诚实交付，不携带仓库搜索、编码、测试等编程助手工作流；具体领域合同、冻结方法与工具schema仍由各自权威提供。上游Codex Core和通用Engine不承载产品文学策略。

基础提示内容参与QualificationContext摘要，改变后旧连接资格不能直接复用，必须重新验证。发布清单同时绑定提示文件SHA；本地受控测试检查实际Core供应商请求中的完整提示，以及开发/发布正文一致性。

## Skills

- `novel-workflow`：默认必需执行契约，要求读取绑定上下文、使用候选权限并验证持久产物；stable 不代表文学质量认证。
- `novel-canon`：实验性故事状态解释，区分正文、候选模型与作者确认。
- `narrative-ir`：实验性 Scene/Beat 规划与修订，不要求作者手填结构，也不把正文当作可丢弃的 IR 渲染。
- `story-tests`：实验性连续性与效果检查；不能把不可执行的检查或模型自评称为通过。

Rust 在任务创建事务中冻结选择、版本、正文和摘要；Host 从任务读取，不再启动时拼接全部 Skill。后三项保持 candidate，Author 创建任务时可显式提供最多两项 `experimentalMethods`；未知、不匹配或重复选择拒绝，缺省不启用。失败步骤恢复继承作者原选择，每次新 Attempt 重新生成绑定；若作者明确传入空列表，则取消本次重试的实验方法。方法和 Context Pack 共用总预算，不能静默裁掉必需契约。技能不授予额外写权限或正式采用能力。详见 [创作方法注册与冻结绑定](../docs/designs/CREATIVE_METHOD_BINDING_DESIGN.md)。

## Candidate MCP

Novel Agent Engine 的私有 App Server 内核通过本地 stdio `novel_workspace` MCP 调用：

- `read_context`（全部八类模型任务可带 `queryPlan`，同来源查询报告与基础上下文共同受整包预算；`queryScope` 明示领域层绑定范围）
- `search_story`（任务范围内的正文与 Writer 前文知识）
- `search_relations`（Writer 的有界前文/POV 关系证据）
- `query_story`（全部八类模型任务的有限联合证据查询；滚动规划只读前章，建模/评估正文绑定源章，Writer 保留章节、场景、POV 和时间门禁）
- `save_brief`
- `save_outline`
- `save_chapter_plans`
- `save_draft_checkpoint`
- `publish_knowledge_batch`（仅 `modelKnowledge` 任务）
- `complete_task`

每个任务创建独立的 30 分钟能力凭证。Runtime 只保存其 SHA-256，MCP sidecar 只持有原始凭证；每次候选请求同时验证凭证、TTL、任务状态以及 `taskId / projectId / threadId / expectedRevision` 绑定。任务完成、失败或取消时立即终止 App Server／MCP、擦除明文并撤销 Runtime 能力；应用重启先取消并撤销遗留的活跃任务。MCP 没有 accept、undo、lock 或任意数据库写入工具。候选只有经过桌面端作者审阅，才能由 author gateway 推进正式版本。

## Provider

`model-catalog.json` 声明 provider/model/operation、固定协议和可复用协议语义，本身不是能力认证。`model-certifications.json` 保存发布前真实行为认证，并在启动时映射为标准 CapabilityEvidence；它不建立第二套运行时状态。Codex 实际模型目录仍只由当前、未过期的 SessionPlan 动态生成；未经认证的流式、多 Agent 或其他扩展不会写入 Codex 目录。

内置连接在发布前通过协议探测和 Codex 内核一致性测试，客户端只检查当前账号访问；实验连接才在本机执行完整认证。Responses 使用受控原生透传，Chat Completions、Messages 和 Generate Content 使用协议转换；两类路径都经过同一 Bridge 租约、预算、可信父子路由和工具提交门禁。多 Agent 只在真实子 Agent + MCP 回环通过后开放；Code Mode、远程压缩等未实现或未认证扩展会在配置编译时明确拒绝。模型若把内部协议模板作为普通文本返回，adapter 会失败关闭并阻止该文本进入 Renderer。

```bash
export NOVEL_CODEX_MODEL=deepseek-flash
export DEEPSEEK_API_KEY="<your-key>"
npm run dev
```

macOS 本地开发也可以把测试 Key 保存到系统 Keychain。开发启动器只在没有显式 `DEEPSEEK_API_KEY` 时读取服务 `com.eternallight.novelcodex.deepseek`、账户 `novel-codex`；环境变量始终优先。Keychain 值不会传给 Vite/Renderer，也不会写入仓库、配置或日志。Windows 与 Linux 开发环境继续使用进程环境变量。

DeepSeek Key 只由 Electron Host 的凭据解析器交给独立 ModelBridge worker，不进入 Codex App Server、Renderer 或 Creative Runtime。`CODEX_API_KEY`、桌面认证、OpenAI 凭据和其他未选 provider 凭据会从子进程环境剥离。任何 API Key 都不写入项目、`config.toml`、Runtime 环境或日志。缺少 `DEEPSEEK_API_KEY` 时只允许手工候选编辑与确定性检查，不生成伪造产物。

Novel Agent Engine 不调用 Codex 桌面应用的审批系统，也不消费 Codex/ChatGPT 订阅额度。模型请求由显式配置的 DeepSeek 账号独立认证、计费和限流；本隔离不会绕过 DeepSeek 自身的余额或配额。

领域 turn 固定使用空执行环境和 `approvalPolicy = "never"`。能力凭证和 operation 白名单约束下的本地 `novel_workspace` 候选工具在私有配置中明确预批准，因此不依赖 Codex 桌面审批；Runtime 仍会逐次验证任务、项目、线程、版本、TTL 和允许的工具集合。若私有内核产生 command/file/network 权限请求，Electron 主进程会立即回复 `decline`，且不会把请求转发到 Renderer。`requestUserInput` 是唯一保留的交互请求，用于向作者询问创作选择。

## 创作协作

角色定义由 `electron/novel-agent-roles.mjs` 生成 TOML 与能力目录，执行 `npm run generate:agent-roles` 同步。角色按具体问题选用，不组成固定团队。`expert_investigation` 通过 `read_creative_strategy` 按需提供证据、反证、修改代价与合成案例，所有策略仍待文学收益验证。

父任务使用实际 `candidate_writer` agent type 创建直属子任务，Native 根据 Core 受信执行角色和关系绑定隔离保存权限，无 UUID 汇报或独立授权工具；提示词中的角色声明不产生权限。子任务只返回不可变候选引用，由父任务检查并整合。研究子任务保持只读，全部任务共用原 Work 额度。

三组质量评测入口：`npm run evaluate:collaboration-quality -- scripts/fixtures/creative-quality-suite.json runs.json output-directory`。runs 记录格式及三组要求见 `scripts/collaboration-quality-evaluation.mjs`；空数组仅生成未执行计划，不产生验收通过。评审只取得匿名 packet，映射 key 单独保存；完成真人评分后沿用 `evaluate:creative-quality assess`，模型评分不能替代真人。

## 官方内核与上层适配

当前官方源码基线为 `12cca49c81c8aa6a37a5cc4e39964ee3b70f9458`，不保留产品补丁。后续官方源码修改须先按 B09 经开发者审核并明确同意。

工具 schema 由实际 Native MCP 当前目录注册到 Engine，在发现与供应商编码前精确恢复；调用身份由官方请求元数据、Host `thread/read` 角色事实和 Engine 的租约/采样/callId 关联共同提供。MCP 通过认证回环端点核验原始参数和目录摘要，再进入既有领域权限校验；提示词和模型参数不授予权限。

构建、迁移和实测范围见 [迁移评审](../design-reviews/2026-10-13-core-upper-migration.md)。源码迁移不等于安装包发布，真实模型资格及小说质量需分别验收。
