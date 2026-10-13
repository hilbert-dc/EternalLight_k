# 存储执行与领域边界重构

## 方案与责任归属（实施前）

执行者：当前 Codex 代理，五视角自审。基线 HEAD `c51b76f853509e104b69ae415b343cda6fee20b5`；以包含用户未提交工作区的实际源码为基线，不覆盖已有修改。

根因分类：设计与实现兼有。Runtime 143 个打开点反复运行 DDL，同步 SQLite 占用异步执行器；校验仅含基础表部分列属性。源码风险已确认，尚未测量线上延迟或认定已发生数据损坏。

目标：Story Core 拥有唯一 schema、连接生命周期、领域事务和保存用例；Runtime 拥有认证、协议、有限并发的阻塞执行边界和响应映射。使用 B01/B03/B05/B06/B10；不增加迁移、双轨或领域决策。

1. 聚合所有静态与动态 DDL，SQLite 参照库生成结构权威。初始化前比较现存表/索引/触发器定义，拒绝不匹配且不写原数据；初始化在事务内完成。校验涵盖约束、默认值及触发器，避免仅校验字段。
2. `StoryDatabase` 在启动初始化一次，按有界容量复用连接。每个连接配置外键、busy timeout、同步级别；WAL 在启动确定。每次借用构造新的 Repository 调用上下文，只将连接归还池，恢复身份不能跨请求残留。进程内句柄不作为新的作品事实权威。
3. Runtime 所有同步领域调用通过统一阻塞执行入口。异步信号量在占用 blocking 线程之前控制排队；连接持有不跨网络 await。审计工作线程复用同一个数据库句柄。已开始的事务不因客户端断开伪装回滚，保留原幂等和持久回执。
4. 按领域拆路由并保留 author/candidate/Host 专属认证分组。草稿与产物保存的身份和领域 envelope 下沉 Core 用例。按实际职责组织 Core 子域，提供窄 store 能力；跨子域事务仍由 Core 协调，不宣称目录拆分带来独立编译。

## 状态判定

| 输入事实 | 合法结果 | 副作用/后续 | 验证 |
| --- | --- | --- | --- |
| 新独立空库 | 初始化全部当前结构 | 原子提交后提供连接 | E1 |
| 已有表、索引或触发器定义不符 | CURRENT_SCHEMA_MISMATCH | 初始化前拒绝，保留数据 | E1 |
| 并发读取/写入、SQLite锁争用 | 在阻塞执行边界等待或返回真实存储错误 | 异步健康请求仍可调度，不改CAS | E2 |
| 请求结束后归还连接 | 新请求获得空调用上下文 | 不继承恢复身份、控制代次 | E2 |
| 已开始写入后请求取消 | 结果可能已保存，保留幂等/回执事实 | 按原身份核对，不声明零执行 | E2/E3 |
| Candidate访问Author/Host接口 | 原认证拒绝 | 零领域写入 | E3 |

## 验证计划与结果

| 编号 | 内容 | 状态 |
| --- | --- | --- |
| E1 | 新库、重开、模块结构、约束、索引、触发器拒绝且原数据不变 | 最终 9/9 通过，含 SQLite 大小写名称与无前缀触发器 |
| E2 | 连接复用、请求上下文隔离、并发锁等待与异步调度、取消写入 | 3 项连接测试 + 3 项异步/取消测试通过 |
| E3 | Runtime HTTP全部相关回归及媒体Host往返、认证、草稿保存/CAS/幂等 | 最终 Runtime lib、wire HTTP 3/3、审阅/恢复/事务故障/认证回归通过；全 Native 1397 通过，两项旧断言修正后所在目标 5/5 复验通过 |
| E4 | Core/Authoring/MCP编译回归、architecture:check、相关门禁 | 全工作区编译通过；architecture/fork/build/Host 联调通过；完整 Native 112 个测试目标及失败目标复验已关闭 |

## 五视角前审

| 视角 | 结论与条件 |
| --- | --- |
| 架构 | APPROVE WITH CONDITIONS：事实与事务留Core，Runtime只调公开用例；E3/E4关闭 |
| 技术A | APPROVE WITH CONDITIONS：池只复用连接，阻塞隔离与并发有界；E1/E2关闭 |
| 技术B | APPROVE WITH CONDITIONS：同步全调用方与测试，删除请求DDL旧路径；E1–E4关闭 |
| 产品 | APPROVE：不引入作者操作，保护资料、幂等与恢复 |
| UI/UX | APPROVE：无新增UI，错误及已保存事实保持原合同 |

零兼容前审：无迁移或旧字段fallback；真实资料不重建。实施后在本记录填写实证，不以前审计划代替完成。

## 后审

实施完成。E1–E4 已按下方全量与失败目标复验记录关闭。

### 审阅应用边界补充（2026-10-12）

