# Novel Codex

Novel Codex 是面向长篇小说创作的独立桌面产品。它在完整 Codex Fork 单仓库中开发，以便直接同步上游 Agent 内核；产品运行时不依赖 Codex 桌面应用的登录、配置、会话、审批或模型额度。

## 本地开发

```bash
npm install
npm run dev
```

## 验证

```bash
npm run verify
npm run test:ui
```

`repo:check` 验证产品提交没有污染 Codex 上游实现路径。发布和保护分支应使用 `repo:check:strict`，要求 `origin` 指向产品 Fork、`upstream` 指向 OpenAI Codex。

## 独立安装包

发布前先从当前 Git 提交构建 `codex-cli`，再把固定版本和摘要写入产品资源：

```bash
cargo build --release -p codex-cli --bin codex --manifest-path ../../codex-rs/Cargo.toml
NOVEL_CODEX_RELEASE_CLI=../../codex-rs/target/release/codex \
NOVEL_CODEX_RELEASE_RESOURCES="$PWD/.release/resources" \
npm run stage:release-runtime
npm run package:mac
```

Windows 在对应主机上使用 `codex.exe` 和 `npm run package:win`。GitHub Actions 的 `novel-codex-ci` 手动任务会在 macOS 与 Windows 分别执行这一流程。

上游远程配置和直接 merge 流程见 [UPSTREAM_SYNC.md](UPSTREAM_SYNC.md)，运行架构见 [ARCHITECTURE.md](ARCHITECTURE.md)。
