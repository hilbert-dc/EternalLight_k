# Native → Core → Engine 工具参数合同接线

## 实施前五视角

责任与范围：Engine唯一配置编译器 `agent-engine/native/engine-core/src/config.rs`、既有Rust/公开JS配置测试；公开 `src/session/codex-config.mjs`薄入口只记录契约，不另造编译器。Host `electron/codex-runtime.mjs` 在现有小说server显式选择单schema字节上限40000；现有 `codex-runtime.test.mjs`真实14操作目录测试必须先经过Core公开 `schema_wire`转换，再验证实际Responses序列化、原codec和Bridge。Core DTO/parser/public转换、通用example由对应负责人实现，不在本范围重复修改。适用B01/B04/B05/B09/B10。

现有事实：Engine `schema::convert`已拒绝超过40000字节的单工具schema，当前Core默认5000可拒绝Native大schema。Root采集的14目录证据 `/tmp/autonomous-v3-core-schema-live-native-inventory-20261008.json` 最大单schema12241字节；这证明需要明确现有单工具转换边界，不证明模型能力或实际写作质量。只调整单schema传输接线，不扩大上下文、请求/输出用量、预算、工具目录或权限。

公开字段 `mcpServer.inputSchemaMaxBytes` 在唯一Native配置编译器检查为正安全整数（大于0、不超过JS安全整数及当前usize），只输出已有Core `tool_input_schema_max_bytes`；缺省不输出并保留Core默认策略。null、布尔、字符串、非整数、非正与不安全值明确拒绝，蛇形内部字段不作为公开别名。小说当前公开调用显式40000，其他调用方不自动改变。

目录贯通使用实际 `tools/list` 14份目录，全部工具直接交通用Core example；stdout必须保留同序name/description/strict及参数。每份参数与原Native完整schema深等值；只有原缺少根properties且Core现有定义补空对象的规范化例外可明确处理，不删除constraints。原所有操作目录/控制入口/writer无控制权限/延后发现/预算断言保持；无Core旁路、fixture简化schema或私有Engine导入。随后原公开codec+Bridge使用该实际Core产物，必要非法参数拒绝继续验证。

| 视角 | 前审结论 | 必须保持 |
| --- | --- | --- |
| 架构 | APPROVE | Engine Native是唯一配置编译权威，Core真实公开转换取代旁路；Host只选择公开参数。 |
| 技术A | APPROVE | 字段严格有限整数，单schema与总输入/预算界限分开；不得删约束或放宽Native许可。 |
| 技术B | APPROVE WITH CONDITIONS | 现有真实14目录/所有权限断言保留；Core产物精确比原、再经codec/Bridge拒绝非法调用；Root唯一构建执行。 |
| 产品 | APPROVE | 模型获得真实当前工具契约，不能因转换丢字段导致误用；不增加创作步骤。 |
| UI/UX | APPROVE | 无UI或作者输入改变，技术限额不交作者填写，无新确认。 |

零兼容：PASS。当前字段一次性贯通，无旧字段fallback、第二校验框架、版本分支、目录替代或迁移。Core修改依原Fork登记由Core负责人负责。

最小验证由Root执行：Engine对应Rust配置测试与公开contracts测试；构建当前 `codex-tools --example schema_wire`；原 `electron/tests/codex-runtime.test.mjs`。架构门禁及跨层构建身份由Root统一记录，不由本代理另跑build/test/UI/model。原失败保留，静态冻结不能当成实际通过。

## 实施后静态复核与冻结

实际接线：Engine Native唯一编译器新增公开 `inputSchemaMaxBytes` 严格正安全整数验证，缺省不输出、内部蛇形别名仍拒绝，只生成原Core字段。薄JS导出仅补契约注释，无第二套编译或schema。既有Rust/公开contracts增加缺省保持、1/5000/40000精确配置、原计划/其他配置全等及类型/数值/别名拒绝用例。Host现有小说server显式40000，既有生产配置用例精确断言。

真实目录测试保留全部14操作、原权限与预算断言：实际MCP目录全部flatten，交当前 `codex-tools` 公共example，完整Responses序列化四字段和逐schema参数与Native原对象深等值；随后codec使用该Core产物。Core负责人最终选择严格MCP保真（不填根properties、不sanitize、不compact），所以本实现无规范化例外或降级。实际大schema在缺省Core5000下必须带工具名明确exit1、无stdout成功；显式40000实际批次必须保真。

