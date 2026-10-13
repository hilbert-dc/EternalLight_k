# 动作选择错误的精确反馈

前审：第49章准备任务 task-71671fc673514675a3f5b78afbabbabc 将当前 actionKey 多写一个字符，Story Core 未找到动作却返回 TaskOutputForbidden，Runtime 指导 review_authorized_scope。随后模型查计划、能力、状态并两次提交未完成任务。最终原标识纠正后成功；不是已证实授权缺失。证据 .release/token-audit/action-key-root-cause.json；拒绝后到任务结束17次请求1514296 tokens包含成功提交，不能全部判为可消除浪费。

根因：动作选择诊断的实现缺陷；当前严格身份选择与CAS设计保留。Story Core拥有当前可用动作事实，使用既有PayloadValidation公开错误合同，返回work_action_unavailable、/actionKey及read_work_status读取入口，说明按当前readyActions的精确键选择。不得模糊匹配、自动选择或扩权，不增加提示词，不改变工具入参和持久schema。动作不存在只在任务绑定、来源/CAS及准入检查通过之后判定；不会声称必然是拼写错误（也可能当前动作不再可用）。Runtime/MCP沿已有错误传输；Host不自动重试，模型决定如何纠正。

验证：公开HTTP真实生成任务，单字符错误拒绝及明确定位；工作版本/目录/账本不变；当前精确动作能执行；旧CAS及未授权请求继续拒绝。MCP错误传输回归与架构门禁。生产更新须安全暂停并保留原稿，完成后恢复原任务，不人为注入失败、不为测试新发模型请求。

五视角自审（同一执行者）：架构APPROVE（B03/B04，Native唯一动作权威）；技术A APPROVE WITH CONDITIONS（CAS、无写入、精确纠正）；技术B APPROVE WITH CONDITIONS（真实HTTP和传输）；产品APPROVE（避免无效权限调查、不降标准）；UI/UX APPROVE（无需作者处理内部标识）。后审待实际验证。零兼容PASS，不新增迁移/双轨/旧版本支持。

| 当前权威事实 | 结果 | 副作用/后续 |
| --- | --- | --- |
| 绑定及CAS有效，但键不在当前动作集合 | work_action_unavailable，定位/actionKey | 不选择/派发；模型读取当前动作再决定 |
| 当前动作键正确，其他准入有效 | 原有选择事务 | 原有版本、预算、回执处理 |
| 绑定或CAS失效，即使键也有误 | 保留原绑定/版本拒绝 | 不泄露当前动作，不派发 |
| 提案本身越权 | 原授权拒绝 | 不自动扩大权限或生成替代动作 |

长标识仍是精确对象选择合同，不因单次抄写错误自动替换为模糊选择或改变所有调用方；本次消除的是错误类别丢失导致的恢复绕路。是否缩短标识需独立的碰撞、绑定和实际输入错误证据，不能冒充本次已实现效果。

实施：仅替换Story Core当前动作集合精确匹配失败的诊断；复用PayloadValidation传输，不改变任何准入顺序。增加公开HTTP单字符误写、零选择、正确重试与CAS优先测试；MCP实际JSON-RPC错误与持久操作证明透传用例追加该错误形状。静态架构370文件/7crates通过，目标文件diff检查通过。动态测试构建遇到本机宏依赖dlopen/fcntl校验等待，采样保存在.release/action-selection-diagnostics/compiler-wait.txt；动态验证与上线待完成。

回归发现与修正：完整HTTP组12通过/1旧断言失败，runningTask分支实际上选的是已完成父任务的事件，其后继已存在；当前claim_work_execution_event明确先确认已交接通知，再判断新执行准入。因此settled+无lease合法，不能继续期待activeAttempt。fixture只同步此分支：验证事件consumed、所有其他Native表及Engine账本逐行不变、重复claim幂等、decision仍workNotReady且无新step。其他权限/未知写入/费用负例保持。未修改生产恢复逻辑，不将该旧断言记为新动作诊断造成的故障。

实际验证：修正旧fixture后的完整work_decision_http_tests 13/13通过；实际MCP JSON-RPC合同回归1/1通过（包含新错误与持久证明）；架构门禁通过。原任务于第52章完整稿保存后按已授权维护安全暂停，52章版本/摘要保存至.release/action-selection-diagnostics/drafts-before.json，待更新后比对。

上线记录：2026-10-12T09:44:25Z本地packageRelease六项门禁全部通过（staging/runtime-verification/frontend-build/packaging/packaged-resources/application-smoke）。包内runtime与本次构建SHA256一致，证据.release/action-selection-diagnostics/deployed-runtime.json。应用已重启，52章版本及正文摘要逐一完全一致（drafts-before/after.json）。第52章完整稿待独立核验，模型连接需重新验证，当前正在原连接执行必要验证；未增加创作范围或重复产稿。

最终后审：2026-10-12T10:13:03Z原generation恢复running、version768；原Work仍running，接续advisory task-50af58f2a1f8422caf3a0de8669894c6，sourceTask为已完成的第52章writer，不重新起草。连接已重新验证并应用；应用显示执行中。保存52章，已核验完成51章，第52章待独立检查。五视角后审APPROVE：错误与可见性事实由Native给出，原权限/CAS/来源隔离/正文/作者标准/账本保持，必要回归与6项打包检查通过。真实整章省token比例尚无新完整样本，不声称已达成；未恢复定时自动化。
