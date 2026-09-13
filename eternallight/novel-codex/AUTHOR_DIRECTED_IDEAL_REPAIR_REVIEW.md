# 作者主导创作理想态：最终实施复核

版本：2.3（历史记录）· 日期：2026-09-13 · 原通过结论已被后续独立审查撤回。当前修复与验收见 [作者体验修复记录](AUTHOR_EXPERIENCE_REPAIR_REVIEW.md)。以下批准与测试数量仅描述当时的评审，不代表当前版本完整验收。

关联文档：[产品目标](PRODUCT_AUTHOR_DIRECTED_CREATION.md)、[AC-0／AC-1 实施设计](AUTHOR_DIRECTED_AC01_IMPLEMENTATION_REVIEW.md)、[实施状态](AC01_IMPLEMENTATION_STATUS.md)。评审角色均为独立 AI 角色，不代表真人专家签字。

## 变更与架构决策

1. Runtime 继续是小说领域唯一权威；Electron 只负责受信作者入口、任务调度和 app-server 协议转换，React 只负责编辑、比较与作者决定。小说逻辑没有进入 `codex-core`。
2. 当前版本只接受一套 Outline、Plan、Task、Context 和 MCP 契约。`operation` 唯一派生任务权限；旧字段推断、双读、迁移和降级逻辑不属于实现。
3. 每个 Agent 任务在启动独立 sidecar 前生成 capability；thread 创建后、首个模型回合前，Runtime 原子创建绑定 capability hash、作品、thread、版本、operation 和 TTL 的 TaskRun。凭证此前在 Runtime 中不可用，也未进入 prompt／Renderer。任务结束或停止时 Electron 立即终止 App Server／MCP 并擦除明文，Runtime 同时撤销 hash；应用启动先回收崩溃遗留的活跃任务。
4. 模型和 CLI 都由仓库清单约束。`supported-cli.json` 固定协议、真实版本输出以及逐平台／架构 SHA-256；产品操作目录和 Codex 模型元数据分别固定指纹，专用配置用 `model_catalog_json` 装载隔离副本。领域生成只允许审核组合；DeepSeek 保持 advisory-only。
5. Context 的 60,000-byte 总预算与 48-KiB 单核心产物契约一致。工具结果不再复制 payload，按 9,000-byte 模型内容项分块；硬约束完整保留，候选事实和原文摘录才可按优先级移除并报告。
6. Writer 输入机械投影公开字段，并冻结章节、阅读位置、范围、POV、故事时间与精确规划 revision。Plan 保存、绑定回填、Context、checkpoint、Review 和事实位置都在 Runtime 复验；事实场景位置只能等于所属 `SceneChangeGroup` 的一基顺序。
7. 正文、事实和规划反馈形成同一可撤销链。provisional impact、保护冲突决定、手动补链、误报、知情保留和真实改纲关闭都有持久状态与审计来源。
8. 编辑器按作品／视图／章节保存会话元数据、意图、滚动位置和选区；后台刷新不覆盖 dirty buffer。离开使用“保存候选／放弃／留在”三选，409 提供基线、服务器版本、本地稿复制和明确重载。

## 五角色最终评审

### 架构专家

- 结论：`APPROVE`。
- 任务权限已从“应用级 bearer + 调用方字段”收紧为“全局传输认证 + 每任务 capability + Runtime operation 矩阵”。任务跨作品、跨 thread、过期、撤销和结束后的重放均被拒绝。
- 当前方案避免额外 bootstrap API；安全不变量是 TaskRun 在首个模型 turn 前完成绑定。清单、TTL、进程销毁、启动恢复与撤销点都有自动化证据。

### 技术专家 A

- 结论：`APPROVE`。
- 候选 CAS、按 task/author 隔离的幂等回执、仍为当前候选的最新 task output、Plan 来源、POV／故事时间、Review 草稿归属和事实范围均在持有数据的 Runtime 校验。
- 作者 Context 只允许当前 revision；没有历史指针重建时不会再返回混合时间点快照。

