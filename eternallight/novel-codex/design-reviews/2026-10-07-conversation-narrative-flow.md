# 对话叙事连续性修正

## 变更概览
- 目标：按事件顺序直接展示公开进展中的意图、发现、决定和下一步，操作摘要穿插其中，最终答复在末尾；完成不会隐藏公开说明。
- 原因：整轮 details 自动折叠把公开说明与内部执行混为一层；跨操作去重还可能删除动作之前的意图。
- 影响：ConversationTimeline、进度投影、阅读记忆、CSS、当前 UI fixture。
- 非目标：生成或补写模型没有表达的理由，展示私有推理，修改执行或持久化契约。
- 验收：进行/完成/失败均保持说明顺序与可见；只有各组操作明细可展开；相同说明跨操作不去重；阅读位置和明细展开选择保持。

## 零兼容门禁
- [x] 无版本判断、旧字段适配、迁移、双轨、回填或降级。
- [x] 当前组件、样式、fixture 和测试一次性调整。
- [x] 无 schema 或授权变更，保留当前阅读位置及事件身份边界。
- [x] 无兼容例外。

## 五角色评审（逐角色自评）
- 架构专家：APPROVE。公开说明属于对话正文层，工具细节属于原位 disclosure。
- 技术专家 A：APPROVE。保持真实公开事件，不制造因果关系或暴露私有推理。
- 技术专家 B：APPROVE WITH CONDITIONS。验证完成可见、跨操作顺序、明细选择与滚动恢复。
- 产品经理：APPROVE。阅读时能理解工作如何推进，完成后仍可回顾。
- UI/UX：APPROVE。状态为轻量非交互标题；每组操作独立展开，避免整轮嵌套折叠。

## 综合决策
- APPROVE WITH CONDITIONS；无 P0/P1 阻断，无兼容例外。
- 本方案取代 value-centered-conversation 评审中的整轮自动收起和跨操作进度合并规则。

## 实施后复核
- 实际变更：整轮容器改为非折叠 section；轻量状态标题独立于正文。公开说明与相邻操作摘要按事件顺序交错，最终答复在后。仅连续、完全相同的说明去重，任何操作都是去重边界。
- 阅读记忆删除整轮自动折叠专用分支，保留各组明细选择与阅读锚点。同步调整旧 summary 选择器、相关 fixture 与快照语义。
- 叙事专用 fixture 覆盖意图→读取→发现→保存→核验→最终答复的文本顺序与实际布局位置，完成后说明可见、明细默认关闭；窄屏截图位于 /tmp/novel-narrative-flow/conversation-narrative-flow.png。
- 针对性验证：conversation-value、conversation-process-ui、conversation-presentation-ui、conversation-reader-ui、collaboration-progress-ui、agent-events-ui 共 6 项全部通过；包括实际 App 的事件展示、鼠标/键盘展开、滚动及历史恢复。npm run build、git diff --check 通过。
- 扩展验证限制：另行运行 app-recovery-ui、model-connection-ui、creative-workspace-ui 等共 15 项时，3 项通过、12 项失败。失败包括 fixture 中未显示选定对话、交付状态旧文案断言、协调回复归属断言和恢复场景找不到结束记录等；本次未修改这些业务流程，未据此宣称全量应用回归通过，也未把全部失败认定为已确认的既有问题。
- 本次范围无迁移或双轨。五角色复核同意叙事呈现修正；更广应用回归仍需单独定位，不能以本次界面验收替代。
