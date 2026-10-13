# EternalLight 创作机制：源码证据与借鉴边界

日期：2026-09-13。用途：支持[机制重构方案](PRODUCT_CREATIVE_MECHANISMS.md)和[优化主文档](PRODUCT_CREATIVE_QUALITY_REFACTOR.md)的产品取舍，不是源工程或 Codex 的质量验收报告。

## 1. 核查范围与证据分级

源工程为 /Users/hilbert/private/EternalLight，核查时 HEAD 为 698b297caf2b6d41de9b6688cfc1277dfc95a56a，并有大量已修改及未跟踪文件，因此以下“当前”指当时工作树，不把 HEAD 当成全部源码快照。目标是 /Users/hilbert/private/codex/eternallight/novel-codex。本轮只读取源码、配置技法、设计材料与 Git 历史，没有读取密钥或私人作品，也未调用真实模型。

- A：当前 UI／API／执行器调用关系可静态追踪；不表示本轮运行或效果验证通过。
- L：已注册或预载，但有用途／相位限制；不能当作默认创作流程必经步骤。
- H：历史提交存在真实模型调用调度；当前调度已删除或未接回。
- D：当前有领域契约、提示、构建器或算法，未找到完整当前主链消费者。
- R：含关键词、字段、标签或数值启发式；只说明算法存在，不证明文学判断正确。R 可与其他等级并列。

源码调用追踪和历史存在证明只能支持“机制可以怎样实现”。质量价值仍是需要通过作者、读者和同预算对照检验的产品假设。

## 2. 机制证据表

以下相对链接跨到同级 EternalLight 仓库，行号定位列在文字中；文件移动后可按函数名再次检索。