### 技术专家 B

- 结论：`APPROVE`。
- MCP 结果单份表示并逐项硬限；完整合法核心 Brief 可进入受限 Planner Context。app-server v2 的事件、answers、processId、退出与恢复契约由 Node 行为测试覆盖。
- 双模型目录指纹、低于 1K token 的固定基础 instructions、`model_catalog_json`、CLI 版本／平台摘要和 operation 工具白名单让支持面可审计；候选写入响应不回显正文，未知组合在模型调用前失败。

### 产品经理

- 结论：`APPROVE`。
- 空作品有构想、简报、直接写章三入口；规划可跳层，近期细化，独立章不伪造来源。作者始终决定采用、重大变化、误报和是否改纲。
- 失败无输出、部分检查点、已停止和候选可审阅不再混为同一状态。后续 AC-2 的多方案探索不会阻塞 AC-0／AC-1 主旅程。

### UI/UX 设计师

- 结论：`APPROVE`。
- Writer 无公开规划时必须逐次确认；影响目标显示完整路径；技术键和稳定 ID 收入高级详情；退回草稿的标签与行为一致。
- 应用内导航与关闭窗口的三选离开、冲突稿一键基于最新版另存、删除后焦点、可访问字段错误、空首页入口、多节点事实补链、事实来源／类型、保护冲突上下文和始终可返回来源正文的反馈卡已落地。macOS 新用户目录冒烟验证了关键入口和门禁。

## 自动化与走查证据

- Runtime：`npm run test:runtime`，75/75。
- Agent／Electron 契约：`npm run test:agent`，59/59；包含 fresh-home OpenAI ephemeral 登录、provider 凭据隔离、DeepSeek 根层 TOML、配置生命周期和有界 turn 结果。loopback 测试在允许本机监听的环境执行。
- 前端：`npm run build`，TypeScript 与 production build 通过。
- 桌面走查：先前新建空作品、localOnly 第一章、60,000-byte Runtime 预览、无规划确认和三选离开证据仍保留。本轮新增界面修复已通过类型、构建和纯逻辑测试，但尚未在当前 schema 的干净用户目录完成完整点击复验，列入发布证据而不冒充自动覆盖。
- 零兼容复查：当前 schema、HTTP、IPC、MCP、Renderer、fixture 和测试同步；未保留旧 task 权限字段、旧 Outline/Plan 形状、版本 fallback、迁移或双写。
- 认证复查：fresh-home OpenAI 在生成前完成 ephemeral API-key 登录；缺 Key 会 fail-closed；`CODEX_API_KEY` 和非选中 provider 凭据不会进入 Runtime 或 App Server 环境。

## 综合决策

原“无已知 P0/P1 阻断项”的结论已撤回：后续审查实际复现保存丢稿、场景身份错配、正式方向遗漏和任务生命周期问题。当前结论以作者体验修复记录为准。本地匿名产品指标尚属待实现功能；真实模型全旅程、真人作者研究、Windows／安装包属于待补验收证据。

## 2.3 对齐复核补充

上一版复核遗漏了会破坏实际创作路径的问题，本版以可复现输入重新审查并完成修复：DeepSeek provider 曾落入错误 TOML 层级；自由讨论结果曾被固定完成文案替代；Brief、Outline 再生成与滚动 Plan 的必要输入不完整；错误事实即使被拒绝仍会阻塞正文；生成完成可能覆盖作者输入；Writer Preview、事实语义、审阅恢复、场景稳定 ID 和二次冲突基线存在界面状态缺口。对应实现与回归已纳入当前 75 项 Runtime、59 项 Agent／Electron测试及生产构建。

复核同时补上两个未在最初问题清单中的契约边界：显式空 `sceneGroupIds` 不再解释成全选；项目简介只进入 Planner，Writer／Critic 不会因首次构想修复而额外获得未公开秘密。当前结论只覆盖 AC-0／AC-1 实现和自动化契约，不提前声明真实模型文学效果或本轮桌面全旅程已经验收。
