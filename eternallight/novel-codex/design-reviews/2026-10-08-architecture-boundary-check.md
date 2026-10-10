# B01 当前生产依赖边界门禁

## 变更与验收

仅新增 `scripts/verify-architecture.mjs` 及其独立回归；规则 ID 为 B01。读取所有当前生产 JS/TS 文件，用既有 TypeScript AST 检查静态导入、重导出、import type、动态 import 与 require；Rust 只用 `cargo metadata --no-deps --offline --locked` 获取当前生产依赖，不构建、不调用供应商。诊断包含文件、目标和规则。临时工程 API 支持正反例，不污染生产文件。

模型选择创作行动、Host 生命周期、Native 领域事实、Engine 通用执行、UI 事实投影仍是人工评审和行为验收职责。该门禁只认证可检测的源码依赖边界，不能认证语义职责、纯函数实现或所有运行时反射行为。

## 五视角实施前评审

- 架构：APPROVE。Renderer 仅自身/shared/明确浏览器包；shared 不反向依赖；Host 仅 Engine 当前公共 exports；Engine 不依赖小说或 Codex 源码。Rust 按真实 crate 路径执行当前依赖矩阵，不能仅看容易重命名的 package 名。
- 技术 A：APPROVE。扫描全目录而非 git diff；类型导入同样受限；生产引用 tests/testing/fixture 必须拒绝。解析或 metadata 失败上抛，不吞错当通过。
- 技术 B：APPROVE。支持临时 root 与 metadata 注入做确定性负例；覆盖导入各种语法、公共/内部 Engine 路径、Rust dev/build/target 依赖及新文件。仅排除测试作为扫描入口，生产不能借其绕行。
- 产品：APPROVE。不固定作者措辞、创作步骤、方法或专家数量，不新增运行权限和预算。
- UI/UX：APPROVE。改动无用户流程；开发诊断定位具体文件/目标/B01，避免抽象“架构错误”。

## 当前基线事实与限制

- 浏览器当前入口为 React、React DOM、React Markdown、remark-gfm；shared 使用 js-tiktoken 的纯计算接口。
- Host 当前通过 Engine 公共 qualification exports 使用资格证明；生产使用的两个 `scripts/` 模块是构建身份/认证持久发布辅助，须按 Host 规则递归检查，而不是把整个 scripts 目录豁免。
- 初始审计确证 `shared/discussion-excerpts.d.mts` 反向导入 `src/types`；这是真实 B01 违规，已报告 Root 修正类型归属，不为现状加豁免。
- 两个当前 Rust workspace 的 metadata 读取实际 exit 0；Core 无内部路径依赖，authoring→Core，Runtime→Core/authoring/Engine Core，MCP 无 DB/内部路径依赖，Engine 仅内部路径依赖。
- 不新增兼容、迁移、fallback、自然语言关键词规则或供应商调用。Root 负责 package/CI 接线和唯一实际测试执行。

## 实施后复核

- 实际仅新增上述三文件；没有修改 package、CI、AGENTS、业务代码、权限、额度或模型输入。
- 架构：静态 APPROVE。全目录入口加生产可达辅助模块递归检查；Engine 以当前 package exports 为唯一 Host 公共入口；import type 不豁免；Rust 依据 manifest/path 而非包名执行矩阵。
- 技术 A：静态 APPROVE。测试目录仅排除入口，生产依赖它即 B01；语法错误、计算型 import/require、缺失本地依赖、未知 crate 与 metadata 错误都不能冒充通过。文件符号链接不静默跳过。
- 技术 B：静态 APPROVE，条件为 Root 实跑。15 项临时工程回归覆盖七种导入语法、浏览器/纯计算正例、Node/Electron/Host/Native/Engine/Codex 负例、类型反向引用、别名/绝对路径、私有/测试 export、脚本传递依赖、新无引用文件、解析错误、Rust dev 与 build/target 区分、重命名依赖及统一 API。测试未在本代理执行。
- 产品：静态 APPROVE。不按关键词解释作者输入，不固定计划、创作方法或子智能体组织；没有新增第二事实或任务权威。
- UI/UX：静态 APPROVE。CLI 输出每条 B01 的文件、行、依赖目标和原因；无产品交互变更。
- 静态验证：两个新 JS 文件 `node --check` 实际 exit 0，限定文件 `git diff --check` 实际 exit 0。此前 metadata 只读命令实际 exit 0，不属于构建或行为测试。
- 待实际验证：Root 执行 `node --test scripts/tests/verify-architecture.test.mjs` 及 `node scripts/verify-architecture.mjs`；现有 shared 类型反向依赖必须修正后才可认证真实工程门禁通过。
- 有界限制：本工具检查源码声明的直接及生产可达导入，不执行外部包，也不认证 eval、任意反射、网络目标、函数是否纯净或文学决策是否越权。这些继续受架构评审、受信运行边界和实际行为测试约束。未引入兼容/迁移/fallback。
- 冻结前增量五视角复核：架构/技术 A/技术 B 均静态 APPROVE，补上 Engine 不能通过在 dependencies 声明 domain/Core 包或别名 `file:/link:` 外部源码路径来自授权的正交负例；产品/UI 结论不变。只增加依赖约束，无行为执行、预算或兼容改动。

