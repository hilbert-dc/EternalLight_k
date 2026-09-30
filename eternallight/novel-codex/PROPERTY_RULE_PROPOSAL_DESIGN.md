# 对话式属性规则提案

## 目标与边界

S1 自动建模不要求作者手填规则。作者明确要求生成或调整属性结构及事件条件规则时，原
advisory 对话登记 generatePropertyRules，沿同一 WorkTask/Step/Attempt、Codex
执行、MCP、候选修订及恢复账本交付。仅讨论规则不登记修改，不增加斜杠分类器、
模型直连或领域内的第二执行器。本文推进完整 S1–S5，不将此切片视为全部完成。

## 当前版本契约

- 复用 artifact-propertyRules，模型版本 source=agent，作者手改 source=author。
  生成来源与采用权威不同：任何候选都不能生效，只有 Author Review/Acceptance
  才授予结构规则权威。正式规则可以保留模型来源，不伪造作者亲手编写。
- Author 保存无 Task 绑定；Agent 保存必须绑定 generatePropertyRules 根任务及
  propertyRules 产物。其他任务、子 Agent、非 main、伪造来源和过期输入拒绝。
  原 CAS、冻结输入、锁、幂等回执及补偿撤销保持唯一权威。
- 整份候选必填 definitions/eventRules，作为整份替换保留未要求修改的两类规则。
  属性定义最多64条，事件规则最多16条；整个 payload 仍限16KiB，事件单条4KiB。
  类型化有界表达式见 [事件条件契约](EVENT_CONDITION_DESIGN.md)，不执行说明、
  例外、SQL 或任意 DSL。合法表达式不证明条件成立；不自动确认属性值或执行效果，
  不解除 Reader/Character/Writer 的全局规则限制。
- 根任务按原全作品规划查询政策取得证据和冻结上下文；先读取实际上下文，不把
  检索缺口当成全书不存在。首次保存的候选基线来自 inputManifest，保存后必须
  complete_task；没有有效产物不能完成。输入改变不自动重绑或重试。
- Context Pack 的 propertyRulesBase 只提供冻结修订描述；根任务经只读
  read_property_rules 连续读取并核对摘要/范围，原样重组 JSON 后生成候选。
  每页实际序列化最多 7,000 字节，合法长规则可跨页；自身保存不切换基线。
  详见 [冻结规则有界读取](PROPERTY_RULE_INPUT_PAGING_DESIGN.md)。
- 对话的 request_candidate_task 明确支持此操作，原 Host 执行后续任务。能力门禁
  使用通用 base-tools，不依赖某个模型名称或模型自主多 Agent 认证。
- 作者工作区展示模型提案来源，仍可编辑、纯规则审阅、采用和账本撤销；规则
  保存/采用错误与读取错误分离，不将模型成功回复当作候选或正式提交证明。

## 五视角实施者自检（非独立评审）

架构：复用 Revision/Task/Review 和现有 Codex 引擎；没有新的批准指针或执行器。
技术 A：任务、来源、对象、版本和根身份同时校验，事务内复核而非只靠提示词。
技术 B：真实 Runtime 验证候选不生效、采用/撤销、过期和错误操作；MCP 与 Host
契约测试验证工具路由、根/子权限和候选动作。产品：对话为主，表单仍是可选裁决。
UX：模型来源与作者采用明确分离，不把结构批准当成事实确认或例外已解释。

零兼容门禁：修改当前来源/操作契约及所有调用方，不保留旧协议分支、升级迁移
或第二种规则存储。综合结论：按上述契约实施，供应商语义质量仍单独验收。

## 实施后记录

2026-09-27：实现已接入当前任务链，没有新增执行器或上游内核改动。

- Core 来源/绑定回归通过，Core lib 共 109 项；真实 Runtime HTTP 验证对话动作、
  冻结基线、无产物完成拒绝、幂等保存、恢复证明、作者采用/撤销及过期写入拒绝。
- MCP 共 10 项通过，包括实际 TCP 路由及子 Agent/错误任务操作拒绝。
- 生产 Host 处理器证明对话登记此操作后仍走原绑定任务执行路径；真实 Electron
  验证规则提案来源、未生效状态和原导航保护，不依赖模型回复冒充领域结果。
- 完整 `CARGO_NET_OFFLINE=true npm run verify` 退出 0：两轮 native、180 项引擎、
  2 项真实 Codex/子 MCP 委派、7 项发布门禁、149 项 Agent/Host、14 项 UI；最终
  前端独立构建也通过。既有包体警告仍在。
- 原生工作区全目标离线、锁定依赖的严格 Clippy（`-D warnings`）退出 0。

以上为当前 macOS、真实进程与受控模型端点证据，不替代真实供应商、人工金标、
Windows 或文学质量验收。以上是分页变更前的验证记录；分页证据见独立设计文档，
整会话上下文预算及完整 S1–S5 仍未完成。

## 模型可见上下文专项检查

依据 code-review-context 的检查规则，新增的 `propertyRulesBase` 超过 1K tokens
的可能性列为 **P0 上下文复核项**。本次由实现者额外检查，不是独立专家签署。
当前已移除整份正文注入，改为冻结描述与有界只读页。基线来自 Task 冻结的输入
修订；重复读取不会切换到后来保存的新候选，也不会重写模型历史。
这是既有 MCP 工具返回，不是新增上游 `core/context` 用户消息片段，因此没有
为小说业务向 Codex 内核加入新的 ContextualUserFragment。

真实 Codex/MCP/Runtime 的 v1 夹具产生两页，实际模型可见工具文本最大 3,351
字节，cl100k_base/o200k_base 最大 1,586/957 tokens。超过 1K 的页面已作为 P0
专项自检：采用固定源、硬输出上限和连续重组，不据此认证任意分词器、工具参数
或整个历史的总预算。64 条/16KiB 存储界限与 60KB Context Pack 不是 token 证明。
