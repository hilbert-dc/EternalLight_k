# 3.0 必需实施范围只读复核

日期：2026-10-08。范围为《对话驱动自主创作：开放工作区与可靠执行架构 3.0》的 13.2、13.3、14.1，参照 14.2 的证据分层。以下五项是评审视角，不表示五位独立人员。

结论：在本次检查的当前代码路径和模块评审范围内，未发现新的、可确证的必需生产代码缺口。这是有边界的源码复核，不是整份方案、当前全部二进制或真实长篇创作的最终通过声明。统一冻结后的全量构建与测试结果由主任务记录；本次只新增本文件，不修改生产、测试或主实施评审，也不启动新的构建、测试或真实模型请求。

## 范围与判定依据

- [方案](../SIMPLE_REQUEST_AUTONOMOUS_CREATION_DESIGN.md)第 343 行要求减少固定路由、永久方法依赖、重复状态与重复输入；第 352 行把 A—D 定义为工程交付包，而非每次创作流水线。
- 同文件第 360—362 行明确 E 为有证据后增强，按实际缺口建设；不能把能力商城、计算沙箱、多模型文学路由或方法收益比较设为单章闭环的前置门槛。
- 同文件第 368—380 行规定自主性、灵活性、约束、质量边界、能力、协作、长篇、可靠性及体验场景；第 384、392 行要求契约测试、真实机制与真实模型分别报告，第 7 章、前 100 章和 300 万字整部作品分别验收。

## 13.2：当前代码对应关系

| 要求 | 当前代码与位置 | 复核结论及边界 |
| --- | --- | --- |
| 作者输入由模型理解，移除词语到流程的硬映射 | [advisory-prompt.mjs](../electron/advisory-prompt.mjs)第 32—34 行；[creative-task-contract.mjs](../electron/creative-task-contract.mjs)第 3、19 行 | Advisory 传入完整 authorIntent，要求结合原文与对话理解目标、读取实际旧任务、保留原范围与预算；未在已检查入口发现把“继续”等词固定映射为业务行动的分支。显式 UI 操作的 typed operation 校验是接口合同，不是自然语言分类。提示要求本身不证明模型每次理解正确。 |
| 不按固定阶段无条件追加专家 | [conversation-analysis.mjs](../electron/conversation-analysis.mjs)第 54、126—162 行；[novel-collaboration.mjs](../electron/novel-collaboration.mjs)第 20—24 行 | 规划核对执行模型实际选择的必要 1—3 个视角；简单编辑直接处理，通用角色承接临时问题。多项已授权规划先取得真实批准，首次执行后交同 Work 的模型选择后继，不由 Host 预定整条专家流程。作者明确指定视角和 Native 必需独立核验仍保留。 |
| 作者交付约束与模型方法计划分离 | [model_work_plan.rs](../native/story-core/src/model_work_plan.rs)第 204—217、414、540 行；[model_work_plan_tool.rs](../native/story-mcp/src/model_work_plan_tool.rs)第 77—85 行 | 0—12 项近期窗口可替换、合并、撤销和重排，历史快照不可变。pending 方法、描述性 mode 不授予或撤销执行权；真实已满足宣称仍须当前来源和保存证据。删除方法不取消作者范围、预算、已登记行动及独立核验义务。不能把方法未做完推断为作者交付必定未完成。 |
| 同一问题和报告引用避免成为多份完成权威 | [AUTONOMOUS_V3_PLAN_REVIEW.md](../AUTONOMOUS_V3_PLAN_REVIEW.md)第 235—252 行 | 模块复核记录当前问题索引引用真实报告、模型明确选择 issue 同一性及 resolutionOf；负评、争议、失效与未复核状态保留。索引不生成第二套文学判断，也不按措辞自动关闭问题。独立评估身份与证据没有被合并。此处引用已有模块机制证据，未重新执行该测试组。 |
| 不强制每个行动登记全部方法卡或复制工程身份 | [creative_methods.rs](../native/story-core/src/creative_methods.rs)第 51、89、137—138、173 行；[novel-workflow/SKILL.md](../codex-runtime/skills/novel-workflow/SKILL.md)第 8—15 行 | selectedMethods/omittedMethods 是 Native 冻结的执行合同及作者显式实验元数据，不是每次行动必填的模型方法清单。小任务无需计划或方法选择；作用域和默认参数由 Runtime 推导。 mandatory execution contract 仍保护真实来源、授权和回执，不构成封闭创作方法目录。 |
| 前文按需读取；对话不按内部步骤重复作者要求 | [revision_outcome_context.rs](../native/story-core/src/revision_outcome_context.rs)第 8、71 行；[task_workspace.rs](../native/story-core/src/task_workspace.rs)第 161、208、308 行；[thread-history-presentation.mjs](../electron/thread-history-presentation.mjs)第 2—11 行 | 评估通过 workspaceRef 按问题定位 priorText，而非每次搬运全部历史正文；正文版本仍来自原作品存储。对话按真实 Native 作者 turnId 投影，移除 Engine 内部 userMessage，不伪造重发原要求。作者确实发送的两条消息仍以不同真实 ID 保留。源码投影不替代滚动、可访问性与实际阅读体验验收。 |
| 不新增第二个作品库、方法调度器、专家状态库或质量总分平台 | [task_sources.rs](../native/story-core/src/task_sources.rs)第 46、158 行；[quality_notebook.rs](../native/story-core/src/quality_notebook.rs)第 393—434 行；上述 Work/Host 模块评审 | 已检查路径复用原存储与当前 Work：来源缓存为授权投影，方法草案为 proposed 笔记，报告索引引用真实事实；未确证额外作品权威或文学总分调度层。这个结论仅覆盖已检查路径，不声称穷尽整个仓库的一切模块。 |