## 首次实际验证与路径规范化修复

Root 首次真实工程门禁实际 PASS：332 个生产文件、7 个 crate，日志 `/tmp/novel-architecture-rules-check.log`；shared 类型反向引用已由 Root 修正为 shared 所有、Renderer 重导出。Root 的 tsc/excerpts 实际 exit 0。独立 15 项回归首次为 3 PASS / 12 FAIL、actual exit 1，原日志 `/tmp/novel-architecture-rules-tests.log` 保留。

根因是 macOS `/var` 与 `/private/var` 别名：源文件 realpath，而 projectRoot/engineRoot 仍是 resolve 后的别名，合法依赖被误判，fixture 诊断文件匹配也错误。只修正本根因，未改变允许矩阵：统一规范化生产根、公共 export、file/link 声明、Rust manifest/dependency 及 CLI 输入；不存在的文件/metadata 目录只保留未存在后缀，已存在父目录先求真实路径。fixture 同样使用真实根，已有正例加 JS/Rust 根别名断言，已有 Renderer 负例加指向 Host 的符号链接导入仍须拒绝。

增量五视角实施前/静态实施后：架构 APPROVE（身份规范化而非放宽 containment）；技术 A APPROVE（真实外层符号链接仍被拒绝）；技术 B APPROVE WITH CONDITIONS（保留首次失败，Root 统一重跑同 15 项及实际工程）；产品/UI APPROVE（无业务、模型、预算或交互变化）。本代理不执行测试/构建，后续实际结果待 Root。

## 相对根的测试文件识别增量

路径规范化后 Root 已实际运行：`/tmp/novel-architecture-rules-check2.log` actual exit 0、332 文件/7 crate、无诊断；`/tmp/novel-architecture-rules-tests2.log` actual exit 0、15/15、零 fail/skip/cancel。这是本增量前证据，保留首轮 3/15 的失败，不认证尚未执行的新用例。

独立后审发现绝对路径上的测试目录正则会把祖先目录叫 tests/testing/fixtures 的整个仓库跳过。最小修复只将测试识别改为相对所属产品或 Engine 根；public export 的测试路径依然按其本来相对路径识别。真实生产→src/tests、fixture 或 test-support 依赖仍拒绝；外层符号链接和层级 containment 不放宽。新增一个真实临时仓库放在 tests/testing/fixtures 祖先下，必须扫描基线与新增无引用生产文件，并报告 Renderer Node、shared 反向类型、Engine domain 和 Host 借测试四个负例。

五视角增量前/静态后审：架构 APPROVE（所属根才是分类权威）；技术 A APPROVE（禁止零扫描假通过、保留真实测试借用负例）；技术 B APPROVE WITH CONDITIONS（16 项由 Root 唯一重跑）；产品/UI APPROVE（无业务或交互变更，不改变模型组织自由）。本代理未执行测试或构建。

## 最终当前版本验收：PASS

Root 最终实际复验完成：

- `/tmp/novel-architecture-rules-check3.log`：actual exit 0，扫描 332 个生产 JS/TS 文件、7 个当前 Rust crate，零 B01 诊断。
- `/tmp/novel-architecture-rules-tests3.log`：actual exit 0，16/16 PASS，零 fail/skip/cancel。
- 限定变更文件的 `git diff --check`：actual exit 0。

最终五视角后审均 PASS：架构确认实际当前依赖矩阵与相对根分类；技术 A 确认祖先测试名不漏扫、跨层/外层符号链接/生产借测试仍拒绝；技术 B 确认当前正反例及真实工程都已执行通过；产品确认未固定输入词、文学步骤或模型计划；UI/UX 确认 B01 诊断保留文件、行和目标且无产品交互改动。

首次 3/15 FAIL、路径规范化后 15/15 PASS 及各自原日志均保留，不能用最终结果改写历史。此 PASS 只认证可检测的 B01 依赖边界，不证明语义职责、真实模型创作或整个应用已闭环。本次收尾仅更新评审，源码与 fixture 保持冻结，不追加测试或构建。