非法参数由当前Core产物验证：公开chat codec分别精确拒绝实际`read_context`的maxLength越界和实际`read_work_issues`的pattern不匹配；原Bridge两请求正例之后，仍在原4请求/64000token预算内增加一次受控非法参数响应，必须400、`invalid_tool_arguments`、精确工具/schemaPath、不可重试且没有可执行output。原两请求与计费断言保持，末尾另验证3次真实受控请求计费12tokens、无未知/未结算预留。没有外部供应商或新增预算。

| 视角 | 后审结论 | 证据与边界 |
| --- | --- | --- |
| 架构 | PASS | 单一Engine Native编译权威；Native→真实Core公开转换→公开codec/Bridge，不借私有导入或简化声明。 |
| 技术A | PASS | 单schema字节配置与总输入/预算分开，原默认/权限不变；非法参数仍在执行前精确拒绝。 |
| 技术B | APPROVE WITH CONDITIONS | 原14操作与所有权限断言保留；真实逐对象等值、Core缺省拒绝、codec/Bridge负例落盘，Root实际构建/验收待补。 |
| 产品 | PASS | 模型接收原真实工具合同，不增加方法、团队或固定创作流程。 |
| UI/UX | PASS | 无UI、作者操作或确认改变，不将技术限额交作者填写。 |

零兼容后审：PASS，B01/B04/B05/B09/B10无偏移。静态冻结SHA256：

- `agent-engine/native/engine-core/src/config.rs`：`cfe356d87da69bef77d9595f98f5f02d9e30abc9b02f1d84e557ff0d5b78a2a4`。
- `agent-engine/native/engine-core/src/config_developer_tests.rs`：`6e40c8463ba4f3463f173674d920a7400b18811bf025ec624340f7fbe5811405`。
- `agent-engine/src/session/codex-config.mjs`：`a2afa00bc8ca21593a5711aa66783b7b7c2eef927dc8373f8b7d3f18c22f6030`。
- `agent-engine/tests/contracts.test.mjs`：`92cc9173aed131cdbae5d80b03563e4b00d70f579e9d56a684a1771d4a96393c`。
- `novel-codex/electron/codex-runtime.mjs`：`e54a66130dd1f9bcab48538d7aef9b7871e3c1f57ccd4718c8dd7c17bb299845`。
- `novel-codex/electron/tests/codex-runtime.test.mjs`：`7b67fbb1e5b800cf7e10465172a0be2d4b5548639469fb98c09626306d62d203`。

本代理未执行build/test/UI/model，不声明Core/Engine/Native目录实际贯通通过；来源库存的12241上界是Root此前实际采样，不替代新转换测试。Root最小命令：`cargo test --manifest-path eternallight/agent-engine/native/Cargo.toml -p eternallight-engine-core --lib config::tests::mcp_`、当前Engine addon构建后 `node --test eternallight/agent-engine/tests/contracts.test.mjs`，以及当前Core example构建后 `node --test eternallight/novel-codex/electron/tests/codex-runtime.test.mjs`。新EngineCore也由StoryRuntime引用，对应当前产物构建由Root统一保证；不扩大到新功能或付费创作验收。

## 当前生产配置的两个必要集成调用方增量前审

Root实际Core111/111、CLI/example构建、当前来源策略刷新及build身份解析均exit0；当前应用76项实际75通过/1失败，保留 `/tmp/autonomous-v3-current4-schema-wire-application-tests.log`。唯一失败自主修稿链的明确错误为`discovered tool is outside the current admitted catalog`、403。该手工编译配置的fixture没有同步当前小说server显式40000，Core仍默认5000，超过默认限额的`request_work_action`/评估提交schema被明确拒绝；不应放宽发现或权限校验来让它成功。

范围仅两个必要当前调用方：`autonomous-creation-engine.integration.mjs` 的小说server、`tool-output-policy.integration.mjs` 第二项真实Native小说server。各自通过原公开 `compileCodexConfig`传入当前`inputSchemaMaxBytes:40000`，精确断言生成的TOML字段后写入。第一项通用`output_fixture`小schema保留缺省；不改任何production/Core/Adapter、scope、来源、用量预算、独立修稿/保存/作者要求断言或超时。

