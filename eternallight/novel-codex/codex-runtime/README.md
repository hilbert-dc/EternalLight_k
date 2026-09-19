# Novel Agent Engine 私有内核

该目录定义 Novel Agent Engine 的固定协议内核、技能和 MCP 资源边界。Codex App Server 仅作为私有实现依赖，不是产品身份；这里不放置 ChatGPT 桌面版认证或普通 Codex CLI 状态。

## CLI 来源

- 开发：`codex-runtime/bin/codex` 或当前仓库 `codex-rs/target/debug/codex`；两者都必须在 `supported-cli.json` 中登记当前平台／架构的 SHA-256 和精确 `--version` 输出。
- 打包：`resources/codex-runtime/bin/codex`；Windows 为 `codex.exe`。
- 发布构建必须先设置 `NOVEL_CODEX_RELEASE_CLI` 和 `NOVEL_CODEX_RELEASE_RESOURCES` 并运行 `npm run stage:release-runtime`。该步骤复制目标平台 CLI、Runtime sidecar 与模型目录，并在发布资源内生成该平台的真实摘要；未执行时打包态会 fail-closed。
- `supported-cli.json` 固定可用来源、协议、版本、逐平台摘要和两个模型目录指纹；打包态不接受显式路径或开发构建。
- 启动器不从 PATH、ChatGPT.app 或系统级安装查找 CLI。
- 应用使用单实例锁后才启动 Runtime 和回收孤儿任务；隔离 CODEX_HOME 的 config 与模型目录在每次启动时原子重写，旧配置不能恢复动态工具或远端目录。
- 配置、线程、日志和 SQLite 状态位于 Electron `userData/codex-home`；该目录不读取或复用 Codex/ChatGPT 登录态。

## Skills

- `novel-canon`：维护可审阅的角色、关系、规则、事件和信息边界。
- `narrative-ir`：把意图组织为 Scene → Beat，并保留正文锚点。
- `story-tests`：执行结构和引用检查，解释失败原因。
- `novel-workflow`：要求 Agent 读取用途匹配的上下文，并只通过 candidate MCP 保存产物。

技能只约束模型行为，不授予写权限。

## Candidate MCP

Novel Agent Engine 的私有 App Server 内核通过本地 stdio `novel_workspace` MCP 调用：

- `read_context`
- `save_brief`
- `save_outline`
- `save_chapter_plans`
- `save_draft_checkpoint`
- `submit_fact_proposals`
- `complete_task`

每个任务创建独立的 30 分钟能力凭证。Runtime 只保存其 SHA-256，MCP sidecar 只持有原始凭证；每次候选请求同时验证凭证、TTL、任务状态以及 `taskId / projectId / threadId / expectedRevision` 绑定。任务完成、失败或取消时立即终止 App Server／MCP、擦除明文并撤销 Runtime 能力；应用重启先取消并撤销遗留的活跃任务。MCP 没有 accept、undo、lock 或任意数据库写入工具。候选只有经过桌面端作者审阅，才能由 author gateway 推进正式版本。

## Provider

`model-catalog.json` 固定经过审核的 provider/model/operation 组合；`codex-model-catalog.json` 固定 Codex 实际工具与上下文元数据，并由专用配置通过 `model_catalog_json` 装载。Novel Codex 的讨论、规划、写作和评审统一使用 DeepSeek `deepseek-flash`；其他 provider/model 不在当前工程白名单内。

```bash
export NOVEL_CODEX_MODEL=deepseek-flash
export DEEPSEEK_API_KEY="<your-key>"
npm run dev
```

DeepSeek Key 只进入 Novel Agent Engine 的私有 App Server 进程。`CODEX_API_KEY`、桌面认证、OpenAI 凭据和其他 provider 凭据会从 Runtime 与引擎环境剥离。任何 API Key 都不写入项目、`config.toml`、Runtime 环境或日志。缺少 `DEEPSEEK_API_KEY` 时只允许手工候选编辑与确定性检查，不生成伪造产物。

Novel Agent Engine 不调用 Codex 桌面应用的审批系统，也不消费 Codex/ChatGPT 订阅额度。模型请求由显式配置的 DeepSeek 账号独立认证、计费和限流；本隔离不会绕过 DeepSeek 自身的余额或配额。

领域 turn 固定使用空执行环境和 `approvalPolicy = "never"`。若私有内核仍产生 command/file/network 权限请求，Electron 主进程会立即回复 `decline`，且不会把请求转发到 Renderer。`requestUserInput` 是唯一保留的交互请求，用于向作者询问创作选择。
