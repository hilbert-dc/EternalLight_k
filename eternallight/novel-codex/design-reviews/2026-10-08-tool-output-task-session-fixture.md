# 原文分页集成测试的真实任务会话绑定

## 实施前五视角

范围仅 `electron/tests/tool-output-policy.integration.mjs` 第二项集成测试及本记录。责任为Host公开Adapter合同的必要测试调用方，适用B04/B05/B06/B10；生产Adapter由其他代理负责，本改动不修改生产、Native、权限、额度或输出限制。

当前公开合同：`createThread`只创建元数据对话，关闭小说MCP；真实Task创建成功并持有实际capability后，调用 `prepareAdvisorySession(capability)`重绑执行进程，再 `resumeThread(原threadId)` 恢复同根对话，正式启用required小说MCP。原fixture在构造Adapter时预置operation/capability，创建元数据对话和Native Task后直接send，缺少当前执行绑定步骤。

实施：创建时只提供运行时/候选通道，不预置尚未注册的Task capability/operation。捕获真实新对话回执ID；Native POST绑定该ID，成功后按现有公开方法准备advisory执行会话并恢复该ID。核对真实Task/恢复回执/Adapter线程身份与进程更换；原TaskBudget与executionLease均复用，不创建新预算或原话。原分页全文/字节范围/hash/重复读取/压缩前后/原稿不变/实际计费断言全部保留，第一项隔离MCP测试不改。

| 视角 | 前审结论 | 保持条件 |
| --- | --- | --- |
| 架构 | APPROVE | 只同步当前公开生命周期调用方；Native拥有任务授权，Adapter拥有执行进程。 |
| 技术A | APPROVE | 先真实任务注册再capability绑定；恢复原root，不继承元数据进程工具禁用状态。 |
| 技术B | APPROVE WITH CONDITIONS | 保留两项集成测试与全部强断言；增加精确身份/进程断言，由Root唯一执行。 |
| 产品 | APPROVE | 元数据创建不提前执行作品工具，实际阅读仍受同任务范围授权。 |
| UI/UX | APPROVE | 本增量无UI变更或额外确认，不能伪称一次读页测试证明文学质量。 |

零兼容：PASS，无旧字段fallback、分支、降级、门禁放宽或超时变更。模型选择仍不被此受控阅读fixture扩大为固定创作流程。

Root最小验证：`node --test electron/tests/tool-output-policy.integration.mjs`。本代理不执行构建、测试、UI或模型；实际结果另待Root，既有失败保留。

## 实施后静态复核

实际只同步第二项集成调用方：移除创建元数据线程前的operation/capability预置；从真实createThread回执捕获root ID，Native Task成功后复用原budget/lease，准备advisory执行进程并恢复同一ID。新增精确Task/恢复回执/Adapter线程等值和真实进程更换断言。全部原分页、全文hash、重复读取、压缩、原稿不变、预算计量断言及两个测试的超时未改；第一项隔离MCP测试未改。

| 视角 | 后审结论 | 当前证据 |
| --- | --- | --- |
| 架构 | PASS | 当前公开生命周期顺序贯通，无生产或领域实现改动。 |
| 技术A | PASS | 真Task注册后才capability绑定；原root、Task、范围、预算与lease保持。 |
| 技术B | APPROVE WITH CONDITIONS | 原强断言保持并核对执行进程重新绑定；静态冻结，Root实际集成待验。 |
| 产品 | PASS | 元数据动作不提前获得任务工具，原只读原文检验不改变稿件。 |
| UI/UX | PASS | 无UI、确认或用户输入变化，无扩大创作流程。 |

零兼容后审：PASS；B04/B05/B06/B10无偏移。fixture SHA256 `c4259d649c8653bd1c6d0ac189ce6c3e96f3b3edeb9d68b641d01f2fa525fe30`。本代理未执行build/test/UI/model，不宣称运行通过；Root仅需运行上面的原集成文件。

## 实际失败与空对话持久化增量前审

Root实际结果 `/tmp/autonomous-v3-current3-tool-output-lifecycle.log`：exit1、0/2。第一项配置仅有`output_fixture`，产品Adapter为固定`novel_workspace`传入当前enabled/required字段，实际Core拒绝不存在transport；第二项换进程后恢复原ID实际拒绝`no rollout found`。两项原失败保留，不能将上轮静态冻结描述为运行通过。

第二项确切遗漏：实际 `createThread` 尚未保存新空对话。当前生产公开 `saveThreadName` 通过正式name RPC保存并回读同ID/实际名称，成功后才可进入跨进程恢复；在同一测试中增加此调用，精确核对返回ID与fixture名称“原文页额度隔离验收”，随后才Native注册Task。不发送虚构回合、不fallback、不直接写库。

第一项与Adapter负责人协调当前server契约，先确认，不修改生产；原隔离MCP的工具、实际内容和输出上限验收必须保留。

| 视角 | 增量前审结论 | 必须保持 |
| --- | --- | --- |
| 架构 | APPROVE | 通过正式空对话保存API持久化，不把临时内存ID当作恢复事实。 |
| 技术A | APPROVE | 同一真实ID/name保存回执之后才Task与执行绑定；错误明确失败。 |
| 技术B | APPROVE WITH CONDITIONS | 原Task/root/预算、跨进程、正文hash/分页/压缩断言均保持；Root实际重验。 |
| 产品 | APPROVE | 当前fixture原名称真实保存，不增加作者输入或模型调用。 |
| UI/UX | APPROVE | 无UI/流程控件变化，不伪造空历史或隐藏原错误。 |

零兼容前审：PASS。第二项仅当前必要调用方同步，无生产权限或费用变化；第一项范围待当前公开Adapter合同协调确认。

## 空对话持久化增量后审

已实施第二项：真实createThread之后，正式保存fixture对话名“原文页额度隔离验收”，核对保存回执同ID及精确name，再沿原Native Task→原budget/lease→执行准备→原ID恢复。原真实进程更换和所有读页/hash/计费/压缩断言保持。没有额外turn、SQL写入、fixture成功fallback或生产改动。

第一项经Adapter负责人确认属于通用Core输出测试：没有Native Task，应保留显式`output_fixture`配置/allowlist/namespace。将其改名成小说server会掩盖Adapter越界，因此未改第一项。对应生产配置范围由其负责人处理；当前不声明第一项已修复或此集成文件已通过。

| 视角 | 增量后审结论 | 当前证据 |
| --- | --- | --- |
| 架构 | PASS | 第二项使用正式元数据持久化API；第一项保留通用隔离测试，不改名隐藏产品Adapter职责问题。 |
| 技术A | PASS | 真实保存同ID/name后才Native Task注册和跨进程恢复，原权限、范围、预算不变。 |
| 技术B | APPROVE WITH CONDITIONS | 保存回执精确断言已落盘，原全文/分页/压缩/计量断言不删；待Adapter修复后Root重验两项。 |
| 产品 | PASS | 不增加模型回合、作者原话或批准，不伪装未持久化线程可恢复。 |
| UI/UX | PASS | 无UI变更、无额外控件，原错误仍真实保留。 |

零兼容后审：PASS，B04/B05/B06/B10边界保持。fixture当前冻结SHA256 `e502cade2bad6f3ac0cfa04fd3455e344de6debb68ece7179765def3c2eb9cf3`。本代理仍未运行build/test/UI/model；Root原0/2失败日志不删除、不改写。最小验证仍为原文件的两项实际Core集成测试，由Root统一执行。