| 视角 | 增量前审结论 | 必须保持 |
| --- | --- | --- |
| 架构 | APPROVE | 当前测试调用方使用生产已有公开参数，不用手工TOML绕编译器或改Native权威。 |
| 技术A | APPROVE | 仅两小说server单schema配置；默认通用server与真实permission/catalog门禁不变。 |
| 技术B | APPROVE WITH CONDITIONS | 新增精确TOML40000断言；所有原范围/预算/修稿/来源断言全保留，Root统一重验。 |
| 产品 | APPROVE | 当前真实目录可发现完整声明，不能通过额外工具许可掩盖错误。 |
| UI/UX | APPROVE | 无UI、用户输入或批准改变，无新流程。 |

零兼容前审：PASS，B04/B05/B10仍保持；这是当前公开配置的一次性调用方同步。原75/76失败保留，不能宣称该自主链通过。

## 两个必要集成调用方增量后审与冻结

实际仅在这两个原小说server对象传入`inputSchemaMaxBytes:40000`；原公开编译结果先精确断言`tool_input_schema_max_bytes = 40000`，再写到原配置路径。通用第一项`output_fixture`保持缺省，原14目录转换测试、production/Core/Adapter未编辑；全部原范围、预算、独立修稿、原作者全文/来源证明、分页、压缩、任务身份和恢复断言保留，未增RPC、超时或模型调用。

| 视角 | 增量后审结论 | 当前证据 |
| --- | --- | --- |
| 架构 | PASS | 使用Engine唯一公开编译器的当前字段，不另手工拼TOML或绕来源/权限。 |
| 技术A | PASS | 仅单schema40000保持当前生产合同，总上下文、Work/Engine预算和Native范围不变。 |
| 技术B | APPROVE WITH CONDITIONS | 两个精确TOML断言及原全部业务门禁保留，Root实际重验待补。 |
| 产品 | PASS | 实际声明超过默认5000的失败得到明确调用方修正，未扩大工具许可。 |
| UI/UX | PASS | 无UI、原话、批准或流程变化，原失败不隐藏。 |

零兼容后审：PASS。当前冻结：`autonomous-creation-engine.integration.mjs` SHA256 `297bdd687fe81ded3e162bad2b9ca9ec4c4cd01565caeb92b97141ee0b23f55a`；`tool-output-policy.integration.mjs` SHA256 `bef4b2b7f1b0fc110d409dfc4a64e0e605b3c459f3dfe11e1db26fe56f04745c`。

本代理未执行build/test/UI/model；Root75/76实际失败日志保留。Root最小复验（小说目录）：`node --test electron/tests/autonomous-creation-engine.integration.mjs electron/tests/tool-output-policy.integration.mjs`。生产/Core既已冻结，本增量不要求新增构建或扩大回归。

## current4 五视角实际后验收

| 视角 | 结论与实际证据 |
| --- | --- |
| 架构 | PASS（工具契约机制）：通用Core公开转换→Engine真实14目录等值通过；架构332文件/7crates/0diagnostic，types与fork5项通过。 |
| 技术 A | PASS：当前Core完整111/111、Engine92/92实际exit0；原schema约束、默认5000拒绝和显式40000保真合同验证，未增加总上下文、预算或权限。 |
| 技术 B | PASS（必要调用方）：应用整批75/76真实目录项已通过；唯一遗漏40000的fixture同步后相关两文件3/3定向exit0。未重跑整批76/76，旧106/111与75/76失败均保留。 |
| 产品 | 工程机制PASS，真实创作待验：当前CLI/Native/Engine构建及工作区CLI身份通过，应用已重启；修复后约定短输入尚未通过UI发送，不能认作创作闭环。 |
| UI/UX | 待实际样本：用户已解锁，窗口操作连续noWindowsAvailable并已请求切前台；不写成锁屏结论，不以离线生命周期41/41或连接就绪替代实际结果。 |

日志：`/tmp/autonomous-v3-current4-core-tools-tests2.log`、`/tmp/autonomous-v3-current4-schema-wire-engine-tests.log`、`/tmp/autonomous-v3-current4-schema-wire-application-tests.log`、`/tmp/autonomous-v3-current4-schema-wire-creation-output-tests2.log`。构建身份 `/tmp/autonomous-v3-current4-build-policy-proof.json`：verifiedCLI workspace-build、reason null。`repo:check`仍有既有无关Core路径/发布文件失败，不宣称release gate通过。原作品8章只读unchanged，证据 `/tmp/autonomous-v3-current4-protected-novel-readonly.json`。以上为Root实际执行记录，本代理仅更新文档。
