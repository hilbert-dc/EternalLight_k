# Host 层渐进类型检查

## 实施前

目标：`tsconfig.json` 只覆盖 `src`，约 100 个 Electron Host 模块与 `preload.cjs` 没有任何类型检查。新增 `tsconfig.electron.json`（`allowJs`、`checkJs: false`），对当前已零诊断的 Host 文件加 `// @ts-check`，并把 `tsc -p tsconfig.electron.json` 接入 `build` 与 `typecheck:host`，阻止已纳入文件退化。

非目标：不改运行时代码与行为；不修复尚有诊断的文件（逐个纳入另行处理）；不开启全局 `checkJs`，避免把 `agent-engine` 传递导入的文件纳入本工程门禁（其归属在 Engine）；不启用 `strict`。

归属：构建/验证配置（B01 不变，B10 验证）。只读源码，不新增事实、通道或依赖。

五视角前审（同一执行者自审）：架构 APPROVE（门禁只覆盖本工程 Host，Engine 由自身负责）；技术A APPROVE（纯静态检查，无运行时影响）；技术B APPROVE（以 tsc 实际 0 诊断为准入，未纳入文件清单可追溯）；产品、UI/UX APPROVE（无可见变化）。零兼容门禁不涉及。

验证计划：E1 `tsc -p tsconfig.electron.json` 0 诊断；E2 `npm run build`（含 Renderer tsc 与 vite）；E3 `node --check` 全部改动文件；E4 `architecture:check`。

## 实施后

实际变更：新增 [tsconfig.electron.json](../tsconfig.electron.json)；`package.json` 新增 `typecheck:host` 并接入 `build`；86 个当前零诊断的文件（78 个 `electron/*.mjs|cjs`，含 `preload.cjs` 与全部 `*-handlers.mjs`；8 个 `shared/*.mjs`）首行加 `// @ts-check`。偏差：无。

| 编号 | 验证 | 结果 | 证明范围 |
| --- | --- | --- | --- |
| E1 | `tsc -p tsconfig.electron.json` | 后审通过，0 诊断；反向验证：在已纳入文件注入 `"x".toFixed(2)` 后报 TS2551 并以 2 退出，复原后 0 诊断 | 门禁确实覆盖已纳入文件 |
| E2 | `npm run build` | 后审通过（preload 合同、Renderer tsc、Host tsc、vite） | 构建入口 |
| E3 | `node --check` 86 个文件；读取源码文本的 10 个 Host 测试 + `verify-preload-api`/`verify-architecture` 测试 | 全部通过（70/70） | 首行注释不影响源码解析类测试 |
| E4 | `architecture:check` | 0 诊断 | B01 |

尚未纳入（以全局 `checkJs` 扫描时有诊断，非严格模式 361 条，含 Engine 传递导入）：Host 24 个文件，最多为 `work-execution-events.mjs`(48)、`media-host.mjs`(29)、`main.mjs`(25)、`conversation-messages.mjs`(24)、`codex-runtime.mjs`(23)。后续逐个修复后加 `// @ts-check` 纳入；Engine 侧诊断归 `agent-engine` 自身处理。

五视角后审（同一执行者自审）：架构 APPROVE（E4，门禁不越过 Engine 边界）；技术A APPROVE（无运行时改动）；技术B APPROVE（E1 正反验证）；产品、UI/UX APPROVE。零兼容门禁 PASS（无兼容路径）。
