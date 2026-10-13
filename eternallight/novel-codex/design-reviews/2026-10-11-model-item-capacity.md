# 模型片段容量与完整请求容量分离

## 前审（Codex 五视角自审）

真实失败：修复版应用《天衡诡契》生成运行 generation-d6db3e3374164a09809d9dadfc000ab8 已获批，目标 100、完成 1；第 2 章 writer task-e4d0cb8da1fc45fb91e424f2a9312727 第 15 次请求返回 reasoning，序列化 29650 字节、正文 28924 字节、保守估算 10865 token，被 Engine tokens::bounded_output_item 的每片段 10000 token 固定阈值拒绝。日志仅记录计数；新章未保存。Native/Host 准入没有失败。

根因：设计缺陷。tokens::bounded 把完整请求容量的估算单位用于单片段硬阈值，独立于连接上下文/输出能力。合法长推理即使满足完整路由容量和单项内存保护仍会失败。把阈值简单抬高或仅为 reasoning 特判，会继续在下一回合输入、正文、工具参数等相同边界复发。

方案：Engine 的通用 item guard 仅保留既有 256 KiB 序列化 UTF-8 字节保护（解析/缓冲资源保护，输出拒绝零执行）；token 估算继续用于已存在的完整编码请求容量与输出预留。取消无供应商/协议依据的每项 10k token 阈值，不截断/重写推理和 opaque 状态。实际供应商 max_output_tokens、累计预算、完整请求/流传输大小、工具身份/schema/参数校验、取消/结算均不改变。工具参数 schema 独立 40000 字节复杂度保护保持，修正其误称“10k token”的错误单位；不扩大 JSON Schema 编译资源。

责任为 Engine（B01/B04/B05/B06/B09/B10），Native 仍决定保存及权限，Core 仍负责压缩。非官方库缺陷，不修改 Core 或添加 Host 重试循环。公共错误仍为 provider_output_item_limit/context_item_limit，仅当前字节保护触发；诊断 limitKind=utf8Bytes、limit=262144，超字节项不额外 tokenize，保留 null 估算及类型/分项字节信息。输入超完整容量仍 context_input_budget_exceeded，派发和预算预留前拒绝。

| 输入事实 | 结果 | 副作用/恢复 |
| --- | --- | --- |
| 单项 >10k 估算、≤256KiB 且完整请求容量足够 | 允许原文传输/历史回读 | 正常计量，不重试、不截断 |
| 多项合计超过路由输入额度 | 原容量拒绝 | 零派发零预留，Core 压缩路径不变 |
| 单项 >256KiB（含 opaque） | 字节保护拒绝 | 不交付可执行输出，结算真实用量或保留未知 |
| 不合法工具/参数/冲突流终态 | 原合同拒绝 | 不因移除 token 阈值放行 |
| 旧失败的原 Work 恢复 | 原身份/累计账本准入 | 不重建 run、不清费用、不绕过取消或未知保存 |

五视角前审：架构 APPROVE（容量/资源区分）；技术 A/B APPROVE WITH CONDITIONS（长中日韩/emoji/opaque 原文保留，JSON/SSE 及真实 Core 回读；字节拒绝与费用结算回归）；产品 APPROVE（长篇不会因无依据片段阈值停机）；UI APPROVE WITH CONDITIONS（应用原运行恢复后保存新正文）。零兼容：只替换当前规则，不保留旧 token 阈值分支，不改作者数据库。

验证计划：Rust tokens/完整容量边界；四种 codec 输入工具/说明/合并项；Responses JSON/done/terminal；ModelBridge 与独立进程的拒绝/原文保留/实际用量；真实 Core 回读长高 token 推理；既有工具拒绝/费用结算/取消测试。重新构建 Engine、架构检查、打包并恢复原运行。预存失败记录不删除。

## 后审
待实施验证。

E1 已通过：Rust lib 47/47，包含 >10k 高密度文字/emoji 推理完整保留、字节拒绝、完整容量诊断、schema 资源错误 400/不可原样重试与供应商文本隔离。`architecture:check` 366 files/7 crates 无诊断。

E2 已通过：最终构建下 codecs、responses-output-admission、remediation-boundaries、invalid-tool-arguments、late-usage-settlement、bridge、bridge-budget-failure、native-stream-guard、bridge-stream-terminal 和真实 Core output-admission integration 共 171/171。JSON/SSE/原文回读、字节拒绝零可执行输出、实际用量/缺失用量/取消与预算边界保留。真实 Core 两种字节拒绝后仍能在原线程保留 6000 组“界🙂”高 token reasoning 与 opaque 字段，下一轮输入及落盘历史逐项相同。

E3 广泛 Engine 套件 381 项，378 通过、3 失败；失败属于已有预算合同/fixture差异：authority-state 的恢复快照新增两个空证据数组、media-budget 的有限额度未知用量拒绝、unrestricted-metering 的无额度上限准入预期。为排除本次引入，用修改前 00:44:22 包内 engine.node 在隔离目录运行同三项，3/3 复现完全相同失败；没有更改预算生产代码或放宽断言。其他首次隔离整文件测试因沙箱端口权限失败的两项已排除，三项基线证据来自定向重跑 `/private/tmp/tianheng-item-baseline-isolated.log`。本次不宣称完整 Engine 套件全绿。

E4 Engine release 已重建，engine.node SHA256 `ba6f1c0a82fe17ade09acf6bb2ed400746db61e2612984898df9843147dd99ba`；Core 源码和构建未改。当前打包与原生成运行恢复尚待验证。

五视角后审自审：架构/技术 A/技术 B APPROVE（E1–E3，取消虚假 token 子窗口，保留资源/权限/账本边界）；产品/UI APPROVE WITH CONDITIONS（E4 真实应用保存待验证）。零兼容 PASS；原失败/用量/正文均保留，不构造新 run 绕过原失败。

E4 产物验证通过：package:dir 完成 staging/runtime-verification/frontend-build/packaging/packaged-resources/application-smoke，verify:package 通过。应用重新加载后按真实 Engine 变更指纹要求重新验证原 DeepSeek 连接；未绕过验证或复制旧资格结果。接续只操作原运行。
