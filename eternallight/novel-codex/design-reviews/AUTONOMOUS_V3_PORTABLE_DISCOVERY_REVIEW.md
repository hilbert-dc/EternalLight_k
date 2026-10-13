# v3 Portable 按需工具 schema 交付

日期：2026-10-07。实施前五视角由本实现者复核，不冒称五名独立人员。范围：通用 Engine 的可信 SessionPlan 配置、桥接 schema 曝光、实际发现回执与 Responses/Chat 传输；小说 Native 发现 metadata 由 root 同步。不改 Codex Core，不打开当前不可执行的 native toolSearch flag。

## 方案与权威边界

当前完整 operation admitted catalog 始终是权限上界。可信产品 policy.toolDiscovery version1 定义 deferredNamespaces、discoveryTools 与 initialTools，编译进入不可变 SessionPlan。初始只在被配置的 namespace 曝光 bootstrap 工具；其他 Core 生命周期仍按当前真实授权。模型用普通结构化 discovery 函数搜索，Native 对中文 metadata 检索并返回当页 available 的具体工具身份。Engine 不解读作者词句、不固定方法、不给权限，不相信返回的自造 schema；只从当前 admitted catalog 复制所选工具的完整真 schema。

每个可信 caller 线程的 bounded 状态分别保存实际发出的 discovery call 和已加载工具身份。只在此前本执行已通过 codec/终态校验的模型 discovery function_call 与下一请求对应 function_call_output 相配时加载。首请求历史旧输出没有权威；caller 身份、catalog、execution lease 变化使旧 pending 失效。压缩保留本执行加载集合，每次与当前 catalog 核对；冷恢复重新发现，没有全量 fallback。

MCP 回执 data.toolDiscovery={version:1,tools:[{name,namespace:null}]}；null namespace 绑定该实际 discovery 调用的 namespace，显式 namespace 必须相同。isError、未知名字、伪造调用、错误 shape 不能加载。目录只选择工具；故事读/写、预算、作者授权、保存回执和 Native 门禁均保持原权威。每次搜索仍消耗实际模型请求与原预算，完整 schema 不截断绕过 item 限额。

## 实施前五视角

| 视角 | 结论 | 必须验收 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | SessionPlan 来源可信，当前 catalog 作为唯一 schema 权威；选择身份与执行许可分开；状态不通过首请求历史自认证。 |
| 技术 A | APPROVE WITH CONDITIONS | root/child、lease/reset、catalog 变化、未知名称与伪造 schema 均拒绝扩大；响应实际终态验证后才登记 pending，预算/取消/未知写流程不吞错。 |
| 技术 B | APPROVE WITH CONDITIONS | Responses 与 Chat 实际桥传输证明首包小工具集、中文发现回执后真 schema、结构化调用正确恢复；压缩/重启/版本变化与失败隔离测试；实际 Codex 合成链而非只有纯 helper。 |
| 产品 | APPROVE WITH CONDITIONS | 按问题发现可用工具，不要求登记方法或选择固定创作模式；零方法可直接通过 bootstrap 处理简单问答；发现不表示结果已保存或文学质量通过。 |
| UI/UX | APPROVE WITH CONDITIONS | 新机制不增加作者 UI/技术表单；所有 schema/内部身份只在模型工具层；可读回复与成果呈现仍使用原契约。 |

## 零兼容门禁

PASS（预审）：只当前 version1；坏配置拒绝，不做旧配置迁移、双读、字段猜测、native search 降级或默默全量 schema 曝光。未配置 discovery 的其他产品保持其显式直接工具模式，这是独立有效模式，不是失败后的 fallback。

## 实施后复核

本轮实际终态：模型可见工具 schema 已按需加载；Codex Core/MCP 上游仍注册当前 operation 完整 admitted catalog 作为权限上界。没有实现或宣称 Core 原生 toolSearch，supports_search_tool=false，不打开未支持的 native flag。目录检索只交付资源 metadata；角色说明不会加载其他工具；被发现工具仍受实际 task、预算、范围和 Native 保存/验收门禁约束。

