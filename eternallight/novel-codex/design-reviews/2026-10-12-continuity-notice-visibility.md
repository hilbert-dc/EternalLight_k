# 章节衔接待检查提示可见性（2026-10-12）

## 实施前
UI 的 ChapterStructureNotice 展示项目级持久检查状态，但挂在执行视图的条件分支内；空对话进入历史阅读视图后待检查事实被隐藏。将现有实例移至 conversation-reading-area 的公共区域，不改变读取、作者重试、模型准入、dirty、busy 或项目隔离逻辑。职责属于 UI 状态投影（B01/B06）；不创建执行、不复制领域事实。

五视角前审（同一代理自审）：架构 APPROVE，项目级事实不应依赖临时对话视图；技术A APPROVE WITH CONDITIONS，验证多事件失败隔离与dirty/执行门禁；技术B APPROVE WITH CONDITIONS，Electron章节删除回归及构建检查；产品 APPROVE，空对话仍可了解检查待办；UI/UX APPROVE，提示仍在对话栏，不覆盖编辑器/抢焦点。零兼容：复用现有唯一组件，不增加状态或历史分支。

附带三项测试合同同步：来源选择不自动代写作者指令；消息已由Host接受后失败的场景须发出既有 execution-request/message-updated queued 事件。保留来源隔离、后续草稿/引用及拒绝恢复断言，不能将本地preflight拒绝误当已提交失败。

## 补充前审：失败记录重读
自动检查执行失败后，持久待办已转成任务，原notice不再存在；增加“重新读取检查记录”入口，仅调用现有 taskList.refresh，无模型调用、任务恢复或写入。五视角同意此只读投影补全，技术条件沿用章节删除回归（模型调用数保持0、原稿/caret保持、同一任务记录读取）。不改变失败事实或自动重试授权。

## 后审
实际 Electron 回归四项均通过：chapter-deletion、discussion-source、selection-discussion（`/tmp/host-ui-verified.log`），app-agent-availability（`/tmp/host-ui-availability-final.log`）。后者快照仅更新显式作者输入，以及fixture提供TASK_FAILED时的通用失败说明；原始诊断和已配置模型资格仍保留。输入变更不会隐式移除作者选定来源，测试现明确点击移除后核对来源为空且输入未改。

`tsc --noEmit`、相关文件diff空白检查通过。章节删除验证确认：空对话待办可见、多失败事件互不覆盖、dirty保存与作者二次确认仍生效、自动失败只读重查没有模型调用/再次删除、未发送正文及caret保留。

五视角后审：架构/技术A/技术B/产品/UI/UX均APPROVE；技术条件由上述真实Electron测试关闭。不宣称真实付费模型或作者手动验收。
