# 单一对话过程与输入框控制

## 变更概览

目标：删除任务底部的执行步骤清单及暂停、继续、取消按钮。公开进度与工具行按真实回合顺序展示，操作在原地展开；不新增完整记录入口。后台持久执行记录保持事实来源。

范围：WorkTaskPanel、App 输入框与重试绑定、相关样式、测试和现行文档。修订判断属于成果而非执行日志，移至对应成果附近。失败/取消回合恢复到主对话。保留保存状态核对、来源验证、即时停止及明确重试能力。用户自然语言原文送模型，不引入关键词路由。

验收：主对话无第二套步骤列表和控制按钮；失败及作者问题不被删除；输入框对明确对象直接重试，歧义时经图标明确选择原任务，不能自动选择其他对话或多个候选；停止单次提交；状态/来源发生变化时遵守现有 Runtime 检查；窄栏、展开和阅读位置稳定。

## 零兼容门禁

- 不新增版本判断、旧字段 fallback、迁移、双读写或降级。
- 同步修改生产调用方、fixture、测试和现行文档，不保留旧 UI 入口。
- 不变更持久 schema，当前数据一致性、并发和恢复边界必须验证。
- 无兼容例外。

## 实施前五角色评审（本任务逐角色审查）

| 角色 | 结论 | 审查意见 |
| --- | --- | --- |
| 架构专家 | APPROVE | 公开回合是唯一过程视图，Work 状态只供成果绑定与恢复。后台账本不复制到界面。 |
| 技术专家 A | APPROVE | 保留停止票据、恢复单飞、任务身份与版本检查；重试必须限定当前对话。 |
| 技术专家 B | APPROVE | 删除废弃组件参数和样式，同步回归测试，覆盖失败、重开、跨作品与窄栏。 |
| 产品经理 | APPROVE | 删除冗余入口；保留成果、真实阻碍和提问，操作来自输入框与自然对话。 |
| UI/UX 设计师 | APPROVE | 原地折叠和时间顺序保持；修订判断贴近成果，图标有可访问名称和说明。 |

综合：APPROVE，实施前无 P0/P1。具体实现后再次复核，结果填入下文。

## 实施后复核

已实现：

- 删除 WorkTaskPanel 底部执行步骤清单、暂停/继续/取消/核验按钮及废弃参数、样式和恢复提示入口；不新增完整执行记录入口。
- 保留时间线中的公开进展与原地展开。任务绑定的历史回合从当前线程恢复，按身份去重，内部工作提示不重复作为作者消息展示。
- 修订判断贴近成果；无成果卡片的终态修订仍可检查判断。保留错误、来源变化、作者问题及未完成目标。
- 输入框停止与重试复用原任务控制；未发送草稿与选区保持，来源、线程、作品、版本与恢复方式受检查。歧义候选只在用户点开图标后选择。
- 与工作区同时进行的失败内容展示和多任务恢复修改集成，保留其最终回复回执边界及显式选择。

| 角色 | 结论 | 实施后复核 |
| --- | --- | --- |
| 架构专家 | APPROVE | 对话只有一套公开过程，后台记录不转为第二套步骤清单；历史回合仍按真实身份呈现。 |
| 技术专家 A | APPROVE | 原任务恢复、停止竞态、未开始恢复、无模型保存核对、跨作品迟到回执均有 UI 回归。 |
| 技术专家 B | APPROVE | 当前调用方与测试一并更新，构建通过，无兼容分支或迁移；历史进度可见且只出现一次。 |
| 产品经理 | APPROVE | 操作聚合到输入框，不新增常驻管理面板；成果与下一步决定仍在对应内容附近。 |
| UI/UX 设计师 | APPROVE | 已查看实际 App 与窄栏截图；保留原地展开、阅读位置和完整动作名称，图标具备可访问名称。 |

验证范围：

- `npm run build`（TypeScript 与 Vite）通过；Vite 仍提示现有大体积 bundle。
- `conversation-retry`、`work-conversation`、`semantic-work-continuation` 单元测试通过，覆盖原回合身份、歧义、来源阻止及恢复文案。
- Electron 定向测试：`creative-workspace-ui`、`conversation-presentation-ui`、`conversation-action-search-ui`、`delegated-conversation-ui`、`app-recovery-ui`、`app-task-controls-ui`、`multiple-retry-ui`、`revision-decision-ui`、`revision-repair-ui`、`revision-outcome-ui`、`unfinished-output-ui`。
- 已核对 `/tmp/single-conversation-flow/single-conversation-composer.png` 与 `conversation-codex-flow-narrow.png`。
- 最终验收：15 项单元测试通过；21 项定向界面/桥接检查中，20 项首次通过，1 项来源错误文案断言更新后单独复跑通过。验收日志：`/tmp/single-flow-acceptance.log`、`/tmp/single-flow-controls-final.log`、`/tmp/single-flow-unit.log`。最后一次构建通过。
- `git diff --check` 通过。

零兼容复核：未修改持久 schema；没有版本判断、旧格式 fallback、双读写、迁移或降级。模型自然语言决策保持原有通道，未增加关键词意图路由。测试使用受控 Runtime/Agent 回执验证真实 React/Electron 界面，不将其表述为线上模型行为验证。

综合：APPROVE，本次范围无未处理 P0/P1。
