# 模型请求完成但缺少产物：原Work有限恢复

## 真实故障、责任和最小方案

current11原同对话末段修改已保存为1146非空白字符。原未知调用精确核定后，应用自行安排独立检查，报告发现长度未满足模型冻结标准，模型自行登记定向修订。新writer task-184a7581…正常结束Core turn，但只有7个成功只读MCP调用，没有save_draft_checkpoint或complete_task。Host按公开Native complete合同检查零产物，得到TaskOutputMissing后将领域Task置failed；Engine和Native provider charge均保持真实completed、7请求/147410已知tokens/0unknown、grant已settled。没有未知写入。证据 `/tmp/autonomous-v3-current11-repair-public-metadata.json`、`...-repair-blocked-02.json`，只读技术元数据不复制正文、原话或凭据。

Native conversation_receipts::known_zero_failure只接受charge.status=failed，混淆服务商执行与领域交付。retryable=false导致work_tasks::resume_block_reason对TaskOutputMissing显示pendingRecovery，持久attemptSettled事件停在pending。这是Native恢复事实判断的职责错误，模型未写出产物是另一个真实失败，不能替模型制造正文或宣告成功。

B03/B04/B06/B10：只修改Native已知零领域产物证明。保留已有failed provider charge分支，新增仅TaskOutputMissing+completed provider charge分支；不是允许任意completed模型请求重试。其他证据保持：领域Task failed、output_refs=[]、零task_outputs、无任何非rejected Native operation、原grant已settled、真实同Task用量unknown=0且无overrun。公开工具、Native/Engine用量账本、Host事件消费、Core循环和Renderer接口均不改；不改旧charge、不清账、不复活已结束Task、不追加作者输入。

恢复仍由已有原Work决策合同和持久事件驱动，模型选择是否重试、如何修订或如实交付未完成。原授权、版本、输入来源、预算、暂停/取消门禁保持；同条件两次失败必须提出不同恢复计划，四次耗尽不能改名绕过。未知写、部分输出、未知费用、overrun、未结算、非失败Task和不支持错误保持原阻断。UI沿真实resumeBlockReason投影，不新建第二循环或固定文学流水线。

## 五视角实施前审

| 视角 | 结论 | 检查 |
| --- | --- | --- |
| 架构 | APPROVE | Native唯一判断领域零产物；provider执行状态与交付状态分离，复用原Work接力。 |
| 技术A | APPROVE WITH CONDITIONS | 仅明确缺产物+completed结算分支；零写/零产物、settled、unknown/overrun边界保持。 |
| 技术B | APPROVE WITH CONDITIONS | 真实Router/Repository完成拒绝及结算后验证；正常模型请求状态与Task失败同时存在；负例和有限恢复不可绕过。 |
| 产品 | APPROVE | 用户无需技术输入；模型仍负责产物与纠错，不能把未保存判完成。 |
| UI/UX | APPROVE | 当前无未知写不再误报保存未知；真实失败保留，由事实解释状态。 |

独立scoped_save_proof_review前审APPROVE；Root采用比通用terminal charge放宽更窄的TaskOutputMissing专用分支。零兼容PASS：当前版本事实一致性修复，无迁移、回填、fallback、版本判断或权限扩张。实施前冻结原源码和真实失败材料，完成对应回归后再沿原应用重启，验收者不补剧情、工具参数或检查结论。

## 验证和真实闭环要求

真实同Work子任务正常模型计费completed、Native complete零产物拒绝、Taskfailed并settled后，有限决策能领取，读取真实TaskOutputMissing、0产物、已花费用；模型选择合法后继，保存真实产物且原作者与Work授权数量不增。completed其他错误不进入新增分支；cancelled、未settled、unknown、overrun、已有产物/committed或accepted操作及错误身份继续阻断。已有failed分支、两次计划与四次限制继续回归。

运行受影响完整Runtime work_decision目标、Core lib、Native构建及architecture:check；Host/Engine/CoreCLI不变引用冻结边界证据。最后实际应用自动接回原事件，由真实DeepSeek自行修订、检查与交付。未通过环节和全部失败保留，不能用自动化数量代替实际成果。

## 实施后

仅conversation_receipts.rs新增收窄的completed结算条件，SHA4892fecc…50c8。两个真实HTTP用例及11项负例冻结于work_decision_http_tests.rs，SHA73c2dc27…22ec2；独立五视角生产与测试后审PASS，零兼容PASS。没有新增Host循环、补写产物、修改费用或UI接口。

Native bins实际构建exit0；Core lib完整266/266、完整Runtime work_decision目标12/12、architecture332文件/7crates/0diagnostics均exit0。Runtime首次执行曾在dyld入口静止，实际一秒采样仅MainThread _dyld_start、112K footprint，尚未进入libtest；准备切换时原进程已自行执行并exit0，随后同冻结源码本地重跑也12/12通过，不能统计成24个不同测试或业务失败。原日志、采样、退出前身份诊断均保留，未修改timeout、断言或生产代码。实际源码/二进制/日志证据 `/tmp/autonomous-v3-current11-missing-output-build-proof.json`，旧未变Host/Engine/CoreCLI边界引用前次冻结证据，不重复冒用来认证新Native条件。

原QA的20Task/5Work/2章/6版本/3报告及费用均保留，0活动Task/预留后只停止已确认的本次开发实例；无作者输入重发。当前局部修改实际交付仍未通过，待应用重新加载后由真实DeepSeek沿原事件自行决定修订、检查与交付。静态和HTTP正例证明恢复合同，不认证实际模型正文质量或结果。

### 应用再验：恢复合同通过，后续工具合同阻断

实际重新加载后，没有点击Retry或追加作者输入，原持久事件自动接回同Work，DeepSeek决策task-f24fd072…已执行，UI的“保存结果未知”提示消失。新增16请求/597234已知tokens/0unknown；21Task、5Work、6版本（3正文3报告）、5作者记录、0采用保持，费用及失败完整保留。终态证据 `/tmp/autonomous-v3-current11-after-missing-output-recovery-02.json`。

该决策三次同时提交互斥actionKey/proposal，本地拒绝无业务入站，最后tool_argument_correction_limit停止。MCP公开schema未表达互斥关系、Engine runtime coordinator丢失机器错误码是两项独立工程缺口，另立[行动互斥评审](2026-10-09-work-action-choice-contract.md)及通用错误传递评审修复；不将新失败归为缺产物恢复分支未工作。当前正文仍1146、篇幅问题未解、无当前Work最终交付，实际局部修改验收仍未通过。