| ID／机制 | 已查证来源 | 状态与不能推出的结论 |
| --- | --- | --- |
| E01 工作稿续创 | [RuntimeWorkspace](../../../../../EternalLight/webui/src/workspace/RuntimeWorkspace.tsx) 第 2384 行；[路由](../../../../../EternalLight/src/multi_agent/web/routes/agent_runtime.py) 第 2578、2725 行 | A：版本化工作稿来源进入任务。地图入口仍会填入输入框，不证明“保留并继续”已经全自动衔接；作品内部正式版不等于互联网发表 |
| E02 主动取证 | [knowledge.py](../../../../../EternalLight/src/multi_agent/infrastructure/agent_runtime/knowledge.py) runtime_context_bundle，第 204 行；[loop.py](../../../../../EternalLight/src/multi_agent/application/agent_runtime/loop.py) 第 786、1692、1767 行 | A：研究阶段要求非空读取并保留回执；有结果不等于找对证据或覆盖充分。邻接文本的取法可借鉴，不可绕过 Codex 位置权限 |
| E03 核验来源与覆盖 | [builtin_tools.py](../../../../../EternalLight/src/multi_agent/application/agent_runtime/builtin_tools.py) 第 443、469 行；[loop.py](../../../../../EternalLight/src/multi_agent/application/agent_runtime/loop.py) 第 2522、2943 行；[preflight.py](../../../../../EternalLight/src/multi_agent/domain/agent_runtime/preflight.py) 第 1059 行 | A：只读独立核验、来源与候选摘要绑定、逐项 pass/fail/unknown。当前主要为每案一次核验；未找到研究后回执自动扩充启动时核验来源的链路；引文强校验也不是覆盖全部判断 |
| E04 六类语义 | [creative_memory.py](../../../../../EternalLight/src/multi_agent/infrastructure/agent_runtime/creative_memory.py) 第 20、42、72 行 | A：作者约束、正文事件、人物认知、未来计划、假设、未知进入有界记忆。过滤以 owner/book/branch/status 为主，没有与 Codex 相同的 POV／阅读位置投影；8,000 字符不是 token 预算 |
| E05 分层创作职责 | [plan-craft](../../../../../EternalLight/config/agent_skills/plan-craft/SKILL.md) 第 8 行；[volume_plan_quality.py](../../../../../EternalLight/src/multi_agent/volume_plan_quality.py) 第 72、129、220 行 | 当前技法资源；评审协议 H/D：卷纲需落实人物行动、资源／知情渠道、篇章后果与邻卷接口。可借鉴诊断职责，不复制固定字段完备性要求 |
| E06 专业评审与复审 | [outline_spine.py](../../../../../EternalLight/src/multi_agent/outline_spine.py) 第 3737、3971、4040 行；[macro_architecture.py](../../../../../EternalLight/src/multi_agent/macro_architecture.py) 第 3434、3581、3749 行 | H/D：逻辑官／读者分工、仅评已物化材料、意见指向可修范围、修改卷及邻卷复审；历史有调用，当前 v45 不默认执行完整委员会 |
| E07 症状、诊断与分歧 | [whole_book_quality_loop.py](../../../../../EternalLight/src/multi_agent/whole_book_quality_loop.py) 第 106、304、534 行；[macro_architecture.py](../../../../../EternalLight/src/multi_agent/macro_architecture.py) 第 3749 行 | H/D：隐去设计理由／选择分数，读者先描述症状，诊断提出根因和保留项。所谓 author diagnosis 仍是 AI；投影含终局，是全书梗概评估，不是无剧透读者试验；按维度＋字段一致的仲裁不能证明语义共识 |
| E08 事件机制探索 | [volume_event_forge.py](../../../../../EternalLight/src/multi_agent/volume_event_forge.py) 第 188 行；[volume_event_selection.py](../../../../../EternalLight/src/multi_agent/volume_event_selection.py) 第 48、101 行 | H/D/R：选择压力、反制、失败路径、状态残留及最小事件网络有价值。候选数“开放”不适合交互硬预算；匿名卡带预评分，总分差阈值也会影响选优，不能称为无锚定盲评 |
| E09 人物自主行动 | [volume_agency.py](../../../../../EternalLight/src/multi_agent/volume_agency.py) 第 17、158 行；[character_contradiction.py](../../../../../EternalLight/src/multi_agent/character_contradiction.py) 第 24、72 行；[character_arc_engine.py](../../../../../EternalLight/src/multi_agent/character_arc_engine.py) | D/R，部分构建器有旧链消费：公开目标、私欲、误信、筹码、备用行动提供创作视角。强制补两名行动者、默认填造动机、关键词得分不能作为普遍要求 |
| E10 创意兑现 | [creative_fidelity.py](../../../../../EternalLight/src/multi_agent/creative_fidelity.py) 第 21、110、149 行 | D：绑定选中候选的机制、选择、代价和后果，查正文证据。未确认当前主链消费；默认 hard 与不允许作者拒绝锚点不可照搬。引文存在不等于机制真正兑现 |
| E11 修复保护亮点 | [revision_issue_ledger.py](../../../../../EternalLight/src/multi_agent/revision_issue_ledger.py) 第 51、106、142 行；[outline_creative_quality.py](../../../../../EternalLight/src/multi_agent/outline_creative_quality.py) 第 50 行 | H/D/R：记录问题来源和可改范围、保护声音／留白／未点名场面、只重建受影响部分。旧证据消失最多说明可能修复，不能自动关闭问题 |
| E12 故事单元文本关联 | [CreativeMapPanel](../../../../../EternalLight/webui/src/workspace/CreativeMapPanel.tsx) 第 195 行；[creative_workspace.py](../../../../../EternalLight/src/multi_agent/infrastructure/agent_runtime/creative_workspace.py) 第 631、704 行 | A：独立单元和多个版本化锚点可存储，作者维护。UI 绑定新选区可能替换原 links；不等于已实现可靠自动关联、自动兑现或自动重定位 |
| E13 读者承诺与兑现 | [reader_expectation_ledger.py](../../../../../EternalLight/src/multi_agent/reader_expectation_ledger.py) 第 25、442、477 行；[promise_payoff.py](../../../../../EternalLight/src/multi_agent/promise_payoff.py) 第 29、111 行；[narrative_obligations.py](../../../../../EternalLight/src/multi_agent/narrative_obligations.py) | H/D/R：具体问题、生命周期、延期代价与证据要求值得借鉴。到期必须兑现、关键词回收评分、自动生成默认承诺不能替代作者决定与真实阅读效果 |
| E14 滚动分卷 | [serial-architecture](../../../../../EternalLight/config/agent_skills/serial-architecture/SKILL.md) 第 8、34、42 行；[narrative_design.py](../../../../../EternalLight/src/multi_agent/application/agent_runtime/narrative_design.py) 第 121、357 行；[recipes.py](../../../../../EternalLight/src/multi_agent/domain/agent_runtime/recipes.py) 第 210 行 | 技法资源；工具 L/R：非均匀容量、入口出口和保留非常规方案可参考。工具虽预载，但 [loop.py](../../../../../EternalLight/src/multi_agent/application/agent_runtime/loop.py) 第 625 行相位过滤使当前 v2 四相位不开放；相似度和 Pareto 数学不证明文学质量 |
| E15 技法与声音 | [chapter-craft](../../../../../EternalLight/config/agent_skills/chapter-craft/SKILL.md) 第 12、30 行；[creative_context.py](../../../../../EternalLight/src/multi_agent/application/agent_runtime/creative_context.py) 第 128 行；[voice_adaptation.py](../../../../../EternalLight/src/multi_agent/voice_adaptation.py) 第 122、179 行 | 技法加载 A，声音规则 D/R：章节目的与少量相关技法可用；题材关键词路由不是作者意图理解。声音归属与有意变化值得参考，句长等统计不是好文风标准 |
| E16 预算与部分交付 | [preflight.py](../../../../../EternalLight/src/multi_agent/domain/agent_runtime/preflight.py) 第 993、1200 行；[run_store.py](../../../../../EternalLight/src/multi_agent/infrastructure/agent_runtime/run_store.py) 第 3220 行；[loop.py](../../../../../EternalLight/src/multi_agent/application/agent_runtime/loop.py) 第 2726 行 | A：预留核验、按持久化成果交付、失败与未知保留。不能找回未保存模型流；EL 自有请求计量不能直接当成 Codex AppServer turn 的计量 |

