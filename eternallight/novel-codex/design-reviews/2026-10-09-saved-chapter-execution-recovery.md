# 已保存正文的执行失败恢复

前审（五视角 APPROVE WITH CONDITIONS）：Native 已有按持久产物恢复任务为 ready_for_review 的机制，但后台生成恢复仅接纳无写入失败。完整正文已保存后发生响应流失败时，不能重写正文，也不能认定章节已交付。复用 tasks::recoverable_output 校验最新完整候选、输入版本，并核对规划场景映射、原 Work 结算与无未知写入；只恢复产物准备完成，保留原 error 与费用账本，后续仍由模型选择评估和修订。

边界：Story Core 判定保存事实与恢复准入；Host 继续消费原持久事件；模型保留文学评估与行动选择。无关键词路由，无新授权、数据回填或官方 Core 改动。暂停取消、有限额度未知费用、未结算、未知写入、部分稿、过期稿不得通过。恢复不得增加正文版本，不得增加 completedChapters 或伪造模型回复。

实施：为生成任务建立可核验保存结果的准入；原自动事件与 generation decision 使用同一事实；事务内恢复 ready_for_review 后继续原决策，不更换 Work 身份。

后审：架构、技术契约与状态一致性、故障恢复、产品语义、界面事实五视角通过代码评审。修复复用已有保存核验，未降低独立验收条件；新增回归覆盖完整保存、部分保存、输入失效、未知写入、未结算、有限额度未知用量以及暂停取消。真实第12章恢复、独立验收与20章全部完成尚未通过。

隔离真实故障回放：使用 SQLite backup 复制当前数据库到 /tmp/novel-saved-chapter-recovery-audit.sqlite3，调用公开 next_generation_decision 后返回原 Work 的 advisory 动作。原库仍 blocked，副本 running；两者 completedChapters 均为 11/currentChapterNumber 为12；正文版本列表、完整 ledger_json 逐字一致。失败 error 原样保留，产物 stage 仅变 ready_for_review。该证据证明保存恢复，不证明真实模型评估已完成。

界面验收：通过 CUA 在真实应用操作，第9章目录入口及第8、9章“查看正文”均能切换到对应已保存正文，分别显示2817/3134非空白字符。未发送输入框已有草稿“继续推进”，未点击重试。

验证状态（更新）：已有事件消费10项回归通过。新增夹具不能把 advisory 的冻结方法绑定直接用于 writer，现已补齐独立的 generation 作者来源、执行范围和 writer 方法绑定，正在重新运行。Native 构建已完成，尚未加载到真实应用。架构检查通过（339文件、7crate），差异空白检查通过。

macOS 诊断更正：先前系统日志存在拒绝记录，且测试曾停在启动阶段；系统设置中没有可用的“仍要打开”入口。后续测试在未更改任何安全设置的情况下启动并返回夹具错误。因此不要求作者关闭保护或修改开发者权限，也不能把当前所有测试等待均归因为安全拦截。

最终定向回归：`cargo test --manifest-path native/Cargo.toml -p novel-codex-story-core --lib work_execution_events_tests -- --nocapture` 11 passed / 0 failed（/tmp/saved-chapter-fixture3.log）。测试夹具重新冻结合法 generation 来源和 writer 方法契约，生产校验没有放宽。

实机加载待办：准备退出空闲应用时，CUA 返回 Mac 已锁屏，退出动作未确认执行。已请作者解锁；未改变系统安全设置，也未通过新创作指令推动任务。

实机加载（2026-10-09 18:11）：CUA 确认创作输入框为空、工作区已同步、正文已保存；正常退出并以原 user-data 启动新 Native（本机53995）。原 generation 自行由 blocked 转 running，没有发送创作指令、点击重试或修改作者数据库。原 writer 恢复 ready_for_review，error 保留；两条原 draft revision 未新增。应用创建原 Work advisory task-1593dd6b12174967bea7e3c04d0e91dc，完整读回第12章后登记独立评估 task-a3907084fe0046b799d32f00bcce8fc9，目标仍为原 draft-4744f940c1e44e50b84e95cab634b61b。界面显示具体执行步骤与耗时，第12章显示“正文已保存，待独立检查”。此时 completedChapters 仍11，不提前计入交付。

实机闭环证据：独立评估 revision-outcome-757773a859d64e17838320732aabd0f3 已保存，绑定原 writer 和 draft，六项 supported、uncoveredRequirements 为空；应用自主读完整报告并结章，completedChapters 增至12，chapterDeliveries 的第12章仍引用原 draft-4744f940c1e44e50b84e95cab634b61b。随后自动创建第13章 writer task-913ff54a37ea435488d84eef9ba03667，界面实时展示回读相关章节、核对设定、开始起草的过程。原失败 writer 的账本 charge 与隔离回放副本逐项相等，未清零或修改历史未知用量。本次故障的保存→恢复→独立评估→结章→下一章路径已在真实应用通过；原第3—22章任务仅完成第3—12章（10/20），整体20章完成仍未通过。
