# 修订流程当前契约清理

## 目标与范围

清理 revision_outcome 全套测试及关联 Electron 修复任务指令中的旧自动流水线、内嵌全文假设；修复核查发现的历史报告读取缺陷。保留当前授权、原子回滚、幂等、历史冻结与完整分页读取保障。无兼容、迁移或旧入口恢复。

## 前置五角色评审

- 架构专家：APPROVE WITH CONDITIONS。taskSources/read_task_source 是任务内容唯一读取契约；不可变报告出处与当前执行新鲜度必须分离。
- 技术 A：APPROVE WITH CONDITIONS。分页测试检查来源、hash、UTF-8 连续字节和冻结内容；不能以删断言替代完整性保障。
- 技术 B：APPROVE WITH CONDITIONS。历史报告允许读取已过期但合法保存的来源；新执行/新决定必须继续拒绝 stale。一次性同步调用方。
- 产品经理：APPROVE。后续评估由智能体显式选择，不能恢复固定流水线以满足旧测试。
- UI/UX：APPROVE。无需新增作者操作；保留可读历史、明确失败和无重复任务恢复。

## 零兼容门禁

不添加版本判断、旧字段 fallback、双读写或迁移。测试数据库按当前 schema 新建。报告读取保持身份、作品、原始正文与发布来源验证；事务与授权测试不削弱。实施后由完整相关套件结果复核。

## 实施后验收

本轮发现并修复真实历史报告缺陷：不可变出处校验此前依赖 current knowledge head，导致合法保存的旧报告在换稿/重新建模后不可读。改为验证同作品、对象、正文版本与已发布批次；所有新执行/写入仍须 inputs_current。

第一轮修订套件 32/35 通过。余项检查发现同一修复 Work 的合法后续协调器被误判旧报告失效；将按可信任务绑定修复，不能以出现 workDecision 字段即跳过验证。其余失败涉及完成后才取预算的旧fixture顺序和小于元数据本身的上下文预算。

Electron 修复提交/任务交接 36/36 已通过。完整最终结果待本轮完成追加。

## 通用参数纠错前置复核

用户继续完成按需创作能力，当前单一设定工具委托不能覆盖全部领域工具。架构/技术 A/B 有条件批准：固定 bundled exact-name 清单，由 Rust specs 一致性测试约束；每 operation 使用暴露给模型的 schema；领域本地纯校验也进入有界纠错，Runtime/权限/未知提交结果不重分类。每工具独立计数，仅完整本地校验成功重置该工具；任一达到三次全执行器锁定。终止消息绑定当前 tool、originTool、thread/turn、server 和结构化错误码。产品/UX 批准统一失败提示，避免让作者理解 schema 字段或手动维护索引。不得添加通配、配置扩权、跨任务重启或新预算。


## 实施进展补充

- 修订完整 suite 最终 35/35，task sources 2/2，work decision 4/4，共 41 项通过。第一轮三个失败全部解决。
- 修复 Work 保存后继续协调通过完整注册任务绑定验证；新增跨 Work、跨来源与未注册步骤伪造拒绝和事务无副作用断言。
- 真实 Codex + MCP + Runtime 完整改稿/修复/续作旅程 1/1 通过；provider 为受控 fixture（256k 上下文、精确 usage），不代表 DeepSeek 长篇质量或压缩验收。
- 29 个工具名称共享 JSON 清单，按当前操作实际 schema 及显式本地领域校验提供纠错；全部HTTP响应、网络故障和授权拒绝保持独立语义。主机 29 项通过，原生及扩展集成结果待补。


## 最终复核（2026-10-06）

- 完整修订 35/35、资料分页 2/2、任务决策 4/4，原先失败已清零。
- 原生 MCP 全套 39/39，包含全部29工具清单同源、操作特定schema、字段/领域错误零HTTP、身份失败不计次数、每工具独立计数和耗尽后的全局阻止。
- 宿主终止/适配器 29/29；修复提交与任务交接 36/36。
- 真实Codex、原生MCP/bridge 同轮纠错 8/8：Responses/Chat各覆盖设定发布与正文保存的纠正成功/达到上限；原调用ID、绑定、CAS、幂等键及原预算保持。模型与业务HTTP受控fixture。
- 完整真实Codex+MCP+Runtime创作旅程在新工具校验下再次 1/1 通过；保留全历史、分页、修复决定、续作及精确用量检查。使用受控provider，不替代真实DeepSeek长篇质量或压缩验收。
- 五角色实施后复核：边界与前置方案一致；无旧接口回退、兼容层、预算重置、授权放宽或跳过失败测试。零兼容门禁 PASS。
- 不将网络、权限、源版本冲突或未知写入结果当成纯参数错误自动重试。
- 日志：/tmp/novel-current-contract-native-final.log、/tmp/novel-all-tools-native-final.log、/tmp/novel-all-tools-host.log、/tmp/novel-all-tools-engine.log、/tmp/novel-all-tools-journey.log。

- 更新应用：`npm run package:dir` 成功（含打包应用启动冒烟检查），已启动新版 `release-conversation/mac-arm64/小说工坊.app`。界面确认 Runtime 本地在线、测试章正文 867 字符已保存。

- 新包真实 DeepSeek 只读验收：经应用对话请求读取当前第1章，正常返回中文内容并显示讨论结束；只读数据库核对 Work `work-task-57434116724a6223963e60fc7d8a7f26` completed，仅 advisory/completed 一步，保存产物 0。此样本验证阅读执行与无写入，不宣称所有模型响应均满足简洁性或文学质量要求（回复仍出现 contentHash 技术词）。
