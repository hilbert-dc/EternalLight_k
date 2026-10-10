# 当前 Context 最终包预算边界修复

日期：2026-10-08。范围仅 Runtime `context_budget.rs`、`context_pack.rs`、`server.rs` 的上下文 finalization 调用区和直接回归测试。Root 唯一负责构建和实际测试；本任务不修改 Core 权限、媒体、模型额度、profile 或 API。

## 实际缺陷与证据边界

`build_context_pack` 使用 64 个零的 `id`/`contentHash` 和 `byteSize=0` 创建候选。`fit_optional_context` 在这些字段上核对模型条目预算，随后 builder 才确定 byteSize、SHA 和 ID。server 在最终模型条目超限时仅拒绝，不再裁剪原本合法的可选资料。bound 路径在缓存完整可见来源后插入投影，fit 使用旧 hash，随后重算一次，也有相同空隙。相同 UTF-8 字节长度不代表相同分词数。

只读调用当前 Engine tokenizer，严格使用排序 JSON 和生产同构的转义 MCP/model envelope：可选前文短句 719 次时占位包为 8976 tokens，最终真实 SHA/ID/byteSize 包为 9008；720 次为 8986→9011，721 次为 8998→9036。包均小于 27KB，必需目标单独仅 332 tokens。Engine 绑定 SHA 为 `8bcaf73a677796e479a3b21cc383b58601973bfb22f92f1a7b240c54fcae7085`；小元数据保存为 `/tmp/autonomous-v3-context-finalization-tokenizer-witness-20261008.json`。这是确定性算法反例，不是 HTTP 重跑；current12 的单条 422 日志不能单独证明该次请求的唯一原因。

真实必需正文超过 9000 tokens 时仍须拒绝，不能截断目标稿、移除冻结输入或把 422 改成空正文成功。把 101 章 fixture 的请求缩为 10KB 不能认证原可选 fit 正确。

## 当前合同与实施顺序

1. 先只加入确定性回归测试，保留旧生产实现；静态冻结后由 Root 实跑旧实现并记录失败。失败必须指向仍可移除的可选正文导致最终模型条目超限。
2. Root 确认后实施唯一 finalizer：更新实际 coverage 计数，求 byteSize 固定点，按排除 `id`/`contentHash` 的当前完整对象计算 SHA，并设置真实 hash/ID。该函数只更新已有 Context 派生元数据。
3. 每轮用实际 finalized 候选判断原 9000 tokens 和 byteBudget，保留 200 字节可选安全余量。超限只裁已有合法 optional，再重新生成真实候选；必需正文、计划、事实、冻结 modeling source 和当前 target 保持。
4. Bound builder 仍先构造完整当前可见 sections，再由 Core 原路径缓存和冻结来源；server 插入全部 source/mandatory 投影后才最后 fit。不得提前裁剪 bound cache。
5. builder 的早期 scenePlanDraftLinks loop 也使用同一 finalizer 检查候选；保留 bound Task 与 `frozenPlanReferences` 的不裁剪例外，以及现有 scope、来源和截断声明。只使用原预算，不新增链接读取权限。
6. Root 统一构建新实现并运行完整直接回归、原 context-scope HTTP、必需正文超限负例及当前全量。所有旧失败原样保留。

## 五视角实施前评审

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构专家 | APPROVE WITH CONDITIONS | Runtime 只有一份 finalization 算法；Core 事实、权限、来源缓存与模型循环不变，不新增第二预算或兼容入口。 |
| 技术专家 A | APPROVE WITH CONDITIONS | 实际最终转义包仍按 9000 tokens/原字节预算检查；仅移除原有合法可选资料，必需正文、scope、冻结来源和锁定保护不能改变。 |
| 技术专家 B | APPROVE WITH CONDITIONS | 先获得旧实现确定性真实失败，再用新实现复验；独立重建 hash/byteSize、coverage 和幂等，并保留真实 required overflow 与 bound/source HTTP 强验证。 |
| 产品经理 | APPROVE WITH CONDITIONS | 可选资料可依法裁剪时不误报必需输入超限；真实必需超限仍诚实停止，不要求作者理解内部 hash 或无端调参。 |
| UI/UX 设计师 | APPROVE | 不新增 UI、对话框或技术文案，不以隐藏错误或减少原文证明改善表面体验。 |

零兼容门禁 PASS：当前唯一契约；没有版本判断、旧字段 fallback、双读双写、迁移、回填、降级、额度扩大或缺字段猜测。综合 APPROVE WITH CONDITIONS；本次预审时仅准备回归，生产修改和真实终态尚未发生。

## 实施后复核

