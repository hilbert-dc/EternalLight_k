# 章节删除设计评审

状态：本轮章节删除交付完成，2026-10-06；五角色 PRE 条件满足、POST APPROVE，无剩余 P0/P1。Native 最终 2/2、Renderer／提示专项、最终 UI build、Native bins 与规定 fmt PASS；原工作区重启后的菜单／确认／取消验收完成。章节结构变化后的自动故事影响检查仍是后续独立阶段，不属于本轮已完成能力。

## 变更概览

目标：作者能从作品目录删除不需要的章节，正文编辑仍以创作与对话为主。章节删除是明确的低频管理决定，适合章行右键／悬停省略号，不增加正文常驻危险按钮，不把普通对话自动解释为删除授权。

当前 `WorkDirectory` 仅根／卷菜单，没有章行管理；`DirectoryMenu` 已支持边缘定位、Escape、键盘与焦点。最小 scope 是 exact chapter ID 菜单、明确确认、实际 Runtime 删除与只读回执核对。卷删除、批量删除、自动重编号、历史迁移／标题更改、模型调用均非目标。

## 作者流程与取舍

1. 章行右键／Shift+F10／ContextMenu，或 hover／focus 可见的省略号打开“章节操作”，触屏入口可见。省略号是独立按钮，不嵌套正文选择按钮；打开菜单不导航、不写，事件不冒泡成卷／根菜单。
2. 先经过原 dirty save／discard／cancel 门禁，不擅自丢弃当前编辑；取消不进入删除。然后确认显示精确章号与章名，明确“删除此章节及本章正文，不是仅从目录移除”。默认焦点“取消”，Escape 等同取消；明确“删除章节”才执行。长名可完整读，已发布章节的影响按实际后端能力说明，不虚构撤销／回收站。
3. 真 API 完成并权威读回后才移除行，禁重复确认；当前删除章优先选原顺序中的相邻章（建议下一章，无下一章取上一章），非当前章删除不改变作者当前章；最后一章删除进入现零章空态。不重新编号其他章、不改归卷。
4. 运行／忙时禁删除；请求绑定 project／chapter／owner／版本，切作品迟到不能删除新目标或清新状态。写失败显示真实错误并保留行；成功但读取失败、未知写回执仅只读核对，不自动重发删除、不假称已删除。重新核对若确认章不存在，按正常完成处理；仍存在则明确未确认，由技术合同决定是否可重新明确授权，不自动猜。

### 必要边界

- 后端必须原子验证目标与运行／版本条件，并保证目录归属与相关正文读模型一致；不能只删显示行留下假产物。关联事实／来源／任务记录如何处置由架构与技术明确，不能自动顺带删其他故事内容或声称清理全部引用。
- title 是显示，不是匹配键；同名章节只删确认的 ID。dirty 处理后的确认应重新核对该目标，防沿过期 live preview 对错误章确认。
- UI 成功来自 Runtime 回执／重读，不来自模型描述。没有作者明确确认不得执行；本轮不承诺撤销删除能力。

## 五角色 PRE / POST

| 角色 | PRE | POST |
| --- | --- | --- |
| 架构专家 | APPROVE WITH CONDITIONS（根代理本轮汇总） | APPROVE（真实 Author baseline P1 修复与 HTTP 验证后） |
| 技术专家 A | APPROVE WITH CONDITIONS（根代理本轮汇总） | APPROVE（最终源码与 HTTP） |
| 技术专家 B | APPROVE WITH CONDITIONS（根代理本轮汇总） | APPROVE（最终 Native／IPC／Renderer 源码及验证） |
| 产品经理 | APPROVE WITH CONDITIONS | APPROVE，功能条件满足 |
| UI/UX | APPROVE WITH CONDITIONS | APPROVE，功能条件满足 |

PM：删除正文必须清晰、确切且由作者最终决定；管理菜单符合对话主创作路径。UX：保留取消默认焦点与键盘／触屏入口，成功后焦点落有效邻章或空态入口；失败／未确认 alert 外露，不能让菜单操作无反馈。五角色齐备前不实现。

## 当前版本验收

- [x] Renderer exact 同名目标、取消／dirty 取消零删除、其他章编辑 DOM／选区保留；Native 数据一致性另验。
- [x] Renderer 非当前／当前／最后一章删除状态、邻章／空态与 mock 权威读回一致；不将 mock IPC 当真实数据库删除验证。
- [x] Renderer 双击单飞、运行／忙门禁、写成读失败／未知回执仅读核对，无自动重发。
- [x] Renderer 迟到 project／API incarnation 与 StrictMode 准备状态隔离通过。
- [x] 菜单／确认与危险动作可访问名称，准备完成默认焦点取消、关闭回 exact origin；目录既有可达性回归通过。
- [x] 当前 schema 的真实 HTTP 删除验证与 Renderer mock IPC 行为验证分别通过；构建与规定 fmt 通过。实际工作区只验菜单／确认／取消，不执行真实章节删除。

## 零兼容门禁

- [x] PRE 无旧字段 fallback、迁移、回填、双读双写、降级或 deprecated 路径。
- [x] 不为假设中的旧数据降低一致性，删除权限与正式决策仍需当前合同验证。
- [x] 当前生产／types／fixture／文档一次同步，数据引用、安全与失败恢复验证及五角色 POST 完成。

## Supplement PRE 与中途源码核验

Native 边界经根代理明确：删除 chapter／head／knowledgehead／working／continuation／placement／plan binding，当前 snapshot 排除该 draft，tombstone 防旧章重新投影；immutable 历史、模型批次、任务／收据及独立已确认 identity／facts／commitments 保留审计，当前章源不可用，不悄悄撤销作者决定。只 Author IPC，无删除 MCP。确认文案已采用“该章将从作品目录和当前正文中移除。其他章节不会删除或重新编号”，补说明已确认设定与历史保留；不宣称历史清空或可恢复。

