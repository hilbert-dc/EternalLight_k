# 恢复行动工具的互斥选择合同

## 实际故障及责任

current11 原Work恢复决策 task-f24fd072…三次同时提交非null actionKey 与 proposal，均在MCP本地拒绝，未发送Native业务请求。公开目录却允许这两者同时存在或同时缺失：执行器已有二选一检查，schema只声明可选字段。Core真实wire转换及Engine实际schema编译完整保留目录，故约束缺口属于MCP声明；不能通过修改Core、替模型删除字段或放宽Native解决。实际证据 `/tmp/autonomous-v3-current11-decision-arguments-diagnostic.json`、`...-request-work-action-catalog.json`、`...-work-action-engine-schema-diagnostic.json`。

B01/B02/B04/B05/B10：仅为task_source_tools.rs生成的request_work_action inputSchema加入oneOf:[{required:[actionKey]},{required:[proposal]}]及简短根description。保持字段现有非null类型、严格对象、Native版本/摘要/作者范围校验。已有ArgumentValidation在/oneOf错误上取根description作为fieldInstruction；不新增第二校验器或产品关键词路由。actionKey选择已有readyAction并保留冻结目标，proposal提出已有批准范围内新动作；工程不替模型做选择。其他工具目录不变。

读取权威为当前生产工具目录及真实调用身份；无作品/预算/作者记录写入变化。执行仍走MCP→Runtime→Native，错误在业务入站前给模型准确修正信息。不会抹除原失败或回填旧错误。另一故障——Engine丢失machine code——另立评审，不在此改动中引入恢复政策。

## 五视角实施前审

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE | 声明与执行在MCP同一真实定义贯通；通用Core及Engine保留既有职责。 |
| 技术A | APPROVE WITH CONDITIONS | both/neither/显式null在入站前拒绝；合法二选一不改变target、版本、授权或原始参数。 |
| 技术B | APPROVE WITH CONDITIONS | 使用实际tools/list与handle_message入口，持有同一RuntimeClient验证同任务纠错及三次限制；实际Corewire→Engine codec验证生产schema。 |
| 产品 | APPROVE | 模型获得可操作反馈，自主选已有动作或新提案；用户无需技术输入。 |
| UI/UX | APPROVE | 维持事实状态，未保存不显示完成，内部schema与错误明细仍按需展开。 |

零兼容：无迁移、双轨、旧字段fallback、null API放宽、旧错误推断或预算重置。当前QA原记录保留，用于实际同Work恢复；隔离合同测试按现行schema创建。

## 验证及闭环标准

实际生产目录只对本工具增加choice规则；actionKey-only/proposal-only合法，both/neither/null返回constraint、schemaPath=/oneOf或对应类型路径，静态fieldInstruction不泄露参数值。真实RPC校验非法不会发业务请求，同绑定修正后仅发送一次且原关键字段保持。成功的其他只读工具不能清除此工具错误计数，第三次终止且后续合法调用不绕过。

Root负责electron/tests/codex-runtime.test.mjs现有生产目录Corewire→Engine检验的新增choice矩阵；Native agent只改MCP source及其测试。独立五视角复核后构建、运行受影响完整MCP与目标真实目录测试、architecture:check。应用继续原Work且不追加作者输入/工具参数，真实模型交付仍待验，不以合同测试代替。

## 实施后复核

独立五视角前后审通过。仅spec返回处8行增量及对应RPC回归，源码9054d690…5cb1c、MCP测试c23c0ea3…1989c；Root生产目录矩阵fee9c390…30393。目录仅新增本工具oneOf与212-byte静态description，其他字段、required、执行和授权不变。MCP完整98/98实际exit0（Agent工具session76215/chunk998e7b，仅工具输出，无磁盘日志；不补跑制造日志），定向3/3亦通过。首次非法null用例错误预期类型路径已纠正；完整首轮97/98唯一fixture接收socket继承非阻塞导致WouldBlock，测试helper显式设置阻塞+原5s read_timeout后全通过，未改变生产拒绝或断言边界。

Native生产bins构建exit0、真实生产目录→Core wire→Engine的完整Node目标35/35实际exit0、architecture332文件/7crates/0diagnostics。统一冻结源码/二进制及执行日志 `/tmp/autonomous-v3-current11-choice-error-build-proof.json`。旧CoreCLI未变；静态与合同不代表实际修订交付，现沿同QA原对话继续模型恢复。

原实际局部修改未通过，当前稿1146非空白字符、独立报告长度未达模型冻结的600–1100范围；本修复不降低标准或替模型修订。

## 解锁后的实际合同再验

现Mac已解锁，原对话仅点击既有重试一次。真实DeepSeek decision task-9c4af789…选用合法actionKey分支，同Work实际登记并执行writer task-ffe942e…，保存draft-c3c386b…；作者仍5行，无验收者技术纠错或重新输入。公开声明→真实模型选择→Native登记的本次故障路径已接回。新稿1211字符且前文标点变动，后续独立评审失败及重评来源身份拒绝另立评审，不能将本合同修复等同整体创作交付。证据 `/tmp/autonomous-v3-current11-choice-contract-ui-resume-started.json`、`/tmp/autonomous-v3-current11-choice-contract-progress-02.json`。
