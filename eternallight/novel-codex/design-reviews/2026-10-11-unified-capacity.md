# 统一容量与用量原则

## 方案与前审
根因：设计缺陷。应用将模型物理容量、输入传输保护、单条工具结果和费用预算混在多个固定 token 门槛中。真实恢复消息 9031/9000 被 Host 拒绝；DeepSeek `/models` 实测 1048576 context / 393216 output，目录却为 131072/8192。目标是不浪费且正常功能可用，不追求最小 token。

责任：Engine 唯一执行完整模型请求容量校验、费用预留/结算；Host/Shared 仅输入结构与传输保护；Native 保留领域权限、CAS、完成判据及按需读取的字节分页。移除 Shared 7000、Host 9000、Runtime 9000 的模型 token 准入；不删字节分页、不截断必需正文。Host 完整回合传输采用 256 KiB，与 Engine 单项字节保护一致，可容纳作者 48KB 加系统约束；作者原有字符/字节资源保护保留。Native 用最终转义工具条目的字节保护替换 tokenizer，给包装预留 1 KiB。Core 工具显示预算覆盖此字节上界，避免已准入结果被第二层 token 截断。完整上下文仍由 Engine 拒绝/由 Core 有界压缩，不提升授权。

当前模型能力来自官方元数据而非猜测；上限允许完整推理与产物，不是目标消耗。请求用量、未知费用、取消、Work 身份保持；有限作者预算仍拒绝透支。模型决定阅读与写作方法，不设置固定文学流程。无兼容层，无数据库迁移或重置。

状态：合规大 Unicode 输入允许完整传输；超字节输入原文保留并准确归因；超完整模型上下文仍拒绝；真实 output incomplete 保留失败与用量，不执行残缺工具；总预算原规则。

五视角前审（Codex 同一执行者自审）：架构 APPROVE（B01/B04/B05/B06/B10，容量归 Engine）；技术A APPROVE WITH CONDITIONS（字节保护、总上下文、费用反例）；技术B APPROVE WITH CONDITIONS（Shared/Host/Native/配置/测试同步）；产品 APPROVE（完整功能与原标准）；UI/UX APPROVE（系统超限不责怪作者）。后审待实际验证。

## 验证计划
作者大 Unicode、Host 附加恢复上下文与原文完整；转义工具边界与必需来源保存；Engine 上下文和预算现有回归；架构/类型检查；打包后原 Work 恢复。官方依据：https://api-docs.deepseek.com/api/list-models/ 、https://api-docs.deepseek.com/api/create-response/ 。实测仅读取 id/容量，不输出凭据。

## 已完成的验证
- Shared/Host/真实 Core：55/55 通过（`/tmp/unified-js2.log`）。5,000 组 Unicode 和恢复上下文超过原 9,000 token 仍完整；整轮 256 KiB 保护拒绝且不裁剪；真实 Core 在 512 输出 allowance 下完整读取 30KB/60KB 证据，完整 Native 正文与游标跨压缩保持。
- Engine：65/65 通过（`/tmp/unified-engine-tests.log`），覆盖公开配置、完整上下文、预算失败和长推理输出准入。不是移除总容量或费用校验。
- Native 单元：16/16 通过（`/tmp/unified-native2.log`），超字节必需正文不裁剪、最终转义与来源身份不变，原高 token 正文允许。
- 独立 Host 消息组合回归 1/1；TypeScript 与架构 366 文件/7 crate 通过。
- 旧测试更新依据：原 9,000 token 拒绝改为完整成功；原可选裁剪测试改用真正较小的 byteBudget 继续验证裁剪及 coverage；字节溢出拒绝另有大正文用例。适配器已有分段传输的规则在测试中按序完整拼回核对，不略过内容断言。

待 Native HTTP 回归、打包与真实应用原 Work 复验，未据单元测试宣称小说完成。

