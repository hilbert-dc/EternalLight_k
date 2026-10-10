# 当前 handoff 与 context-budget lazy 来源验收合同

日期：2026-10-08。范围仅 `story-runtime/tests/generation_handoff_tests.rs`、`context_budget_tests.rs`。原 current8 二进制失败 stdout/stderr/exit/hash 保留在 `/tmp/autonomous-v3-native-group3-current8/`。生产代码、权限、预算和原负例保持当前合同；统一构建由 Root 执行。

## 首因与方案

handoff 的 10 个失败都停在公共 complete_scope 的 Native 原作者要求完整读取门禁（400，旧预期200）；该 fixture 没有从当前 read_context 的 authorRequirementsSource 实际连续读取作者原话。补每个模型保存计划前的当前 task/root/Candidate 来源发现与全文 raw 读取，逐页校验 sourceId、sectionPath、同一 hash、连续 byteRange、最大2000 UTF-8 字节/6500响应和实际 model-item 9000token；最终解析内容须精确等于冻结 manifest.authorIntent 和 authority/intentRevision。目录查阅或 Author/数据库读不能代替。所有 handoff/暂停/作者变更/精确scope/预算/恢复和未知负例断言保留。

context-budget 在旧 inline relevantCommitments 数组 unwrap 失败。当前 pack 的正文、承诺和知识是 lazy source references；预算检验必须检查实际有界目录与从获准来源实际读取到的完整 scoped 内容，不虚构每个较小 pack 会丢掉已经转移到来源的承诺。保留 target capability 原绑定、精确预算字节/hash/真实 MCP escaped item 9000token、第三章目标原文、第四章资料隔离、承诺候选权威和未验证状态、workingKnowledge POV规则、预算不足拒绝和跨readingChapter拒绝。文本完整性来自逐页原文来源，不代读库；知识 metadata 对照来源实际内容。

## 五角色预审

- 架构专家：APPROVE。仅同步验收当前唯一 lazy source 合同，没有固定创作流水线、兼容入口或生产改动。
- 技术专家 A：APPROVE WITH CONDITIONS。根 Candidate/真实 task/thread/revision/cap 读取，原话精确比对、完整UTF-8分页/hash和当前9000模型项上限；不能放宽 author/unknown/预算/scope。
- 技术专家 B：APPROVE WITH CONDITIONS。保留旧失败；公共 helper 不能自动修补权限或作者输入；Root 统一构建后完整11+1真实终态才通过。
- 产品经理：APPROVE。检查实际模型收到的内容和范围，避免把目录误当原文或由测试作者代读。
- UI/UX 设计师：APPROVE。无 UI 变更；按需来源合同减少正文噪音且不误报阅读/理解。

零兼容门禁：PASS。无版本判断、字段 fallback、ALTER、迁移、回填或旧 inline 路径；当前fixture直接新建当前schema。综合 APPROVE WITH CONDITIONS，实际运行待统一构建。

## 实施后复核

实现后静态复核与当前实际11+1运行已完成，均由 Root 统一构建、完整默认 assertions 执行。handoff 在每次 save_plan 前获取当前 root/Candidate authorRequirementsSource，完整 raw 分页并比对 immutable hash 和原 manifest authorIntent；不更改任何原负例/范围/预算/作者门禁。context-budget 用实际 scoped raw sources 读取承诺/第三章正文/workingKnowledge，保留现有承诺域12000字节选择的真实 truncated+includedEntries，较小 metadata pack 不伪造新的来源正文截断。实际模型项按当前6500响应/2000 UTF-8页/9000tokens escaped wrapper强验；最低合法byteBudget=4000实际拒绝无法容纳的mandatory metadata。

五视角后审：架构 APPROVE（fixture直接当前合同）；技术 A APPROVE（强身份/连续hash/范围/预算负例均保留并实际通过）；技术 B APPROVE（完整11+1 actualexit0，源码与二进制执行前后无漂移，原失败保留；完整工程另验）；产品 APPROVE（验证实际模型读取，目录不冒充阅读）；UI/UX APPROVE（无UI与额外流程）。零兼容 PASS；生产没有修改。

## current10c 实际证据

执行 manifest：`/tmp/autonomous-v3-workspace-current10c-nodebug/manifest.json`，complete=true。profile 为 `debug_assertions=true`、`overflow_checks=true`，关闭 debuginfo；两组均完整默认 assertions，无 ignore/filter。

| 当前组 | 结果 | stdout | source hash | binary hash |
| --- | --- | --- | --- | --- |
| generation_handoff_tests | 11 pass / 0 fail，actual exit0 | `generation_handoff_tests.stdout.log` | `e069236ed38a88ba70518b54699dfcc86b1fe040facc486a949c0a2d337c232e` | `2521d6c91692be4f7147dd0599ce6507312f0501c4843824412a44ddbafc6c3d` |
| context_budget_tests | 1 pass / 0 fail，actual exit0 | `context_budget_tests.stdout.log` | `d5a16d9c0fdb2d08aa184278b38d640332fee95495cf9e9e405ff9756ed2dec2` | `c40234684fba6ca5cdeb941af24e754f79f906576d11a6b39a39410bba177725` |

表中 stdout 位于 manifest 同目录，source/bin before/after hash 完全相等。源与旧独立预审冻结保留 `/tmp/autonomous-v3-handoff-context-current10-static-frozen.json`，后审文档补录另存新冻结清单。current8 handoff 的10失败、context-budget的1失败及原完整 stderr/exit/hash保留在 `/tmp/autonomous-v3-native-group3-current8/`，没有重写运行器结果。

当前11+1通过只证明这两组当前 Native lazy 来源与门禁合同，没有证明外部 DeepSeek 文学生成或前100章交付；完整 Native workspace、Node 全链、实机验收仍待统一证据。
