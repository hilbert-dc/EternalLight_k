# 删章检查测试：当前任务来源读取合同

日期：2026-10-08。范围仅为 `native/story-runtime/tests/chapter_structure_http_tests.rs`；不修改生产实现，不启动构建，不放宽权限或预算。五角色为本次实现的五个审查视角，不代表五位独立人员。

## 原失败与当前合同

冻结 8 的真实已编译二进制 `chapter_structure_http_tests-bbf7a51d70ea5001` 默认执行 2 项，实际 exit 101、1 pass / 1 fail。原证据保留于 `/tmp/autonomous-v3-native-group3-current8/chapter_structure_http_tests.stdout.log` 和同目录的 `chapter_structure_http_tests.result.json`。失败点为第 289 行：测试在 `context.operationContext.chapterStructureChange.source` 读取完整来源对象，实际为 null。

当前 `task_chapter_structure.rs::chapter_structure_context` 仍返回完整已删除来源元数据（对象、保存/采用/工作版本、知识批次），并标注 `removedHistoricalSource` 与尚未交付正文。`task_sources.rs::project_task_sources` 将 `/operationContext/chapterStructureChange` 的完整对象保存到受当前 Task、来源政策和内容哈希约束的来源目录，再把展开字段替换为 `{sourceId,textDelivered:false}`。因此正确验收是先发现当前目录、再通过实际 Task 原文读取接口完整读取该对象；不能恢复无必要 eager 字段或直接用数据库记录替代读取证据。

## 预审

- 架构专家 — APPROVE WITH CONDITIONS：保持 Native 来源构造、可见性与 Task 绑定的单一权威，仅同步 fixture 读取方式。断言完整对象（保留事件来源、版本、批次、邻章、权威与覆盖状态），不新增旧字段 fallback。
- 技术专家 A — APPROVE WITH CONDITIONS：每页检查 HTTP 状态、精确 sourceId/sectionPath/contentHash、连续 UTF-8 字节范围、大小上限；结束时核对总字节与完整 SHA-256，禁止只读目录就宣称完成。增加错误工具与无根谱系子线程读取的负例；保留既有角色游标、篡改游标、已终态和只读写入拒绝。
- 技术专家 B — APPROVE WITH CONDITIONS：只修改这一测试文件与本评审；不删除原版本、预算数量、来源正文、恢复新 Attempt 和陈旧输入断言。由 Root 统一构建新二进制后才能获得当前 fixture 的通过证据，原冻结 8 失败不得改写。
- 产品经理 — APPROVE：验收应证明删章后仍能在既有只读 Work 内追溯真实来源，且没有新增预算或写回故事。来源转为按需读取不能降低真实性。
- UI/UX 设计师 — APPROVE：此次无界面修改；保留当前简洁目录和按需读取，避免为了过测试重新暴露技术元数据到创作界面。

综合结论：APPROVE WITH CONDITIONS。未发现生产元数据遗漏；实际 HTTP 完整读取和权限边界仍须由新二进制复验确认。若真实接口缺失元数据或扩展可见范围，应立即报告，不能靠 fixture 填值掩盖。

## 测试阶段零兼容门禁

- 当前 schema 空库和当前接口；不新增版本判断、fallback、双读双写、迁移、回填、降级或 deprecated 路径。
- 不改生产，原负例和数据、故障恢复、预算断言全部保留。
- 来源目录不证明内容已读；只接受实际完整传输与完整对象断言。
- 构建由 Root 集中执行；不将旧二进制或中间测试通过写成当前全量通过。

## 实施后复核

实际修改仅本测试文件：通过本轮 Context 的精确 `sectionPath` 发现 `sourceId`，确认 Context 为当前按需读取标记；通过受当前 Task/inputRevision/rootThread 绑定的实际 `task-source` HTTP 接口连续读取完整原 JSON。每页核对 sourceId、sectionPath、hash、UTF-8 字节范围、总量与传输大小，最后核对完整 SHA-256；将完整对象与实际删章事件（移除 assessment、加上当前 authority/coverage）逐字段整体比较，因此原对象 ID、保存/采用/工作版本、知识批次、前后章节、事件版本与快照信息均仍有严格断言。来源正文的原 saved/working 读取、资金和任务数量、readonly 输出限制、游标/新 Attempt/陈旧输入负例全部保留；增加错误工具、缺少父谱系的子线程、Task 完成后再次读取 lazy 元数据拒绝。

五视角复核：架构与技术 A/B 确认无生产变更、来源并未由 fixture 合成作为实际读取结果、没有跨来源拼页；产品与 UI/UX 确认只读删章评估和当前按需阅读保持一致，未恢复技术信息展开界面。零兼容门禁 PASS。未改第二用例，未放宽原负例。

静态验证：`rustfmt --edition 2024 --check native/story-runtime/tests/chapter_structure_http_tests.rs` 实际 exit 0；限定文件 `git diff --check` 实际 exit 0。Root 集中 current9 构建后的真实 2 项定向验收已完成：actual exit 0，2 pass / 0 fail / 0 ignored，Rust 测试耗时1.68秒；macOS首次启动等待计入外部耗时624.97秒，不混作断言耗时。证据 `/tmp/autonomous-v3-native-current9-fixtures/manifest.json` 与该目录 `chapter_structure_http_tests.stdout.log`；source SHA-256 `b3a807d560f90c4f9f18b746891da0c9039996ca13d6ac183204c0cceae122b3`，bin SHA-256 `d416108c895170e1c8130a43186c18b13aaa16bcb418c0c82f1ae5586aa1769b`，执行前后均相同。原 current8 的1/2失败完整保留。本次没有启动构建，current9的2/2也不等于之后workspace生产改动的全量通过，后续统一当前批次仍由Root验收。
