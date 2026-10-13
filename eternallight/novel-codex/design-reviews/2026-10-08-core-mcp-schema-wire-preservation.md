# Core MCP schema 保真传输（B04）

## 责任与事实

责任模块为 Codex 通用工具声明/协议转换；适用 B01/B04/B09/B10，并保持 B02/B03/B05–B08。读取当前 MCP 生产目录与显式 schema 字节额度；仅修改通用 schema 表示、转换和测试，不读写作品、作者授权、预算、交付或供应商。公共合同为 `rmcp::model::Tool` → `codex_tools::mcp_tool_to_responses_api_tool` → 实际 `ResponsesApiTool` JSON → Engine 公开 codec/Bridge。Engine 已能完整编译/执行原 schema，故必须在 Core 传输层保留，不能移到模型、Native 或 Host 去猜缺失约束。

原限制已证实于源码：typed JsonSchema 未表达的约束丢失，sanitize 会清空只有 if/then 的分支，普通 MCP 大声明被有损压缩。本轮不以日志承认限制代替完成，保留既有失败。只读当前 Native tools/list 量化证据 `/tmp/autonomous-v3-core-schema-live-native-inventory-20261008.json`：14 个 operation；最大 `patch_media_document` 12241 UTF-8 字节，`submit_revision_outcome` 7808、`request_work_action` 5091，三者超过默认 5000。

## 最小方案

- JsonSchema 用通用 flatten 字段保留未知 JSON-schema 关键字；现有 typed 子字段、内部构造器及受信 metadata 保持职责。不会逐条硬编码小说字段。generic sanitize 不清空只有未建模关键字的合法对象；其他非 MCP 归一化保持原职责。MCP 明确绕过归一化，不宣称通用内部 parser 提供原始 wire 全保真语义。
- MCP 声明使用明确的保真入口，直接解析原 schema，不做类型推断、const 改写、补字段、删定义或压缩。显式字节上限超出时返回真正错误；未配置仍沿 5000，产品由既有公开 MCP 配置明确 40000（与 Engine 原每项保守10K合同一致），不增加总上下文、请求数或费用。
- 合法的当前 Native 原声明必须在转换、序列化、进入 Engine 后全部等值。当前 typed 表示不能承载的标准 schema 形状须拒绝，不能伪造成宽松成功。损坏 JSON/非法已建模形状须拒绝并关联工具。
- 新通用 `codex-tools` example `schema_wire` 仅作验证适配器：stdin `{tools:[实际 MCP Tool...],maxBytes:40000}`，调用上述公开转换器，stdout `{tools:[实际 ResponsesApiTool... ]}`。错误 stderr/exit1；无产品命令、小说操作、网络或模型。应用现有14操作目录测试经此产物再进真实 Engine，并核对原 schema；Root 唯一构建执行。
- Root 负责上游补丁登记与 Core CLI/example 构建；另一代理负责公开 Engine 配置和产品配置、原目录测试接线。本代理仅 Core schema/converter/current generic tests/example。

## 五视角前审

| 视角 | 决定 |
| --- | --- |
| 架构 | APPROVE：通用 Core 表示与协议保留，生产无小说依赖；测试仅沿公开接口跨层，Patch登记由Root完成。 |
| 技术 A | APPROVE WITH CONDITIONS：原 schema 全量字节上限；超限/不支持明确拒绝，无宽松fallback、无新授权或账本。 |
| 技术 B | APPROVE WITH CONDITIONS：合法原声明/实际wire等值与非法输入、额度边界均测；实际14目录贯通，不用简化schema fixture替代。 |
| 产品 | APPROVE：模型得到原格式和范围要求，减少无意义参数纠错；输入仍保持一句短请求。 |
| UI/UX | APPROVE WITH CONDITIONS：无新增交互；真正工具错误可解释、不能清空成成功。实际短句样本仍由应用执行。 |

零兼容 `PASS`：当前合同一次性切换，fixture同步；无旧版本检测、迁移、双轨或降级。不同当前协议对象的明确限制不承担历史兼容。本轮未构建/测试/调用模型，不宣称当前全部绿。

## 后审

