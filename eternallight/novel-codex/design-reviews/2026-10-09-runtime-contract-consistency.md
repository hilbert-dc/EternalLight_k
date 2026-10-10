# 当前运行合同一致性

前审：Renderer 已调用 listMessages，preload 尚未暴露；Native 当前 control_revision 字段在旧库缺失，CREATE IF NOT EXISTS 不更新旧表；语言契约更新使资格指纹失效是正常准入。并行任务仍在修改 Host，不能把瞬时源码当稳定发布。

用户明确不做兼容、不协调另一任务。范围：增加真实 preload 与 TypeScript 公共 API 一致性门禁（Host/Renderer B01/B04），当前 schema 在领域存储打开时拒绝不支持结构（Native B03/B06），在全新隔离数据上验证。无旧库回填、删除或迁移；不伪造连接资格；不改官方 Core。门禁不能代替 IPC 业务与真实模型验收。

五视角前审 APPROVE：架构依赖沿公开合同；技术A保证无数据修补副作用；技术B使用实际 preload 导出与类型解析，含故障负例；产品防止半成品进入构建；UX提前明确不兼容结构而不在创作中暴露 SQL。零兼容 PASS。原任务保持未恢复。

后审：五视角 APPROVE，零兼容 PASS。补齐现有 Host 已注册的三个消息 IPC 在 preload 的导出；构建与 npm predev 增加公共 API 一致性检查，保留原 Native 构建前置。门禁覆盖真实类型（含导入接口）与真实 preload 导出，正例/缺失/多余导出三个测试通过。Host 消息合同五项通过；SQLite 当前结构初始化/重开及损坏结构拒绝前无写入两项通过。TypeScript、Vite 构建和 B01 门禁通过（347 文件/7 crates/0诊断），既有 bundle/未使用字段警告保留。

Native 基础表列形状由当前 SCHEMA 的内存参考库派生，进程内缓存，不另存版本真相；现存基础表不符合当前列形状即在初始化前拒绝，新增库按现有 DDL 创建。不承担全部索引/约束或附属表一致性校验，未声称全面 schema 证明。Runtime 现有 main 在监听前打开仓库，因此错误前置到启动。

原作者库只读确认缺 control_revision，按用户要求未迁移、回填、删除或重建。当前运行应用没有重启到本次 Native/Host 构建；另一任务仍在持续改写 Host，未取得统一真实应用验收。连接规则变更导致资格失效为正确保护，未放宽指纹或复制隔离证据。语言隔离实测结果沿用原记录。源码回归完成不等于原任务恢复。日志：/tmp/current-contracts-final.log、/tmp/current-schema-tests.log、/tmp/contracts-build.log。

## 后续验收记录（2026-10-09）

`npm run predev` 全链路完成，Engine 和 Native 当前二进制已重建；TypeScript 与 preload API 门禁通过，B01 再次扫描 347 文件、7 crates，零诊断。聚焦 UI/契约组合测试 11 项中 10 项通过，`multi-conversation-ui` 等待 `.transcript-error button` 失败，不能计为通过；验收期间其共享 fixture 又发生修改，尚未形成统一稳定快照。证据：`/tmp/acceptance-current-build.log`、`/tmp/acceptance-current-ui.log`。

已在 `.certification-reports/current-contract-acceptance-20261009/user-data` 建立独立当前版本环境，仅复用已有作者会话，不复制作品、账本或模型资格。端口 5184 的新实例启动成功，Runtime 监听正常；该实例仍需真实模型资格验证和新作品对话验收。原端口 5173 与《潮汐邮局》数据不变。操作工具仍选中旧窗口，已请求作者将新空白窗口置前；在确认窗口前不发送创作指令。代码验证、应用启动、真实模型创作和原 20 章完成分别记录，后两项未完成。

### 实际应用发现的配置断链

新实例真实点击模型验证后，`agent:configure` 抛出 `ReferenceError: agentStorage is not defined`。原因是 Host 从全局状态改为执行域上下文后，构造模型子进程环境的对象展开仍保留旧变量名；语法检查不能发现这种未绑定标识符。前审五视角 APPROVE：职责仍归 Host，统一使用当前执行域存储，保留既有凭据/隔离路径构造；不改模型、Native 或 Core，不迁移作者数据。验证需包含未绑定引用检查及实际应用模型连接成功，不能只靠节点语法检查。

后审：配置环境使用 `executionContext().agentStorage` 后，新增 `electron/tests/host-bindings.test.mjs` 以 TypeScript 语义解析验证生产 Host 未绑定变量，并注入不存在的存储标识符作故障负例，两项通过。该检查覆盖未绑定名称，不声称完整 Host 类型检查。五视角对此定点修复 APPROVE；仍需统一验收。

实际应用已切换到 5184，新作品《雾港来信》持久创建成功，重启后保留。界面通过现有真实连接提交一次“生成第1章，约800字”，原文未追加。应用仅登记 `generation-proposal-4ec853ea7861417ba4f0161b7c49a718`，状态 proposed，未消费、无章节正文；conversation Work 显示 completed，但 `work-event-75c1b7d291374ddda59b67895ef15eec` 的 attemptSettled 事件持续 pending，Host 自动接续反复报 invalid_request。故单章端到端验收失败，不能将 conversation 完成视为作品交付。界面仍有“确认范围与额度”的模型答复，需继续排查指令/授权/接续链的实际职责。

补跑模型连接可靠性 4 项中 3 项通过，1 项失败：`agent:resume-work-task` 在连接门禁阻断时进入 executionIdentity 的 Runtime 读取，预期边界未满足。此失败与仍在并行变动的执行域改造关联，尚未修复；证据 `/tmp/acceptance-model-connection-regression.log`。本次未放宽门禁、伪造回执、迁移原库或再次发送创作指令。已停止本任务先前启动的旧开发实例以消除同名窗口冲突，原作品文件保留。
