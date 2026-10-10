# 委派目标与按需正文来源当前合同同步

日期：2026-10-08。以下五项为评审视角，不代表五位独立人员。

## 根因与实施范围

冻结 8 原二进制 `delegation_targets_http_tests` 实际 exit 101、3/5。`focused_target_survives_prose_truncation_and_registers_both_operations_without_story_writes` 在原第 199 行、`project_index_is_byte_bounded_and_never_implies_complete_prose_or_writer_authority` 在原第 261 行对 relevantText.as_array().unwrap() 失败。完整 stdout/stderr/exit 保留于 `/tmp/autonomous-v3-native-group1-current8/`，不覆盖原失败。

源码确证：`native/story-core/src/task_sources.rs:186、229–233` 把 /relevantText 投影成真实 sourceId/textDelivered:false；`native/story-runtime/src/server.rs:1396–1401` 对绑定任务先注册授权范围内最多 80 章的冻结来源，query.budget 不再删除已准入正文；`server.rs:1475–1481、2088–2109` 保留原文、注册来源后才拟合传输包。原 relevantText 数组假设、绑定任务 budget=1 只含一章假设均已失效。真实来源应包含本 fixture 当前授权的全部 2/6 章，不能继续要求丢弃本章长正文。

只改 `native/story-runtime/tests/delegation_targets_http_tests.rs` 与本独立评审。用精确 task/thread/revision/source/hash 的真实 raw API，从 byte 0 连续读回完整 JSON；逐页校验 2,000 bytes 原文、6,500 bytes 响应、总长度和 SHA256。绑定传输入口不含原文；来源正文逐章与已保存原文、版本、对象身份完整等值。保留未绑定 Author 预算 1 的真实入口裁剪，以及现有 768 bytes 整体目标元数据、精确委派身份、两种实际行动登记、无正文写入、writer 无委派权限、历史源不可 rebase、变更拒绝、晚章节定位、转义溢出和 focused 越界拒绝。另验证 writer 授权正文不包含后章，且不能借另一任务来源扩展可见范围。

不改生产、共享 helper、授权、来源缓存限额、正文/响应预算或业务超时，不构建，不向外部供应商发送内容。修后验收由主任务集中构建当前二进制。

## 实施前五视角

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 按需来源为唯一正文交付合同；目标身份仍是 sourceIdentityOnly，不等于写权限。 |
| 技术 A | APPROVE WITH CONDITIONS | 回读必须真实、完整、有界、source/hash/字节连续，writer 后章和跨任务来源不可进入授权范围。 |
| 技术 B | APPROVE WITH CONDITIONS | 保留全对象目标、实际两种 action 和所有原负例；旧传输裁剪目的在未绑定入口验证，不用来源引用存在替代正文断言。 |
| 产品 | APPROVE | 目标定位不依赖模型先获得内联长正文；实际需要原文时能完整读取，未阅读不伪报完成。 |
| UI/UX | APPROVE | 只同步当前测试，无用户技术入口或重复界面。 |

综合：批准当前 fixture 同步。主任务已明确授权所有权。零兼容门禁 PASS：没有旧数组 fallback、版本检测、迁移、双轨或门禁降低。

## 实施后复核

实际只改目标 fixture。focused 用真实 source API 连续回读两章，逐章对象、版本和完整 JSON 正文等值；30,000 bytes 的第二章正文跨真实页回读，未被入口省略或截断，入口仍无内联长正文。project index 保持现有 768 bytes、1–4 个完整目标和精确 coverage 全对象等值；绑定来源读取全部六章完整内容。writer 实际来源仅第一章，尝试借 advisory task 的六章 sourceId 必须得到 task_source_not_in_catalog，不能得到 payloadPart。未绑定 Author budget=1 实际只返回一章，保持传输裁剪目的。

原两种 action 的真实登记与精确 target、无 story writes、writer/unbound author 无 delegationTargets、历史 revision 不 rebase、源变更拒绝旧 action、第 30 章可按需定位、第 31 章无源、focus 跨章拒绝、转义身份的整体 768 bytes 超额拒绝均未删除或放宽。子任务、caller lineage、公私知识和任务来源隔离的其他测试文件未编辑；本次不把这些其他测试的当前运行结果预先当作通过。

仅 `rustfmt --edition 2024` 格式解析目标文件 exit 0、作用域 git diff --check exit 0。未构建或运行修后目标。原 3/5 保留；需要主任务集中重建并运行 `novel-codex-story-runtime --test delegation_targets_http_tests`，当前状态为“已同步，等待实际终态”。源码现冻结。

| 实施后视角 | 结论 | 条件及证据边界 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 正文源与身份元数据仍各自承担真实内容/定位职责，入口不恢复内联，不从身份推导写权限。 |
| 技术 A | APPROVE WITH CONDITIONS | 完整源/hash/连续分页/真实页预算及 writer 后章/跨任务拒绝均有强断言；待新二进制证明实际行为。 |
| 技术 B | APPROVE WITH CONDITIONS | 原 3/5、格式解析与待业务结果明确区分；保留原全部目标及授权断言，不把引用存在当作全文交付。 |
| 产品 | APPROVE | 模型能按需读取完整资料，目标选择不依赖内联正文或假设入口即全文。 |
| UI/UX | APPROVE | 无产品界面变更、重复入口或技术文案新增。 |

零兼容后审 PASS：无旧数组 fallback、schema 升级、双轨、生产限额变化、权限扩大或超时增加。