| 视角 | 静态收尾与待验证证据 |
| --- | --- |
| 架构 | APPROVE：flatten 保留未知字段；32 个内部构造器均经 Default，无额外调用方修改；单一 MCP 保真入口，公开跨层链由 Root 验证。 |
| 技术 A | APPROVE WITH CONDITIONS：先核原 UTF-8 字节，再严格反序列化并核序列化等值；不支持/有损形状带工具名拒绝，不回退。 |
| 技术 B | APPROVE WITH CONDITIONS：现有测试保留 type-only、条件/枚举/未知约束；默认 5000 超限拒绝、精确额度正例、非法已建模字段与 boolean child 拒绝；插件超限不伪造宽松参数。MCP 已有 output wrapper 同步真实合同。 |
| 产品 | APPROVE：实际最大 12241 小于显式 40000；与原 Engine 每项 40K 字节保守 10K token 合同一致，无总上下文、预算或权限增加。 |
| UI/UX | APPROVE WITH CONDITIONS：无空 schema 成功；实际14目录→公开 Core example→Engine 与同短句应用样本待 Root 执行。 |

静态范围已完成，构建、测试及真实模型验收由 Root 唯一执行；此前 Engine 90/90、应用 76/76 属于本 Core 改动前证据，不作为当前 Core 通过证据。

## 完整 Core 测试首轮与调用方同步

Root 的完整 `codex-tools --lib` 实际退出101，111项中106通过、5失败，原日志 `/tmp/autonomous-v3-current4-core-tools-tests.log` 保留。4项旧期望要求丢失合法声明：dynamic annotation、对象子 annotation、allOf 子 annotation、prefixItems；更新精确 expected 内容并保留对象类型/数组默认 items/allOf 结构等全部原断言。另1项上轮误写 deferred MCP output wrapper 期望，真实 `ToolDefinition::into_deferred` 主动清除 output schema，恢复 None；不改其生产语义。

| 视角 | 增量前审与静态后审 |
| --- | --- |
| 架构 | APPROVE：只同步现有 Core 调用方 fixture 与原 sanitizer 文档；不扩 MCP、DTO、领域或权限职责。 |
| 技术 A | APPROVE：严格保留完整字段、结构与等值断言，无 skip/filter、无有损回退；生产 MCP 保真不变。 |
| 技术 B | APPROVE WITH CONDITIONS：5项根因已由实际 diff/源代码明确；静态修正待 Root 原完整111项重新验收。 |
| 产品 | APPROVE：annotation 与 tuple 约束继续传输；作者、预算、模型上下文额度无变化。 |
| UI/UX | APPROVE：无新增界面或交互；deferred output 仍由现有后续加载语义负责。 |

新增补丁测试路径 `tools/src/dynamic_tool_tests.rs`，Root 上游登记需同步；本代理不执行构建、测试或模型。该首轮历史结果为106/111；后续完整111/111实际结果见下方current4后验收。

## current4 五视角实际后验收

| 视角 | 结论与实际证据 |
| --- | --- |
| 架构 | PASS（工具契约机制）：通用Core公开转换→Engine真实14目录等值通过；架构332文件/7crates/0diagnostic，types与fork5项通过。 |
| 技术 A | PASS：当前Core完整111/111、Engine92/92实际exit0；原schema约束、默认5000拒绝和显式40000保真合同验证，未增加总上下文、预算或权限。 |
| 技术 B | PASS（必要调用方）：应用整批75/76真实目录项已通过；唯一遗漏40000的fixture同步后相关两文件3/3定向exit0。未重跑整批76/76，旧106/111与75/76失败均保留。 |
| 产品 | 工程机制PASS，真实创作待验：当前CLI/Native/Engine构建及工作区CLI身份通过，应用已重启；修复后约定短输入尚未通过UI发送，不能认作创作闭环。 |
| UI/UX | 待实际样本：用户已解锁，窗口操作连续noWindowsAvailable并已请求切前台；不写成锁屏结论，不以离线生命周期41/41或连接就绪替代实际结果。 |

日志：`/tmp/autonomous-v3-current4-core-tools-tests2.log`、`/tmp/autonomous-v3-current4-schema-wire-engine-tests.log`、`/tmp/autonomous-v3-current4-schema-wire-application-tests.log`、`/tmp/autonomous-v3-current4-schema-wire-creation-output-tests2.log`。构建身份 `/tmp/autonomous-v3-current4-build-policy-proof.json`：verifiedCLI workspace-build、reason null。`repo:check`仍有既有无关Core路径/发布文件失败，不宣称release gate通过。原作品8章只读unchanged，证据 `/tmp/autonomous-v3-current4-protected-novel-readonly.json`。以上为Root实际执行记录，本代理仅更新文档。
