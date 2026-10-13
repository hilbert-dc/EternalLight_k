# 3.0 来源与状态工具的独立返回额度

日期：2026-10-07。范围仅为产品逐工具输出策略及其配置/真实Core测试；五视角是职责自审，不冒称五名独立人员。

## 实施前事实与最小方案

当前Core未显式指定工具返回额度时使用模型的输出额度，512输出模型会裁剪合法的来源/状态JSON，破坏完整原文、hash及nextCursor回读。Native原始/parsed/index/workspace来源和Work状态成功包各有6500 UTF-8 bytes硬限；MCP可追加有界重复读取progressHint。read_context的Native成功包最多60000 bytes，并受9000-token模型item门禁；MCP另追加可信executionIdentity。6500不是实际token计数，以下配置使用当前Core明确的近似字节截断合同及20%序列化余量，并由真实Core验证完整MCP返回。

复用已有严格 `mcpServers[].tools[name].outputTokenLimit`：read_task_source、read_source_content、read_work_status各4096；read_context为16384；原read_work_issues的8192保持。这些控制Core的结果呈现，不改变模型max输出、Native包/资料范围、Bridge单item及总上下文、实际用量或作者预算。未确认完整包界限的read_revision_outcome、read_delegated_draft、能力发现及其他工具不在此次保护声明内；不声称所有低输出模型工具结果都已保护。

真实专项使用临时当前schema Native Runtime与生产MCP、真实Core及本地固定脚本provider：512输出模型完整读当前小说原文的raw/parsed分页，复制实际nextCursor、核对字节范围/hash/正文；重复读取包括MCP新增hint；压缩后再次完整读回。同文件原30KB问题页、压缩和超限仍裁剪的专项保留。项目创建使用当前medium及idempotencyKey，不读取.env/凭据或真实作品。

| 视角 | 结论 | 实施条件 |
| --- | --- | --- |
| 架构 | APPROVE | 产品独立声明可读结果额度，复用当前Core逐工具机制；不改变Engine或领域权威 |
| 技术 A | APPROVE WITH CONDITIONS | 保留原权限、当前来源、Native包、Bridge输入和累计费用门禁；真实MCP身份及重复提示整体可读 |
| 技术 B | APPROVE WITH CONDITIONS | 精确配置名单及TOML断言，真实512输出读Native原文连续页与hash，压缩后不丢策略，超限仍有界 |
| 产品 | APPROVE | 长来源不能因短回复额度损失合法原文；完整收到不等于理解或文学检查通过 |
| UI/UX | APPROVE | 没有作者表单、界面、授权或回复内容变化；真实错误和证据保持 |

零兼容预审：PASS。仅当前配置与测试，无旧字段fallback、迁移、双轨或Core修改。不构建Native，不覆盖共享构建产物。

## 实施后结果与五视角复核

已实施上述精确五工具策略；配置回归核对完整名单及逐工具TOML。新增测试真实启动现有Native Runtime和生产MCP，使用当前medium/idempotencyKey项目、临时数据库与只读advisory任务；实际model wire完整接收上下文及raw/parsed原文页，核对同源hash、连续范围、全部实际nextCursor和完整原稿。三次原样重复读取实际产生的MCP progressHint也完整收到；真实Core压缩后同thread再次完整回读同sourceId/hash。请求总数包含压缩，没有重置预算；当前稿revision不变。

日志 `/tmp/autonomous-v3-tool-read-budget-final.log` 首轮36 passed / 1 failed：既有MCP sidecar的initialize90秒超时，新实际Core两个专项均通过。原日志保留；没有放宽启动门槛或正文断言。相同命令和已有Native在成功启动后复验，`/tmp/autonomous-v3-tool-read-budget-final2.log` **37 passed / 0 failed / 0 cancelled，exit0，6.63秒**，包括35项当前配置/Runtime回归及2项真实Core专项。新原文分页专项4.00秒；原30KB问题页、压缩及超限仍截断专项0.95秒。两次均明确使用require_escalated，仅localhost模拟provider。三个JS语法检查与限定diff检查通过；未构建Native或读取真实作品/外部模型。

| 视角 | 后复核 | 证据与边界 |
| --- | --- | --- |
| 架构 | PASS | 当前产品policy复用Core机制及资格指纹，不改SessionPlan权限、Engine/Native执行或事实权威 |
| 技术 A | PASS | 完整真实MCP包包含hint/身份，原权限、锁定稿、稿件版本、总上下文及累计请求不变；其它未核完整包工具不在声明内 |
| 技术 B | PASS（本专项） | 当前35+2项通过；真实连续分页/hash/parsed覆盖、实际nextCursor、压缩后回读及原超限负例保留。启动失败单独保留 |
| 产品 | PASS（机制） | 短输出模型实际可读代表性当前小说原文；没有真实模型理解、文学质量或整个Engine全套通过声明 |
| UI/UX | PASS（范围） | 无新表单、授权、模型答复或UI生产改动；只改善已授权工具结果的可读性 |

独立只读代理未发现新增权限或测试证据阻断。最大包界限来自当前Native成功包及Core截断合同推导：此次实际status是较小conversation包，context请求为24000；未把6500字节generation状态或60000字节context上限说成已经逐一跑过的最大包。其它read_revision_outcome/delegated/discovery等工具未做全包上限认证；本轮不宣称所有低输出模型工具都受到保护。

新测试位于原 `tool-output-policy.integration.mjs`，已有 `test:autonomous-creation` 包含该文件，配置回归在既有codex-runtime组；无需新增npm脚本。生产及本范围测试冻结。全量统一验收与总实施文档由Root记录，本专项不能替代它们。