| 视角 | 后复核 | 实证 |
| --- | --- | --- |
| 架构 | PASS | trusted policy 编译为冻结 SessionPlan 并纳入 plan 身份；当前完整 catalog 提供唯一真实 schema。root 独立只读快审未发现新增权限缺口。 |
| 技术 A | PASS | Rust 7/7 + 既有 BridgePolicy 2/2；历史伪回执、未知工具、自造 schema、跨 namespace、不同 caller、不同 args、目录变化、旧并行响应与 bounded pending 都有测试。 |
| 技术 B | PASS | Portable wire 6/6，Responses JSON/SSE 与 Chat JSON/SSE 分别真实传输；既有 Bridge/续期 28 项通过。实际 Core+MCP 压缩 1/1，实际 Core+MCP+Runtime 自改稿闭环 1/1；明确当前官方 Chat 空工具 wire 为省略 tools。 |
| 产品 | PASS（机制） | 首包只有 bootstrap；模型按问题发现写作和独立评估 schema；同一 Work 因真实 failed report 修订后复评通过，不增加批准预算、不重复作者输入、不登记 mandatory 方法。模型为脚本，不能据此断言 DeepSeek 文学质量。 |
| UI/UX | PASS（范围） | 没有新增作者表单或 UI。实际调用、完整保存、独立报告、最终交付仍使用当前产品结构；发现回执不冒充保存或正式质量认证。 |

### 可复现实证

- `/tmp/autonomous-v3-portable-build-final.log`：Engine Native release build 成功；随后新模块只做 rustfmt，root 最终统一构建需包含该源树。
- `/tmp/autonomous-v3-portable-rust-final.log`：Rust 新 discovery 7/7、原 policy 2/2。
- `/tmp/autonomous-v3-portable-wire-focused.log`：最终 portable wire 6/6；`/tmp/autonomous-v3-portable-wire-final.log` 保留先前两个仅测试断言失败（Chat 官方省略空 tools），同批既有 Bridge/续期 28 项通过；不将这个旧批 log 写作全绿。
- `/tmp/autonomous-v3-portable-actual-codex-final.log`：自动创作实际链 1/1（40.9 秒），同时保留新增压缩 fixture 初次传错 sendTurn 文本类型的失败；该 fixture 已按当前 text-item 数组契约修正。
- `/tmp/autonomous-v3-portable-real-compaction.log`：修正后实际 Codex 压缩 1/1（1.63 秒）；空工具 compaction 不破坏载入集合，继续回显无需 rediscovery，新 lease 同对话必须 rediscovery。
- `/tmp/autonomous-v3-portable-production-config.log`：Novel 真正 resolveCodexProvider 编译配置 1/1。
- `/tmp/autonomous-v3-portable-runtime-config-final.log`：完整当前 provider/config/MCP fixture 35/35（1.88 秒）；同步新 metadata discovery schema、当前公开表达意义、Core 原生 callId 与 read-only Runtime ledger 查询。Ledger absence 明确 operation:null，不伪造提交。Initialize 启动预算有界 90 秒、测试 180 秒，业务 RPC 仍 5 秒；冷 loader 超时不冒充业务失败。

首轮实际链因旧 debug MCP 没有新 toolDiscovery 回执失败；root 构建当前 MCP 后完成，不添加旧字段兼容或绕过未知形状。所有 provider 响应为本机脚本、所有故事数据为临时合成项目，未读取或修改真实小说，未调用付费模型。

### 测试集成

Engine `npm test` 的 tests/*.test.mjs 已自动包含 `portable-discovery.test.mjs`；Rust workspace 测试发现新增 `tool_discovery_tests.rs`。Novel `test:autonomous-creation` 已包含更新的 `autonomous-creation-engine.integration.mjs` 和新增 `portable-discovery-compaction.integration.mjs`，pretest 由 root 加入 build:native-engine。新增 production config 测试在原 `codex-runtime.test.mjs`，现有 test:agent 已覆盖。Engine portable 独立 step 由 root 统一集成。

## 真实压缩协议增补预审（实施前）

Core compact.rs 使用 tools=[] 的摘要请求，可信 reserved request_kind=compaction。该请求不是新的 operation 工具目录，不能清空本执行已加载身份。只在可信 metadata 标注 compaction 且请求没有工具时保持已有目录/集合，不消费任何历史发现回执、不登记响应调用；带工具的 compaction 拒绝。随后正常请求仍以完整当前 admitted catalog 重验。不是从正文或缺省空 tools 猜测压缩。

架构、技术 A、技术 B、产品、UI/UX 五视角均 APPROVE WITH CONDITIONS：caller 与 lease 原核验保留；压缩没有工具权限；实际 Core 压缩与双协议空工具请求测试必须证明正常 schema 保留，真正目录变化仍失效；不增加作者交互或预算。
