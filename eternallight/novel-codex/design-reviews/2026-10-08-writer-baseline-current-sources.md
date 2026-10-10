# 当前验收 fixture 同步预审

日期：2026-10-08。影响仅 `native/story-runtime/tests/writer_baseline_http_tests.rs`；不改生产、授权、schema、共享 transport 或实际应用，不构建。按五个职责视角审查，不冒称五名独立评审者。

## 方案与当前验收

task-bound context 有意取消旧 requiredSourceReads 全文入口；当前 taskSources 的 relevantText 实际源保存冻结整章资料，task-source-content 有界分页。

用当前 taskSources/sectionPath 的真实 sourceId/hash 完整读取 parsedFields，连续字符/entry coverage、UTF8页额度与模型额度、重建 hash，核对选定 revision/完整正文/场景；候选保存后再次读该冻结基准。原 writerBaselinePassages 所有分页 quote、完整覆盖、跨 task cursor 拒绝、selectedScenes 拒绝与作者选稿/empty baseline 负例全部保留。

原冻结8二进制失败日志保留在 `/tmp/autonomous-v3-native-group2-current8/`；Root 唯一编译后须完整运行原测试，未实测前不宣称通过。

## 五视角实施前评审

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 同步当前合同，无旧字段 fallback/第二读库/生产兼容 |
| 技术 A | APPROVE WITH CONDITIONS | 权限/选定版本/费用与真实证据强断言保持，实际 HTTP 才算读取 |
| 技术 B | APPROVE WITH CONDITIONS | 原测试完整运行，不过滤/忽略旧失败；由 Root 唯一编译 |
| 产品 | APPROVE WITH CONDITIONS | 完整冻结原稿与真实交付回执，不以目录或文本存在代替完成 |
| UI/UX | APPROVE WITH CONDITIONS | 不新增 UI，原文完整读取与费用状态不冒充文学质量 |

综合：APPROVE WITH CONDITIONS。零兼容门禁 PASS；当前破坏性运输合同直接替换，没有迁移、回填、fallback、双轨或保护降级。未获任何兼容例外。

## 实施后复核

Root统一冻结9编译后完整 `writer_baseline_http_tests` 默认断言实际 exit 0，2/2 passed、0 failed/ignored/filtered；日志 `/tmp/autonomous-v3-native-current9-fixtures/writer_baseline_http_tests.stdout.log`，manifest `/tmp/autonomous-v3-native-current9-fixtures/manifest.json` 记录二进制与主源哈希前后相同。原冻结8失败记录保持。

| 视角 | 后审结论 | 证据与边界 |
| --- | --- | --- |
| 架构 | APPROVE | 仅当前fixture同步，未建旧运输兼容或改生产 |
| 技术A | APPROVE | 原完整版本/权限/费用/回执负例与强equality保留，实际接口验证 |
| 技术B | APPROVE | 全2用例默认完整运行退出；不filter/ignore，无测试保护放宽 |
| 产品 | APPROVE | 保存原文、费用已结、真实回执与目标完成区分保持 |
| UI/UX | APPROVE | 无UI修改/新作者操作，不把机械fixture通过当文学质量 |

综合 APPROVE，零兼容门禁 PASS。此证据属于冻结9当前编译，后续生产新增 workspace 后仍由Root统一重验；不宣称全工程验收或100章已完成。
