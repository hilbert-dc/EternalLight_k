# Renderer HTTP 响应合同完整闭环

## 范围与缺口清单（前审）

初始盘点：`NovelCodexApi.runtime` 93 个方法，`MultimediaApi` 16 个方法。不能把所有方法直接算作 HTTP：运行时启动状态、媒体文件/连接/导出含 Host 事务；需要逐项跟踪 preload → IPC → HTTP 路由。现有 ts-rs 生成具名 serde DTO，但 Renderer 工作区、知识、规划、质量等响应仍以手写 TS 描述 Value 投影；`reviewQualityPacket` 甚至返回 unknown。当前28个消费别名不等于全部合同覆盖。

Rust serde 为 HTTP wire 唯一权威。新增稳定投影 DTO，封套、身份、版本、来源与状态字段具名；任意作者内容仅在明确的内容字段使用 JsonValue。生产边界严格反序列化核对响应，不截断未知字段、不把校验失败变成空结果。TS 仅消费 ts-rs 生成产物及明确 UI 投影。一次性起草工具可以帮助录入现有结构，不保留 TS→Rust 自动镜像链；正式生成方向始终 Rust→TS。

## 五视角前审

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | HTTP DTO在Runtime，Core业务及Host权限不改变，生产路径实际执行合同 |
| 技术A | APPROVE WITH CONDITIONS | 逐方法路由清单与消费门禁有新增遗漏负例，生成零漂移 |
| 技术B | APPROVE WITH CONDITIONS | 真实HTTP覆盖null/optional/enum/身份/来源，错误不吞且未知字段不丢 |
| 产品 | APPROVE WITH CONDITIONS | 不靠放宽到unknown或伪造空数据让检查通过，现行写入/权限语义保持 |
| UI/UX | APPROVE WITH CONDITIONS | Renderer类型检查与真实UI回归通过，稳定身份来源同源 |

## 验证计划

1. 分类清单逐方法明确HTTP、Host-only或复合响应，自动检查新增方法未分类即失败。
2. DTO生成全量tsc和漂移检查；消费方无稳定响应手写副本。
3. 响应校验正负例（缺身份、错误来源类型、非法状态、nullable与optional区分、保留未知字段、错误原样）。
4. 实际Runtime HTTP回归与Renderer构建；记录不等于远端CI执行。

后审待实施证据补入，不预先标为通过。

## 实施与覆盖记录

- `shared/runtime-http-contracts.json`：Runtime API 93 个方法，90 个 HTTP、3 个 Host 生命周期/模型调度入口；`shared/multimedia-http-contracts.json`：16 个媒体方法，10 个 HTTP、6 个 Host 文件/连接/导出组合入口。每个 HTTP 方法均有具名 Rust response root、实际路由与生成类型消费；Host 条目逐项说明归属，不用 unknown 冒充 HTTP 覆盖。
- `renderer_responses/` 按工作区、知识、规划、任务、提案、质量、多媒体等子域组织。已有 Core DTO 直接复用；Runtime Value 投影使用稳定封套与嵌套结构。113 个旧 TS 结构改为生成别名，通用 CreativeRevision 与媒体 preview URL 装饰使用明确 Omit/派生表达。
- 枚举使用语义 variant；版本、计数、偏移、时间戳等确定整数采用整数类型；可分数的测量值保留浮点。API 合同来自生产 JSON，未用旧 TS 的 number 或非空声明掩盖实际数据。
- 真实 HTTP 暴露旧模型差异：空章节 updated_at 为 null，原始修订投影中的 version/baseProjectRevision/source/sourcePlanRefs/payload 可为 null；规划评审 coverage.reason 可缺省；预算可进入 budgetExhausted；同一知识接口支持 world/character/reader 视角。已按实际 producer 校准，Renderer 对缺失信息显式处理，不制造作者内容或版本。
- `response_contract` 仅检验 wire 结构，复用 `repository_executor::execute` 的有界 blocking 准入。验证成功返回原始 bytes，扩展字段不丢；既有错误状态与内容不改。写入后校验失败是 500，并明确可能已经提交、须用原请求 identity 核对，不能盲目重试；64 KiB 内的原回执完整保留，大响应诊断保留请求路径/原状态/省略字节数，不复制无限量正文。此层不重新执行、回滚或解释业务决策。
- `runtime-http-content-fields.json` 明确登记 14 个任意内容叶子：作者 artifact payload、断言值、条件 operand、操作特定选择和任务输入。身份、来源、修订、接受回执、建模实体、知识冲突和证据结构均具名。新增 Value 叶子未登记理由会失败。
- `check:runtime-contracts` 已接 HTTP 消费/路由/生产校验门禁；`test:verification` 包含遗漏方法、手写返回、未导出、未注册校验、虚构路由、媒体方法、动态内容绕过的 7 个正负例。自定义 serde `into` 与 `try_from` 同样需明确映射，不能静默改 wire。