Root 在旧生产实现上实际运行精确新单项，结果为 0 pass / 1 fail / 12 filtered、actual exit101；业务耗时 0.56 秒，失败明确为仍保留可裁前文的最终响应为 9008 tokens。完整旧日志 `/tmp/autonomous-v3-finalization-old-regression.stdout.log` 及主任务 result/source/binary 证据保留。该结果仅证明此单项旧实现失败，不冒充全库结果。

实际生产变更已完整落盘：`context_pack::finalize_context_pack` 统一更新真实 coverage 计数、byteSize 固定点与 SHA/ID；`context_budget` 每轮先生成真正 finalized 候选再按原 9000 tokens、byteBudget 和 200 字节可选余量检查。builder 的 scenePlanDraftLinks loop 使用同一 finalizer，裁剪声明在下一次候选测量前更新；bound Task 和 frozenPlanReferences 保持不裁剪。server 仍在 `project_task_sources` 完整来源缓存后最后 fit，删除重复且过晚的 hash/byteSize 计算。其他 server 分支、Core 权限、原目标、角色、当前性、水位、媒体、API、profile 和额度没有修改。

直接回归覆盖确定性占位→最终包误拒绝、真实 hash/byteSize/计数重建、重复收尾幂等、48KB 必需正文仍超限且不可删除、合法场景链接裁剪及 bound/frozen plan 原输入不提前裁剪。原三个最小单位 fixture 仅补齐当前 Context 派生元数据，没有为缺字段新增生产 fallback。独立代理只读确认三处 finalization/cache 次序一致，无新增阻断。

五视角静态后审：架构 APPROVE（唯一 Runtime finalizer，来源缓存与领域事实权威不变）；技术 A APPROVE WITH CONDITIONS（保留原额度、必需正文、scope 和冻结来源，待实际新回归）；技术 B APPROVE WITH CONDITIONS（补丁、模块及当前 fixture 同步，旧真实失败保留，静态格式/差异检查通过，完整新单位与原 HTTP 待 Root）；产品 APPROVE WITH CONDITIONS（修正可选资料误拒绝机制，未以静态修复宣称实机自主成功）；UI/UX APPROVE（无界面或确认流程新增）。零兼容门禁 PASS。

本任务没有自行构建、重跑 HTTP、调用模型或服务商。补丁静态冻结后由 Root 统一完成下述实际直接回归；新单项不代替全组通过，真实模型 QA 待授权也不与本轮源码完成混淆。

## current13 实际直接回归与五视角后审

Root 的 `/tmp/autonomous-v3-final-direct13/manifest.json` 已完整终态：Runtime lib 15/15、authoring 12/12、context-scope 12/12、workspace reports 4/4，共 43 pass / 0 fail / 0 ignored / 0 filtered，各目标 actual exit0。所有执行入口源码与真实测试二进制的 before/after SHA 相同；profile 保持 opt0、debuginfo0、debug_assertions=true、overflow_checks=true。完整 stdout/stderr 与 manifest 位于同目录，未过滤新反例或原正负例。

context-scope 使用恢复后的原 `budget=8&byteBudget=20000` 101章正例，最终真实模型条目、byteSize/hash、目标正文、冻结范围、完整分页/水位及角色权限全部原强断言通过；48KB 必需正文仍实际拒绝且错误结果不交付正文。Runtime lib 中确定性旧失败反例、新最终包计数/哈希/幂等、sceneLinks 可选裁剪和 bound/frozen plan 保留均实际通过。authoring 与 reports 的真实同 Work、报告全文回读、修稿 writer 隔离、原作者与预算回执断言保持。

五视角实际后审：架构 APPROVE（统一 finalizer 与缓存后传输裁剪，唯一领域权威保持）；技术 A APPROVE（43项实际断言保留必需原文、范围、9000 tokens、字节预算、角色权限与真实来源）；技术 B APPROVE（旧实现单项 actual101、新完整直接组43/43实际exit0，源码/二进制身份稳定，未过滤或弱化断言）；产品 APPROVE（已验证本次可选资料误拒绝修复及原自主创作机制，未宣称真实供应商文学质量）；UI/UX APPROVE（无界面新增或用户调参要求，真实错误与结果边界保持）。零兼容门禁 PASS。

旧 `/tmp/autonomous-v3-finalization-old-regression.stdout.log` 的 actual exit101、0/1与9008-token失败完整保留，不倒改为通过。当前43项只认证本需求直接回归；最终完整 Native workspace、原完整 Node201 尚在主任务执行，结果未出。本记录不宣称全量或真实 DeepSeek 创作验收通过。
