# Host composition root 收敛

## 前审方案
main 当前兼有传输/Engine turn/Work协调/创作与规划执行/启动资源管理。按真实能力切分成多个工厂，每个工厂通过明确列出的依赖访问器获取所需能力，内部实现拥有完整职责，main只创建共享资源并接线。跨模块循环仅通过延迟能力访问解决，不复制状态、不用通用服务定位器或整main搬家。生产模块全部@ts-check，不增加检查抑制或any。业务IPC接口及可信renderer/执行身份包装原样保留。

五视角（同一代理自审）：架构 APPROVE WITH CONDITIONS（窄职责模块无Godmodule，main保留组装）；技术A APPROVE WITH CONDITIONS（取消、关闭、失败结算与来源绑定回归）；技术B APPROVE WITH CONDITIONS（全Host类型、架构门禁、生产模块在handler harness下的真实Adapter回归）；产品 APPROVE（无操作/授权变化）；UI/UX APPROVE（界面和状态事件合同不变）。

验证计划：Host全类型覆盖+tsc；现行生命周期/并行IPC/配置回滚/执行恢复组；已有真实Runtime/MCP二进制联调。先完成定向再按集成决定完整Host；不声称真实付费模型或作者手动验收。

## 实施后

main 从本轮开始的 2091 行收敛为 394 行资源创建与能力接线；13 个模块分别承担窗口信任/导航、Runtime进程与HTTP、模型传输、turn/恢复事件、Work所有权协调、创作任务、生成、规划、advisory、启动/资源清理、消息投递、自动消费者及可信IPC组装。最大模块 creative-task-coordinator 441 行；未以单一第二入口替代main。模块依赖以JSDoc对象合同声明，引用实际工厂结果/类类型；读依赖为readonly，只有实际赋值的状态有setter，避免新边界吞掉类型检查或复制状态。跨能力调用保持原事务/执行身份，不新增通用Host服务定位器。

验证：124 个生产Host文件均显式类型检查、tsc零诊断；architecture B01零诊断；218项Host受控进程/恢复/并发/配置/生命周期回归；4项真实Runtime/MCP写入恢复和工具合同；2项可信IPC与真实Electron隔离preload取消传播；3项绑定/负例检查（缺失变量与未提供能力均被检查拒绝）；5项真实production main/preload桌面联调（正常、31秒MCP延迟、MCP失败无模型请求、失败后重试、损坏认证缓存保留本地工作区）全部通过。日志分别为 /tmp/host-composition-{types,architecture,full,native,boundary,bindings,desktop-full}.log。

额外真实入口测试发现测试认证元数据仍取上游prompt，生产已取本应用engine-prompt；仅修测试助手使用与真实Host一致且trim后的本应用提示词生成认证摘要。认证、来源验证及过期校验未放宽。真实桌面测试使用本地脚本模型和临时证据，不是供应商能力认证或付费模型验收。

五视角后审（同一代理自审）：架构 APPROVE（main仅composition、无转发Godmodule、依赖可查）；技术A APPROVE（来源/取消/所有权/清理边界保留，真实失败与重试通过）；技术B APPROVE（全生产类型门禁与两个类型负例，harness加载实际生产模块，非复制实现）；产品 APPROVE（无新增作者动作，失败时本地工作区仍可用）；UI/UX APPROVE（接口与事件合同不变，真实preload验证）。前审关闭条件完成。
