# 作品目录章节行对齐

状态：完成，2026-10-06；本轮五角色 PRE／POST 全部通过，无 P0/P1，零兼容 PASS。未继承删除分组标题阶段的批准。

## 证据与范围

当前 `.work-directory-volume > ol` 比直属章列表多 16px padding，且章行用 flex，章号长度改变标题起点。拟仅 Renderer 样式与局部展示 custom property：两个 ol 同为 16px；章行改二列 grid，统一章号列宽按当前目录最大章号位数计算，数字等宽、不折行，标题占剩余宽度并保持两行截断、完整 DOM／title。

所有章节行采用同一正文起点，卷的原生展开、卷名／章数及容器归属继续表达层级。直属章不变成卷内章，排序、菜单、选中、dirty 导航及数据不变；不调用模型或修改 Runtime/API。

## 五角色 PRE / POST

| 角色 | PRE | POST |
| --- | --- | --- |
| 架构 | APPROVE（根代理本轮汇总） | APPROVE（源码亲核） |
| 技术 A | APPROVE（根代理本轮汇总） | APPROVE（源码亲核） |
| 技术 B | APPROVE（根代理本轮汇总） | APPROVE（源码与暗 330 图亲核） |
| 产品 | APPROVE WITH CONDITIONS | APPROVE |
| UI/UX | APPROVE WITH CONDITIONS | APPROVE |

条件：统一宽度基于整个当前目录而非每卷或展开部分，折叠不改变其他标题起点；章号不可被截断，标题保持可用宽度，窄栏不得横向溢出。等宽数字只是排版，不能补零或改变作者看见的章号。

## 验收

- [x] fixture 使用 1／10／100／1000 章号，卷内与直属章的章号及标题起点四矩阵 offsets 全为 0；列宽取全部 `sortedChapters` 最大位数，最小 1，不随展开部分改变。
- [x] 亮暗 330／740、章号无截断／横溢、长标题两行且完整 title／DOM 保留；目录 3/3 PASS、零 skip，选中／焦点／键盘行为保留。
- [x] 根代理实机截图与自动 fixture 分别记录；构建 PASS。
- [x] 规定 `just fmt` exit 0 PASS，源与测试冻结后未重跑测试。
- [x] PRE 零兼容：无 schema、数据、迁移、fallback 或权限变更。
- [x] POST 五角色与零兼容最终收口。

最终源码核验：两个 ol 同 16px；按钮二列 grid 共用 `--directory-number-width`，数字 nowrap／tabular，标题 `minmax(0, 1fr)` 与原两行截断保持。PM/UX 亲审 `/tmp/novel-directory-alignment/work-directory-light-330.png`：卷内选中 10 与直属 1／100／1000 起点一致，章号完整，长标题两行，卷展开层级仍明确，无横向越界。

根代理真实 CUA：用户作品 1 卷 5 章，卷内 1／3／5 与直属 2／4 全部同起点，选中 2 保持蓝色；只读查看，未改作品或调用模型。上述截图是自动 Electron fixture 视觉结果，真实 CUA 是根代理操作，两者不混称。

PM/UX 未改 src/tests、未运行测试或操作 GUI；只读源码／截图并维护本记录。