## 3. 历史真实调用的复核

历史提交 9c8033616c05b002430e7df656b8590742896818 的以下文件含实际 invoke_json_gate／Worker 调用，主代理与角色已读取；这些路径在当前工作树已删除，因此不伪装成现行文件链接。

| 历史路径（均位于 src/multi_agent/infrastructure/workflows/） | 定位与行为 |
| --- | --- |
| outline_semantic_review.py | 第 37～149 行：logic_officer、reader 分别调用模型并保存意见；旧合并为双席 AND，协议失败与内容失败有区别 |
| planning_pipeline.py | 第 913～1040 行：卷纲模型评审 → 定向补修 → 结构检查 → 再模型复审 |
| planning_pipeline.py | 第 1042～1106 行：专业 Worker 评审、保存原始有界意见、由定稿角色逐项裁定；不再额外调用书记模型重复总结 |
| planning_pipeline.py | 第 1112～1138 行：规划反推只留演进请求，明确不直接 update_outline |
| outline_spine_repair.py | 第 220～295 行：修改卷与相邻卷的 before/after 双席复验 |

可复核命令示例：

```sh
git -C /Users/hilbert/private/EternalLight show 9c8033616c05b002430e7df656b8590742896818:src/multi_agent/infrastructure/workflows/planning_pipeline.py
```

事件机制选择及全书诊断的历史调度还由作者 AI 角色追查到同提交的 planning_event_forge.py 与 whole_book_quality.py；未将其历史设计整体认定为现行可用。当前新增能力应以窄任务接入 Codex，避免恢复已删除的工作流和另一套写入权威。

## 4. Codex 的真实基础与新增缺口