- HTTP 已完成 context_budget 1/1、context_scope 15/15（含 101 章、48KB 高 token 必需正文、CAS/作用域/未来章节隔离）。初轮仅旧 `<=9000 tokens` 断言失败，真实响应已成功；对应断言同步为最终转义字节边界，原文及保护断言保留。余下两组分析 HTTP 待结束。
- 本机运行出现多次加载等待：采样 `/tmp/unified-test-startup.sample` 显示 `_dyld_start`，尚未进入程序；同一测试启动后仅需数秒。首次 MCP 30 秒超时、下一次应用 smoke 30 秒超时、再次 FFmpeg 30 秒超时均保留为失败，未跳过检查。独立完整 `verifyReleaseRuntime({timeoutMs:180000})` 已返回 CLI/MCP/Runtime/media 全 verified。后续 packageRelease 使用现有 execute 注入点只将本次应用 smoke 等待改为 180 秒，内容/回执/资源校验不变，生产默认超时未改。

## 发布启动诊断补充前审
多次真实证据显示同一程序停在 `_dyld_start`，30 秒门禁误判启动；不是 token 功能失败。增加发布专用 `startupTimeoutMs`（默认 30000，整数且 1000..300000），贯穿 Runtime/MCP/媒体/应用 smoke，写入 package report。只控制启动验收等待，不改变生产请求超时、内容验证或成功判据；不跳过任何门禁。架构/技术A/B/产品/UI五视角同一执行者前审 APPROVE WITH CONDITIONS：参数校验、默认保持、传递与报告测试，实机使用 180000 后才认定打包通过。无兼容或权限变化。

## 后审进展
Native HTTP 四组共 22/22 全通过（`/tmp/unified-http2.log`）。新增发布等待配置的包与媒体测试 19/19 通过：非法/无界时间在任何执行前拒绝，180000 实际传递到 Runtime 检查与最终应用 smoke 并写入报告，延长等待仍不能将失败当作成功。架构再次通过。

容量改动五视角后审：架构 APPROVE（完整模型容量在 Engine，Native 只传输/领域）；技术A APPROVE（完整正文、字节拒绝、费用/权限/CAS边界）；技术B APPROVE（Shared/Host/Native/目录/资格指纹与合同测试同步）；产品 APPROVE WITH CONDITIONS（包与原任务实测待完成，未声称小说完成）；UI/UX APPROVE（资源错误准确归因，不要求作者删系统约束）。零兼容 PASS。发布诊断配置后审 APPROVE WITH CONDITIONS：实际最终包仍须成功回执。

## 最终包结果
2026-10-11 09:21:33（Asia/Shanghai）完整 packageRelease 已通过 staging/runtime-verification/frontend-build/packaging/packaged-resources/application-smoke；报告 `release/package-report.json` 明确 startupTimeoutMs=180000。使用正式参数实现，无 execute 替换或检查跳过。启动新版应用后保留原作品和任务，连接因能力指纹变化按正常 UI 重新验证。仍为本地 unsigned 开发包，不声称签名发布。

发布补充五视角后审 APPROVE：参数与报告完整、失败保留旧包、全门禁实测成功；未修改生产模型调用超时或弱化成功条件。容量部分仍待原 Work 真实恢复观察。

## 实机资格检查发现与补充前审
09:22 新版连接基础 text/structuredToolCalls/streaming/usage 均 verified，Core toolLoop 因 `task_token_budget_exhausted` 未发送。来源为 qualification_run 内置 200000 总 token，而新模型单次输出预留 393216；不是作者额度或供应商费用。按本次原则移除该无作者来源的固定 token 总额，保留资格任务 16 次请求上界、既有 60/90 秒时限、固定合成 nonce 输入与真实计量；不改真实作者有限预算。五视角前审 APPROVE WITH CONDITIONS：验证 budgetCreate 保留请求/时间限制、无 token 总额及真实 Core 资格恢复。此处是验证器开销管理，不将生产模型能力改小来迁就验证器。

