# 来源续读合同只保留完整调用入口

证据：新版运行第52章独立核验任务task-65e43f5a81f44f10af62e3768a2d5e75在10:15:22Z把query=draft-chapter响应的cursor用于无query的请求，Native正确拒绝。随后模型重新读取上下文并大量按章查询目录。此前MCP已经生成保留全部过滤条件的nextRead，但同时返回裸nextCursor，schema的query/cursor说明仍要求手动复制nextCursor；存在矛盾的导航合同。不能将此后所有请求计为可消除浪费，核验本身仍需证据。

责任与目标：B04/B07，MCP只把Native已验证的游标呈现为完整调用请求，Native的分页、CAS、任务身份、范围校验保持。read_source_content模型响应仅保留nextRead，裸nextCursor移入nextRead.arguments.cursor；这是等值导航投影，不删除业务内容、覆盖、来源摘要或错误。工具schema/参数错误说明同步要求复制完整nextRead.arguments，原生HTTP仍保留自身分页合同；其他工具的nextCursor不受影响。无schema迁移或兼容双轨，无新增提示层，不自动读取或代替模型选择来源。

验证：跨真实JSON-RPC→HTTP的两页查询，复制返回nextRead即可执行下一页，query/view/版本不丢；结束nextRead=null且无顶层nextCursor；沿用既有成功只读响应的文本表示，业务字段原样保留。既有source导航、source工具及错误传输测试。架构门禁和本地打包，安全步骤边界更新后恢复原任务；不注入真实失败或重生成章节。

五视角前审：架构APPROVE（Native游标权威、MCP导航适配）；技术A APPROVE WITH CONDITIONS（参数完整、去可信身份）；技术B APPROVE WITH CONDITIONS（实际协议测试、业务值等价）；产品APPROVE（减少手动拼续读参数）；UI/UX APPROVE（不需要作者处理）。零兼容PASS。不能保证模型永不误用工具，也不预先承诺整章省率。原始证据保留于.release/action-selection-diagnostics/post-deployment-audit.json。

实测统计校准：按 session_meta 的 subagent.thread_spawn.parent_thread_id 纳入派生线程，第52章独立核验共62请求、3,806,550 token，与Engine结算逐项相符：input 3,707,066，cached input 2,804,608，uncached input 902,458，output 99,484（其中reasoning 56,037，不另加）。预留及unknown均0。此前仅根线程统计漏掉子线程；审计脚本已修正，生产账本无漏计。父核验者还抽查子执行已读的10章，独立核验需要的证据不能一概视为浪费。期间一次引用歧义拒绝后成功修正，未降低引用唯一性校验。

根因分类：契约设计缺陷与实现同步遗漏并存。nextRead本已具备保留全部条件的能力，MCP却仍暴露第二种不完整入口；本次无需新增Native状态机或权限层。真实当前游标失效仍按Native原错误返回，完整请求不承诺跨保存版本继续有效。调用者仍自主选择来源；MCP只呈现Native给出的续读参数。只读返回不改变输入覆盖记录、写入事务、预算、采用/发布或UI状态。五视角由同一执行者自审，不称独立复核。

验证中间结果：125项MCP回归全部通过（continuation-mcp-tests.log，含真实JSON-RPC/HTTP两页续读、绑定/错误/受信身份既有回归）。architecture:check扫描373文件、7crate，0诊断（continuation-architecture.log）。详细指南遗漏的旧nextCursor用语已同步为nextRead；最终产物正在构建并对最终源码重跑验证。Mac测试程序曾在_dyld_start等待后自行恢复，实际测试耗时1.12秒；未关闭安全控制或强杀其他构建。

后审：五视角自审均APPROVE；技术前置条件已由125/125最终MCP回归关闭（continuation-tests-final.log）。架构门禁373文件/7crate零诊断，本地打包6门禁全部passed（2026-10-12T10:37:19.703Z）。打包MCP与构建SHA256均e480c3cb6275b6fd93cd0bdbd0237f2682d63bb9f632ba9f956352ffd39cd6ab；Native与Core身份保持。全部52章revision/hash更新前后完全一致（continuation-drafts-before/after.json）。原任务已通过UI在步骤边界暂停后更新；已启动新版并点击原任务恢复，后续运行核验另记。无发布，无新增根任务或测量模型请求，未恢复定时任务。真实续读调用尚待后续任务自然触发，不宣称整章省率。

实际应用再验：10:38:39Z 原run generation-d6db3e3374164a09809d9dadfc000ab8 running/version781，活动task-132f0e4918c74bbc83baa988b9e2aa1f，未建新run。10:38:48—57Z 已实际完成read_context、read_work_status及3次read_source_content，读取作者要求和第52章核验报告；3次source响应均含nextRead、无顶层nextCursor、无错误。新版呈现合同已在真实运行生效；多页续读参数保留由实际RPC回归证明，本次实测尚未观察到非null续读，不能声称模型所有续读均已改善。
