# 工具纠错按真实采样批次计数

## 前审：故障、职责与方案

真实证据：原 generation-d6db3e3374164a09809d9dadfc000ab8 第42章核验 task-88953aba77fc4a5b9cf51a98a89a9017，于07:02:43Z一次输出8个 locate 调用，全部缺 sourceId。第1/2个仅获 /anyOf constraint，第3个触发 tool_argument_correction_limit；模型尚未收到反馈就被Host终止。后来另一核验重新读取正文。这是诊断实现缺陷与按调用数代替纠错机会的设计缺陷，不是小说质量问题，也不是token额度不足。

Core拥有采样请求身份：StepContext创建不可由模型设置的 samplingRequestId，通过既有MCP元数据发送。同一采样的工具共享标识，新采样具有新标识。MCP按工具和不同失败采样批次计数，原3次无进展保护保留，同批多个拒绝仅算一次；同批成功不能抹掉失败，后续批次合法调用才清除该工具连续失败。缺失批次身份明确拒绝，不猜时间、callId前缀或使用模型字段。错误仍不发送业务请求，身份、CAS、原Work账本和授权不变。Core只提供通用执行身份；MCP拥有参数检查；Host仍消费可信终态。

read_source_content schema明确以view区分workspace与其余读取，使locate缺sourceId返回准确missingField。不给缺失来源自动选当前稿；不取消原文引用校验，不强制模型多读或新增提示段落。

## 判定与验证

| 条件 | 结果 |
| --- | --- |
| 同批8个错误调用 | 全部拒绝、相同attempt=1、零业务派发 |
| 后续2个批次重复错误 | attempt=2，然后原保护终止 |
| 同批合法调用 | 不重置本批错误 |
| 后续批次合法调用 | 清除该工具连续错误，其他工具不变 |
| 缺少可信批次标识 | binding拒绝，无计数、无业务执行 |

计划：实际MCP JSON-RPC错误及无HTTP派发；纠正后合法读取；元数据同批/异批Core测试；原相关纠错负例；架构/Fork门禁；构建验证。真实原任务不为测量重做章节，不注入人为失败。安装需安全章间暂停，当前运行不得换半套Core/MCP。

五视角自审（同一执行者，非五人）：架构 APPROVE（B01/B04/B06，通用身份与领域分离）；技术A APPROVE WITH CONDITIONS（批次不可伪造、并行成功不得冲掉失败）；技术B APPROVE WITH CONDITIONS（真实RPC、Core传输、原终止负例及产物一致）；产品 APPROVE（减少失败重启和重读，质量标准不变）；UI/UX APPROVE（不增加作者步骤，错误仍如实显示）。条件以实际测试/部署关闭。零兼容：PASS，不增迁移、旧字段兜底或双轨。

## 后审

实际验证：MCP 123/123通过，包含真实JSON-RPC同批8次缺sourceId均attempt=1且无HTTP派发、后续不同批次触发原保护、后续合法校验按工具清零及模型字段不能伪造缺失可信身份。Core相关整组111通过/1自动审批断言失败，该唯一用例在减轻构建并发后单独复验通过；新采样身份同批共享/异批变化及既有元数据测试通过，原失败日志保留。Host实际MCP子进程到HTTP模拟端用例1/1通过；架构扫描369文件/7crates无诊断，Fork严格角色及补丁漂移门禁通过。

Core/MCP均构建完成，Core buildIdentity=ac6db752404b39a001b9647af53477ae695f909e3735821b4fd92dc71930146a。遇到本机程序加载延迟：Core首个120秒版本探针失败，后续有界探针43秒成功，正式构建身份随后成功；MCP测试曾停在dyld单线程，之后原产物与字节相同临时副本均123通过。不修改系统安全设置，不将加载等待归为业务失败。

五视角后审：架构APPROVE（Core通用可信身份，MCP计数，Native事实/权限不变）；技术A APPROVE（并发批次和后续成功/终止/缺身份拒绝均验证）；技术B APPROVE WITH CONDITIONS（代码、契约、构建及实际进程传输通过，应用安装复验待关闭）；产品APPROVE（不缩短正文、不重置账本、不新增付费测量）；UI/UX APPROVE WITH CONDITIONS（原任务在第46章步骤结束后安全暂停，安装后应恢复且保留46个现稿）。零兼容PASS。证据目录 `.release/tool-correction-batches/`，原失败与测试层级分别保留。没有可比完整章节样本，不宣称节省比例。

部署：07:51:37Z原run paused/version685，completedChapters45，activeTasks为空；已退出应用，更新前46个现稿修订/摘要已存 before-update-drafts.json。打包/安装/恢复仍待收尾。

部署收尾：正式打包六项门禁全部通过（staging/runtime-verification/frontend-build/packaging/packaged-resources/application-smoke），新应用已启动。更新前后46个现稿的revisionId及content_json摘要完全一致。通过应用验证并应用既有模型连接后，仅点击一次原任务重试；08:05:06Z原run已running/version687、completedChapters45，第46章已保存待核验。08:05:29—36Z原协调线程read_context、read_work_status及3个read_source_content真实调用均成功返回领域数据，无binding拒绝或参数错误，证明必需采样身份已贯通Core→MCP。没有人为注入错误或为测量重写章节。应用启动后初次任务细节被viewingExecution隐藏，重新选中原对话后恢复入口；未修改账号或连接凭据。

技术B与UI/UX部署条件关闭，后审均APPROVE。真实失败分支的计数行为以公开RPC测试为依据；线上成功调用仅证明传输与读取正常，不声称已观察到线上错误纠正或整章token节省比例。当前任务继续运行，不重建已删除的定时任务。

跟进（08:18Z）：原run running/version705，46、47章已保存并核验（2675/2528非空白字符；各8条标准supported，无遗漏，25/20处引用均在各自现稿中），第48章步骤已登记。08:05—08:18Z固定观察段累计输入2,032,080，其中缓存1,667,200、非缓存364,880；输出98,582（含推理67,638），总计2,130,662。这是跨请求累计，并非单次上下文，也不是完整第48章样本；仅已报告usage，不含尚未报告请求或预留。协调器不同步骤各读context/status不能按相同参数直接判重复；第47章worker的9次来源读取及第48章当时19次读取没有完全相同的重复参数调用，后者读取多章承接资料，不据调用次数认定无用。08:14:45Z complete_task额外字段触发精确additionalProperties反馈，08:14:50Z已纠正成功，无失败重启或正文重做。没有证据将本观察段全部用量判为有效，也没有可比样本可计算节省比例。证据followup-audit.json。
