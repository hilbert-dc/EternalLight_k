# 冻结属性规则输入的有界读取

## 目标与非目标

删除 Context Pack 内整份 propertyRulesBase 正文，改为冻结基线描述和根任务
只读 `read_property_rules`。不缩减当前 64 条/16KiB 规则存储容量，不增加供应商
调用、第二任务状态机、迁移或旧返回格式。作者工作区仍读取完整候选。

## 当前契约

- 领域仓储在单一读取快照中重新加载持久任务，并检查项目、线程、操作与输入
  水位。仅 generatePropertyRules 根任务读取；其他操作和子 Agent 不获此入口。
- Context Pack 返回 id、source、definitionCount、eventRuleCount、payloadBytes、contentHash 和
  readTool，不再携带 payload。信息只描述冻结规则，不伪装全书知识完整。
- 页面传输规范 JSON payload 的 UTF-8 文本片段，最多 2,800 原始字节，按完整
  Unicode 标量边界切分。页面 JSON 序列化后另设 7,000 字节硬上限，超限拒绝，
  不静默截断。这样包含控制字符、引号和反斜线的单条长规则也可无损读取。
- cursor 绑定任务/作品/线程、冻结修订和 payload 摘要及字节偏移；换任务、源
  改变、非法偏移或非字符边界拒绝。相同 cursor 返回相同页面，自身候选写入
  不改变捕获基线。末页仅表示该位置到基线结尾，不认证之前阅读或语义质量。
- 根任务从 cursor=null 开始，按 nextCursor 读取并核对 contentHash、byteRange
  连续性，将 payloadPart 逐页原样拼接后解析为完整 payload。保留未要求修改
  的属性与事件规则；保存及完成仍用原候选 CAS、幂等键、Task 产物和 Author Review。
  当前 payload 必填 definitions/eventRules，页面也携带同一冻结 eventRuleCount；
  这个计数不是条件解释或成立证明。
- 该工具严格只读，不产生恢复写入证明；Host 只读恢复分类同步更新。输出
  超过 1K tokens 的可能性按 code-review-context 作为 P0 专项自检；验证真实
  页面大小及已支持分词器，不将单工具有界读取冒充任意模型整会话预算认证。

## 五视角实施者自检（非独立评审）

架构：规则/修订/任务权威不变，分页为领域读取，不侵入上游 core/context。
技术 A：固定源、快照、权限、游标围栏、UTF-8 和实际序列化上限；不依赖提示词
防越权。技术 B：真实 HTTP 验证最大合法规则、逃逸字符、连续重组、跨任务/源
失效和自身写入后基线不变；MCP/真实 Codex 回环验证传输。产品：不削减规则
容量，不把预算失败解释为无规则。UX：作者原规则审阅流程不变。

零兼容门禁：替换当前基线形状并同步调用方和测试，无 schema 迁移或双轨。
综合决策：按上述条件实施。全局 Context Pack、模型工具参数及会话总预算
仍分别验收，不以此消除它们的独立风险。

## 验证记录

2026-09-27 专项验证：Runtime 三项 HTTP 场景通过；其中超过 12KB 的单条合法
规则包含 Unicode 与控制字符，逐页无损重组。覆盖空基线、重复读取、跨任务及
非法 UTF-8 偏移、取消、外部输入改变、作者/子 Agent/其他操作拒绝。自身候选
保存后捕获基线保持不变。损坏修订的拒绝读取另纳入 Core 原权威链回归。

MCP 11 项传输/权限测试通过。真实 Codex App Server、native MCP/Runtime 和
独立 Engine Bridge 使用受控模型端点完成读取、候选保存、再次读取与任务完成，
作者 accepted head 仍为空；只读页不进入写入恢复账本。v1 夹具两页，实际模型
可见工具文本最大 3,351 字节，cl100k_base/o200k_base 最大 1,586/957 tokens。
该超过 1K 的输出按 P0 专项自检保留明确大小和来源边界，不认证其他分词器。

完整 `CARGO_NET_OFFLINE=true npm run verify` 退出 0：边界检查、前端构建、两轮
原生工作区、独立引擎、真实 Codex 委派和发布门禁全部通过；Core lib 109 项、
MCP 11 项、Agent/Host 150 项、Electron UI 14 项通过。完整验证中回环页文本
仍为最大 3,351 字节，两个分词器最大 1,580/949 tokens；随机执行标识使分词
计数略有变化，以上两次观察最大值为 1,586/957。既有前端大包体警告仍存在。

原生全工作区/全目标严格 Clippy（锁定依赖、离线、`-D warnings`）退出 0。
最终产品 `cargo fmt`、上游 `just fmt` 与 `git diff --check` 通过；codex-rs
工作区无改动，未增加旧协议兼容层。
以上是 macOS、真实进程与受控模型端点证据，
不替代真实供应商连续创作、Windows、人工金标或文学质量验收；完整 S1–S5、
整包/工具参数/会话总预算仍待继续实施。不使用模型自述作为完整性证明。
