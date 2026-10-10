# 候选伏笔来源与预算当前合同同步

日期：2026-10-08。五项为评审视角，不代表五位独立人员。

## 根因与最小范围

当前冻结 8 的 `commitment_proposal_budget_tests` 原二进制实际 exit 101、0/1，`planning_pack_trims_whole_proposals_and_rehashes_accurate_coverage` 在第 116 行对 candidateCommitmentProposals 调用 as_array().unwrap() 失败。原 stdout/stderr/exit 保留于 `/tmp/autonomous-v3-native-group1-current8/`。

源码确证：`context_pack.rs:257–259` 为绑定任务先保留完整可见资料；`server.rs:2088–2109` 先注册真实来源，再对传输包预算及内容 hash 校验；`task_sources.rs:186、229–233` 把候选数组投影为 sourceId/textDelivered:false。数组仍由授权来源读取，不在入口包内交付。旧测试没有读取来源，尚未进入实际裁剪或 hash 判定；不能据此放宽生产。

只修改 `native/story-runtime/tests/commitment_proposal_budget_tests.rs`。在原 8 条候选内把合法 summary 从重复 30 次调整到 60 次，触发现有 `commitment_proposal_context.rs` 的固定 8,000 bytes 投影上限；保持 2,000 bytes 单条上限和原 context/page 限额。通过实际 raw API 连续读回有界来源，校核每页 6,500 bytes、source/hash/字节连续性、全 payload/hash、完整 JSON 和未截断候选字段。验证 Native 仅删除完整条目，coverage 与实际条目数/预算一致；紧 context 预算严格拒绝而不删冻结源；rolling plan 回读合法同源投影，包与来源 hash 均保持强等值检查。

不改生产、共享 helper 或主实施评审，不构建、不扩权限、不增加预算，不用 fallback 接受旧数组。旧失败继续作为原失败记录，修后通过须由主任务统一当前二进制认证。

## 实施前五视角

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 按需来源与原 Native proposal selection 仍是唯一权威；测试不要求重新内联正文。 |
| 技术 A | APPROVE WITH CONDITIONS | 必须真实读取精确 task/source/hash，各页有界、连续、完整；预算不足不得无证裁剪冻结源。 |
| 技术 B | APPROVE WITH CONDITIONS | 保留裁剪、coverage、内容 hash 和预算强断言，真实触发 Native 上限；保留原 0/1，不声明修后已通过。 |
| 产品 | APPROVE | 用户无需处理来源分页；候选仍未确立，预算裁剪不能截断原文或伪报完整覆盖。 |
| UI/UX | APPROVE | 只同步测试，无新界面或技术状态入口。 |

综合：批准当前合同同步。主任务已授权本 fixture 所有权。零兼容门禁 PASS，没有旧字段 fallback、迁移或双轨。

## 实施后复核

已只改目标 fixture：原 8 条候选保持，合法 summary 重复 60 次以触发已有 8,000 bytes source-selection 限额，断言保留数小于原数且非空；逐项核对原 summary、fulfillmentCriteria、setupEvidenceRefs、kind、来源版本、章节、候选身份和未确立状态。验证 ID 唯一、被省略候选及数量、scannedCandidates、includedEntries、truncated/complete、单条 2,000 bytes 和整体 8,000 bytes 实际预算。来源按真实 Native sourceId/hash 从 byte 0 完整回读，各页最多 6,500 bytes，连续字节、总长、完整 JSON 和 SHA256 均校验。

紧 context 预算仍用原 roomy 包长度减 300，要求当前明确 422 context_budget_exceeded，而非删掉被冻结 proposal source。失败后原来源完整回读等值。随后在 59,999 bytes 再取得合法 context，检查包实际 byteSize、完整内容 hash 和原 sourceId 不变；与 60,000 bytes 原包的 hash 差异有明确 budget 字段变化依据，不依赖原文阅读是否改变 parsed coverage。rolling plan 对同一合法章节资料读取同一完整 proposal 投影，includedEntries 精确等于真实条目数，不再把旧传输阶段裁成 1 条当作内容合同。

`rustfmt --edition 2024` 仅格式解析目标测试文件 exit 0；作用域 git diff --check exit 0。没有构建、运行修后二进制、放宽生产限额、改变权限或编辑共享 helper。需要主任务集中重建并验证 `novel-codex-story-runtime --test commitment_proposal_budget_tests`；源码冻结，当前状态仅为“已修改，等待统一自动化终态”。原 0/1 保留，不计为通过。

| 实施后视角 | 结论 | 条件及证据边界 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 全文通过当前授权来源按需读回；Native projection selection 与冻结来源保持唯一权威，没有恢复旧内联表示。 |
| 技术 A | APPROVE WITH CONDITIONS | 页/单条/总量预算、精确 source/hash/完整性、原文不截断与错误预算拒绝均为强断言；还需真实修后二进制验证。 |
| 技术 B | APPROVE WITH CONDITIONS | 0/1 原失败、格式解析与源码同步分别记录；未把差异检查当成业务通过。所有旧文学候选属性、准确 coverage/hash/预算目的保留。 |
| 产品 | APPROVE | 候选只按完整条目省略，未读和不完整覆盖诚实表示，不把 proposal 升为既定事实。 |
| UI/UX | APPROVE | 无界面变更；用户不承担底层 source 分页或 hash 操作。 |

零兼容后审 PASS：没有旧数组 fallback、迁移、双轨、限额提高或门禁降低。修后结果待主任务记录。

## current 9 实际终态

主任务集中构建的当前测试二进制实际 exit 0、1/1，无忽略或过滤，业务 1.30 秒，首次启动总 exec 603.007 秒。原冻结 8 的 0/1 继续保留。日志与源码/二进制前后 hash 清单为 `/tmp/autonomous-v3-native-current9-fixtures/commitment_proposal_budget_tests.stdout.log`、同目录 manifest.json。

本项 source SHA256 为 `78c451224b2a43996465f040e2d9444736b38fc679ec65944c70b08c32885be3`；executable SHA256 为 `270b4135ecca9c9c18397c5dbb75a0dffd2f9b5c48f1a7d6547c02868dc91faa`。独立复核当前文件与当前实际二进制分别等于运行前后记录，没有以变化后的来源冒充验收。五视角对本 fixture 当前强合同后审更新为 APPROVE：架构依赖唯一 Native source，技术 A 的完整分页/预算/拒绝断言实际通过，技术 B 的内容及 coverage/hash 全部强检查实际通过，产品候选身份和不截断目的保留，UI/UX 无界面变更。零兼容 PASS。

此处仅认证该当前自动化合同；主任务完整 Cargo/Node 仍另行记录，不推导真实外部模型文学质量或前 100 章创作已完成。