## 已完成验证与最终检查交接

- 最新统一生成：958 个具名 serde DTO，966 个生成文件；生成过程对所有导出 TypeScript 严格检查通过。
- HTTP 覆盖门禁：109 个入口通过；7 个新门禁测试通过；serde 覆盖测试 5 个通过。
- 已有定向真实回归：knowledge_conflicts 与 work_budget_authorization 25 项通过。此前 Runtime/story_event 的共享投影差异已校准并成功编译；全量 Native 正由主代理统一复核，避免重复运行。
- 新增 Rust 中间件测试涵盖必填/nullable/非法枚举、保留成功扩展字段、既有错误原样、保存回执不丢与大诊断有界。媒体新增真实 DTO 正负例与真实 HTTP 定向由 Host 子代理运行。
- 本记录不将仅编译过的测试写为已执行。五视角最终结论须随主代理完整验证结果收口；当前实现已冻结，若发现失败仅按具体合同偏差修复。

媒体交接实测补充：Host 代理确认 8 个真实媒体 HTTP 测试（context 1、media 7）与 2 个结构正负例均通过；最新 Renderer `tsc --noEmit` 无诊断。第一次媒体测试的本机端口沙箱限制已用获授权的沙箱外同一目标重跑验证，不计作产品失败。Cargo 窗口已交主代理完整验证。

### 完整集成中的知识筛选合同校准前审（2026-10-13）

真实 scoped-revision 查询使用 world 视角和 predicate，Core `knowledge_projection` 输出两个 `Option<String>` 筛选字段；旧 Renderer 专用描述却把 predicate 固定为 null、entityId 固定为字符串。完整集成已证明该 DTO 错误拒绝有效请求。两字段改为必填可空字符串，与同一路由的实际源投影一致，保持字段缺失及错误类型拒绝；补真实 HTTP 无筛选/有筛选覆盖。架构、技术A/B、产品、UI前审 APPROVE WITH CONDITIONS：同源生成零漂移及真实 HTTP 验证，不把筛选放宽为任意JSON，不改查询权限或结果。

## 完整 Native 回归后的校准与后审补充

完整 no-fail-fast 揭示两类精确差异，未降低产品合同或权限：

1. 测试直接使用低层 SaveCandidateRequest 构造草稿时遗漏 chapterId 或 sceneGroups，导致创作工作区无法通过真实响应校验，随后 AuthorTaskFixture 无法读取预算授权来源，最终呈现 TaskBindingInvalid。已补 commitment_extraction（含后续编辑）、conversation_analysis 手动保存、delegation_targets、passage_query、quality_notebook、task_evidence 的实际草稿封套；delegation 的两个整对象正文断言同步包含 sceneGroups，仍逐字段精确比较，未删除权限/引用/内容断言。
2. Native 知识接口的实体/谓词选择器和 snapshotId 并非固定 null，主代理依据 producer 校准。另生产桌面的 getState 真实创建路径诊断为 `project.updated_at: integer, expected string`；`read_api.rs` 输出的时间戳是 i64，chapter.summary/updated_at 与 passage.scene_id 省略。state DTO 已如实表达整数及缺省，未增加产品占位字段。新增真实 HTTP 测试覆盖创建空项目→读取→保存正文（201）→再读取，正文与身份保留，缺省属性仍缺省。

最终定向复核：完整失败清单 9 个目标全部通过，合计35项：commitment 1、conversation 3、delegation 5、passage 4、quality 1、scoped revision 14、state selector 1、story event 4、task evidence 2。新增 getState 空/非空路径1项另行通过。日志 `/tmp/native-final-nine-plus-state.log`（附加新测试首轮仅保存状态断言写成200，已按实际201修正）、`/tmp/native-state-final.log`。因此没有用测试夹具修补产品授权，也没有把业务500降级成成功。

