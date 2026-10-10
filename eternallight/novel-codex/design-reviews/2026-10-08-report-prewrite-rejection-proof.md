# 独立报告预写拒绝的真实回执

## 变更与责任（实施前）

实机样本 `task-1c1826c2a6214d778f124354b0467b8a` 的两次报告提交在 `prepare_revision_outcome_payload` 中确定拒绝，却发生在 Runtime `recovery_binding::bind` 之前。MCP 只能查询真实 Native ledger，不能凭 HTTP 400 产生 rejected proof。证据见 `/tmp/autonomous-v3-current4-critic-technical-evidence-20261008.json`；原失败及 61,961 unknown provider tokens 保留。

本次仅修改 Runtime 报告提交的绑定顺序与 Core 现有恢复模块的窄领域包装，另新增专项 HTTP / Core 单元测试。认证、当前任务/来源、root identity、路由绑定和幂等键先验证，再绑定真实调用；Core 包装执行原报告预校验，将确定的类型化预写拒绝交现有 `settle_prewrite_rejection` 登记。MCP/Host 无新事实权威，无新接口、数据库、预算、迁移、fallback 或自动重试。类型化报告错误由另一独立补丁提供，按当前明确 variant 同步，不猜字符串。

公开合同仍为现有 `submit_revision_outcome` HTTP 与精确 recovery-operation 查询。唯一领域回执存储是 Native `story_tool_operations_v1`；Engine provider ledger 保留实际已知/未知费用，拒绝写回执不代表 provider 未调用、费用已知、Task 完成或作品交付。模型仍选择评估及纠正内容。

适用 B01、B03–B06、B10：Core→Runtime 的原公开调用方向不变；B02/B07/B09 不新增创作流程、方法或内核行为；B08 不修改 UI，锁屏 AX 缓存不能独立证明缺失刷新。

## 零兼容与五视角预审

| 视角 | 结论 | 核对 |
| --- | --- | --- |
| 架构专家 | APPROVE WITH CONDITIONS | Runtime 仅认证/绑定与调用，Core 在原恢复存储登记；不暴露通用任意结果结算 API。 |
| 技术专家 A | APPROVE WITH CONDITIONS | 只有已证明无业务写入的预校验可结算；Storage/CorruptState/提交不确定保持 accepted/unknown，身份失败不登记。 |
| 技术专家 B | APPROVE WITH CONDITIONS | 新 HTTP 测试证明 rejected 精确身份与原状态不变、同任务新调用可纠正；Core 负例证明未知错误不伪拒绝。Root 唯一运行构建/测试。 |
| 产品经理 | APPROVE | 修复应用自主纠错所需的可核对事实；不替作者改稿、不恢复原未知费用样本。 |
| UI/UX 设计师 | APPROVE | 回执表达“本次未保存”，不冒称成功或清除错误；本次无 UI 改动。 |

综合决定：按上述条件实施。无兼容例外。旧样本保留，测试从当前 schema 创建。

## 预定验收

- 真实报告 criterion mismatch：精确 rejected operation 可查询，零 revision/issues/output/Task 状态/Work 用量改变；新 callId 的正确报告仍可保存，receipt committed。
- 外部 caller、无效 capability、stale revision 和错误路由：原权限/版本错误不变，不生成该调用的 ledger entry。
- 实际保存事务存储失败：零业务写入，操作仍 accepted，不能取得 rejected/committed proof。
- Core 类型化 validation 可结算；Storage、CorruptState、成功值与已经 committed 的回执不被改写；不清 Engine unknown tokens。

## 实施后复核

实际变更：Runtime 只移动现有绑定调用，并改用 Core 窄 `prepare_revision_outcome_payload_with_recovery`；Core 包装原纯预校验并沿现有 settlement 存储。明确类型化 `PayloadValidation` 纳入确定拒绝，不以状态码或错误字符串认定。没有改业务报告 validator、schema、Task/Work 完成路径、Engine费用、Host 或 UI。

新增一个 Core 单元用例覆盖明确 validation、Storage、CorruptState 与成功值；新增三个实际 Router/SQLite HTTP 用例覆盖真实报告拒绝及新调用纠正、身份/capability/版本/路由负例、保存事务 trigger 失败后的 accepted。HTTP fixture 复用当前显式 16 请求/200,000 tokens 原 Work 授权、原作者来源完整阅读与公开 Task/保存接口。没有外部供应商请求、输出伪造、权限 SQL 修补或减少原断言。

| 实施后视角 | 静态结论 | 依据及实际证据边界 |
| --- | --- | --- |
| 架构专家 | PASS（待实际验收） | 原 Core/Runtime 公开边界；包装只产生真实预校验结果，未开放任意结算入口。B01/B03/B06 保持。 |
| 技术专家 A | PASS（待实际验收） | bind 前保持当前来源/权限/root/路由/幂等检查；业务存储失败仍 accepted，provider unknown 未被引用或改写。 |
| 技术专家 B | PASS（待实际验收） | 正负合同已静态核对；Root 唯一构建/执行，尚无新测试实际结果。原实机失败证据保留。 |
| 产品经理 | PASS（待实际验收） | 本次拒绝只表示没有保存报告；不宣布交付、不自动恢复费用未知的失败样本。 |
| UI/UX 设计师 | PASS（待实际验收） | 本次未改 UI；当前仅 AX 旧展示不足以证明 missing-refresh，不扩展修改。 |

零兼容门禁：静态 PASS，无版本判断/旧字段/双读写/迁移/回填/吞错。B01–B10 归属静态 PASS；`architecture:check` 与行为验收均由 Root 待跑。静态/合同证据不证明真实 DeepSeek 文学交付完成。

冻结生产及测试 SHA-256：

- `native/story-runtime/src/revision_outcome_api.rs`：`c647edcc6cb64a1677c8ed9979315a403ebcf75f50a4cb11f9129cdd8544ef01`。
- `native/story-core/src/recovery_operations.rs`：`34f458dd8423342572f3322183a961dc1e6d17238bedd152549fa0197df1bbb4`。
- `native/story-runtime/tests/report_prewrite_recovery_http_tests.rs`：`91ea857ed24324d2b6dc5e9fa3b076f2a98bbc3305dba6f1c50fbde7c328d1b1`。

Root 必需定向入口：Core `--lib report_prewrite_rejection_tests`（1 项）；Runtime `--test report_prewrite_recovery_http_tests`（3 项完整断言）；与当前独立 criterion-contract 补丁共同构建并验收。未启动构建/测试/应用/模型。原样本 61,961 unknown tokens 仍是独立停止条件。

### Root 实际验收后的复核

当前Native构建实际exit0、Core完整lib265/265实际exit0（包含上述新增单元），3项进程内真实Router/SQLite回执测试全部通过。联合cargo因另一新增测试误读决策摘要曾exit101，原日志保留；仅修正该fixture读取后，报告全目标40/40实际exit0。新增单元不直接构造committed后再次settlement，已提交回执不覆盖由原条件及回执测试支持，不扩大声明。架构检查332文件/7crate、0诊断；该修复生产源码与冻结清单一致。完整证据见当前实施验收记录。

五视角后审：架构 PASS（恢复记录仍归Native）；技术A PASS（身份失败不登记，确定拒绝精确回执，实际存储失败accepted）；技术B PASS（真实Router、SQLite事务trigger及同任务新调用纠正，不伪称外部网络）；产品 PASS（没有恢复旧样本或清掉61,961 unknown tokens）；UI/UX PASS（未改界面或让作者处理技术字段）。独立只读复核未发现必须修复问题。样本2保持blocked，样本3在独立新作品验收；真实模型最终交付尚无通过声明。