## 13.3：开放能力与可靠执行的关键落点

**能力不是封闭清单。** [capability_discovery.rs](../native/story-mcp/src/capability_discovery.rs)第 15、94、146—177 行把目录定位为资源发现，catalogIsExhaustive=false；实际可调用工具通过 toolDiscovery 返回当前真实工具名，角色原文独立有界读取。目录外临时思考方法合法，目录结果及角色名不产生权限。实际 schema 加载与当前 admitted 工具权限的 Engine 边界见 [Portable discovery 评审](../AUTONOMOUS_V3_PORTABLE_DISCOVERY_REVIEW.md)，不宣称修改 Codex 内核或取得不存在的工具。

**临时方法有真实存储与边界。** [creative-strategy-policy.mjs](../electron/creative-strategy-policy.mjs)第 1—2 行和 [creative_strategy.rs](../native/story-mcp/src/creative_strategy.rs)第 37 行允许零方法、按需组合及临时方法；[quality_notebook.rs](../native/story-core/src/quality_notebook.rs)第 393—434 行真实保存 task-scoped proposed 方法，允许同 Native Work 按需复用并记录效果来源。模型不能自动升为作者长期偏好、安装工具或获得额外预算。方法可用不等于已证明文学收益。

**原文阅读是实际已返回内容的证明。** [task_sources.rs](../native/story-core/src/task_sources.rs)第 276—301、317—368 行只给实际主线程返回内容登记主任务阅读证明；目录、子任务阅读、旧来源和摘要不能替代它。第 640—661 行的当前作者原话 guard 分别检查同 Task、当前 hash 的每个合法 source，接受任一单 source 完整主任务读取，不拼接不同 source 的部分分页。同作者要求被两个合法 policy 缓存时误选未读来源的真实缺陷已修，详见 [专项前后评审](2026-10-08-current-author-source-read-proof.md)。旧失败 7/9、修后 9/9及相关 Runtime 15/15 均保留；这些既有定向结果不代替最新共享源码的统一构建。

**持久唤醒已接入真实消费者。** [work_execution_events.rs](../native/story-core/src/work_execution_events.rs)第 24、130、480—490、557—595 行提供终态事件唯一性、事务内登记、IMMEDIATE claim 和精确 ack；暂停、取消、作者来源变更及无真实后继 Attempt/最终交付证据继续拒绝。仅 queued Step 不构成消费证明；实际附着的后继 Attempt 是接力证明，不声称正文已完成。

