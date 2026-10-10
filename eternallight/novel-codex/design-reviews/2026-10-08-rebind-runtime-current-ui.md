# 关联接续集成测试与当前简洁界面

日期：2026-10-08。

## 变更概览

当前实际 Electron→HTTP→Native 关联接续测试等待旧 `.writer-runtime-preview` DOM。当前简洁工作区已不渲染这一技术预览，因此测试在进入关联确认之前超时。保留必要的正文、章纲比较、确认、错误和控制界面；不为测试恢复无必要 UI。

方案：作者侧通过真实 Native writer 范围读取接口确认原稿与当前不可变章纲绑定；重新关联后再次读取同接口验证新章纲与原稿；后续真实 writer 继续通过任务来源目录和完整 hash 校验的原文分页读取，证明实际任务使用了新章纲。仅同步测试 fixture 和对应域事实快照。项目 seed 由另一代理负责，不改动 seed。

## 五视角预审

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | Native 绑定与任务冻结来源为权威，不依赖额外 DOM 或创造新来源 |
| 技术A | APPROVE WITH CONDITIONS | 原正文修订 ID、绑定规划 ID、实际正文、任务来源 hash 与重启持久化断言均保持 |
| 技术B | APPROVE WITH CONDITIONS | 当前接口和快照一次同步；不恢复旧 UI，不改生产权限或 seed |
| 产品 | APPROVE WITH CONDITIONS | 核对新旧章纲和保留正文仍由真实界面完成；领域事实由 Native 验证 |
| UI/UX | APPROVE WITH CONDITIONS | 不向作者重新加入技术范围包；保留确认、未保存输入阻断及持久成功/刷新失败分离 |

上述为审查视角，并非五位独立评审者。零兼容门禁：PASS；没有 fallback、迁移或第二链路。当前范围是测试合同同步。

## 实施后复核

实际变更：fixture 使用真实 Native 作者范围读取，逐一断言当前候选修订、正文、不可变原/新章纲绑定、视角和故事时间；仍明确作者预览不等于任务冻结来源或语义认证。当前简洁 UI 不包含旧技术包。后续 writer 继续通过现有任务来源目录、完整原文分页与 hash 验证核对新章纲；迟到旧 writer 仍被拒绝，当前正文保持候选，建模/承诺提取/场景检查/独立检查/下一章规划及重启全部断言保留。

首轮 `/tmp/autonomous-v3-rebind-current-ui.log` 实际 exit1：已越过旧 DOM 阻断，后续发现 seed 只聚合 readyActions/receipts，没有聚合每页 requirements，取不到独立报告。seed 所有者独立同步完整分页，保留每页版本/来源以及 current/passed/精确报告原断言。本任务未改 seed。

快照同步轮 `/tmp/autonomous-v3-rebind-current-ui2.log` 1/1通过、实际 exit0，9.42秒。随后只读快照复验 `node --test electron/tests/rebind-runtime-integration.test.mjs` 的 `/tmp/autonomous-v3-rebind-current-ui-accepted.log` 实际 exit1、0/1通过：最终完成父任务时 Native 拒绝，指出作者原要求未由主任务完整读取。此失败保留，尚不能确认当前整条集成链通过。快照前两项仍为真实 UI 文案，后两项成为真实 Native 域事实，不伪装成已移除 DOM。scoped `git diff --check`通过。

五视角后审：架构、技术A、技术B 保留待验条件（当前完成父任务前必须取得完整作者原文读取证据）；产品待验完整接续终态；UI/UX PASS（未恢复冗余技术包，保留真实正文、确认和错误阅读）。均仅为本受控模型集成机制验收，不认证真实服务商文学质量。

当前复验：第二次 `/tmp/autonomous-v3-rebind-current-ui-accepted2.log` 实际 exit1（最终源强断言把父步骤的缩短输入当成原作者要求）；seed 所有者改为实际 generation-run 最初作者原话，并额外断言 UI 追加“保留作者藏信稿，按新章纲补全”。当前第三次只读快照复验 `/tmp/autonomous-v3-rebind-current-ui-accepted3.log` 实际 exit0，1/1通过，101.14秒。原失败全部保留。最终五视角后审均 PASS：当前原稿/新章纲域事实、完整任务来源读取、精确当前原作者要求与 UI 改向、完整接续及重启链有真实隔离 Native 机制证据；不认证实际服务商文学质量。源码仍处于并行实施阶段，该单项通过不等于统一全量通过。
