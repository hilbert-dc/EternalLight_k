# Engine运行时失败保留机器错误码

## 真实故障、责任和方案

current11恢复决策task-f24fd072…实际Core/MCP第三次工具参数拒绝为tool_argument_correction_limit，但最终Native Task.error仅27字符的中文展示消息。公开Core历史与只读技术证据 `/tmp/autonomous-v3-current11-decision-arguments-diagnostic.json` 保留原码；Engine CodexSessionRuntime回调将Error转成message字符串，Rust RuntimeCoordinator的错误槽及reply仅String，JS consumer仅重建TASK_CANCELLED。Host既有失败合同能保留code，却收到无code的Error。Native无法按已有机器合同认识失败；不能加中文关键词推断或回填旧Task.error。

B01/B04/B06/B10：修复属于Engine通用执行错误传输，不属于小说恢复规则或CLI Core。最终生产范围为src/host/codex-session-runtime.mjs、native/engine-core/src/runtime_coordinator.rs及必需当前消费者src/host/engine-host.mjs，同步当前callback producer、Rust typed slots/reply及JS consumer。一份RuntimeFailure {code:null或有效ASCII机器标识（最多80字节）,message:string（最多1000 Unicode字符）}贯穿所有失败槽；Rust严格对象形状与边界校验，未知字段/旧string/缺code/类型错误拒绝，非法callback不消费原pending身份。JS只序列化code/message，不spread Error、不复制cause/stack/参数、非Error抛出值用固定通用消息。底层binding异常与当前结构化reply解码分开，无旧字符串fallback。

内部取消依据当前明确revoked/cancel事实生成TASK_CANCELLED对象，不根据展示消息前缀推断。所有当前内部错误一次同步至typed结构，plain Error允许code:null。另一个确认缺口cleanup_error.or(turn_error)会覆盖原模型失败，改为原turn_error优先；所有资源清理仍执行，cleanup失败仍使Native state blocked且不允许新bind。保留费用failed结算、取消和晚到资源的既有状态机，不增加循环或放宽恢复。

读取/写入权威为Engine执行生命周期及账本，不写作品、作者输入、保存回执或Native领域状态。EngineHost既有turn_result传输对象和errorFrom已保留code，预计无需生产改动；以真实Native穿透回归确认。MCP选择合同另修，Native已有机器码集合及有限恢复限制不变。原已丢码的失败不回填；实际继续须用应用公开原Work恢复，不追加作者输入。

## 五视角实施前审

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE | Engine拥有通用错误传输，作品和恢复政策保持Native权威；不改CoreCLI。 |
| 技术A | APPROVE WITH CONDITIONS | 严格当前wire、有限静态code/message、无任意对象序列化；非法callback不消费identity；原错误与资源释放同时保证。 |
| 技术B | APPROVE WITH CONDITIONS | 真实NAPI→NativeCoordinator→Host completion证明机器码保留，生产者/消费者及所有当前fixture一次切换，无旧字符串通道。 |
| 产品 | APPROVE | 准确失败交给模型已有有限决策，保留原费用/失败；工程不替模型生成或扩大目标。 |
| UI/UX | APPROVE | UI仍接收可读message，内部机器code用于真实状态；不能把错误码保留当创作完成。 |

零兼容PASS：不迁移、回填、词语识别、错误string fallback、双协议或Native通用重试放宽。独立前审后实施。

独立五视角前审APPROVE WITH CONDITIONS。编码安全补充同属当前wire保证：Node>=22在已经确认为字符串的Error.message截断前使用toWellFormed()，将孤立surrogate转为Unicode替换字符，避免Rust拒绝失败回调而卡住pending；只规范传输消息，不改机器code或1000字符边界。独立审查批准，新增真实NAPI含原code的孤立surrogate回归；未引入任意对象String、旧协议或业务行为。

### 必需当前消费者同步：EngineHost对象失败

后审确认src/host/engine-host.mjs errorFrom虽收到当前对象code:null，仍按message的TASK_CANCELLED前缀覆盖为取消。Runtime已正确区分普通Error，但跨Host又丢失该语义，这是同一合同的必要消费者同步。增量前审五视角APPROVE WITH CONDITIONS：仅将当前对象失败按原显式code（包括null）重建，不以其展示前缀选码；NativeHost当前独立command reject的string合同不变，不是为旧Runtime callback添加fallback。追加真实CodexSessionRuntime→NativeHost completion plain-prefix Error负例：run状态failed、message原样且code=null；保留真实取消回归和当前机器码用例。只增加这一生产文件，不修改NativeHost状态机/小说恢复/CLI Core；重跑受影响完整Host与续期目标、架构检查后再后审。

## 回归与应用再验

三个受影响完整目标：RuntimeCoordinator Rust tests；tests/codex-session-runtime.test.mjs；tests/engine-host.test.mjs。覆盖同步/异步真实coded tool错误、nullcode普通Error、strictwire负矩阵、非法callback身份保持、原turn错误+cleanup失败、全部cleanup/failed结算/不可复用/close资源，以及既有取消和晚到资源。跨Host真实Native结果保留code/message，run failed。仅有当前Error.message不允许伪造历史machinecode。

Root与MCP修复统一冻结构建、architecture:check及受影响应用再验。实际原Work自行修订/新独立报告/解决原问题/最终交付仍待验证；目前1146字候选不符合模型冻结范围，不能人工缩稿或以旧报告认证新稿。完整长篇、暂停、多章及预算实测不随本修复声明通过。

## 实施后复核

独立五视角最终后审APPROVE，零兼容PASS；当前Host对象消费者前缀覆盖阻断已解除。Rust55df6f…3a50、Runtime JS c2062874…cd70、Host JS ed6d3711…ba758、Runtime test cf7e15ec…e2faa、Host test e0322455…a8f9均冻结。NAPI实际构建exit0，新binary SHA76d1bd2d…85067；Rust回归3/3、完整Runtime16/16、最终Host13/13、最终续期11/11分别实际exit0。首轮39测试中两项本机监听EPERM的沙箱日志保留，只对需要监听的受控fixture授权重跑；没有业务断言修改或生产回退。统一Native bins、架构检查及完整生产目录Node35/35也通过，源码/二进制/日志 `/tmp/autonomous-v3-current11-choice-error-build-proof.json`。

这证明当前code/message跨真实NAPI及Host completion保持、取消不从对象展示文案推断、清理故障不盖原执行原因且资源不可复用。旧任务已丢失码不回填。实际局部修订、独立报告与最终交付仍未通过，应用以同QA同原对话继续，不冒称全部架构或长篇完成。
