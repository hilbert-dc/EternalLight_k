# 已读工作区证据的事实与读取权限分离

## 变更概览与模块归属

实际第 2 章检查任务已读取同 Work 的 writer 保存版本。当前稿、前文候选 head、critic captured source digest 与 Run source digest 都一致，作者身份及要求也有效。Work 在检查失败后变为 blocked，而 `task_workspace::ref_current` 对 nativeSavedCheckpoint 仍要求 Work 为 running，导致 `dependencies_current` 为 false，`assert_task_inputs_current` 返回 TaskInputStale，进而阻止 `/retry-failed-task`。这是把活跃读取权限混入已读证据事实的循环阻断，不是正文来源或作者范围变化。

修复限 Native Story Core 私有来源验证、现有测试与本审查文档；B04 负责真实输入/版本/来源事实，B03/B06 的工作生命周期、活跃执行、预算与恢复控制继续负责能否读取或运行。B01/B10 要求共用验证与全部调用方一次性明确用途。不修改 Host、Engine、Renderer、MCP 公共 schema、预算、Run 摘要 guard、当前稿/批次、作者要求或数据库。

## 目标合同与实施前方案

将 `ref_current` 分为私有 ActiveRead 与 RecordedDependency 两种用途。`materialize_workspace_source` 与 `frozen_comparison` 使用 ActiveRead，仍要求 nativeSavedCheckpoint/nativePlanningReport 的 Work 正在 running；`dependencies_current` 只验证已经记录读取的来源，其不可变保存出处不因 Work 的 paused/blocked/completed/cancelled 状态而消失。无论哪个用途，同项目、同 Work、真实根线程、producer/reader 顺序、reader currentAttempt、精确 output ordinal/revision/object/kind、task.output_refs 与模型输入合同的全部现有条件保持。

RecordedDependency 只由既读依赖校验使用，不能列出/读取新的来源，不能授予工具权限、恢复任务、启动模型、申请预算、替换当前稿或关闭检查。事实有效与执行授权分离：completed 的报告仍应能验证已读不可变事实；cancelled 的历史事实也不能被删除，但取消任务继续禁止新读取与执行。公开 source_task / validate_active_task / workspace_page、严格 running 的预算授权和原 Retry/next/Work 状态门禁全部不改。当前稿 candidate、knowledge batch 和可选已读前文的现行 head 新鲜度继续单独检查。

模型仍决定如何纠正检查来源和文学判断；工程不重生成正文、不自动选择创作流水线、不回填任何数据。原 Work、保存正文和累计用量保持，由 Root 通过正常 UI 重试并验收真实结果。

## 五视角前审

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构专家 | APPROVE | Native 的来源事实与活动生命周期分离；只有私有验证用途，没有第二权限层或公共新 API。 |
| 技术专家 A | APPROVE WITH CONDITIONS | 两个不可变保存出处分支只在 RecordedDependency 中解除 status 条件；全部身份/出处/currentAttempt 条件及动态来源 head guard 保留。取消与结束不能授予读取或执行。 |
| 技术专家 B | APPROVE WITH CONDITIONS | 在现有真实 Native 测试覆盖 paused/blocked/completed/cancelled 既读事实仍有效而 active materialize 拒绝，以及跨 Work、伪出处/currentAttempt、当前稿变化拒绝；Root 唯一构建、测试和实机执行。 |
| 产品经理 | APPROVE | 一个 Retry 能沿原 Work 纠正检查，原保存与用量继续真实；不要求作者替应用组织技术恢复。 |
| UI/UX 设计师 | APPROVE | 无新界面，现有重试行为仍以真实状态和成果回执展示；事实修复不产生虚假“已完成”。 |

综合决策：按条件实施。零兼容门禁 PASS：无版本分支、迁移、回填、双轨、fallback 或自动补旧数据；本次是当前 schema 下的事实校验修复。

## 验证及实施后复核

实际变更仅为 `task_workspace.rs` 的私有 ReferenceValidation 及全部内部调用方：RecordedDependency 只用于 `dependencies_current`；两个 Native immutable 来源分支以该用途区别事实验证和 running 读取条件；materialize 与 frozen_comparison 明确 ActiveRead。其余 SQL、当前稿/批次/作者来源、生命周期与预算条件均未修改，无公共 API、跨层新合同或数据回填。

