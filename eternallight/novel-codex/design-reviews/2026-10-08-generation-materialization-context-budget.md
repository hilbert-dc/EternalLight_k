# 生成源场景物化：当前目录预算 fixture 同步

日期：2026-10-08。仅 `native/story-runtime/tests/generation_author_source/materialization_tests.rs`；不改生产/模型限额/业务deadline，不build。五个职责视角并非五名独立评审者。

## 原失败与方案

冻结8旧二进制 `generation_author_source_tests` 12/13，唯一失败在大源案例的 14000-byte context：Native 明确返回 context_budget_exceeded。原日志 `/tmp/autonomous-v3-native-group3-current8/generation_author_source_tests.stdout.log` 保留。当前 context 在 byteBudget 内保留所有强制来源目录与身份，原文分页；14KB并不是当前必需目录的固定可承载保证。原初始60000-byte包已真实成功且仍受9000模型token硬限额，本次不改这些生产边界。

保持原测试名称、全部13案例、所有来源revision/场景结构/事件UTF8位置、连续raw-source UTF8页和完整hash、合法passage查询全原文覆盖/分页终止、4096-byte明确拒绝、作者变更后的stale拒绝。仅大源情景将正文重复数从350改为1000，原6000空格仍保留，使实际UTF8原文大于24000-byte context；读取24000-byte context并严格核对实际包字节与真实escaped MCP model-item≤9000。原文字符总量保持实际passage每页400×2、少于20页的既有限制，不能放宽页数断言。短源其余情景100重复不变。

未用索引替代正文、未删除完整断言或新授权限；真实原文始终从当前Task与source/hash分页返回。Root唯一编译后须运行完整 generation_author_source_tests，未实测前不记通过。

## 五视角实施前评审

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 当前有界目录运输；不将旧14KB最小假设迁入生产或新增兼容 |
| 技术A | APPROVE WITH CONDITIONS | 实际原文大于context预算，UTF8有界连续分页/hash/范围及权限负例保留；模型9000不变 |
| 技术B | APPROVE WITH CONDITIONS | 全13案例实际退出，原失败日志留存；不build、不过滤/忽略/绕过实际来源 |
| 产品 | APPROVE WITH CONDITIONS | 大源可读取且冻结身份清晰，目录不证明阅读/理解；原输入保护保留 |
| UI/UX | APPROVE WITH CONDITIONS | 无新UI或用户参数；仅验证资料运输与范围，不证明小说质量 |

综合 APPROVE WITH CONDITIONS；零兼容门禁 PASS，无schema迁移/回填/旧字段fallback/双轨/安全降级。没有获准兼容例外。

## 实施后复核

待Root当前完整13案例编译与退出。未经实际结果不能把预算调整记为生产缺陷修复或大源验收成功。

## 当前实测后审

Root current10b 完整 `generation_author_source_tests` 实际 exit0、13/13、0 ignored/0 filtered，日志 `/tmp/autonomous-v3-fixtures-current10b-nodebug/generation_author_source_tests.stdout.log`。真实源大于24KB context、每页有界 UTF-8/完整 hash/场景与事件范围、可见原文与父来源阅读证明及错误/过期/预算拒绝负例均通过；test profile 的 debug_assertions/overflow_checks 保持 true，binary hashes 和实际退出记录在同目录 manifest。五视角后审 APPROVE；这是来源运输验证，未增加模型额度或宣称创作质量通过。