依本记录既定五视角前审的 B01/B03 职责划分，将 review API 的 review/fact/commit/undo/snapshot 身份、默认检查、提案来源及决定、对象定位与摘要构造移入 `revision/review_use_cases.rs`，由显式导出的借用式 `ReviewStore` 提供 create/accept/reject_identity/undo 用例。Runtime 保留受信角色与 task capability 校验、路径/请求形状校验及原 HTTP 状态/响应映射；Core 继续调用原 create_review/accept_review/reject_identity_review/undo_acceptance，未拆事务，也未另存事实。CAS、主分支、默认 selections、建模批次、回执比较及 enriched-proposal digest 格式保持不变（包括现有含生成 fact ID 的摘要行为，未借重构改动其幂等语义）。

补充后审：架构 APPROVE；技术 A/B APPROVE WITH CONDITIONS（静态路径核对与 rustfmt 通过，编译及现有审阅/接受/撤销 HTTP 回归交主任务统一运行后才能关闭）；产品与 UI/UX APPROVE（响应字段、角色拒绝和作者动作不变，无 UI 改动）。尚未单独执行 Cargo，不将静态通过写为运行验证通过。

## 集成发现与修复依据

- 故障注入测试过去每个请求重新初始化 Runtime；完整 schema 校验现在会正确拒绝安装的临时 trigger。测试改为先启动并保留同一 Runtime/连接池，再注入事务故障，仍检查所有零写入和重试断言；不对生产校验增加测试例外。
- 合成长篇 fixture 的 draft 内容缺少 `version`，原 HTTP 路径静默当 0。本次 Core 用例对损坏版本报错，fixture 补齐当前版本字段；不恢复运行时容错。
- 三项全文分页测试使用过时的 6500 字节控制回执预算，而 Core 当前全文预算是 63 KiB。测试改引用公开的同源 `SOURCE_CONTENT_PAGE_BYTES`，保留完整页拼接、来源冻结和 MCP 双层编码预算断言。
- 草稿内容规范化（章节/场景身份、标题和段落一致性）与产物锁对象身份也归 Core CandidateStore；Runtime 原 server 派生的五个 API 文件不再使用 `expect` 构造领域 ID。
- 不宣称性能提升百分比：已验证重复 DDL 不在连接借用路径，锁等待不占用异步执行器，但未做真实作品延迟基准。

### 最终集成验证记录

完整离线 Native workspace 执行记录 `/tmp/eternallight-native-final.log`：1397 通过、2 失败；失败仅在 working_source/writer_baseline 的旧单条 9000 token 断言。经核实当前 `agent-engine/native/engine-core/src/tokens.rs` 单条使用字节准入，整请求统一约束 token；将测试接到真实 `tokens.bound`，不改生产限额，不放宽全文拼接、冻结来源与游标断言。最终定向复验结果在下方补记。Core lib 502/502、current_schema 9/9、Runtime 阻塞/取消保护 3/3 均已包含在完整执行中。

分页准入补充后审：沿用本记录五视角责任边界，架构/技术A/技术B/产品/UI均 APPROVE。`engine-core/tokens.rs::bounded` 与 `responses_request.rs` 实际准入仅约束256KiB字节，tokens_tests明确允许超过10000 token的完整项；两项旧9000 token检查改调用同源 `tokens.bound`，仍构造实际双层MCP JSON及160字符call_id，并要求至少1024字节metadata余量。完整来源、冻结身份、hash、游标与页拼接断言保持。使用 `/tmp/eternallight-boundaries-target` 定向执行 working_source_tests **3/3**、writer_baseline_http_tests **2/2**，exit0，日志 `/tmp/frontend-native-page-bounds.txt`。这关闭完整执行中的两个已识别失败；没有另行声称重新运行了全workspace。

最终失败目标复验：`working_source_tests` 3/3、`writer_baseline_http_tests` 2/2，通过日志 `/tmp/frontend-native-page-bounds.txt`。不是重新执行完整 workspace 的结果；全量 1397/2 与定向 5/5 合并闭环。生产 Engine 预算不变。

五视角后审：架构 APPROVE（所有请求经统一执行器，schema 与领域身份由 Core 持有）；技术 A APPROVE（完整结构校验、连接上下文隔离、锁等待/取消与事务保护验证通过）；技术 B APPROVE（编译、合同、相关 HTTP、完整 Native 及失败目标复验完成，无遗留失败）；产品 APPROVE（作者权限/CAS/幂等及已保存事实保持）；UI/UX APPROVE（存储重构未引入作者操作，错误按原合同呈现）。ReviewStore 补充技术条件随接受/撤销等 HTTP 回归关闭。

剩余边界：跨子域原子协调仍使用 SqliteStoryRepository；目录与窄 store 改善能力边界，不代表整个 crate 独立增量编译。动态 JSON 响应尚未全部改为具名 DTO，具体已生成/消费范围见合同评审。以上不构成未经验证的性能或全类型覆盖声明。

整体交付交叉验证：合并后完整 UI 116/116、生产构建、111 Host 文件类型检查、720 文件/7 crate 架构检查以及验证选择器 12/12 均通过。各分项设计与前后审见同日 frontend-boundaries、host-boundaries、core-domain-boundaries、contracts-and-verification、continuity-notice-visibility 记录。本次未执行整个 `npm run verify` 聚合命令；以上列出实际执行范围，不代表外部在线模型探针或跨平台发布认证。
