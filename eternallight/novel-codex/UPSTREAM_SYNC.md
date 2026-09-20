# Codex 上游同步与独立产品发布

Novel Codex 是独立产品，但保留在完整 Codex Fork 单仓库中。这样可以直接合并 OpenAI Codex 的提交，同时避免复制引擎源码、submodule 或失去 Git 历史。

固定 SHA 候选、隔离 worktree、内核补丁登记及双平台验收的目标流程见 [多模型 Codex 引擎基础层技术方案](../agent-engine/ENGINE_FOUNDATION_TECHNICAL_DESIGN.md)。同步脚本现已创建确定候选和影响报告；候选验证与晋升仍需在对应平台执行。

## 远程仓库角色

```text
origin    -> 你的产品 Fork（可写）
upstream  -> https://github.com/openai/codex.git（只读）
```

2026-09-20 核对：当前工作副本的 `origin` 已指向 `https://github.com/hilbert-dc/EternalLight_k.git`，`upstream` 已指向官方仓库。已有远程无需重复添加。可执行只读边界检查：

```bash
npm run repo:check:strict --prefix eternallight/novel-codex
```

严格检查会拒绝以下状态：没有官方 `upstream`、`origin` 与 `upstream` 相同、产品提交修改未经批准的上游路径。

## 合并上游

同步脚本只接受干净工作树，不执行 reset、不 stash、不自动解决冲突：

```bash
npm run sync:upstream --prefix eternallight/novel-codex
```

脚本记录当前已提交的产品基线，fetch 后固定官方目标 SHA，并创建独立候选：

```text
git fetch upstream main
git worktree add -b codex/upstream-sync-<date>-<sha> <isolated-path> <product-base-sha>
git -C <isolated-path> merge --no-ff --no-edit <fixed-upstream-sha>
```

当前工作区的未提交内容不会进入候选。脚本在 `.upstream-sync-reports/` 写入基线、目标、候选、排除的本地修改和接口影响分类。如发生冲突，Git 保留候选 worktree 和标准 merge 状态。完成后运行：

```bash
npm run verify --prefix eternallight/novel-codex
npm run test:ui --prefix eternallight/novel-codex
```

验证通过后，再把同步分支以 `--no-ff` 合入产品主分支。

## 产品差异边界

正常产品提交只能修改：

- `eternallight/**`
- `.github/workflows/novel-codex-*`

如确实需要修改 `codex-rs`，补丁必须登记在 `codex-runtime/upstream-patches.json`，包含基线、路径、符号、测试、差异摘要和删除条件。边界检查会校验实际差异摘要；不能通过把整个 Rust 文件加入白名单绕过检查。上游出现等价能力后应验证并删除补丁。

## 独立发布

Novel Codex 使用自己的应用 ID、产品名、安装包名称和发布工作流。发布任务从当前同一 Git 提交构建 `codex-cli`，再通过 `stage:release-runtime` 写入版本和 SHA-256 摘要后放入安装包。最终应用不需要安装或登录 Codex 桌面版。