资格预算补充验证：新增真实 NativeQualificationRun → budgetCreate → TaskBudget 测试及真实有限/无限预算拒绝/断流回放组 15/15 通过（`/tmp/unified-qualification-tests.log`）。Engine 已重建，新包于 09:27:08 再次全阶段通过，startupTimeoutMs=180000。架构通过。五视角补充后审 APPROVE（原作者有限预算不变，合成资格检查仍受 16 请求和有限时长约束，实际用量仍计量）。等待重新连接的实际结果。

## 实机恢复结果
09:28 最新 capability report `2026-10-11T01-28-19.748Z-205d4ad279a6d8b65a7df414df9207c6324917ab2952d79d1a590ae2db32a189.json` 为 verified，text/structuredToolCalls/streaming/usage/toolLoop/四项协作能力均 verified；原先资格内部 token 总额误拒绝已闭合。通过 UI 点击原任务“重试当前任务”（不是新建 Work），新版已实际读到第1章原文及故事知识，运行约50秒仍进行中，无原 9031/9000 Host 准入拒绝。

容量修复的产品后审 APPROVE：源代码、完整回归、打包、真实连接与原任务恢复均完成。验证边界：验收者手工点击重试，不能声称无人推动恢复；本次尚未以真实保存回执确认第2章完成，前100章创作目标仍未完成，运行由应用继续。不存在正文或费用清零、额外作者授权、发布或兼容迁移。

## 16:10 后续实机观察与恢复边界
- Native 原运行 `generation-d6db3e3374164a09809d9dadfc000ab8` 的 completedChapters=5，保存交付回执覆盖新写第2—5章。第6章仅有空章节，不计完成。原容量拦截已未再出现，但这不证明长篇效率或持续运行稳定性。
- 第6章 `task-13d89e7354ce4738be616ebbe1234736` 请求 :148 为 downstream timeout，:151 为 cancelled（502）。Host 又记 provider usage remains unsettled / TaskBindingInvalid；重启前 Native task 仍 queued、Work/run 仍 running。重启后 Native 将孤立 Attempt 标为 failed/interrupted，原账本完成结算，Work/run 一度仍显示 running。不能把这次失败归因于 token 超限，也不能据取消日志推断作者主动取消。
- 代码定位：`work_tasks::refresh_work_task` 对 generation 直接返回；生成状态推进由 Authoring `/next` 完成。后台事件另外先要求失败可自动恢复，故存在终止 Attempt 与运行状态不同步、只显示等待的恢复缺口。停止时为何未写入终态仍需核对执行 owner 失效和取消链路；尚未宣称完整根因或实现修复。
- 实际操作：重启现有应用；发送一次续写消息（普通 advisory 被恢复交接绑定拒绝，未产生重复运行）；在作品概况点击原运行“继续创作”，状态才转为 blocked；随后预览并确认“基于当前章重写”，明确保留前1—5章、原100章目标和累计用量，再点击继续。应用工作决策已重新排入第6章，并进入 writer。手动重规划成功不等于自动恢复修复。
- 恢复预览显示累计184次请求、17,343,794 token、1,361,264 token 未确认。未确认值是引擎保留的用量未知量，不宣称供应商实际收费。第6章旧 Attempt 单独引擎记录16次请求、552,320已知 token、915,700未知 token。需再拆分成功请求输入/输出、缓存和重试开销才能判断浪费来源；不得直接按总数认定费用或通过清零/减小能力上限处理。
- 本补充仅记录事实与剩余问题，未改生产代码/契约/数据库。文档五视角同一执行者自审：APPROVE（区分保存与交付、已知与未知用量、人工恢复与自动恢复；没有把后续缺口记为已修复）。无需重复既有容量测试。100章仍未完成；后续生命周期恢复与开销优化需要独立方案、回归和应用复验。
