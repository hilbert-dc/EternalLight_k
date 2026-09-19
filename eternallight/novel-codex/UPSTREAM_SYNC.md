# Codex 上游同步与独立产品发布

Novel Codex 是独立产品，但保留在完整 Codex Fork 单仓库中。这样可以直接合并 OpenAI Codex 的提交，同时避免复制引擎源码、submodule 或失去 Git 历史。

## 远程仓库角色

```text
origin    -> 你的产品 Fork（可写）
upstream  -> https://github.com/openai/codex.git（只读）
```

当前工作副本已将 OpenAI 官方仓库配置为 `upstream`，尚未配置产品 `origin`。获得产品 Fork URL 后执行一次：

```bash
git remote add origin <你的 Fork URL>
git fetch origin
git branch --set-upstream-to=origin/main main
npm run repo:check:strict --prefix eternallight/novel-codex
```

严格检查会拒绝以下状态：没有官方 `upstream`、`origin` 与 `upstream` 相同、产品提交修改未经批准的上游路径。

## 合并上游

同步脚本只接受干净工作树，不执行 reset、不 stash、不自动解决冲突：

```bash
npm run sync:upstream --prefix eternallight/novel-codex
```

脚本从当前产品分支创建 `codex-sync/YYYY-MM-DD`，然后执行：

```text
git fetch upstream main
git switch -c codex-sync/YYYY-MM-DD
git merge --no-ff --no-edit upstream/main
```

如发生冲突，Git 保留标准 merge 状态，由开发者在同步分支处理。完成后运行：

```bash
npm run verify --prefix eternallight/novel-codex
npm run test:ui --prefix eternallight/novel-codex
```

验证通过后，再把同步分支以 `--no-ff` 合入产品主分支。

## 产品差异边界

正常产品提交只能修改：

- `eternallight/**`
- `.github/workflows/novel-codex-*`

如确实需要修改 `codex-rs`，应先独立形成可贡献给上游的 Codex 提交；产品分支不长期携带未隔离的引擎补丁。这样后续直接 merge `upstream/main` 时，冲突集中在真正的上游改动，而不会污染 Novel Codex 产品代码。

## 独立发布

Novel Codex 使用自己的应用 ID、产品名、安装包名称和发布工作流。发布任务从当前同一 Git 提交构建 `codex-cli`，再通过 `stage:release-runtime` 写入版本和 SHA-256 摘要后放入安装包。最终应用不需要安装或登录 Codex 桌面版。