五视角后审：架构 APPROVE（DTO校准保持层次）；技术A APPROVE（真实 producer 与生成权威）；技术B APPROVE（九组失败闭环及真实保存读取）；产品 APPROVE（权限/预算/精确内容断言保留）；UI/UX APPROVE（实际桌面本地读取合同修复，无伪造内容）。主代理仍负责最终生成零漂移、桌面重测与完整验证链汇总。

### 空界面状态的时间戳类型同步

生成后的真实 project.updated_at 为 Unix 秒数。App 无作品/加载中投影改用数值 0，保持空 id，不伪造已保存作品。五视角前审通过：Native 事实格式权威；技术两视角要求通过编译及现有空作品/加载旅程；产品与 UI 无显示变化。

### 2026-10-13 Core 纠错与独立质量读者集成夹具前审

实际请求证据：draft correction 的 MCP namespace 已到达 Core，但 checkpoint schema 未进入 provider；夹具遗漏生产 Host 使用的 executorValidatedTools。quality reader 的 Runtime mock 将真实 GET /api/tool-binding 的空 body 当 JSON 解析，尚未实现可信绑定协议。五视角：架构保持 Native 绑定权威；正确性保留真实工具 schema、纠错、CAS 与预算验证；安全保留独立线程、读者权限与隐藏凭证断言；性能不增加 timeout/重试；维护以现行生产 transport 和 canonical executor policy 修正夹具，不修改产品授权。前审通过，定向实测后补后审。

前审补充实证：生产桥策略并非草稿缺失根因。Core 明确拒绝 checkpoint inputSchema 5152 bytes > 5000；保持原硬限，缩写 payload transport 前言，完整内嵌 DraftPayload schema、patch schema、互斥约束及 Native 校验均保留。增加 5000-byte 门禁及完整 payload/patch 保留检查。质量读者另有恢复账本 GET，模拟无记录返回 operation:null，绝不伪造 committed；现行第四个辅助工具 read_tool_guide 仅解释已准入工具，补读者不可指导 source/context/checkpoint 的负例。五视角仍通过。

后审：checkpoint 真实 tools/list inputSchema 从 5152 缩至 4922 bytes（Core 硬限仍 5000）；tool_guidance 三项回归全过，包括完整 payload/patch 约束与 reader guide 权限负例。真实 Core/MCP/Runtime 纠错两项和独立质量读者一项全部通过（/tmp/correction-reader-final2.log）。草稿回执断言继续精确列全字段，新增 completeness、savedSource.revisionId、contentRead=false 及 source read locator 一致性；当前 Engine 已以 256KiB 作为单输出缓冲限制，故拒绝夹具使用真正超限的重复正文，保持正确修订 4500 汉字、原线程、3 次请求/15 tokens、单次写入、CAS、取消污染及无新增授权断言。没有修改 Engine 限制或超时。读者模拟只读恢复账本返回 operation:null，不制造提交证明。架构、正确性、安全、性能、维护五视角后审通过。

### 2026-10-13 完整 scoped revision 旅程夹具校准前审

现行 Core 在 authorRequirementsSource 提供 rootReadComplete=false，明确目录不构成根阅读；MCP source_navigation 将游标转为 nextRead 完整调用。夹具应精确验证新增字段并使用 Native 提供的下一页参数，而不是构造旧 nextCursor。五视角：只调整模拟模型协议；保留整对象、原文 SHA256、字节边界及完整读取；保留预算/授权/作用域；不加超时重试；统一真实生产合同，前审通过。

首轮已推进至 decision-13，暴露现行工作状态以 chapterAcceptanceSource 代替重复内嵌标准。按 task_sources::work_acceptance_source 与 work_decision_status 的真实契约，模拟模型通过 read_source_content 完整读取该 task-bound 不可变标准，校验 sourceId/contentHash、字节总长及 SHA256 后才构造章节检查；同 Work 修订省略已有冻结标准，避免重新发明或覆盖要求。首个无标准章节继续使用原测试批准的首写标准。未放松 Native 交付、预算或作用域规则。

后审：完整真实 Core/MCP/Runtime scoped revision → remodel → GenerationRun 全旅程通过，61.6 秒（/tmp/scoped-journey-final.log，1/1）。所有原最终任务/回执、原文哈希、预算、受保护场景、授权范围与重建后续写断言保留；新增冻结标准真实读取证据。未修改生产逻辑、权限、时间限制。五视角后审通过。

空界面 created_at/updated_at 已同步为数值0，构建与完整UI117/117通过；真实桌面也以本次Runtime产物完整5/5通过，后审条件关闭。
