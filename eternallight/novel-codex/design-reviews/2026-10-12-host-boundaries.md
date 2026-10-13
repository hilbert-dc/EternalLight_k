# Host 职责与类型覆盖（2026-10-12）

## 方案与边界（实施前）
Host 负责受信 IPC、配置与进程生命周期（B01、B04、B06）；Native 保持领域事实权威，Engine 保持通用执行权威。将 main 的 IPC 注册按配置、会话控制、创作调度归组，注入公开依赖，不改变 sender 校验和执行身份绑定。所有 electron 生产 JS 开启 @ts-check，用类型推导及具体合同修复诊断；保留 Engine 传递源文件原有检查策略，不借 any、ignore 或降级屏蔽 Host 问题。新增覆盖检查避免后来文件遗漏。

公开 IPC、Runtime 和 Engine 合同及 UI 行为保持当前定义；无领域/schema/授权/预算修改，无历史兼容层。类型收敛不更改未知结果为成功。取消、重复、晚到事件继续由既有身份与生命周期机制隔离。已有未提交修改在原内容上增量编辑。

## 五视角前审（同一代理自审）
|视角|结论与依据|
|---|---|
|架构|APPROVE：注册与实现按 Host 职责拆分，依赖保持公开边界。|
|技术 A|APPROVE WITH CONDITIONS：必须验证取消、事件归属与配置回滚。|
|技术 B|APPROVE WITH CONDITIONS：Host 全覆盖检查及 tsc 均通过，无检查抑制。|
|产品|APPROVE：不增加作者操作或改变创作准入。|
|UI/UX|APPROVE：保留状态事件、恢复错误及现有入口。|

## 验证计划
E1：typecheck:host 与生产文件覆盖门禁；E2：相关生命周期、配置、IPC 测试；E3：architecture:check。静态及受控测试不冒称真实供应商/完整桌面验收。后审完成后补实际证据。

## 实施与后审

- 26 个原未检查生产文件加入 `@ts-check`。构造器的动态批量赋值改显式字段初始化；补充真实可选参数、回调、错误扩展和进程环境类型。没有 `any`、ts-ignore、ts-nocheck 或放宽已有检查；ES2024 lib 对应已经使用的 Map.groupBy。
- 所有 43 处业务 IPC 注册移入 workspace-observation、model-configuration、session-control、creative-execution 四个模块。注册仍统一经过 sender 校验、执行请求包装及 ExecutionDomains 绑定，失败仍撤销此前注册。可变配置通过显式 getter/setter 保持实时绑定，不捕获陈旧值。
- `DesktopLifecycle` 显式注入 Electron、窗口、可信注册器和初始化/清理依赖，拥有退出确认、幂等启动和清理状态。提前退出后不创建晚到窗口；启动错误先清理再退出。main 从 2680 降至 2091 行，仍负责产品执行用例组装，不以机械搬行为目标。
- 生产检查覆盖全部 111 个 Host JS，递归扫描且排除测试；门禁拒绝缺少 opt-in 及诊断抑制。`typecheck:host` 已接线（工程代理）。Engine 仅三个公开 JS 参数签名与真实可选语义对齐，无执行逻辑修改。

|证据|实际结果与范围|
|---|---|
|E1|`npm run typecheck:host` 通过；111 文件无遗漏。覆盖门禁负例测试 1/1 通过。|
|E2|17 组 Host 回归 218/218 通过：真实 Adapter 进程与并行 IPC、取消、配置回滚、事件租约、消息、媒体服务及启动；DesktopLifecycle 四个新增回归均通过。首轮沙箱 localhost 监听 EPERM，批准沙箱外重跑后通过。|
|E3|`npm run architecture:check` 在 DesktopLifecycle 与 Core 子域调整后复查通过；714 文件、7 crate 无诊断。|
|E4|使用本轮构建的 Runtime/MCP binary，4/4 真实联调通过：两种跨进程重启恢复、MCP 工具列表/鉴权转发、生产 provider discovery；日志 `/tmp/host-real-native.log`。|

五视角后审（同一代理自审）：架构 APPROVE，职责仍在 Host、没有复制领域事实；技术A APPROVE（E4 实际联调通过），E2 证明取消及并行资源隔离；技术B APPROVE（E4 实际联调通过），E1 和实际生产 handler harness 证明类型与注册切换；产品 APPROVE，没有新增作者确认或调整创作准入；UI/UX APPROVE，关闭确认/取消及错误投影保留。零兼容门禁 PASS；B01/B04/B06/B08 范围内无新增越界。公开 DTO 不在本项独立复制。
