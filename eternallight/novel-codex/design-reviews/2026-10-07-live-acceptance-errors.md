# 实机验收错误的清晰展示

## 服务商 HTTP 402 计费失败（实施前）

Root实际验收：第7章新稿已保存，后续advisory失败，框架错误为 `unexpected status 402 Payment Required: Insufficient Balance (request_id: ...), url: http://127.0.0.1:59475/responses`。该错误来自模型请求的服务商计费/余额状态，不能解释为作者任务预算用完，也不能据此授予重试、支付或扩大任务权限。本轮实际请求已停止，已保存成果仍按Native记录保留。

最小方案只修改 Renderer 的纯错误说明函数 `src/agent-error-description.ts`：从字符串起始处解析当前框架 `unexpected status 402 Payment Required` 与合法结束/冒号边界，可包含标准Electron IPC前缀及Error前缀。依据确定HTTP状态展示中文账户计费/余额说明；不扫描作者自然语言、任意402数字、英文余额词或URL内容。原错误字符串与现行详情入口保留，函数不修改失败事件、任务状态、持久保存回执、模型预算或恢复决定；不新增模型调用、支付、重试或输入路由。

下列为实施者按当前代码与Root授权完成的五角色视角评审，不代表五位独立人员。

| 角色 | 实施前意见 |
| --- | --- |
| 架构专家 | APPROVE：仅纯展示层消费现行可信框架错误形状，Native/Host仍拥有失败与恢复权威。 |
| 技术专家 A | APPROVE WITH CONDITIONS：HTTP状态及完整框架前缀锚定，不按余额词匹配；说明不回显URL、请求ID或参数，原详情不删除。 |
| 技术专家 B | APPROVE WITH CONDITIONS：定向回归包含实机格式、IPC前缀、其他HTTP状态、作者引用402与URL/正文含402不误匹配；验证原失败及保存/重试权威不改变。 |
| 产品经理 | APPROVE：区分服务商账户与作者任务预算，避免用户反复提交创作目标或误认为小说成果丢失。 |
| UI/UX | APPROVE：复用当前中文错误位置与按需详情，不添加计费表单、支付入口或新任务面板。 |

零兼容门禁 PASS：只消费当前实机框架及Electron传递格式，无旧字段fallback、历史迁移、回填或双轨。实施范围只有纯说明函数、当前测试与评审记录；不触碰共享App/WorkPanel、Host/Core/Engine，不操作真实应用，不调用模型，不读取或写入小说。

## 服务商 HTTP 402 计费失败（实施后）

纯说明函数新增一个从字符串开始锚定的当前框架HTTP402规则，支持当前合法的直接错误、Error前缀、标准Electron IPC包裹方式；不扫描正文或余额词，不把其它HTTP错误、URL、JSON正文、作者引用或不完整状态误判为计费失败。作者说明最终为：“模型服务商账户的余额或计费状态需要处理，本轮已停止。已保存的内容仍保留，请先处理服务商账户。” 原始request诊断不进入主说明，现行原始错误与详情仍保留。原任务预算保持是实现边界，按Root实机反馈从作者主提示中删除额外预算术语，未改变预算、恢复协议或任务状态。

独立代理durable_host只读快审通过：匹配依据为完整可信框架状态，当前调用只在错误展示通道；预处理、转义和正文内容不授予任何执行权。新增测试验证原失败状态、项目/任务/回合身份、连接状态、原错误详情及已有成果标记不被修改；既有重试候选测试证明纯说明没有增加恢复目标或补权限。

| 角色 | 实施后意见 |
| --- | --- |
| 架构专家 | APPROVE：只改错误说明，执行/恢复与保存权威仍在原协议；没有输入关键词路由。 |
| 技术专家 A | APPROVE：完整状态锚定与边界通过正反用例，固定中文不泄露请求ID、URL或伪凭据；原失败证据保持。 |
| 技术专家 B | APPROVE：最终Node31/31、已有隐藏Agent UI1/1通过，tsc与Vite构建成功，diff检查通过；测试包含实机形状、IPC前缀、其它状态、作者引用和精确回合边界。 |
| 产品经理 | APPROVE WITH CONDITIONS：Root通过真实CUA已在主失败说明和编辑器通知看到HTTP402中文，展示链路成立；账户余额/计费仍需作者处理，不能声称模型已恢复或小说创作已全部完成。 |
| UI/UX | APPROVE：沿用现有失败位置和原始详情入口，最终文案直接说明处理账户，减少额外预算术语，没有新面板或支付流程。 |

零兼容门禁复查 PASS：只消费当前错误格式，无历史迁移或旧契约分支。日志：`/tmp/closed-loop-billing-error-tests-final.log`（31通过0失败）、`/tmp/closed-loop-billing-error-ui-final.log`（1通过0失败）、`/tmp/closed-loop-billing-error-build-final.log`（tsc随后Vite成功）。测试已写入现有 `electron/tests/agent-events.test.mjs`，该入口原已列于test:ui，无新增package入口。生产冻结SHA256：`ddd63780df266fe67529ecfec4ef13288375608cba6f609f922eabe885a38321`（agent-error-description.ts）。Root的真实CUA反馈来自主任务；本实施者未操作真实应用、读取/写入小说、调用模型、重试、支付或编辑共享App/WorkPanel。
