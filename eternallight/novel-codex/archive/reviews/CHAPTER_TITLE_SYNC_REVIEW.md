# 章节标题展示同步

状态：完成，2026-10-06；五角色 PRE 条件满足、POST 全部 APPROVE，无 P0/P1。Electron 9/9、最终 build／规定 fmt PASS；零兼容 PASS，无剩余工作。

## 问题与最小范围

编辑器 `loadEditor` 使用 `candidate ?? accepted` 的 title，否则章节基础 title；目录与切章菜单直接使用 App 基础章节 title。根代理进一步核对：`sqlite_repository.rs:1412` 保存章节 metadata 会 UPDATE title，`read_api.rs` 的 state 读取此 title。因此主要断点不是数据库不更新，而是章节保存后的 `perform.reload` 只刷新局部 workspace，没有 `onFormalChange` 更新 App state／Sidebar token；本地 reload 仍旧，重新真实 getState 可取到新名。初版根因描述已纠正，不改存储来修展示。

统一纯展示 helper：精确有效的当前 editor live preview → 已保存 candidate → accepted → 基础 chapter title。候选标题不是正式采用；live preview 不是保存回执。读取选择按来源是否存在，不能用 `||` 将作者清空的标题偷偷换回旧值；空值的可读占位仅为展示，不写回内容。

目录复用现有同项目 workspace／卷纲读取响应收取已保存标题，不新增另一路重复读取；初始加载、保存刷新与重启读回恢复同一候选标题。切章器亦用当前 workspace＋本地标题。live preview 由有效 editor session 经 App 传递，至少绑定 api／project／chapter／owner；取消、放弃、离开该 session 或 scope 变化立即清除，回已保存来源。保存失败保留编辑输入及其预览，但明确维持现有未保存状态，不能标保存成功；下一章／作品不继承旧预览。

保存闭环实际补修：拿到已知 receipt 后先核验 owner，再记 pending 与 saved revision，调用 `onFormalChange` 刷新共享 state/token，随后原 reload／receipt validate。任一读失败保留 pending receipt，下次“保存”只读核对、不再写；来源或输入变化仍沿原守卫，不偷偷换稿。该补修是当前成功写后反馈完整性，不是兼容或存储变更。owner guard 位于 pending 存储之前，旧项目保存回包不能污染新 session。

不新增存储更新逻辑，保留 Native 原保存 metadata 行为及正式采用／生成 authority。此次确实补全保存后的共享读取／反馈流程，不能称其未改变。无自动保存／发布、历史标题回填或兼容层；对话上下文与 Runtime 目标不从 preview 推导授权。

## 五角色 PRE / POST

| 角色 | PRE | POST |
| --- | --- | --- |
| 架构 | APPROVE（根代理汇总，含 shared read 补修条件） | APPROVE |
| 技术 A | APPROVE（根代理汇总，含 shared read 补修条件） | APPROVE |
| 技术 B | APPROVE（根代理本轮汇总） | APPROVE |
| 产品 | APPROVE WITH CONDITIONS | APPROVE |
| UI/UX | APPROVE WITH CONDITIONS | APPROVE |

条件：保存与未保存语义可区分；live owner 与已保存读取 owner 分开防串；旧读取不能覆盖新 session 的 live 标题；同 scope 读失败保留已知已保存标题并给真实错误，新 scope 不露旧作品状态。目录与切章仅改变展示，不把预览 Chapter 对象送入 writer 权威目标。

## 验收

- [x] candidate／accepted／基础 title 各不同：编辑器、目录、切章菜单同取候选；保存后共享 state/token 刷新，初始读取／remount 合同一致。
- [x] 输入标题实时在当前目录与切章显示，零保存／模型；清空不跳回旧标题；DOM／反向选区保留。
- [x] 保存失败输入／预览保留；已写成但共享读取失败时保留 receipt，仅读恢复且保存次数仍 1；放弃回候选，离开／返回取已保存标题。
- [x] 同章号／ID 的跨项目 A→B→A 迟到读取不覆盖新 session；现目录、恢复与手工流程回归通过。
- [x] 当前 fixture／snapshot 与 `test:ui` runner 同步，build PASS。
- [x] 规定格式化与五角色 POST 最终汇总。
- [x] PRE 零兼容：无 API/schema、迁移、fallback、双轨、自动保存或 authority 改动。
- [x] POST 零兼容 PASS，保留当前 Native metadata 更新与正式采用权限，无兼容例外。

已读工程 AGENTS／策略／模板并只读当前源码，仅新增本评审记录，未改 src/tests、操作 GUI 或调用模型。

## POST 证据与边界

PM/UX 亲读 `chapter-title.ts` 的 nullish 来源选择（保留空串），Sidebar 复用已有 workspace 请求并按 owner 隔离标题，App token cleanup 不清新预览，Workspace 有效 editor owner＋session 才发布 live preview，切章投影不替换原导航／写作 Chapter。已知 receipt 在 owner 核验后存储，共享刷新后继续原 exact revision 读取校验；未扩大权限。

实际 Electron 新标题专项＋目录 3＋App recovery 4＋manual 1，共 9/9 PASS、零 skip，日志 `/tmp/novel-chapter-title-final-ui.log`，快照已由技术 B 与根代理审阅。fixture 刻意保留 base 旧名证明候选投影，不能将 mock base 未改当作 Native 不更新 metadata 的证据。

根代理真实 Electron 只读 AX：用户第 5 章目录“第5章 试试试试”，编辑标题“试试试试”、正文“请求”一致；没有修改或保存用户文本。无本轮模型调用，不声称生成交付验证。

架构／技术 A 最终亲核源、fixture、runner、快照与日志，POST APPROVE；技术 B、PM／UX 最终 APPROVE。最终 build session 85988 exit 0（tsc／Vite 通过）；规定 codex-rs `just fmt` session 99607 exit 0，fmt 后未重跑测试或继续修改源码。全部验收闭合，无剩余 P0/P1。
