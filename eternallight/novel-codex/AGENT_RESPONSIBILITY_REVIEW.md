# 按需创作与同轮参数纠错

当前状态（2026-10-06 后续验收）：纠错已覆盖 bundled 清单中的全部 29 个工具，本页下文记录最初单工具阶段。旧失败已由完整修订套件 35/35 清零，关联原生 41/41；工具原生 39/39、主机 29/29、真实 Codex 纠错 8/8 与完整创作旅程 1/1 均通过。详见 [当前契约清理与复核](CURRENT_REVISION_CONTRACT_REVIEW.md)。

目标：作者表达意图，Codex 选择必要小说工具，Runtime 保证写入与数据一致性。修正一句措辞不再必经知识建模；参数错误不再必然变成整轮传输失败。

## 实施边界与前置评审

- 架构专家、技术专家 A（recovery_review）：受信 host 精确指定可由执行器校验的工具；只先启用 `novel_workspace.publish_knowledge_batch`。MCP 按完整原始 schema 校验后才可发 HTTP。Bridge 保留工具身份、tool_choice、JSON、大小、重复 ID 与流提交校验。不改写模型历史，不增加 Agent 循环。
- 技术专家 B、产品经理（runtime_policy）：普通改稿的知识分析可选；连续生成、明确 desiredOutcomes 保留当前 revision 依赖。协调完成后后台不得再偷偷派发同一写作 revision。保存仍原子失效旧知识，不能伪造 ready。
- UI/UX、产品视角（diagnostic_ui）：高级排障默认折叠并按需加载。日常索引维护已有事务路径；六类全量审计不是作者检查清单，也不表示文学质量。
- 综合决策：按上述约束实施。三次错误上限必须在 MCP 先锁住后续调用，再由 host 停止执行，避免同批写入竞态；须实际 Codex 集成验收。

## 零兼容门禁

无版本判断、迁移、双读写或历史入口。接口一次性同步 host/worker/bridge；MCP 结构化失败是现有工具结果扩展。权限、预算、任务租约、幂等性和 Runtime 最终授权不变。未重置或删除作者测试作品。

## 实现

1. Host `executorValidatedTools` 私有配置注入 codec context，绝不来自模型响应、连接配置或对话内容。其他工具继续 bridge schema 校验。
2. 设定保存工具参数错误由真实 MCP `isError` 返回。诊断只含 schema 字段和有限说明，不回显非法值。连续三次 schema 错误使本 MCP 进程停止接收业务调用；host 只信任当前 thread/turn 对应的结构化终止码。
3. Work 以保存回执定位改稿，按作者意图选择后续检查。旧知识仍失效，连续生成仍校验精确来源。
4. 设置中诊断收拢为紧凑、按需展开的高级排障，未运行和报告过期采用中性文案。

## 验收记录

2026-10-06 实际验证：

- 原生 MCP 35/35 通过；参数非法不产生业务 HTTP，连续三次失败后合法读取/写入也被阻止。
- 新增桥接协议 8/8 通过，覆盖 Responses/Chat 与 stream/non-stream、精确受信身份、原始调用参数和 ID；原有严格验证 23 项通过。
- 真实 Codex + 原生 bridge/MCP 的集成 4/4 通过；供应商与 Runtime HTTP 为 fixtures，验证同轮纠错和有界终止，不等同真实模型验收。首次冷启动 thread/start 超时后全项复验通过。
- Runtime 按需策略 3/3、交付回执 5/5、连续生成 1/1 通过（生成测试更新为当前 execution scope 契约）；前端构建及此次相关诊断/编辑/发布/恢复 UI 专项通过。
- 新独立应用已打包并启动。打包内 30 秒首次冒烟超时，独立复验返回 NOVEL_CODEX_PACKAGE_SMOKE_OK / exit 0。
- 通过实际应用 UI 重新验证 DeepSeek 连接后，向测试作品发出限定一句措辞的修改指令。界面显示正文 863→867 字符、执行结束且结果已保存；未新建章节、未发布。模型回复说明只作局部检查，未进行设定分析或全篇检查。只读数据库独立核对：Work `work-task-4662992a6ecc786b17002287543df4db` completed，三个步骤 advisory→generateChapter→advisory 均 deepseek-flash/completed；新旧正文严格等于唯一指定句替换，标题、场景元信息/顺序/锁定与 sourcePlanRefs 不变。新 revision 无 modeling batch，当前章 outbox 为空，项目无 active task，仍仅一章且 draft 未发布。
- 验收范围：本次改动的专项通过不代表全工程全绿；更广的 revision_outcome_tests 初跑 22 通过、10 失败，涉及当前契约与旧 fixture 预期，未为使其通过增加兼容分支。旧任务的失败/待核对记录保留，未伪造清除。
