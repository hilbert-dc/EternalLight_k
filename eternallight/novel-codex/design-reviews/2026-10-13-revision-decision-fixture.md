# 改稿作者决策真实 UI 夹具核对

## 五视角前审

完整验证中该用例在挂载 App 后找不到章节编辑器，DOM 为空。先保留并报告真实 renderer 异常与意外 API 调用，区分产品错误与夹具遗漏。不得移除作者 accept/repair/defer 选择、最新决策核对、跨项目/版本/作者迟到回执隔离、后继确认和普通创作不得继承前置来源断言。

架构 APPROVE WITH CONDITIONS（遵从真实 API）；技术A APPROVE WITH CONDITIONS（身份与确认护栏不变）；技术B APPROVE WITH CONDITIONS（定向真实 App 测试与精确失败证据）；产品 APPROVE（无自动运行/采用）；UI/UX APPROVE（保留当前交互）。

## 根因与后审

真实 renderer 异常为 `TypeError: step.outputRefs is not iterable`，发生于 workProjectionKey：夹具手工 Work step 漏现行必需 `outputRefs` 数组，渲染树因此卸载。仅为该 step 补 `outputRefs: []`，不虚构保存证明、不让生产接受不完整 step。永久保留 renderer 与意外 API 诊断，使后续空白失败可定位。

五视角后审均 APPROVE：架构（现行明确字段）、技术A（无决策权限变化）、技术B（完整此用例1/1通过）、产品（原作者选择与最终确认全保留）、UI/UX（不改产品或快照）。`node --test electron/tests/revision-decision-ui.test.mjs` 1/1 通过，日志 `/tmp/revision-decision-ui-current-contract.log`；原快照完全未改，accept/repair/defer、历史/过期来源、跨项目/版本/作者迟到回执、对话重复失败不复用旧后继、普通创作 null predecessor 全部原断言执行通过。无需修改 Creative/editor 或生产执行逻辑。
