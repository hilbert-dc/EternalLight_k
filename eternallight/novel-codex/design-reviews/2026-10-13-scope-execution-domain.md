# 范围确认执行域与现行测试合同

## 前审
Native 拥有 source task→Work/thread 与许可事实；Host 确认范围后可执行候选，故必须纳入与 approveAction 一致的 ExecutionRequests/ExecutionDomains，不能在base域共享适配器。旧全局互斥测试与现有多对话并行设计不符，但同原Work冲突和可停止身份仍必须保护。preload 改 invokeExecution，Host从Native读取原task owner，校验project/work/thread再进入域；Renderer不能任意传域。确认读/预览完成后、许可写后再次检查原flow停止，已提交许可不回滚、不派发后继。

|事实|结果|副作用|
|---|---|---|
|同Native thread有执行|拒绝范围确认/发送|无许可写或模型派发|
|独立thread|允许独立域|各自预算/停止身份|
|读许可期间停止原request|TASK_CANCELLED|不POST确认、不派发|
|确认写已提交后停止|保留许可事实，停止派发|不制造回滚|

同期测试fixture只补现行合同：planning-review从冻结Native authorIntent及source binding验证作者要求，内部触发用分页来源描述；补精确task GET/PATCH状态模拟。revision-outcome对每个真实domain adapter spy，不再仅root adapter；普通对话新增mediaBudget精确验证，repair不误带媒体预算。保留预算/权限/作者意图与未知终态断言。

五视角前审：架构 APPROVE；技术A APPROVE WITH CONDITIONS（同域/独立域/停止）；技术B APPROVE WITH CONDITIONS（四文件定向+preload/typecheck/architecture）；产品 APPROVE（并行不扩大权限）；UI/UX APPROVE（原停止身份可用，无新增确认）。

## 后审
范围确认已与preload一起接入ExecutionRequests和ExecutionDomains。原Work/thread从Native task/work-task取得并校验project；flow从已验证domain继承workId，停止发生在许可写入期间也能暂停原Work。不会回滚已提交许可，不会晚到后继续派发。

7个完整测试文件57/57通过（/tmp/host-scope-final.log）：同thread拒绝发生在conversation/task写前；独立thread在原确认未释放前完成；跨sender停止拒绝；读期间停止零许可写；提交期间停止仅一次确认和一次原Work pause、无新task/decision；preload传原参数和可停止requestId；foreign Native owner拒绝。规划评审5例及修复/后继来源原断言均保留，spy覆盖每个实际domain adapter。

check:preload-api无missing/undeclared；typecheck:host覆盖124文件、无问题；architecture:check无诊断；diffcheck通过（/tmp/host-scope-{preload,types,architecture}.log）。五视角后审均APPROVE，条件关闭。
