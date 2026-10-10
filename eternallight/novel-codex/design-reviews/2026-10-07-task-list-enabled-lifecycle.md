# 空作品环境的任务读取生命周期

日期：2026-10-07。五项为审查视角，不代表五名独立审查人员。

## 变更概览

目标：未选择或尚未读就绪的作品不启动任务读取，也不显示接口缺失；真正启用后的接口缺失和读取失败仍明确可见。

范围：`useWorkTaskList` 的当前必需 `enabled` 参数、App 的该调用一行、当前 UI fixture 与隔离 Electron 验证。领域执行、Native、模型连接、作品数据及媒体逻辑不改动。

读取生命周期由 `{api, projectId, enabled}` 明确拥有。禁用时是未启用而非已成功读取：返回空数据、`loaded=false`、`reading=false`、无错误；刷新、分页和轮询无请求。启用后必须有实际 `listWorkTasks`；缺接口报错，调用失败继续显示真实错误。导航或禁用立即撤销旧结果的 UI 归属，再启用重新读取，不恢复旧成功状态。

## 零兼容门禁

PASS：新参数必填，全部当前调用一次更新；不新增旧入口、默认兼容参数、旧字段 fallback、双轨、迁移或回填。响应竞态及失败重试是当前可靠性要求。

## 实施前五视角评审

| 视角 | 结论 | 批准条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 启用状态与 API 能力分开；hook 只管理读取，不能把空库解释成任务成功或创作完成。 |
| 技术 A | APPROVE WITH CONDITIONS | 当前 scope live ref 与 cleanup 双重撤销；禁用时无定时轮询/刷新请求，在途结果不能污染新生命周期。 |
| 技术 B | APPROVE WITH CONDITIONS | 必需参数同步 App 与两个当前 fixture；实际 Electron 验证接口缺失、真实失败、禁用竞态和再启用。 |
| 产品 | APPROVE WITH CONDITIONS | 空库保持正常创建作品流程；有作品的真实故障保留重试，不删除或隐藏任务。 |
| UI/UX | APPROVE WITH CONDITIONS | 无作品时不显示伪错误/重读按钮/无尽读取；不新增提示面板，已存在的明确故障仍可访问。 |

综合批准上述最小实施；无 P0/P1 未决设计项，无兼容例外。App 只精确修改任务读取调用一行，其他用户并行媒体修改保留。

## 实施后复核

已实施当前必需的第五参数 `enabled`，scope 纳入启用状态，实时 scope ref 和 effect cleanup 同时隔离旧响应；禁用不读取、不轮询、不把旧结果认证为当前成功。启用且缺少实际 `runtime.listWorkTasks` 的界面仍明确报接口错误。App 仅对任务读取调用一行做精确补丁，现有两个 fixture 调用同步为当前参数，不修改并行媒体代码。

| 复核视角 | 结论 | 证据 |
| --- | --- | --- |
| 架构 | APPROVE | 启用生命周期和能力分别表达；没有新增任务事实或业务执行。 |
| 技术 A | APPROVE | 禁用后的在途 running 结果和旧刷新/分页/轮询无效；再启用重新首读。原单飞、分页失效、A–B–A 竞态和结算轮询回归仍通过。 |
| 技术 B | APPROVE | 当前三处调用同步；缺桥接、缺实际方法、真实读取失败及重试均验证，TypeScript 通过。 |
| 产品 | APPROVE | 空库正常创建作品、任务读数为0；有效作品的实际失败仍可恢复，不隐藏未知状态。 |
| UI/UX | APPROVE | 实际 App 在 Electron 中呈现创建作品入口与输入框前置提示，无伪任务错误、重读入口和任务加载状态；未添加常驻面板。 |

验证结果：

- `node --test --test-name-pattern='task list serializes|actual empty App' electron/tests/creative-workspace-ui.test.mjs`：2/2，通过日志 `/tmp/task-list-enabled-ui-escalated-2026-10-07.log`。任务列表增加8项生命周期检查；实际 App 覆盖空库、有意禁用时缺任务方法、重开3项检查。保留原单飞、真实错误与恢复、分页、归属及结算场景的原断言和快照。
- 当前两个相关调用方：`node --test --test-name-pattern='project conversation persists|actual workspace preserves' electron/tests/creative-workspace-ui.test.mjs`：2/2，`/tmp/task-list-enabled-consumers-2026-10-07.log`。实际 App 跨视图保留对话和归属；创作工作区保持场景、作者输入与保存边界。
- `npx tsc --noEmit`：通过，`/tmp/task-list-enabled-tsc-2026-10-07.log`；相关文件 `git diff --check` 通过。
- 截图 `/tmp/task-list-enabled-ui-screenshots/empty workspace quiet conversation.png` 已视觉检查，来自实际 Electron 渲染内存 fixture，不是设计图。

首次沙箱启动 Electron 两项均在进入 fixture 前以 SIGABRT 退出：`/tmp/task-list-enabled-ui-2026-10-07.log`。隔离 Electron 环境获准后重跑通过；不将启动失败隐去或当作界面断言失败。

零兼容后审 PASS。没有默认旧参数或旧入口，无 Native 构建、真实作品库或模型操作。剩余为 Root 的当前生产 App 实机空库复核及最终统一验收；这些隔离回归不替代真实模型创作验收。当前改动冻结。
