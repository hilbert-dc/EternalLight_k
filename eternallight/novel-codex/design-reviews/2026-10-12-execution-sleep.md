# 执行域空闲休眠保护

## 前审：职责和故障证据

原任务两次 execution_interrupted 与 macOS 唤醒时间 11:53:07、12:21:05 相合；pmset 记录之前分别发生 Idle Sleep / Sleep Service Back to Sleep。Native owner 租约 30 秒，Host 每 10 秒续约；失效后 Host 取消执行。没有原始续约错误持久证据，故时间关联加实现路径是强根因证据，不称已捕获全部因果日志。

分类：Host 生命周期设计缺口。Native 短租约保障唯一执行权是合理边界，应保留。Host 在可信执行域准备到清理完成期间申请 Electron prevent-app-suspension；普通锁屏/显示器休眠仍允许。用户强制休眠、关盖和进程崩溃不承诺避免，继续遵循 Native 失效和恢复规则。

责任：B01/B06/B08，Host 管理 OS 资源，ExecutionDomains 持有释放回调，main 绑定 Electron API。Engine/Core 不增加业务规则或提示词。作品、权限、CAS、作者要求、核验标准、累计用量均不变。无历史兼容、迁移、双写或旧字段 fallback。

| Host 事实 | 行为 | 资源/后续 |
| --- | --- | --- |
| 无活动执行域 | 不持有保护 | 正常系统休眠 |
| 准备/执行/清理中 | 每域持有独立 assertion | 整个生命周期保护 |
| 嵌套执行 | 复用原域 | 不重复申请 |
| 并发执行 | 各自释放 | 一个结束不解除另一个 |
| 失败/取消/关机清理 | finally 释放 | 不改变账本与恢复判断 |

## 五视角前审（同一执行者自审）

- 架构 APPROVE：OS 行为归 Host，Native 唯一权威不变。
- 技术A APPROVE WITH CONDITIONS：测试并发、嵌套、失败、清理异常和 shutdown 生命周期。
- 技术B APPROVE WITH CONDITIONS：Host 类型检查、架构门禁，打包和真实 OS assertion 检查。
- 产品 APPROVE：防止无人操作导致正常创作中断，不限制正常 token 用量。
- UI/UX APPROVE：无需作者保持屏幕常亮；不增加确认交互。

## 验证计划

执行域回归验证资源实际计数和生命周期；现有 owner 测试确保失效拒绝边界；Host 类型与架构检查。打包后在原任务检查 pmset assertion，保留真实数据，不为测量另起生成。后审待实际结果。

## 续接条件补充前审

04:40UTC 的作者续接已成功完成 advisory、保存原运行交接；随后运行 version492→494，仍 recoveryStalled，没有新核验任务。原保护 fingerprint 只包括模型/Engine条件与原 Work 作者修订，新对话交接没有改变失败条件。这不是新的模型失败，也不能靠重复交接解除。将实际使用的 Host power policy 同源纳入 executionConditionDigest，使部署后的真实生命周期变化可重新评估；版本号/重启/认证时间/随机ID仍不改变条件，旧失败和费用不删除。五视角前审 APPROVE WITH CONDITIONS：验证条件稳定性及真实策略变化，保留 Native 防循环保护。此次只增加当前合同字段，无兼容或迁移。

## 后审与实际证据

- 51 项 Host 测试通过（执行域、Native owner 消费合同、执行条件/展示）；包含准备/执行/清理、并发、嵌套、各阶段失败、关机准备中取消、旧条件与新策略差异、随机认证信息不变性。日志 `/tmp/novel-sleep-tests.log`。
- `typecheck:host` 通过；`architecture:check` 扫描369文件、7 crate、无诊断；差异空白检查通过。
- 发布门禁 passed，完成时间 2026-10-12T04:47:33.626Z，报告 `release/package-report.json`。复用已通过编译方法一致性检查的 Native，不更改其权威/租约。
- 真实运行：12:49左右原 generation-d6db3e3374164a09809d9dadfc000ab8 自动恢复 running，version496，blockedReason=null；新 coordinator task-bf4adb9b71c643edb16ecd1feea5ec28 已建立，仍 completedChapters=29/currentChapterNumber=30。
- OS 确认 pid94188 小说工坊持有 NoIdleSleepAssertion named Electron，PreventUserIdleDisplaySleep=0。证据 `.release/context-live-validation/2026-10-12-sleep-assertion.txt`；原睡眠证据同目录 `2026-10-12-sleep-evidence.txt`。
- 启动后检查期间一次续接输入被“该对话已有任务正在运行”拒绝，未产生新模型任务；已清除输入，不重发，沿用自动恢复任务。
- 尚未证明第30章正式核验通过；不能据启动成功宣称长期零中断或 token 节约比例。强制休眠仍由 OS 决定，保持租约失效拒绝边界。

五视角后审（同一执行者自审）：架构 APPROVE（Host资源与Native权威分离）；技术A APPROVE（生命周期与并发失败回归、OS实证）；技术B APPROVE（测试/类型/门禁/真实恢复）；产品 APPROVE（原范围、正文、标准、用量保留）；UI/UX APPROVE WITH CONDITIONS（无需保持屏幕常亮已验证，任务核验完成状态继续随真实回执观察）。无兼容例外。当前修复已部署，长运行效果由原任务持续观察，不为测试重复生成。

### 真实核验闭合（04:50UTC后）

原运行 version499、running、completedChapters=30。第30章现稿 hash 与修复前完全一致（ca4339b22eaf7521678948b19d1647db069625f491829d9370753fc6d87f5c6e），2343 非空白正文字符。核验 verification-42ee47a22749477180ad948cddfde069：六项 supported、uncoveredRequirements=[]，18处引文均命中现稿。这里只确认持久模型验收及引文一致性，不称独立文学认证。

04:48:47.826UTC 模型确实使用 read_work_status.currentDraftSource.read 指定的 source-937404fe17aa6dfd205871000625de9c2904d229916c996dfaebd4d694006db2，单次读当前稿，然后保存 direct 核验及结章回执；新后续决策已启动。此前 UI/UX 后审条件的核验进度回执已闭合，APPROVE。

证据 `.release/context-live-validation/2026-10-12-sleep-fix-live.json`。该观察段累计报告输入629499（其中缓存513408）、输出18144（其中推理8514），总计647643；包括启动恢复、核验和后续决策，不是单章生成上下文或纯验收成本，未结算预留未混入；没有对照完整样本，不报节约比例。
