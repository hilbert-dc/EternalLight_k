# 正文证据错误与精确恢复合同 fixture

日期：2026-10-08。五项为评审视角，不代表五位独立人员。

## 原失败与源码确证

冻结 8 `modeling_evidence_path_tests` 完整默认1项实际0/1，原日志 `/tmp/autonomous-v3-native-group1-current8/modeling_evidence_path_tests.stdout.log` 保留。原118行只预期 invalid_request、空details和null recovery；实际仍严格400拒绝，并返回当前更精确的 invalid_source_evidence、原失败字段、错误reason、publicationState=rejected和允许局部修正的完整恢复指令。

`native/story-runtime/src/modeling_api.rs:372–395` 在校验当前正文证据、发布以前生成精确 feedback。未找到和歧义引文分别要求读取绑定原文逐字复制、扩展唯一引文或选择实际 resolve_story_evidence 返回锚点；不得猜偏移、复用其他revision偏移或原样重试。这个 error 不代表任意实体或断言已经发布。

## 最小方案与验收

仅 `native/story-runtime/tests/modeling_evidence_path_tests.rs` 与本独立评审，生产和主实施记录冻结，无构建、权限或超时改动。

- 四种失败继续使用同一真实正文版本与精确 entities[1].evidence、assertions[0].evidence/validUntilEvidence、assertions[1].epistemicUntilEvidence 原路径。
- HTTP仍严格400，并对完整error对象精确等值：code、message、field、reason、rejected发布状态、正确全文指令、correct_evidence、保留合法条目以及禁止原样重试。
- 每次拒绝后原完整真实SQL状态仍是 queued、entities=[]、assertions=[]、实体/断言/知识head全部0；真实纠正完整发布仍必须committed/ready和准确4/2/1计数。不得换成泛is_client_error或绕过正文锚点验证。

## 实施前五视角

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE | Native 错误和发布终态权威，测试只同步更精确的当前合同。 |
| 技术 A | APPROVE WITH CONDITIONS | 精确字段/原因/指令和拒绝原子性全部保留，未找到与歧义要各自严格指令。 |
| 技术 B | APPROVE WITH CONDITIONS | 原0/1保留，四完整error对象断言替换旧错误包装；实际当前二进制待统一构建。 |
| 产品 | APPROVE | 模型可根据具体证据问题做局部修正，不把失败当已发布。 |
| UI/UX | APPROVE | 仅fixture，不增加内部字段或界面冗余。 |

零兼容门禁 PASS：没有fallback、旧格式分支、迁移、权限扩大或门禁削弱。综合允许本fixture同步；实际1/1结果由主任务集中当前构建提供。

## 实施后复核

实际增量 `modeling_evidence_path_tests.rs:84–135` 用两个精确当前恢复全文指令构造四个完整error对象，仍严格400；保留原字段路径、引文、reason和当前发布状态，新增 preserveValidEntries=true、retryUnchanged=false 和 correct_evidence 精确断言。`:136–160` 原真实SQL全对象原子性和最终committed/ready及4/2/1准确计数全部未改。此前当前项目建库 medium/idempotencyKey 字段已存在冻结8，本增量不改。

| 视角 | 后审结论 | 证据与边界 |
| --- | --- | --- |
| 架构 | APPROVE | 未改生产发布、错误或权限；只验证精确当前错误契约。 |
| 技术 A | APPROVE WITH CONDITIONS | 四字段与全文反馈、真实SQL无部分写入保护保持；当前1项执行待集中构建。 |
| 技术 B | APPROVE WITH CONDITIONS | 原0/1日志保留，没有泛状态检查或跳过；不能把格式检查当业务实测通过。 |
| 产品 | APPROVE | 精确修正指令没有伪造成功或让模型原样重试。 |
| UI/UX | APPROVE | 无界面和操作增量。 |

限定 rustfmt actual exit 0、限定文件与本评审 `git diff --check` actual exit 0。未构建，当前1/1尚未认证。零兼容复查 PASS，无fallback、版本判断、迁移、限额/超时/OS或权限改动。剩余项仅主任务当前二进制默认完整验证。
