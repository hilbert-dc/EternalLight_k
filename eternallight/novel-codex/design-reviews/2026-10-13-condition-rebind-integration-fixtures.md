# 条件检查与章节重绑定真实集成夹具

## 五视角前审

条件检查种子仍以仅 title 创建作品，当前创建接口要求明确 medium 与 idempotencyKey。先按真实创建合同补齐，transport 保留 HTTP 状态与非 JSON 错误正文用于诊断。重绑定种子在取消旧任务后假设父生成仍 running，需读取真实 blocked 原因和当前任务结束推进规则，再决定合法种子顺序；不放宽生产准入，不伪造任务/预算/保存回执，不去掉重绑定结果及作者稿保留断言。

架构 APPROVE WITH CONDITIONS：夹具跟随现行合同；技术A APPROVE WITH CONDITIONS：真实终态/预算与冻结来源；技术B APPROVE WITH CONDITIONS：两个真实Runtime/Electron定向验证；产品 APPROVE：保持候选、作者编辑与重绑定权限；UI/UX APPROVE：生产界面不变、保留真实用户操作路径。

## 已证实根因与实施

- 条件检查作品创建补齐真实 medium=novel、logline 与固定 idempotencyKey；错误 transport 读取 HTTP 正文后解析，非 JSON 响应保留方法/路径/状态，避免丢失原错误。
- `work_decision_generation::observe_terminal_attempt` 在 task 取消同一事务中将父运行置为 blocked/executionBlocked。夹具先严格断言取消前 running 与 activeTaskId，断言真实取消回执 cancelled，再对父运行做完整深比较：除 status、blockedReason、version+1 外不变。没有强行制造 running，也没有将断言放宽为多个可接受状态。之后真实 UI 重绑定的完整 JSON 深比较仍验证 chapter_plan_rebound 和再加一版，预算仍按真实账本结算。
- 完成章节的标准现由 chapterAcceptanceSource 冻结来源返回。夹具通过既有 raw source 分页读取连续字节、验证 hash 后使用真实 requirements/length，保留独立评估覆盖每条标准的断言，没有回填虚假标准。
- 阻塞文案由共享说明边界改为“章纲已更新，需要根据当前内容重新规划。”，同步精确 UI 等待及仅此一条快照；其余绑定、来源、作者稿、预算、重启后续写断言保留。

## 五视角后审

架构 APPROVE：仅真实集成夹具校正，没有修改生产准入/推进/合同。技术A APPROVE：增加精确取消事务父状态比较、按真实冻结来源读取验收标准。技术B APPROVE：条件与重绑定真实 Runtime/Electron 旅程通过；与追加作者输入用例合并最终 3/3 通过，日志 `/tmp/condition-rebind-author-final.log`，目标 diff whitespace 检查通过。产品 APPROVE：作者藏信稿保留、候选不正式采用、预算不追加、重启后根据新来源继续等原断言执行通过。UI/UX APPROVE：仅同步当前可见说明，未隐藏失败或改用户流程。
