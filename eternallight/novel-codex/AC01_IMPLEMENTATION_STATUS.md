# Author-directed AC-0／AC-1 实施状态

现行工程策略：[测试阶段工程策略](TEST_STAGE_ENGINEERING_POLICY.md)。本工程只支持当前 schema、API、IPC、MCP 与 app-server v2 契约，不保留旧逻辑或迁移路径。

- 日期：2026-09-13
- 对应产品目标：[PRODUCT_AUTHOR_DIRECTED_CREATION.md](PRODUCT_AUTHOR_DIRECTED_CREATION.md)
- 对应实施设计：[AUTHOR_DIRECTED_AC01_IMPLEMENTATION_REVIEW.md](AUTHOR_DIRECTED_AC01_IMPLEMENTATION_REVIEW.md)
- 后续产品方案：[创作质量提升的产品重构与优化](PRODUCT_CREATIVE_QUALITY_REFACTOR.md)，已更新为 v1.1，配套[机制方案](PRODUCT_CREATIVE_MECHANISMS.md)和[源码证据](ETERNALLIGHT_MECHANISM_EVIDENCE.md)；F1 创作依据首个闭环已实施，其余新增阶段仍待实施。
- 当前结论：主流程已接通，本轮体验缺陷修复已通过回归与定向交叉复核；证据见 [作者体验修复记录](AUTHOR_EXPERIENCE_REPAIR_REVIEW.md)。本轮继续补齐长对话交接、作者分范围导出和本地聚合指标，见 [交接、导出与指标验收](AUTHOR_HANDOFF_EXPORT_METRICS_REVIEW.md)。自动化通过不代表完整产品验收。

## 已落地的闭环

1. 作者可从构想讨论、创作简报或直接写第一章开始。Brief、全书／卷／单元 Outline、近章 Plan、场景与正文都进入候选、审阅、采用和撤销链。
2. Outline 只有 `{nodes}` 一种层级契约；稳定 ID、父子关系、排序、保护意图／安排、公开写作指引和依赖键由 Runtime 校验。Plan 必须引用当前正式 Outline，并使用稳定 `chapterPlanId` 与 `primaryNodeId`。
3. 章纲按需物化；没有 binding 时创建 `localOnly` 独立章节，任务只冻结 `localOnlySnapshot`，不会继承作品内其他章节的正式 Outline/Plan。独立章节可保持独立、关联正式章纲或返回 Planner 建立规划。
4. Planner 会获得有界的作品标题与原始简介；生成 Brief 不再丢失作者构想。重生成 Outline 会获得当前正式 Outline 的稳定 ID 与保护信息；滚动 Plan 会获得最近三章正式正文的有界尾部摘录。Writer 只获得公开标题、公开承诺、公开指引、POV、故事时间、允许位置内的正式前文和机械脱敏的约束；项目简介、私有 title／goal／conflict／dependencyKeys／秘密不进入 Writer。
5. Writer target 显式冻结 `chapterId/chapterNumber/readingChapter/scope/sceneOrder/povCharacterId/storyTime`。Context、checkpoint、Review 与事实提案逐项验证相同作品、线程、版本、章节和授权范围。
6. `operation` 是任务语义唯一权威。Runtime 只接受 `advisory` 与四个 generation operation，并自行派生 purpose、kind 和 required output；旧 kind/purpose/output 推断与调用方权限字段已删除。
7. 每任务使用独立的 256-bit、30 分钟 capability。Runtime 只保存 hash；MCP sidecar 持有明文并在候选请求中发送。任务完成、失败或取消时立即销毁 App Server／MCP 进程并擦除明文；Runtime 同步撤销能力。应用启动会在开放 UI 前取消并撤销上次崩溃遗留的所有活跃任务。
8. `model-catalog.json` 固定 provider/model/operation 组合；`codex-model-catalog.json` 固定 Codex 实际模型元数据，并复制到隔离 CODEX_HOME 后通过 `model_catalog_json` 装载。DeepSeek 的 `model_provider` 位于配置根层并由真实 TOML 解析测试验证，仍只允许自由讨论。浏览历史启动的空闲 App Server 可安全重配；活跃 turn 在写盘前拒绝，落盘失败保留原内存配置与进程。基础 instructions 已收敛到 1K token 以下，避免单个新增大上下文项。两个目录都有固定 SHA-256。`supported-cli.json` 逐来源、平台与架构固定真实 CLI 摘要和 `--version` 输出；发布暂存脚本为目标平台复制 CLI/Runtime 并生成 bundled 摘要，任一摘要、版本或目录指纹漂移都会在模型调用前拒绝。
9. MCP Context 结果只有一份模型可见表示，并拆成每项最多 9,000 UTF-8 bytes 的内容块。Context 总预算最多 60,000 bytes；单个规划产物最多 48 KiB，可容纳一份合法核心产物，硬约束放不下时明确失败。
10. 正文采用前冻结 provisional impacts。事实证据问题在审阅中可见，但采用时只重新检查所选场景中被作者确认的事实；作者拒绝的错误建议不会阻塞正确正文。显式空场景清单会被拒绝，不会意外回退成全选。保护冲突必须逐项选择退回草稿、采用并打开反馈或带说明标记误报；无自动命中时作者可用完整层级路径手动补链。
11. 事实采用与 impact 创建同事务完成；事实的 `sceneOrder` 由所属 `SceneChangeGroup` 的一基顺序派生并强制一致，不能由模型另报。反馈链保留来源状态事件、来源章节 ID／标题、证据、原因和目标，卡片可直接返回来源正文。kept、updated、false-positive、withdrawn 分开记录，撤销会恢复相应反馈与依赖链接。
12. UI 提供 Runtime Writer 来源预览、规划与正文任务的失败／停止／部分稿恢复状态、候选 CAS、真实三方冲突元数据、工作版本选用／取消、可恢复续创意图、“保留并按此意图继续”、本地稿复制、服务器重载、应用内导航和关闭窗口的保存候选／放弃／留在三选。作者的生成意图已经构成授权，不再为无公开规划增加第二次确认。事实审阅完整显示 kind、subject、value、owner 与 epistemic state，相关表单、场景选择和决定按作品／章节／候选或 review 恢复。
13. 候选写入幂等记录按 author/task 隔离；所有 HTTP 候选写入必须显式提交 candidate CAS（首次为 `null`）；任务完成前确认其输出仍是当前候选。MCP 写入响应只返回修订与检查点元数据，不把模型刚提交的正文或完整 payload 再注入上下文。
14. 应用在启动 Runtime 前取得单实例锁，第二实例只聚焦现有窗口，不会误回收正在运行的任务；隔离 Codex 配置每次原子覆盖。
15. 规划选择器显示“全书→卷→单元→章”路径；技术依赖键和稳定 POV ID 位于高级详情，角色使用可读名称选择。Outline 删除、字段错误和关键状态具备键盘焦点与可访问说明。
16. app-server v2 适配器只接收匹配 thread/turn 的事件，并返回当前 turn 有界的真实最终助手文本；自由讨论结论可以进入后续规划意图，不再由固定完成文案代替。当前任务可停止；自由讨论恢复作品自己的 advisory thread，领域生成每次重启隔离 sidecar。

