# 作品目录直属章节评审

状态：完成，2026-10-06；本轮五角色 PRE／POST 全部 APPROVE，无 P0/P1，零兼容 PASS。

## 范围与证据

`WorkDirectory.tsx:93–96` 已按 `placementOf(chapter.id) === null` 筛选未归卷章节，按原 `sortedChapters` 顺序渲染一次；“其他章节”只是有卷时额外展示的 h3。本轮删除该标题及 `work-directory.css` 的专用 h3 样式，让章节直属作品目录。卷仍展开包含自身章节，不删除正文／目录条目、不更改归卷、计数、创建菜单、选择与导航门禁。

## 五角色 PRE / POST

| 角色 | PRE | POST |
| --- | --- | --- |
| 架构专家 | APPROVE（根代理本轮汇总） | APPROVE（实际源码复核） |
| 技术专家 A | APPROVE（根代理本轮汇总） | APPROVE（实际源码复核） |
| 技术专家 B | APPROVE（根代理本轮汇总） | APPROVE（源码与 fixture 复核） |
| 产品经理 | APPROVE | APPROVE |
| UI/UX | APPROVE | APPROVE |

PM/UX：人工分组标题无必要，会使未归卷章节看起来像另一种内容。直接列为根级符合用户要求，保留卷／章缩进差异，不能把用户“删目录”解释为删除实际章节。未沿用前一阶段批准。

## 验收与门禁

- [x] 有卷与直属章节：无“其他章节”标题；卷内和直属章各仅出现一次，章序、计数、选中与键盘点击正常。
- [x] 无卷直接显示章节，空目录引导保持；亮暗窄栏不溢出，根级章与卷内章层次可辨。
- [x] 当前 fixture／snapshot 同步，构建与规定格式化通过；无模型或 Runtime 调用变化。
- [x] PRE 零兼容：无 schema/API、迁移、fallback、双轨或数据改动。
- [x] POST 零兼容与五角色门禁最终收口。

最终源码复核：仅删除 h3 和专用 CSS，直属列表的 exact ID、排序、选中与导航保持。fixture 新增直属行 exact ID 与可见性正例。

根代理验证：`work-directory` 3/3 PASS、零 skip；快照执行成功，原快照未包含人工 h3，无需更新。build PASS，`just fmt` exit 0 PASS，fmt 后未重跑测试。根代理真实 CUA 的 screenshot／AX 显示 1 卷 5 章：卷内 1／3／5 与直属 2／4，人工标题消失，原第 1 章蓝色选中与章数保持；未修改作品或调用模型。自动化与真实 GUI 证据分别记录，不声称模型体验。

PM/UX 只读最终源码并更新本记录，未改 src/tests、操作 GUI 或运行测试。本轮关闭，无剩余阻断。
