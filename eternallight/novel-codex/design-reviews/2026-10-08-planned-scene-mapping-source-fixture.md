# 规划场景映射的当前资料运输合同

日期：2026-10-08。范围仅 `native/story-runtime/tests/planned_scene_mapping_http_tests.rs`；不改生产权限、写入或预算。以下为五个职责视角，并非五名独立评审者。

## 方案与验收

当前 task-bound context 将 scenePlanDraftLinks 放入不可变 taskSources，旧 fixture 从 context 内联数组读取而失败。通过当前 sourceId/hash、原任务身份和真实 HTTP 完整读取原始分页，核对连续 UTF-8 byteRange、结束证明、总长度和内容 hash，再断言两个规划场景均没有真实场景组或事件映射。

规划不等于实际事件。缺失 verdict、借用整章事件、外章场景、空场景映射、模型整章 body-only 保存与 partial 完成拒绝均保持；作者 body-only 保存、合法候选修订、原稿未采用以及正文和知识未被评价修改的断言保持。

## 实施前五视角评审

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 只切换到当前 source 合同，没有内联 fallback、直接读库或迁移 |
| 技术 A | APPROVE WITH CONDITIONS | 使用原 root/caller/task/thread，完整连续分页和 hash，目录不算内容读取 |
| 技术 B | APPROVE WITH CONDITIONS | 保留所有原报告、写入和状态断言；Root 统一重建并默认完整运行 |
| 产品 | APPROVE WITH CONDITIONS | 缺少映射诚实报告不足证据，规划效果不得冒充已实现 |
| UI/UX | APPROVE WITH CONDITIONS | 无新 UI；不以 fixture 通过冒充实机或文学质量验收 |

综合：APPROVE WITH CONDITIONS。零兼容门禁 PASS。没有预算放宽或授权扩大。

## 实施后复核

当前完整原目标已实际1/1通过、exit0、0 ignored/filtered；日志 `/tmp/autonomous-v3-fixtures-current10b-nodebug/planned_scene_mapping_http_tests.stdout.log` 与 manifest 对应，源码及二进制前后hash不变。五视角后审：当前分页来源合同与原映射/权限/保存/采用负例均保留，零兼容PASS；该目标通过不替代完整Native和实机文学验收。冻结8原失败保留在 `/tmp/autonomous-v3-native-group1-current8/planned_scene_mapping_http_tests.stdout.log`。
