# 作者导出设计与实施复核

日期：2026-09-13。范围：产品文档 § 隐私与导出、C21。

## 目标和边界

替换整份界面状态 JSON 下载。作者选择正式正文、正式规划（公开指引）、当前未采用候选或作者私密资料，下载 TXT／Markdown。默认只正式正文，不带项目简介、内部章节标题、候选、事实或私密规划。正文采用章序号标题，保留作者已经采用的原文。

导出不自动保存编辑、不创建领域版本、不调用模型。对话框明确仅导出已经保存的内容。完整 Brief 和内部规划归入私密资料；候选完整规划须同时选择候选与私密资料。公开字段为空时明确说明，不伪装成完整大纲。私密事实保留主体、认知所有者／状态、故事时间、阅读位置、来源与版本；被替代记录明确标记为历史。

## 设计前五角色门禁

- 架构专家：APPROVE。Runtime 单个只读 SQLite 事务从权威指针读取；新增 author-only API，Electron 只桥接，不接受任意文件路径。
- 技术专家 A：APPROVE。公开规划按字段白名单投影；私密内容显式选入；文件及回执带作品版本、产物 ID、导出时间。
- 技术专家 B：APPROVE WITH CONDITIONS。要求真实 Runtime 权限／指针／秘密 canary 集成测试及真实 IPC 参数回归；实施后已满足。
- 产品经理：APPROVE。默认已采用正文；公开规划和完整内部规划的范围准确说明；不引入旧 JSON 导出双轨。
- UI/UX 设计师：APPROVE WITH CONDITIONS。保留失败选择、禁重复提交、焦点循环／Esc／恢复焦点；实施后已满足。

综合结论：APPROVE。无 P0／P1 设计阻断，无兼容例外。

## 零兼容门禁

- [x] 无版本判断、旧字段 fallback、迁移、回填或 deprecated 入口。
- [x] 删除旧 downloadState，接口、preload、类型与界面一次性切换。
- [x] Runtime 测试使用当前 schema 新建临时数据库。
- [x] 作者权限、项目隔离、快照一致性与错误可恢复均有明确验证。

## 实施后复核

实际变更：`runtime/creative_workspace/author_export.py`、Runtime 路由、Electron IPC／preload、`ExportDialog.tsx`、样式、App 导出入口与类型。

验证证据：

- `python3 -m unittest runtime.tests.test_author_export`：4 项通过，涵盖已采用／候选指针、采用后最新版本、公开规划与完整私密规划、私密事实 canary、空态、权限、参数校验及作品隔离。
- `node --test electron/tests/export-dialog.test.mjs`：真实 React／Electron 交互通过，默认范围、失败可见、私密提示、busy、回执、Esc 与焦点恢复。
- `agent-configuration.test.mjs` 的真实 main handler 回归验证导出使用 POST JSON，并验证指标 GET／POST。该回归修正了初次实现中的 IPC 调用形状错误。
- `npm run build`：通过。
- 导出对话框截图已检查，修正原复用表单样式导致的 checkbox 错位。

五角色实施后结论均为 APPROVE。零兼容门禁 PASS。未声称通过 Windows 实机验收；无本范围遗留功能缺口。