[main.mjs](../electron/main.mjs)第 183—199 行实例化 Native execution owner 和 event consumer，并在活动 Work/Engine/generation、资格、核账和未保存编辑等占用时停止 idle 竞争；[work-execution-events.mjs](../electron/work-execution-events.mjs)第 129—171、202—251 行串行消费真实原 Work/root thread，停止时不伪造完成 ack，不创建新作者输入或预算。当前 Native 和 Host 的模块终态证据分别见 [Native 唤醒评审](../AUTONOMOUS_V3_DURABLE_WAKE_REVIEW.md)、[Host 唤醒评审](../AUTONOMOUS_V3_DURABLE_WAKE_HOST_REVIEW.md)。后者已记录真实 Host→Native 重启 2/2 及原批准 action 来源修复；模块证据仍不替代主任务最终全量验收。

## 14.1：证据边界与剩余验收

| 维度 | 本次可核对的证据 | 不能由本次推出的结论 |
| --- | --- | --- |
| 自主性、灵活性、约束 | 模型接收完整输入与状态；可调整方法；原 Work、范围、来源、预算、最低核验独立保留。已有真实 Engine/Runtime 配脚本 provider 自修复、规划接力与最终门禁证据由专项评审记录。 | 所有短输入和作品状态下，真实模型都能恰当选择并稳定交付。 |
| 质量边界 | 真实负评与问题来源保存；当前独立报告及原要求是完成依据，未评、过期、争议、缺证不能冒充通过。 | 检查清单已穷尽作者全部审美、人物行为或远章关联；通过报告保证文学质量。 |
| 能力、协作 | 当前真实工具 schema、按需角色、task-local 方法、child 真实身份授权、隔离候选、父任务核对、取消边界。参照 [Host 评审](../AUTONOMOUS_V3_HOST_REVIEW.md) 与 Portable discovery 评审。 | 更多方法或更多子任务一定降低成本或提高小说质量；无需实际样本即可选出最佳团队。 |
| 长篇、可靠性 | 有界 workspace、真实版本来源、依赖失效、主任务完整阅读、持久 owner/事件/实际后继证明；参照 [Workspace 评审](../AUTONOMOUS_V3_WORKSPACE_REVIEW.md)。 | 合成千章资料或一次重启成功等同于前 100 章、300 万字全程连续性通过。 |
| 体验 | 真实作者 turn 唯一投影，内部输入不重复显示；主任务负责完整 UI 与交付路径。 | 仅凭数据投影即可证明全部展开文案、正文/差异导航、滚动及可访问控制已经实机通过。 |

本次未操作真实应用、读取私有小说数据库或调用 DeepSeek，没有代替应用安排剧情、要求重新生成或追加纠错。真实模型理解与创作效果未在本次复核中认证，不构成已确证的生产代码缺口。它们须按照已有明确授权、预先确定的样本与预算分别验收，保留失败样本和外部介入记录。

主实施矩阵中历史“尚未接入”或旧测试计数需要主任务按最终冻结证据更新；本次没有改写历史记录或主实施评审。当前全量 Cargo/Node、最终二进制及全部 14.1 场景状态由主任务统一公布。E 中未开展的增强不列作 A—D 必需阻断项；若未来实证发现能力缺口，再单独设计与测量。

## 五视角复核结论

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构专家 | APPROVE WITH CONDITIONS | 已检查路径由模型组织方法，Native 维护授权与保存事实；没有新增第二完成权威。结论限于本次必需范围，不替代全仓库或文学验收。 |
| 技术专家 A | APPROVE WITH CONDITIONS | 来源、精确身份、CAS、累计预算、未知结果与事件消费边界不能降低。作者多 policy 完整阅读修复有真实负例；最终共享源码需统一认证。 |
| 技术专家 B | APPROVE WITH CONDITIONS | 模块终态、原失败和当前源码分别记录；不把源码存在、早期成功或脚本 provider 互相当作最终二进制/真实模型证据。 |
| 产品经理 | APPROVE WITH CONDITIONS | 简短作者输入由应用自主处理，不要求作者交付内部流程和技术身份；实际文学价值、纠错负担与长篇完成率仍需样本验收。 |
| UI/UX 设计师 | APPROVE WITH CONDITIONS | 真实输入与成果优先，内部事件不复制为作者消息；界面文案、导航、滚动与可访问性仍按实际应用证据判断。 |

零兼容复核：本次为只读审查与独立文档记录，没有新增接口、schema、fallback、升级迁移、双读双写、权限或测试门禁；没有兼容例外。没有因未经开展的 E 增强或真实文学质量专项，把已检查当前代码推断为新的生产缺陷。
