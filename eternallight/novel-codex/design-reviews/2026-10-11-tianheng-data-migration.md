# 《天衡诡契》一次性迁移

用户于本次明确授权“仅对《天衡诡契》现有数据做一次性迁移，保留备份”。该授权是 TEST_STAGE_ENGINEERING_POLICY 专项例外，不增加生产自动迁移或通用兼容路径。

责任：离线维护操作，Native 当前 DDL 及现有作品库分别是结构与内容权威。现有库内还有其他作品；共享表结构必须一致，但所有作品的全部原有列和值均逐行保留，不修改其他作品内容/状态，不清空任务或累计账本。只对当前已发现的三个缺字段做结构补齐：对话 scope_id=NULL（原小说对话无媒体 scope），work control_revision=0（本次结构初始代次，不重置 version/status），预算 initial_authorization_id=NULL（不伪造授权证明）。现有预算 JSON、序号和请求摘要原样保留。

先关闭应用，生成只读原库备份；在副本按当前 DDL 重建三表，保留索引及全部原列；事务完成前比较每张表所有原列内容的多重集合，检查完整性与外键不退化。副本经当前 Runtime 启动验证后才应用同一操作到原库（再次备份，事务内执行）。任何差异或失败回滚，备份持久保留在用户数据目录的 backups 中。后续仅由应用公开界面恢复原作品创作，迁移脚本不修改创作正文、任务状态或授权。

五视角自审前审：架构 APPROVE（离线维护，无生产兼容）；技术 A/B APPROVE WITH CONDITIONS（逐行比较、备份、事务与真实 Runtime 验证）；产品/UI APPROVE WITH CONDITIONS（原作品界面与正文可读后确认迁移完成）。未声明 100 章已完成。

副本与原库迁移均通过：130 张表 28092 行既有列多重集合完全一致；integrity_check=ok，foreign_key_check 未新增问题。副本经当前打包 Runtime 实际启动，输出 listening on 127.0.0.1。原库事务已完成，备份 `/Users/hilbert/Library/Application Support/小说工坊/backups/tianheng-before-20261011/novel-codex-v2.sqlite3`，SHA256 `a9bf5169c55f7e7dad449b3669d801d2f94dfca38d24e4646039d9b3c333b007`。一次性脚本与执行资料留在 `.release/tianheng-migration-20261011`，未接入生产启动。

五视角后审：架构/技术 A/B APPROVE，依据逐行比较、事务与副本真实启动；产品/UI 仍以实际窗口读取及续写证据为准。专项例外仅此次用户授权数据，不改变默认零兼容政策。未修改源码，无需重新编译或重复架构门禁。

实际界面复核发现第二层缺口：《天衡诡契》无 story_editions_v1 条目，不能通过当前 require_novel_project。补充方案仅对此 project 按 multimedia::initialize_project/insert_edition 当前语义建立 canon 空分支和指向原 main 的 novel edition；不复制或重写正文、不把候选设定伪装为共同设定，原 main 快照和所有原行不变。该新增结构是本次已授权项目迁移的一部分。五视角自审批准条件：新增行均属于目标 project，原行集合不减少，原 main head 不变，真实 UI 读取正文。

第二层迁移已完成：按当前领域初始化合同仅为目标项目新增 7 行版本/范围/空 canon 分支记录，原 main head 与全部原作品行保持；新增快照依法触发该项目 fact_audit generation +1（派生审计状态，不是内容变化）。旧 snapshot_chapters trigger 更新为当前按 novel branch 捕获的定义，避免 canon 分支错误捕获正文目录。副本首次断言发现该触发器差异，事务已回滚；修正后副本及原库均通过，未删除原章节。

实际 UI 已恢复《天衡诡契》第 1 章《死契上的活人》，显示“3007 字符 正文已保存”，任职 5 年等修订仍在。模型连接用工程已有凭据重新验证通过。前 100 章请求已提交应用，尚不表示已生成。产品/UI 的作品可读条件关闭；后台任务健康仍显示核对失败，需独立检查旧任务内容合同，不能以 schema 通过掩盖该问题。

完整结构后审：用当前打包 Runtime 新建空库，对全部已存在的表、索引和触发器做规范化 DDL 对照，补充发现 quality_packets.created_by_task_id、workspace_refs.producer_task_id/output_ordinal 及 task_intents 的 notSent CHECK 状态。辅助表和 CHECK 已按当前 DDL 重建，全部原行不变，新增来源字段保持 NULL 不伪造证据。辅助阶段副本 135 表/28099 行、实际库 135 表/27909 行迁移前后各自完全一致（两次快照间应用已经运行，派生索引/审计与执行数据有正常变化，不混称同一快照）。保留额外 before-auxiliary.sqlite3，SHA256 `6c9242769fb24bb87f87c95ecc1e6d386e1da9f83637de2472a34ab5b4962426`。

作品读取已经恢复；首次真实模型回合在辅助表迁移前稳定失败，未新增正文。补齐辅助表后准备原对话续发，工具自动审批拒绝最终发送，理由为尚需明确授权向 DeepSeek 传输现有私有作品上下文。没有绕过拒绝，恢复文本留在输入框未发送。未进行第二次模型读取及续写验收，后台任务健康核对仍有失败提示，保留为未闭合项。当前小说仍 1 章/3007 字符，不宣称 100 章完成。

后续授权与真实复验：用户明确回复“授权”后，已通过应用将现有正文/简报/续写要求交给当前 DeepSeek。迁移后的模型完整读取第 1 章与简报成功，登记 targetChapters=99 的 generation handoff，证明辅助表读取阻断已解除。尚无第 2 章保存。随后发现 Host 空闲 close 无条件发失败事件、项目扫描错误逃逸全局两个独立实现缺陷，见 [执行结束与扫描隔离评审](2026-10-11-execution-cleanup-and-scan-isolation.md)。不能把提案登记算作百章生成，也不为其他作品的旧结构增加迁移。