17. 长输入在提交前统一校验字符、UTF-8 字节及应用文本预算；组装后的技能／任务／作者指令再次校验整轮预算，再按 Unicode 边界分片，同一轮传递且不截断作者全文。
18. 作者导出从同一 Runtime 快照生成 TXT／Markdown；默认正式正文，公开规划、候选稿及私密资料分别选择。私密事实保留完整语义并标记已被替代的历史事件，不再直接下载全量 RuntimeState。
19. 设置提供本地聚合指标查看、匿名汇总导出与明确记录放弃；计数随领域事务提交，重试去重，实际口径见下文。

## 验证证据

- `npm run test:runtime`：97/97 通过；新增工作版本 CAS、选中 B 忽略未选 C、输入快照逐字回放、工作 Outline→Plan→Writer 来源、required 超限失败及旧任务安全收尾。
- `npm run test:agent`：69/69 通过；新增完整执行期配置保护、Unicode 摘要、历史恢复与空页查询的真实 handler/Adapter 回归，保留 MCP、凭据、版本目录和任务生命周期检查。
- `npm run test:ui`：3/3 真实 React/Electron 交互回归通过，覆盖保存失败与二次保存、稳定场景身份、部分采用选择、意图恢复、旧回包隔离、无指导确认和焦点，以及导出范围／失败重试、本地统计幂等重试与匿名下载。
- `npm run build`：TypeScript 与 Vite production build 通过。
- 先前 macOS 新用户目录冒烟覆盖空作品三入口、localOnly 第一章、Runtime 预览、无规划确认和三选离开。本轮事实审阅、生成锁定、预览身份和配置重载修复尚未完成干净目录的桌面点击复验，因此不把自动化与构建结果表述为完整 UI 旅程证据。
- 五角色实施复核记录见 [AUTHOR_DIRECTED_IDEAL_REPAIR_REVIEW.md](AUTHOR_DIRECTED_IDEAL_REPAIR_REVIEW.md)。

## 发布前证据

以下为尚未完成的发布验收证据，不能用自动化测试数量替代：

- 用固定模型执行完整“构想→Brief→Outline→Plan→第一章→事实反馈→改纲”真实调用，并审计完整模型输入、MCP、错误与日志中的秘密 canary。
- 在当前 schema 的干净用户目录中复验本轮修复的桌面路径：生成期间编辑锁定、切章预览、完整事实语义、拒绝错误事实、审阅决定恢复、场景插入／换序和空闲模型重配。
- 由目标作者完成可用性测试，验证规划成本、正文返工、方向控制和中途放弃原因；不能以测试数或字数替代文学质量结论。
- 完成 Windows 实机、签名安装包、对应平台的 bundled CLI 摘要登记和人工崩溃恢复演练。
## 产品指标与后续范围

本地 31 项聚合指标已实现，设置支持查看及匿名 JSON 导出。阶段采用历时、任务历时、反馈处理、独立章去向与作者明确记录的放弃均有事务内计数；失败重试去重，部分采用生成的内部版本不会增加保存或返工次数。指标不含正文、名称、项目／人物 ID 或原始时间戳；另存最近 1,000 项随机操作回执用于重试去重，不导出。

采用历时从采用的候选保存时起算，包含等待和离开；“内容变更保存”和“旧基线续存”是实际行为代理，不等同于文学上的实质重写或复制按钮点击。匿名聚合可用于作者试用比较，不能据此宣称已降低返工或提高创作质量。

后续路线按 [创作质量提升方案](PRODUCT_CREATIVE_QUALITY_REFACTOR.md) 细化为 Q0 质量基线、F1/F2 创作依据与取证、AC-2.1 隔离短试写、P1/P2 规划深审与修订、AC-2.2a/b 局部改纲与组合采用、N1/N2 单元与承诺、Q1 声音依据与 Q2 文学反馈／创意兑现；连续多章和更大范围分支仍在其后。这些属于产品方案，不计入 AC-0／AC-1 已实现能力。
