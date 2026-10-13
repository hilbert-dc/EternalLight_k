# 工具发现生命周期修复

## 故障与归类
真实任务 generation-d6db3e3374164a09809d9dadfc000ab8：2026-10-11 09:45 UTC read_work_issues 发现并成功调用；09:47:54 / 09:48:02 UTC 两个后续 advisory 同因 not_in_current_request 失败，Native 以 recoveryStalled 停止。代码 lease:start 无条件重建 BridgePolicy，旧测试明确要求每执行代次重新发现。归类：设计缺陷；会话已加载 schema 的生命周期错误绑定执行租约，且必要问题状态入口被当作可选能力延迟加载。领域防重复失败规则保持。

## 责任与方案
Engine 拥有受信发现回执、会话身份和 schema 选择；Native 拥有当前准入目录和作品事实；Host 保持原 Work/费用/执行生命周期。只在 Engine 保留同会话同定义的已加载身份，绝不重用旧 schema/权限。新租约清空子请求计数、未完成发现回执；已有激活缓存保留。目录变化时逐工具核对当前声明与 namespace 元数据，相同声明保留，消失或改变的声明失效；配置改变清空。真正 reset、进程退出和新会话仍清空。小说应用把 read_work_issues 加入必要状态入口，仍与 Native 当前目录取交集，使重启后核验问题状态可读。不引入持久第二权限账本，不从历史文本或旧回执激活，不新增模型循环，不改变恢复次数。

公开合同：ModelBridge lease:start 与 portable discovery schema 选择；NOVEL_TOOL_DISCOVERY_POLICY 初始状态入口。调用方：两个 provider codec、Core 真实 MCP/压缩、小说后续决策。Core 不改。相邻层不保存缓存，不决定文学行动。

|事实|结果|副作用及下一步|
|历史回执但无受信激活|仍未加载|不执行；经现有发现入口|
|同会话新租约、工具声明相同且仍获准|保留加载|只使用当前 schema；正常模型选择|
|其他工具增删/变更|不清空无关已加载项|当前目录仍为权威|
|权限收回/声明改变|相应项失效|不执行旧工具，不能因再次出现自动恢复|
|待完成发现跨租约|丢弃|旧输出不能激活|
|新线程/子任务/会话或完整 reset|不继承|当前初始工具与发现入口|
|应用重启读取问题状态|当前 Native 允许时直接加载|只读；不扩大权限|
|相同失败无进展、暂停、取消|原准入规则|不自动绕过；修复后在原 Work 显式恢复|

## 五视角前审（同一执行者自审）
架构 APPROVE：B01/B04/B06，区分当前授权与 schema 缓存。技术A APPROVE WITH CONDITIONS：必须覆盖撤权、定义变化、未完成回执和线程隔离。技术B APPROVE WITH CONDITIONS：Rust、两种 codec 与真实 Core/MCP 租约接续验证。产品 APPROVE：不增加 token/恢复额度，保留已保存内容与原目标。UI/UX APPROVE：不改 UI，仅通过真实任务状态显示恢复。
零兼容 PASS：无迁移、双写、旧协议兼容。仅可丢弃 fixture 可重建。

## 验证计划与后审
E1 Rust 发现合同；E2 Responses/Chat JSON/SSE；E3 实际 Core + MCP + 压缩 + 新租约；E4 应用初始状态工具合同与 architecture:check；E5 打包与实际原任务恢复。结果待实施后填写；受控测试不等同真实模型长篇完成。

## 实施后复核

证据目录 `.release/tianheng-migration-20261011/discovery-lifecycle-validation/`，源码身份见 source-sha256.json。

- E1 PASS：Rust 9 项；同租约/跨租约、未完成回执清除、定义变化、撤权再出现、其他工具变化、线程/会话隔离、历史伪回执等。
- E2 PASS：6 项，Responses/Chat JSON/SSE 的真实 Bridge lease 边界与当前 schema 校验。
- E3 PASS：实际 Core/MCP 压缩与跨租约继续；独立失败→定向改稿→原观察显式复核→交付的完整 Native/Host/Engine/Core 链路。保留原作者、保护正文、无扩预算、真实观察 resolutionOf 断言。
- E4 PASS：真实生产 MCP 目录/原 schema/Engine 传输的基础工具合同 1 项；租约、取消、续期等相邻回归 35 项；architecture:check 368 文件/7 crates 无诊断；git diff --check 通过。
- E5 打包6门禁 PASS（2026-10-11T10:39:02.827Z），新应用已启动、DeepSeek 连接资格验证已就绪。实际原 Work 仍待恢复：操作期间 Mac 锁屏，CUA 明确要求作者手动解锁，未绕过系统保护、未修改真实数据库、未清空失败/预算。不能将受控验证记作原作品已闭环或100章完成。

测试修订说明：初始白名单同步 read_work_issues；原“每步骤只能bootstrap”的错误假设改为同线程已加载且声明完全相同。旧 fixture 仍要求 save_draft_checkpoint 再发现，但该操作已是既有基础入口，改为验证无冗余发现；独立核验提交工具仍验证真实发现。创作闭环 fixture 使用生产 1M 上下文容量，以避免未实现的压缩模拟越出其验证职责；真实压缩另由 Core/MCP 用例覆盖，原失败日志保留。

五视角后审（同一执行者自审）：架构 APPROVE（权威不变、缓存按作用域和当前声明）；技术A APPROVE（撤权/隔离/未完成回执验证）；技术B APPROVE WITH CONDITIONS（受控链路及发布通过，实际作品续行待解锁）；产品 APPROVE WITH CONDITIONS（原内容保留，未宣称完成100章）；UI/UX APPROVE WITH CONDITIONS（新应用与连接就绪，实际恢复待解锁）。零兼容门禁 PASS；B01–B10 本次影响边界 PASS。无新重试额度、未知写回放或 Core 补丁；恢复停滞保护保持。剩余项：手动解锁后通过原应用授权入口恢复原 Work，读取新回执验证跨失败点推进。

### 解锁后实际复验
新版在 10:41:31–10:41:50 UTC 已完成新的原 Work 协调回合，Native completedChapters 从11变为12，证明原工具缺失阻塞已越过。之后 task-5683513b9f9f4e908d200e2f94c08cab 在模型发送前因执行归属失效而失败，并留下尚未结算的 grant。作者解锁后，通过原对话提交原范围接续，模型登记恢复但正式交接被未结算记录拒绝，未扩大范围/清账。新增同链路结算缺口见 2026-10-11-unstarted-generation-accounting.md，未将其误报为本次工具发现复发。
