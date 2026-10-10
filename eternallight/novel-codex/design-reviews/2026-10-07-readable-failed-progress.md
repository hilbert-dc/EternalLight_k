# 失败与停止后保持对话可读

## 变更概览

目标：公开进展不因回合失败或停止退化成重复折叠入口；当前、历史与回看使用同一阅读规则。原话与操作按顺序呈现，可读失败原因直接显示。

方案：明确 commentary 直接显示，不经过未完成答复包装。短的未完成 answer（至多 600 个 Unicode 字符）带状态直显，较长内容按需展开；长度只控制阅读密度，不判断意图、草稿种类或交付。recorded/truncated 的已保存 final 在原答复区显示，对应残留 stream 无论长短仍折叠，避免重复或误认。unknown 保持保存未核实，只有 absent 才说明未交付。

Timeline 中真实 kind=error 用既有可读错误说明直显，原始诊断折叠。当前 transcript 已有 run.error 时关闭 Timeline 的重复失败说明；缺少外部错误时仍在 Timeline 显示。相邻操作仍原地展开，不吸收 commentary。

非目标：新增日志、模式、菜单、卡片、模型摘要、关键词意图路由、执行权或历史兼容。无 Host/Native、协议或 schema 改动。

影响：ConversationOutput、ConversationTimeline、ConversationReader、App 的当前/历史呈现，未完成与过程 UI fixture、现行架构文档。

验收：运行变失败/停止时进展仍可读且原序；600/601 字符边界正确；short unknown/absent 与 recorded/truncated 的交付提示准确；当前与历史公开进展无 details 祖先；可读失败原因无需点击且不重复；工具与原始诊断仍按需展开；保存来源、迟到回执、恢复和保存成果边界不变。

## 零兼容门禁

- [x] 无旧字段 fallback、双轨、迁移、回填、降级或 deprecated 路径。
- [x] 删除错误呈现规则，不为旧行为保留重复入口。
- [x] 新增必传 text 的当前调用方、fixture、测试和文档一次同步。
- [x] 当前临时 fixture 初始化，不操作真实作品或模型。
- [x] 保存凭证、身份隔离、失败恢复、可访问性及 Unicode 规则保留验证。
- [x] 无兼容例外。

## 实施前五角色评审

| 角色 | 结论 | 意见 |
| --- | --- | --- |
| 架构专家 | APPROVE | 按明确消息性质区分进展与 answer；文本长度不改变交付证据。 |
| 技术专家 A | APPROVE | 已保存 final 与原 stream 分离；unknown 不冒充 absent；外部错误存在时才关闭重复说明。 |
| 技术专家 B | APPROVE | 全部 text 调用同步，600/601 Unicode 与三入口状态变化、来源迟到及恢复边界需覆盖。 |
| 产品经理 | APPROVE | 用户能直接理解动作、发现和受阻原因，不新增日志或模式。 |
| UI/UX 设计师 | APPROVE | 原位说明和轻操作保持；短 answer 可读，长内容按需；只显示一次可读原因。 |

证据：上下文与呈现只读审计、产品/UIUX审计、独立测试审计及方案补充确认。综合 APPROVE；无 P0/P1；无兼容例外。

## 实施后复核

实际变更：当前 Timeline、Reader 和历史 commentary 均直接渲染原段落。ConversationOutput 只接受 answer 文本，600 Unicode code point 以内带真实未完成状态直显，更长内容折叠；recorded/truncated 凭证优先，原 stream 始终折叠，独立已保存 final 正常显示。unknown 明示保存未核实，absent 才标未交付。复制标签仍区分未完成内容。

Timeline 终态 error 独立于工具分组，可读原因在外、raw 诊断一层展开。当前 transcript 已有 run.error 时跳过重复 error 展示，不将其混入成功操作或伪标“部分操作未完成”。实际工具失败的原状态保留。未修改消息投影、保存读取、事件、权限、Host 或 Native。

与方案偏差：无。后复核发现关闭重复原因时 error 曾落入操作组，已在独立分支中跳过；回归检查 raw 内容完全不再位于被关闭的 Timeline，锁住该边界。

验证：

- `npm run build` 通过；现有 bundle 体积提示保留。日志 `/tmp/readable-failed-progress-build.log`。
- Electron 三专项通过：`unfinished-output-ui`（17 个场景）、`conversation-process-ui`、`conversation-reader-ui`。日志 `/tmp/readable-failed-progress-ui.log`。
- 覆盖失败/取消进展、短 unknown/absent、短 recorded/truncated 残留 stream、Unicode 600/601、同节点运行到终态、Reader/保留历史/历史 commentary 字段、保存凭证归属与迟到结果、详情状态重置、错误去重、恢复入口和已保存章节。
- 已查看 `/tmp/readable-failed-progress-ui/readable-conversation-process.png` 和 `conversation-codex-flow-narrow.png`，公开段落、工具层级与 300px 换行保持可读；没有重复进展折叠入口。
- 本次生产与 fixture 文件 `git diff --check` 通过，旧折叠入口未在生产呈现中保留。

架构专家、技术专家 A、技术专家 B、产品经理、UI/UX 设计师实施后均 APPROVE；截图与实际定向验证由主线程完成。零兼容门禁 PASS，范围内无剩余项。未运行真实模型、读取或修改作者小说；这些证据验证呈现、状态和来源边界，不用于证明真实文学质量或任务交付完成。
