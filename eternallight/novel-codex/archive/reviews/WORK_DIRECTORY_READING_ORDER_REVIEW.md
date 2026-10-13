# 目录阅读顺序与默认章标题

状态：完成，2026-10-06；五角色 PRE 条件均满足，五角色 POST 全部 APPROVE，无 P0/P1。Native 3/3、Electron 4/4、构建、规定格式化及 diff-check PASS；零兼容 PASS，无剩余工作。

## 目标与证据

原目录卷内按章号排序，再列未归卷章节，因此 1／3／5 后出现 2／4 是归属结果，不是“智能阅读排序”。保留卷章层级，禁止按章号、卷名或相邻关系自动猜归属。作者随后明确要求将 2／4 放入第一卷；根代理经现有两个 exact select 保存完成，实际目录为本卷 1–5。此为明确作者操作，不是自动归卷规则。

实施前 `StandaloneChapterForm` 把预计章号写成 input value 且禁止空标题；Workspace 的 `nextChapterNumber` 从正式阅读状态计算，同时 `materializeStandalone` 再拒空字符串。Runtime `chapters.rs` 原已在事务中从所有实际章节分配 `MAX(number)+1`，且空 title 用真实编号生成“第 N 章”。前端预填“第2章”被当成人工标题，不能随 Runtime 实际分配 5 自动纠正。

实际根修：输入初值为空、placeholder 为预计第 N 章，label“章节标题（可选）”；可直接创建，原 title string API 传空字符串，不新增 nullable 契约。预计编号取当前 workspace 全部章节，Runtime 回执仍为权威。手输非空标题保留；不识别“第N章”样式并偷偷改写。历史标题未自动更改，单飞／CAS／精确卷 ID／未知回执只读恢复不变。表单 key 为 project＋chapterCreationRequest，预计编号刷新不重挂输入或丢选区。

## 五角色 PRE / POST

| 角色 | PRE | POST |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS（根代理本轮汇总） | APPROVE |
| 技术 A | APPROVE WITH CONDITIONS（根代理本轮汇总） | APPROVE（Native 条件满足后最终报告） |
| 技术 B | APPROVE WITH CONDITIONS（根代理本轮汇总） | APPROVE |
| 产品 | APPROVE WITH CONDITIONS | APPROVE |
| UI/UX | APPROVE WITH CONDITIONS | APPROVE |

条件：Form 与 Workspace 两处空字符串拦截一并移除；空标题不是缺失参数。placeholder 只是预计，创建后展示真实回执编号／标题；输入失败保值，不因刷新预计编号覆盖作者文字。标题 label 可访问，Enter 与按钮均可创建；busy 与来源／dirty 门禁保留。

## 验收与零兼容

- [x] Renderer 合同：正式阅读仅第 1 章、workspace 已有 1–4，预计提示取 workspace，空字符串传递，不再保存前端预填“第2章”。Native HTTP 另验证真实 Runtime 空标题分配第 2 章的 exact id／number／title 与 same-key replay。
- [x] 非空手输标题：manual test 与 Native 原 custom title 保存断言 PASS；空标题 Runtime 默认命名 PASS。代码未判断标题是否为“第N章”形式，但没有专门测试手输数字同形标题，不声称存在该特定 case。
- [x] 根／卷创建、双击单飞、失败保输入、创建成功读失败只读恢复合同回归通过。
- [x] 未授权不移动；作者明确授权后经现有 exact select 将 2／4 归第一卷，根 GUI／AX／截图确认本卷 1–5。章序与卷层级明确，不宣称智能排序。
- [x] 当前 fixture、Runtime 默认标题证据、构建与五角色 POST 收口，Native rustfmt 完成。
- [x] 规定 `just fmt` session 95917 exit 0、Native rustfmt exit 0、diff-check PASS。
- [x] PRE 零兼容：复用当前空字符串契约，无 schema、fallback、迁移、旧标题回填或额外权限。
- [x] POST 零兼容 PASS；归卷为作者明确授权后的真实保存，不自动更名历史 title。

PM/UX 仅只读源码及工程规则并写本记录，未改 src/tests、运行模型或操作 GUI。

## 当前验收证据

Electron 目录 3＋手工创建 1，共 4/4 PASS、零 skip，build PASS。PM/UX 亲核表单空字符串通过 Form 与 Workspace、nextNumber 取 workspace 全章、稳定 key 保输入；没有本轮源码 P0/P1。

根代理真实 GUI：作者自行编辑第 5 章后打开新建表单，预计第 6 章与标题可选正确显示；根代理未操作其输入，历史标题未自动改。归卷仅按作者明确指示完成，没有模型调用。

Native 定向 3/3 PASS、零 skip（原 session 71136 与同机权限 session 68420 均 exit 0；重复执行不累计成 6 项），默认标题 exact id／number／title、原 custom title 与 same-key replay 均通过。Native rustfmt 与规定 `just fmt` session 95917 均 exit 0，diff-check PASS；本轮条件全部满足，无剩余阻断，格式化后不再测试或改源码。