删除后 generation 编号必须维护真实章号而非 count：正数／distinct 验证及 MAX frontier 属于保持原继续创作能力的必要窄修。PM／UX Supplement PRE 条件批准，需真实 HTTP 删除中章后续写合同证明，不扩模型权限、不自动重新编号。

亲读 `useChapterDeletion`／`ChapterDeleteDialog`：prepare 精确读取保存章名、confirm 校验回执与 project／chapter／revision；check 只读 state／workspace／catalog，读回后才 apply。初次 Cancel／Escape 可取消、默认焦点取消；已发出删除后 check 锁住 Cancel／Escape 合理，避免继续编辑可能已删正文，仅核对不重复写。无已知 receipt 且 fresh read 确认章仍在时回 fresh confirm、新 revision 与 key，取消重新可用，必须作者再次明确点击删除。未出现本轮新 P1。

## Renderer POST 与当前剩余证据

本请求按 Native 事务／API 合同与 Renderer 展示／恢复两独立可审 stage 实施，最终仍需一体可运行。PM／UX 亲读最新版 source／actual App fixture／snapshot：read 完成可取消时 effect 默认聚焦 Cancel；hook.open 在导航被禁前捕获 trigger，close／check success 后 rAF 验 current owner 且无新 dialog 才归 exact origin，原行消失则有效当前行／根入口。界面准备、确认、已发出待核对三阶段区分清楚，未发现新 Renderer P1。

根代理报告 UI 10/10 与焦点专项 1/1（91528）PASS；此前 build 27024 PASS，焦点补修后 final build 待根代理。自动 actual App 回归为真实 React／Electron＋mock IPC，不是用户数据库删除或实模。

真实 GUI 证据只为 AX：根代理查看用户第 4 章菜单／确认并 Cancel，原 5 章完整、零 delete；不声称生成了实际截图 PNG。禁止在用户真实章节执行试删。

Native 初轮 2/2 PASS 后，根代理独立审查发现 workingBaseline 使用了 draft object ID 而非 canonical raw chapter ID，初版 fixture 掩盖问题。该 P1 已修：最终 HTTP 使用真实 Author PUT 设置依据，验证删除前 workspace 的 raw chapter ID 键与删除后的空依据；continuation 精确按 draft／raw 章键清理，不扩大匹配。最终 Native session 16288 exit 0，真实 HTTP 2/2 PASS、零 skip，日志 `/tmp/novel-chapter-deletion-native-final.log`，包含删除中章后 MAX frontier 继续合同。重复初轮不累计成额外唯一项。独立 IPC 1/1 通过，非真实模型调用。

当前搜索 join heads 不残留删除当前章，显式 immutable 历史可读是设计边界；不能把历史审计保留误称删除失败。架构／技术 A 最终亲核 source 与上述证据 APPROVE，无剩余 P0/P1。

PM／UX 仅只读源码与证据并更新本记录，未改 src/tests、运行测试／模型或操作 GUI。功能整体 POST APPROVE；剩余仅既定交付门禁，不新增设计 scope。

## Supplement PRE：中间章衔接提示

PM／UX APPROVE：保留任意章删除，不限制“只能最后一章”。删除冗余／误写中间章是正常作者决定，末章限制会迫使额外操作，并不能解决故事衔接。

在同次精确 workspace 准备读取中计算是否存在更大 chapter.number；存在时确认附“后续章节会保留，故事衔接可能需要调整，可在对话中继续处理。”最后章不增加此提示。该句是提醒，不宣称已分析出冲突、不承诺自动修复，不调用模型、重编号或删除后续章。

条件：使用本作品全部实际章节而非当前卷列表／正式章投影；unknown 核对后回 fresh confirm 时同样重算，与新 expectedRevision 一致，不沿用过期布尔值。覆盖中间／最后章提示差异及零额外模型／写即可；不增加回收站等范围，最终代码待补后只读复核。

Supplement 源码 POST：PM／UX APPROVE，亲核 `prepare` 与无 receipt／仍存在分支都使用本次 `workspace.chapters.some(item => item.number > chapter.number)`，与本次 expectedRevision 一起构造 fresh confirm；初始 false，只有 `hasLaterChapters` 为真才显示警示。无模型／后续章改动／自动纠正。此前 build 61582 PASS；该提示专项与补修后 build 尚在执行，未提前记 PASS。Native raw chapterId working baseline P1 仍待最终修复，整体完成状态不变。

最新补证取代前段当时待完成状态：中章提示 focused session 85753 PASS；最终 UI build 92571 exit 0 PASS，日志 `/tmp/novel-chapter-delete-final-build.log`。Native P1 已按上节完成；root 对三个新增 Native 文件 rustfmt 完成。bins session 7535 与 mandatory `just fmt` session 49491 尚在执行，重启后的实际 Electron IPC 只读检查待根代理，不在本记录提前算 PASS；不删除用户真实章或调用模型。

最终交付补证：`just fmt` session 49491 exit 0；中断后确认没有 cargo/rustc 进程，再完成两包 `--bins` build，session 23380 exit 0（35.37 秒，日志 `/tmp/novel-chapter-deletion-bins-final.log`）。未在 fmt 后追加测试。技术 B 最终源码 POST APPROVE，无 P0/P1。应用使用已核实的原独立数据目录 `/tmp/novel-planning-live-current-290Hdi`，在 5174 重启；实际 AX 显示原作品 1 卷 5 章，第 5 章标题“试试试试”、正文“请求”仍在。第 4 章菜单进入精确确认，显示后续章节衔接提示且默认焦点为取消；取消后回到原操作按钮，5 章完整。未执行用户章节删除、保存或模型调用。
