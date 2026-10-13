# 当前 Native 构建产物的集成验证

责任：测试装配层；B01/B04/B10。condition/rebind Electron fixture 与 probe 集成测试硬编码 native/target/debug，不能反映显式 NOVEL_CODEX_NATIVE_DIR 指定的本轮构建。复用 tests/helpers/native-binaries.mjs 作为唯一测试产物解析器；删除本地重复平台后缀。不修改产品、权限、预算、来源、事务或断言。真实事实仍由实际启动的 Core/Runtime 拥有；无历史兼容或降级。

## 五视角前审（同一执行代理自审）

|视角|结论与依据|
|---|---|
|架构|APPROVE：仅测试依赖，统一已有 Native 解析边界。|
|技术A|APPROVE：显式绝对目录选择，缺产物启动失败，不静默回退。|
|技术B|APPROVE WITH CONDITIONS：定向真实 condition/rebind/probe 必须使用本轮 /tmp 产物运行。|
|产品|APPROVE：修复验证证据的构建身份，不改变用户流程或降低验收。|
|UI/UX|APPROVE：保留全部真实 UI 行为断言；产品界面无修改。|

验证计划：NOVEL_CODEX_NATIVE_DIR=/tmp/eternallight-boundaries-target/debug，定向运行三个现有集成用例；记录准确结果及失败，不以前次默认路径通过推断本轮通过。

## 后审与实际证据

显式当前目录下三个定向集成测试 3/3 通过，0 skip/0 failure；日志 `/tmp/current-native-three-integration.log`。condition 通过真实 preload/IPC/Native worker、结果重开与来源变更失效；rebind 跨 Runtime 重启完成重绑、建模及下一章规划；probe 通过批准目标与正式评估回执。全部现有断言保留，未替换测试回执或降低合同。平台后缀仍由已有 helper 统一处理，preload 所需 fileURLToPath 保留。

五视角后审：架构、技术A、技术B、产品、UI/UX 均 APPROVE；技术B前置条件已由上述真实运行关闭。B01/B04/B10及零兼容复核 PASS。本次仅测试装配与记录，不改生产；架构聚合门禁由主线程最终验证记录，本记录不把进行中的全套结果作为通过证据。
