# Bridge retirement 测试时序校准

范围：只修改 Agent Engine 测试，不改产品状态机、预算权限或超时。

证据：完整 Engine 403/404，失败发生在 SIGTERM 后 restart，错误为 bridge process is closing。单独原用例通过。临时诊断副本延迟已有 transport.close 的完成，稳定观察到 processId=null 而 acceptingStart=false，随后同一 start 栈报错。Native retire 先进入 Retiring/清除 pid，CloseTransport 回调才进入 Stopped；Git HEAD 中相同实现和旧测试等待条件均已存在。故这是测试把“进程退出”误当“清理完成”的既有竞态，与本轮 requestTimeoutMs 透传无关。

## 五视角前审

| 视角 | 结论 | 理由/约束 |
| --- | --- | --- |
| 架构 | APPROVE | 等待 Native 权威生命周期状态，不新增 Host 状态副本 |
| 技术A | APPROVE | 修复等待条件，不增加 sleep 或 timeout |
| 技术B | APPROVE | deferred retirement 屏障确定性覆盖 pid 已空但重启仍拒绝 |
| 产品 | APPROVE | 预算隔离、取消安全、未知预算拒绝保持既有断言 |
| UI/UX | APPROVE | 无用户流程或产品代码变更 |

验证：先定向原用例和新增屏障用例，之后一次完整 Engine suite。后审待实测补入。

## 五视角后审

| 视角 | 结论 | 实测证据 |
| --- | --- | --- |
| 架构 | APPROVE | 只更改测试等待条件和增加测试；Native supervisor、JS adapter 均未改动 |
| 技术A | APPROVE | 旧测试等待 pid=null 且 Native acceptingStart=true；保持原 2 秒 waitUntil 上限 |
| 技术B | APPROVE | 新增 deferred close 屏障，无固定等待时长；屏障内断言 pid 已空、acceptingStart=false、start 拒绝，释放后方可重启 |
| 产品 | APPROVE | 原预算消耗、额度拒绝、lease/budget结束、重启后403等断言完整保留 |
| UI/UX | APPROVE | 完整 Engine 405/405，无用户流程变更 |

验证命令与结果：定向 `node --test --test-name-pattern='bridge process isolates transport|bridge restart waits for transport retirement' tests/bridge.test.mjs` 2/2；之后唯一一次完整 `node --test tests/*.test.mjs` 405/405，耗时约7.6秒。执行日志分别为 `/tmp/engine-bridge-retirement-fixed.log` 与 `/tmp/engine-retirement-full405.log`。首次失败属于旧测试竞态，不以增大 timeout 作为修复。
