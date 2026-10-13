# 工作区可见范围的结构化事实

前审证据：第51章writer回执声明搜不到简报，并搜索brief/plan/outline/critique；同Work协调任务能读取简报。context_read_api对writer隐藏完整规划，task_workspace的Scope.artifacts=false也一致隐藏。这不是数据丢失，而是目录合同只报告空结果、未明确返回适用角色范围，造成模型将不可见混同不存在。

责任：B03/B04 Native唯一可见性权威。目录响应增加visibility字段，直接取本次查询已计算的Scope：planningArtifacts可见策略、正文起止章/场景、搜索模式与existenceOutsideScope=unknown。规划可见仅指允许进入目录，仍受冻结输入/Work/范围约束；不声称对象存在，不返回隐藏数量/标题/正文。不修改权限、正文、账本、模型策略或提示词，不跳过测试，不新发付费请求。

五视角自审：架构APPROVE（复用权限事实不另建策略）；技术A APPROVE WITH CONDITIONS（writer/非writer对照）；技术B APPROVE WITH CONDITIONS（秘密不泄露、精确读取合同不变）；产品APPROVE（可见性与不存在可区分）；UI/UX APPROVE（无需作者判断内部来源）。零兼容PASS；无schema和迁移。实施后须记录测试与部署，真实节省比例不预先声称。

实施：task_workspace_page直接投影本次Scope，新增visibility，不修改SQL准入条件。原真实日志确认第51章存在5次规划目录空读（brief/plan/outline/critique及brief关键词），证据.release/action-selection-diagnostics/visibility-live-evidence.json；不能把这些请求所在全部token认定为可节省。新增writer隐藏正文/规划秘密不泄露断言和非writer实际HTTP范围回执断言；架构门禁370文件/7crates通过。

Native实际回归14/14通过：writer排除规划资料、POV范围、未来章/后缀隐藏、跨任务拒绝、冻结输入与多保存版本、取消任务、目录游标、整章阅读证据均保留；测试日志.release/action-selection-diagnostics/workspace-tests.log。HTTP与部署仍待完成。

HTTP回归1/1通过，非writer目录明确返回eligibleWithinTaskScope及范围外存在性unknown；精确版本定位、跨来源拒绝、子页不伪造根阅读凭证的既有断言均通过。五视角后审代码与合同APPROVE；上线和真实行为观察仍待完成。

上线记录：2026-10-12T09:44:25Z本地packageRelease六项门禁全部通过（staging/runtime-verification/frontend-build/packaging/packaged-resources/application-smoke）。包内runtime与本次构建SHA256一致，证据.release/action-selection-diagnostics/deployed-runtime.json。应用已重启，52章版本及正文摘要逐一完全一致（drafts-before/after.json）。第52章完整稿待独立核验，模型连接需重新验证，当前正在原连接执行必要验证；未增加创作范围或重复产稿。

最终后审：2026-10-12T10:13:03Z原generation恢复running、version768；原Work仍running，接续advisory task-50af58f2a1f8422caf3a0de8669894c6，sourceTask为已完成的第52章writer，不重新起草。连接已重新验证并应用；应用显示执行中。保存52章，已核验完成51章，第52章待独立检查。五视角后审APPROVE：错误与可见性事实由Native给出，原权限/CAS/来源隔离/正文/作者标准/账本保持，必要回归与6项打包检查通过。真实整章省token比例尚无新完整样本，不声称已达成；未恢复定时自动化。
