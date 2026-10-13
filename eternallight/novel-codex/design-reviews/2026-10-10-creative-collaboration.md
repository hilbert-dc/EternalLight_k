# 创作质量优先的协作设计与实施

## 目标与责任（实施前评审）

根因是设计缺陷与证据不足并存：模型重复搬运身份授权；角色数量不能证明专业收益；方法缺少示例与对照。目标是主写作者按具体问题选择零个或多个专家，以原文证据和作者目标决定采用；不用固定团队或文学总分门禁。

本次实现：
1. 将父任务通过 Core 正式创建 `candidate_writer` 的动作作为明确的隔离写作委派。Core 已从 SessionSource 生成不可由工具参数覆盖的 agentRole、caller、parent、root 元数据；MCP 传输，Native 验证直属关系、写作角色、活跃任务、来源、范围和 CAS。移除第二次 UUID 授权工具、HTTP 入口和 grant 表，保留不可变候选与父任务引用整合。正文或提示词自称 Writer 不授予权限，研究角色及孙任务拒绝写入。采用/发布权限不变。
2. 共享可选协作方法明确问题、证据、反证、修订代价和保留原文条件；加入短小合成正反例，按需读取，不全部塞入系统提示。角色只说明分工，共用方法提供诊断与综合步骤。
3. 在既有质量评测上建立三组配对：单 Agent、定向自检、隔离专家；同材料/模型/额度、全部运行与失败保留，匿名比较与人工评分沿用现有评测。缺少模型/人工结果标为待验证，不用工程测试声称文学提升。

事实权威：Native 管候选、范围、任务和权限；Core/Engine 管受信执行身份与真实用量；Host 组装生命周期与创作指令；Shared 仅方法数据；离线评测只评估真实生产记录，不另建模型循环。适用 B01–B10，无 Core/Engine 代码变更，无 UI 新步骤，无历史兼容路径。作者原作不用于测试。

## 状态与验收

| 状态 | 处理 |
| --- | --- |
| 正式 writer 直属子线程、活跃 generateChapter | 原子保存隔离候选；不写正式稿，不增加额度 |
| 研究角色、缺少受信角色、伪造正文身份、孙线程、根线程 | 拒绝；不保存 |
| 暂停/取消/过期来源/锁定区变化 | 沿用 Native 拒绝 |
| 同键同内容重试、同键不同内容、旧候选 CAS | 分别回放、冲突、拒绝；父任务整合仍使用准确候选引用 |

验证计划：Core 委派权限/保存测试；真实 MCP→HTTP→Native 合同；真实 Core 受控 provider 协作；角色生成及 Host 合同；方法发现/按需读取；三组评测缺失/失败/混淆条件；architecture:check。真实模型外发与文学质量结果不冒用此前授权批次或人工结论。

## 五视角前审（同一执行者自审）

| 视角 | 结论与依据 |
| --- | --- |
| 架构 | APPROVE WITH CONDITIONS：受信角色必须来自 Core SessionSource，Native 判权限，不依据自然语言或角色声明 |
| 技术 A | APPROVE WITH CONDITIONS：补研究角色/伪造/间接子线程及暂停、CAS、重复保存拒绝验证 |
| 技术 B | APPROVE WITH CONDITIONS：一次性删除旧入口、同步生产与测试/生成配置，验证真实传输 |
| 产品 | APPROVE：减协调，不强迫协作；收益保持 unvalidated |
| UI/UX | APPROVE：作者不处理 UUID；无新确认步骤，失败继续呈现真实状态 |

## 实施后复核

已完成本轮工程实施；文学收益保持 unvalidated。真实作者资料未参与测试，也没有调用外部模型或填写人工评分。

