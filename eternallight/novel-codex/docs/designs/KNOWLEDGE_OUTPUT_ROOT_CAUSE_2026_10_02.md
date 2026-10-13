# 设定分析输出超限：根因调查与修复

问题：合成体验作品《五轮体验：午夜回信》第一章修订正文 351 字，三次知识分析都失败 `provider_output_item_limit`；第二章 217 字分析成功。该对比不能证明正文长度是原因。前次“供应商输出超限”的归因过宽；当前已证实此错误由本地桥接准入产生。

## 已验证链路

- `modelKnowledge` 首次及恢复均创建绑定 capability 与新的模型会话，不复用失败 rollout 累加上下文。
- 模型请求确实带 `max_output_tokens`，桥接与 Responses 编码都夹到声明的 outputBudget；没有漏传输出额度。
- Responses 准入对整个 canonical JSON 输出条目使用本地保守计数，不等于供应商原生 token。检查包括结构字段和 opaque 字段，40KB 以上直接拒绝；较小项采用两个编码器分片计数的最大值。
- 上次 8KiB 提示只约束工具 JSON 参数，不能证明推理条目或带结构的整个输出能入站。原固定错误没有条目类型/大小，尚不能把失败归于具体字段。

## 阶段 A：保持准入规则，补足拒绝证据

五角色 PRE：架构及技术 A（主代理自检与 Cursor）、技术 B（Codex 产品代理技术审查）APPROVE WITH CONDITIONS；产品和 UI（小说 PM）要求根因具体、同一351字正文真实分析成功才能计作修复。

实施条件：拒绝诊断仅有固定类型及字节/token 数值，不含输出原文、内部身份或凭据；非流、done、terminal 使用相同准入；拒绝项不入模型历史，不截断/放大上限，保持不可重试，预算无 usage 时维持 unknown。供应商 HTTP 错误不得伪装成本地诊断。同步当前自动化测试及真实 Core 集成后，对原正文明确发起分析以取证。

零兼容门禁：PASS。没有迁移、旧格式 fallback 或双轨。先取证，随后修复已被证实的控制失效点。

阶段 A 已实施：只有拒绝时产生匿名尺寸统计；超过 256KiB 的条目不为诊断额外分词，token 数为 null；缺失字段为 null、存在的空数组为 0。Native release 构建通过；定向 Node 15/15 包含真实 Codex Core 的拒项不入 rollout/下一请求和不自动重试；`just test --manifest-path .../agent-engine/Cargo.toml -p eternallight-engine-core --profile default` 47/47。旧环境的能力认证随内核指纹失效，真实产品通过正常验证入口重新认证，不沿用旧证据。

补充验证：桥接预算、生命周期、四种协议及认证契约回归 82/82 通过。真实产品当前九项模型能力全部通过并应用连接。

## 阶段 B：真实恢复入口的确定性绑定错误

同一351字正文点击“继续分析本章”立刻失败 `TaskBindingInvalid`，尚未调用模型。只读审计确认：writer 的产物修订仍是当前源，writer 本身 completed，但父 work 已 cancelled，知识 step cancelled、输入作品版本2，当前作品版本5。`chapter_followup`仅按产物找writer；Electron将其 `sourceTaskId`继续用于执行归属；原生 `attach_attempt`复用固定旧step后，被 running/queued/输入版本校验正确拒绝。产物出处与可执行工作流归属被混用，这是该恢复入口的本质错误，与输出超限是两个问题。

PRE 五角色：主代理架构/技术A、Codex代理架构/技术B、小说产品/UI代理均有条件批准。只对作者明确继续分析时返回仍合法可续的工作流来源；已取消或来源输入已过期时，建立绑定当前精确正文修订的独立分析。不得复活旧work、放宽CAS、重写正文、借新任务绕过额度授权；自动writer后续仍保留原显式绑定。

已实施：`chapter_followup`核对来源工作流与目标知识步骤；terminal 或明确 inputChanged 不再提供旧执行归属。其它权威阻断仍保留原归属，避免绕开额度或恢复门禁；数据库错误传播。Electron 对合法 paused/blocked 的 queued/blocked 知识步骤使用现有 `resumeWorkTask`，不重新创建批次。没有修改 `task_attempts` 的安全守卫。

### 同源真实模型验收

正常重启产品后明确点击“继续分析本章”，真实 DeepSeek 保存批次 `modeling-34071151fb1b45eeb4d17a15b0962da3`。只读复核：新 completed 分析任务 inputRevision=5、target 仅含新batchId，所属独立工作流 completed；旧writer工作流仍 cancelled/inputRevision=2。新batch ready，精确source仍为 `draft-615cfb0a29a84fe7bdc79ef0d2fb199b`，与原351字修订一致。正文和已发布状态保持，作品仍v5。

实际“更新故事设定”检查列出10项未决定建议（含1条灯塔熄灭事件），8个实体；所有建议仍待作者决定，没有重新发布正文或自动确认设定。旧失败记录仍可见。新会话最大完整推理项 17,692 UTF-8字节 / 5,785保守tokens；发布工具完整项3,768字节 / 1,308tokens，参数3,074字节，未触发输出门禁。

POST：主代理架构/技术A、Cursor技术审查、Codex架构/技术B以及小说产品/UI审查均批准阶段B归属修复。当前UI已真实确认同源10项可检查。没有迁移、fallback、双轨、模型历史截断或放大10K上限。

### 结论边界

已证实并修复的是作者继续分析入口把产物出处误用为旧工作流执行归属；同一份失败正文此次真实分析成功。三次旧 `provider_output_item_limit` 的拒绝项未保存，无法事后证明具体是哪一种条目或字段。阶段A是保留安全门禁的诊断增强，不能把本次成功归因于某一提示改动，也不能宣称所有超限已根治。若再次拒绝，固定匿名诊断现在能区分具体条目类型、字段字节数和本地计数；供应商错误与本地准入错误仍分开处理。

验证：阶段B Electron 主流程/恢复/配置 44/44，产品 TypeScript 与生产构建通过；`just test --manifest-path .../novel-codex/native/Cargo.toml -p novel-codex-story-core --lib --profile default task_followup` 6/6（其余174项按本次过滤跳过）。原生覆盖：取消+仅正文发布后独立精确源、blocked/paused复用、自动writer旧输入CAS拒绝、ready无新动作、DB错误传播、attemptBudget不能绕开。枚举阶段短暂停在macOS加载器入口，后来自行完成；没有终止Rust、改签名或弱化系统保护。格式在全部验证完成后统一处理。