| 真实失败 → 责任模块 | 定向回归 | 实际再验与证据层级 |
| --- | --- | --- |
| 正常 UI Retry 返回 TaskInputStale；当前稿/来源摘要相同，只有已读 nativeSavedCheckpoint 被 Work blocked 状态拒绝。证据 `/tmp/autonomous-v3-current8-chapter2-retry-diagnosis.json`。B04 的 task_workspace 既读来源事实校验负责修复。 | 既有 Core 测试文件新增 `recorded_saved_checkpoint_facts_survive_work_stops_without_granting_read_authority`。真实 Native 保存、同 Work 决策、读取之后，对 paused/blocked/completed/cancelled 检查既读事实仍真而 ActiveRead、公开缓存读取及目录拒绝，且 heads/budget 不变；真实其他 Work、伪 producer/ordinal、替换 currentAttempt、根身份变化和另行读取的 candidate head 变化继续拒绝。 | Core fixture 只证明来源与权限合同，四种生命周期状态是显式状态种子；不冒充完整 Runtime/模型恢复。Root 在已保存第 2 章原 Work 上以原 UI Retry 再验，确认正文/累计用量保留、检查能重新组织、结果真实保存与最终交付。 |
| 同一错误也可能影响已结束 Work 的 savedReport 事实查询。B04 共用 Native immutable 分支需保持语义一致。 | 既有 `real_planning_report_store_keeps_cancelled_saved_analysis_readable_to_same_work_decision_only` 的真实报告存储/读取用例追加上述四状态事实与 ActiveRead 拒绝断言，随后恢复测试原状态继续出处负例。 | 完成/取消不会取消历史事实，也不授权新的 source materialize、预算或执行；相关 Runtime 未决写入/预算与原真实恢复路径由 Root 验证。 |

| 视角 | 静态后审 | 依据与限制 |
| --- | --- | --- |
| 架构专家 | APPROVE | 事实、读取与执行职责已分离，无第二权限来源或文学流程变化。 |
| 技术专家 A | APPROVE | RecordedDependency 无生命周期条件，但全部真实身份/出处/currentAttempt guard 保留；ActiveRead 与公开权限 guard 不变，completed/cancelled 事实不授予恢复权。 |
| 技术专家 B | APPROVE WITH CONDITIONS | 原失败、两个不可变来源分支及主要出处/当前稿负例均有现有文件中的 meaningful 回归；Root 独占构建、测试、实机，不以 fixture 代替实际恢复验收。 |
| 产品经理 | APPROVE | 修复原 Work 的恢复阻断，保留草稿和用量；模型依真实反馈重新决定检查，不要求作者替应用推进。 |
| UI/UX 设计师 | APPROVE | 不新建界面，重试与成果展示继续依真实回执；不会把事实有效误称已完成。 |

独立只读静态复核 PASS，无必须修复项；`git diff --check` 静态通过。零兼容及 B01–B10 边界复查 PASS。生产与测试最终冻结，Root 已启动全 Core lib 与既有相关 Runtime 合同，实际退出码及原 UI 再验结果尚由 Root 确认，本子任务未运行构建/测试/应用/模型。

冻结 SHA256：`task_workspace.rs` 为 `d4a0235ba0f7ad3d086b98b4723bb642678d7445ec98b0e1ca231d2ef850b52c`，`task_workspace_output_tests.rs` 为 `2244b72829f9dffd8cd6e5af07b7199c5b245773ba89ce0cacce627ee08a3c1f`。最小定向命令为 `cargo test --offline --locked --manifest-path native/Cargo.toml -p novel-codex-story-core --lib task_workspace::`；真实恢复成功与交付只能由实际成果和回执证明。

## Root实际验证与应用恢复

当前Native build实际exit0，53.50秒。完整Core266/266实际exit0；其编译排队期间测试另补既有planning-report停止状态断言，因此最终测试SHA冻结后再编译运行对应工作区全部13项，13/13实际exit0，未冒用旧测试身份。Runtime三个完整目标实际exit0：checkpoint retry3/3、provider budget4/4、report prewrite5/5，保留外部修改、未结预算、真实accepted未知写入与空/非空作者遗漏等拒绝。Host相关既有合同16/16、exit0；architecture332文件/7crate/0诊断、exit0。实际产物、日志SHA、退出结果与源码限制见 `/tmp/autonomous-v3-current8-workspace-facts-build-proof.json`。独立静态后审APPROVE，无must-fix；五视角工程合同条件已满足，不代表整个创作任务已交付。

重启前原profile无活动Task/Engine reservation、历史unknown112,525保留。沿原应用正常重启，原第2章正文和作者记录保留，点击既有“重试当前任务”实际进入“正在继续任务”。`/tmp/autonomous-v3-current8-chapter2-retry-started.json`：Task12（原11）、Work仍4、章仍2、正文/报告版本仍3、作者记录仍4；原Work登记真实后继Attempt和provider grant，没有重复作者输入、新Work、新章或外部改稿。模型是否自行纠正报告、当前检查是否通过与最终交付仍待观察，不能仅凭成功启动宣布闭环。

随后真实任务终态已核对：新的检查Attempt正常运行，15次DeepSeek请求/406591已知tokens/0新增unknown/0活动预留，原Work累计用量保留。模型自行改成空的作者遗漏列表，报告仍按原四项标准提交，未重写正文。报告首次缺writerTaskId被schema拒绝，模型纠正后又两次提交字符串 `"null"` 候选版本；通用候选校验误回TaskInputStale且缺精确拒绝证明，两条调用accepted无回执。任务最终failed、Work blocked；章数、正文版本和原作者记录不变。见 `/tmp/autonomous-v3-current8-chapter2-retry-terminal.json` 和 `/tmp/autonomous-v3-current8-chapter2-report-argument-evidence.json`。恢复准入故障已在原应用路径证明修复；后续报告保存与整章交付仍未闭环，新故障单独归入Native候选保存合同，不放宽本次任何来源、权限或未知写边界。