| 证据 | 实际结果与范围 |
| --- | --- |
| E1 Native 权限与一致性 | macOS Core 委派测试 7/7：研究角色/空角色/错误身份/孙任务/目标变更拒绝；锁定及隐藏范围、CAS、幂等、暂停取消、重开后候选均覆盖 |
| E2 生产边界 | macOS Host 与集成 38/38。真实 Core+本地受控模型证明实际 agentRole/直属关系及无握手流程；真实 MCP→HTTP→Native 证明权限、候选保存与父任务准确引用整合。这是互补分层验证，不称无脚本真实模型自主创作 |
| E3 MCP 全量 | 离线 Linux 119/119，无失败、忽略、过滤；已有 Rust 1.98.1 镜像、源码及 registry 只读、网络关闭、无凭据挂载 |
| E4 方法和质量评测 | 最终生成角色/方法合同 3/3，质量评测与上下文 10/10；三组缺失、伪造协作记录形状、预算/构建/疑难不一致拒绝，模型评分不能代替真人 |
| E5 静态 | architecture:check 扫描 365 文件/7 crates，无诊断；Host 类型检查通过；生成配置一致性和本轮差异检查通过 |

本轮原始失败保留：首次 sandbox 不允许 localhost；更新过程中旧“7 方法”断言和已删 grant 变量失败，随后同步新合同再通过。新增方法读取验收发现测试助手给所有工具注入 expectedRevision，按实际 schema 修正后通过；生产校验未放宽。macOS MCP 完整测试两次停在编译阶段，进程长时间无 CPU 进展后停止，未取得足以归因的栈；不把 Linux 通过当作 macOS 编译/启动问题已解决。生产 macOS 二进制构建已完成，最终重建 MCP 的 Native 集成 1/1 通过，并验证生产工具实际返回新方法及三个案例。

[源码指纹与分层验收清单](evidence/2026-10-10-collaboration/manifest.json)；同目录保留成功及失败日志。[质量实验清单](evidence/2026-10-10-collaboration-quality/manifest.json)明确 24 次运行均 not_started。实际真人结果缺失，不产生质量增益声明。

### 五视角后审（同一执行者自审）

- 架构：APPROVE，Core 受信身份由真实执行验证，MCP 仅传输，Native 判定权限；未在 Engine/Host 新建领域状态。
- 技术 A：APPROVE，E1/E2/E3 保留研究只读、直属关系、原范围、来源、CAS、暂停及父任务整合。旧授权表/函数/工具/HTTP 入口均删除，无双轨。
- 技术 B：APPROVE，生成角色、生产目录、指南、执行审计目录、公共子任务指令及测试同步；E2/E3 覆盖真实边界。
- 产品：APPROVE WITH CONDITIONS，工程方案可用；是否提高创作质量及真实总成本，需要完成三组真实运行与真人评审，当前不宣称已验证。
- UI/UX：APPROVE，去掉作者无须理解的内部身份协调，无新增确认或强制团队；界面保存/采用/完成语义不变。

零兼容门禁 PASS；B01–B10 本轮职责复核 PASS。残留限制为 macOS 全量测试编译环境、真实模型对照与真人质量结果；未部署或发布应用。

### 三组实验怎么使用

运行 `npm run evaluate:collaboration-quality -- scripts/fixtures/creative-quality-suite.json runs.json output-directory`。每个样本三组共用预先登记的创作疑难（self_review 与 specialist 的 question 必须一致）、来源、构建、模型配置与完整 Work 额度。源运行逐一保留状态、产物回执、真实用量及条件执行证据，不把重试替换第一次失败。

`conditionEvidence` 必须引用实际执行记录：rootThreadId、evidenceSha256，self_review 的 selfReviewRef，specialist 的直属子线程、reportRef 与 exposure。预算使用 maxRequests/maxTokens/durationSeconds；未知用量保留 null。JSON 一致性检查不能认证日志或人工身份，需核对对应证据。每组允许不采用专家建议、保留原文长处，但 specialist 条件须实际取得专家报告才能用于该条件比较。

只把每个 comparison 目录的 packet.json 提供评审，key.json 与条件、成本隔离保存。使用既有 `evaluate:creative-quality assess` 分别评估完成的真人评分；三个比较复用同一批产物，不重复累计成本，不当作独立样本扩充分母。八对是有限产品决策起点，不证明普遍文学优势。
