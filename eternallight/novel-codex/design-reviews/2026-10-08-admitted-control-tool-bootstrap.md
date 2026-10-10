# 已准入控制工具的初始可见性

日期：2026-10-08。范围仅 `electron/codex-runtime.mjs` 的 `NOVEL_TOOL_DISCOVERY_POLICY`、对应现有 `electron/tests/codex-runtime.test.mjs` 和本评审。Root 统一执行必要定向；本任务不构建、不调用外部模型、不修改 Native/Engine 或实际作品。

## 首轮观察与最小方案

主任务提供首轮真实应用失败：模型使用6次读取/目录调用，两次搜索首屏主要是 outsideCurrentTask 的正文工具且有下一页，没有选择生成提案，实际0章。原失败 rollout 和本轮结果由主验收记录保留，不追加人工剧情/工具指导来将该轮改算成功。这个观察证明本轮控制入口没有及时被找到，不证明单改策略后真实模型必然成功。

当前初始工具只有上下文、资料、状态读取和完成。任务准入被与领域资源一起延迟，模型必须先搜索才能看到已经准入的下一步入口。将三项通用控制工具加入产品 bootstrap：`request_candidate_task`、`request_generation_handoff`、`request_work_action`；其余初始读取/完成入口及独立 discovery 入口保持。

三个当前合同可作控制面：候选工具在原作者指令与真实目标下登记行动；handoff只登记范围提案，新的生成范围仍须 Host 作者批准与有限预算；Work action依据原Work版本/sourceDigest、原要求与真实许可登记后续一步。它们不保存正文，不授予采用/发布权限，也不代替完成回执。Native 的角色、根身份、范围、版本和批准校核完全保持。

Engine 原 PortableDiscovery 从请求的实际 operation catalog 取 schema，再与初始 identity 交集；policy 不构造缺失schema，也不授予工具。MCP 原 `operation_tools` 为 advisory 提供这三个入口，generateChapter writer不提供它们。正文保存、规划/报告写入和方法仍按需 discover。没有输入词判断、自动动作、强制文学步骤或新权限表。

## 当前验收与证据边界

扩充已有 production-provider policy 测试，不新增测试文件或构建目标：保留冻结policy/命名空间/模型配置断言；通过真实当前 MCP `tools/list` 取得 advisory 和 generateChapter 的实际 operation schema，再由真实 Engine `ModelBridge` 向注入式本机下游输出，核对初始选择。advisory三个控制入口必须可见且schema完整等于实际目录；writer不能获得目录未准入的三个控制，仍可见原读取与完成；writer实际已准入的save_draft_checkpoint和方法schema仍延迟。下游只返回协议fixture，不能称为LLM或Native执行准入端到端证明。

这里只运行 tools/list，不向 Native 注册任何行动，也不发 provider 请求到外部服务商。原领域运行时校核和现有权限负例不改。本策略影响新编译的SessionPlan；第二实际样本必须由加载新Host策略的应用实例发送同一句输入，不在首轮对话追加修正步骤。

## 五视角实施前评审

| 视角 | 结论 | 边界 |
| --- | --- | --- |
| 架构专家 | APPROVE | 控制入口与领域资源分开可见；Engine交集和Native事实/权限仍唯一，没有文学调度器。 |
| 技术专家A | APPROVE WITH CONDITIONS | 从真实MCP当前operation目录证明writer不获额外控制，完整schema来自目录；不得加入正文、采用、发布或跳过Native校核。 |
| 技术专家B | APPROVE WITH CONDITIONS | 同步现有policy测试并验证实际Engine选择正负；Root定向实际结果与真实第二样本分开，失败保留。 |
| 产品经理 | APPROVE | 作者短输入无需教授搜索词或流程；模型仍决定是否以及怎样提出后续工作，不将多调用当自主成功。 |
| UI/UX设计师 | APPROVE | 无新UI、技术ID输入或重复作者消息；原审批与真实状态显示保持。 |

综合 APPROVE WITH CONDITIONS。零兼容门禁 PASS：无版本判断、旧字段fallback、迁移、回填、双轨或扩大授权；没有兼容例外。实施后仅静态检查，当前定向实际结果待Root。

## 实施后复核

实际变更仅上述两文件：policy初始列表增加三个登记/准入入口，保持同一命名空间、discovery和其他读取/完成入口；没有加入正文、规划写入、方法、采用或发布。既有 production-provider 测试保留原配置断言，新增真实Native MCP `tools/list` advisory/writer目录和真实Engine Bridge初始schema交集证明；注入式下游仅协议fixture，没有外部provider请求。测试只读取schema，不运行Native登记或保存API，不能冒称执行授权端到端通过。

Native子进程使用当前真实测试二进制，环境经现有隔离函数过滤、只传本机假凭据；不读取实际密钥。没有跳过缺二进制、伪造role目录、从policy制造schema或旧值fallback。两次实际Bridge选择仍计入同一测试预算，catalog切换建立新的精确execution lease。

五视角静态后审：架构 APPROVE（只有Host可见性policy，原Engine/Native交集权威不变）；技术A APPROVE WITH CONDITIONS（实际role catalog和schema完整 equality、writer控制与domain延迟负例已写，待Root真跑）；技术B APPROVE WITH CONDITIONS（只扩既有单项、无构建，必要定向实际终态和新应用第二样本待Root）；产品 APPROVE WITH CONDITIONS（首轮失败保留、不人工续聊纠错，修复后同短句第二样本另记）；UI/UX APPROVE（没有新增UI/重复输入/隐藏状态）。零兼容门禁 PASS。

`node --check` 两个JavaScript文件及scoped `git diff --check` 静态exit0；本代理未运行测试、构建、操作应用、发送模型或修改实际作品。静态冻结后Root独立复核并统一执行必要定向。当前没有第二轮实机或LLM通过证据。