| 当前源码 | 已有基础 | 本方案新增内容 |
| --- | --- | --- |
| service.py（历史源码路径 `runtime/creative_workspace/service.py`，现文件已删除） 第 2798 行 | 当前目标章可使用冻结候选头 | 作者选定跨章／跨规划工作版本，不能误称当前完全无法续写候选 |
| service.py（历史源码路径 `runtime/creative_workspace/service.py`，现文件已删除） 第 2713、2743、2769 行 | 规划与前章主要从正式采用来源取材；Plan 取最近三章尾部，Writer 取最近六章有界片段 | 按问题取回远距证据、显式工作版本与可解释材料覆盖 |
| context_limits.py（历史源码路径 `runtime/creative_workspace/context_limits.py`，现文件已删除） 第 18 行 | 有界 UTF-8 组装、遗漏计数、硬材料无法容纳时报错 | 区分本次 required 证据和 supporting 材料，不能把待检查证据裁掉仍称通过 |
| schema.py（历史源码路径 `runtime/creative_workspace/schema.py`，现文件已删除） 第 62 行；service.py（历史源码路径 `runtime/creative_workspace/service.py`，现文件已删除） 第 2587 行 | world/reader/character/author-secret 与 unknown/suspected/known/false，含 POV 和阅读位置过滤 | 在正交维度补来源权威／计划／假设，不替换现有知情模型 |
| generation.py（历史源码路径 `runtime/creative_workspace/generation.py`，现文件已删除） 第 120、375 行；service.py（历史源码路径 `runtime/creative_workspace/service.py`，现文件已删除） 第 2433 行 | 任务引用、版本校验与每次重编译上下文 | 持久保存实际投影文本的不可变快照与用途隔离 |
| service.py（历史源码路径 `runtime/creative_workspace/service.py`，现文件已删除） 第 1353、1602 行 | Review 有确定性检查；semantic-review 是未执行的非阻断项 | 真正的只读叙事评审、逐项覆盖、发现与分歧；不能将现有 ready 当作文学通过 |
| [types.ts](../../src/types.ts) 第 40 行；planning_bindings.py（历史源码路径 `runtime/creative_workspace/planning_bindings.py`，现文件已删除） | unit 是 Outline 节点类别，章节有主要规划绑定 | 单一叙事单元身份、多对多文本链接与历史定位 |
| [codex-adapter.mjs](../../electron/codex-adapter.mjs) 第 598 行 | 一次 sendTurn 发起 AppServer turn/start 并等待完成 | 区分应用尝试、turn、内部模型请求与实际 token；当前未见完整底层用量汇总，未知不能写零或一 |
| generation.py（历史源码路径 `runtime/creative_workspace/generation.py`，现文件已删除） 第 415、486、505 行 | 已有输出引用、取消与恢复、候选交付校验 | 多评审／补证／修订步骤成果投影；各部分完整度分开 |

## 5. 明确不照搬的做法

1. 任何“多评审一致即文学真理”、固定总分选优、无目的循环到 PASS；保留模型判断的不确定性和作者取舍。
2. [outline_spine.py](../../../../../EternalLight/src/multi_agent/outline_spine.py) 第 3916～3924 行将无效引文换成 source 开头；新实现须保留证据无效状态。
3. [adversarial_creative_review.py](../../../../../EternalLight/src/multi_agent/adversarial_creative_review.py)、[dialogue_subtext.py](../../../../../EternalLight/src/multi_agent/dialogue_subtext.py)、[character_contradiction.py](../../../../../EternalLight/src/multi_agent/character_contradiction.py) 的关键词和字段检查作为文学裁判；可转为需核实的诊断问题。
4. [narrative_structure_arena.py](../../../../../EternalLight/src/multi_agent/narrative_structure_arena.py) 的结构标签相同或节拍集合相似就判同构；[volume_event_selection.py](../../../../../EternalLight/src/multi_agent/volume_event_selection.py) 的预评分锚定与总分差否决。
5. [voice_fingerprint_guard.py](../../../../../EternalLight/src/multi_agent/voice_fingerprint_guard.py) 的固定质量分、句长或疑问句比例被当成声音质量真值。
6. 自动把角色假设、原文转述、AI author diagnosis 或“采用工作稿”变为正式作者事实；未经作者选择就学习永久偏好。
7. 强制全书先定卷数、全体人物都有成长、每卷都升级与不可逆损失、每项伏笔按期闭合；保留适用范围与有意例外。
8. 没有覆盖就报全书通过、协议坏了就判作品坏、源引文消失就标问题解决、预算耗尽后不交付已存成果。

## 6. 研究边界

本轮没有证明任何机制提高了真实文学质量，没有把源工程历史评审报告、模拟作者会话或测试数量作为真人证据。提出的采用顺序兼顾上限机制与可靠性基础，不因当前旧调度不存在否定有价值的设计，也不因历史存在就假定适合直接移植。验收与研究方法见主文档第 8 节及机制方案第 9～10 节。
