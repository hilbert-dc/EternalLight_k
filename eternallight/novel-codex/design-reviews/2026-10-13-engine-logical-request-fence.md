# Engine 逻辑请求派发围栏

## 前审
真实失败证据：/tmp/host-budget-unlimited-evidence.log，真实 Codex 在同轮流中断、费用未知后访问本地供应商6次（期望1），不是文案漂移。预算无上限允许新请求，但 Bridge 没有把 HTTP 自动重试绑定到同逻辑请求；B05/B06禁止未知请求重派。

责任与权威：Engine Bridge 用可信执行lease.runId、已验证 Core thread/turn 身份、route 和策略改写前的规范请求生成 SHA256 identity；只传摘要给持久预算账本，供应商请求不变。不采用模型生成的身份。Budget 在现有 reservationDefinitions 记录 identity，每次真实尝试仍独立reservation；同identity在途或已未知拒绝。已有已知回执可创建全新reservation并独立计量，禁止复用旧预留免费派发。未发出退款的reservation不封锁后续尝试。新的turn/body/run可独立申请额度；已知零费用失败可由当前执行准入发起新计量请求，不免费复用旧reservation。

|原请求事实|同identity|不同turn/body/run|
|---|---|---|
|在途|拒绝，无新请求/预留|按剩余额度准入|
|已未知|拒绝，原费用保留|无token上限可准入；有限上限仍受未知费用约束|
|已知完成（含0）|新reservation独立计量，原回执保留|按新请求独立计量|
|确认未发送|允许新reservation，旧退款回执保留|正常准入|

Bridge 专用identity不改变既有Host显式reservationId幂等接口。固定本地重复派发错误按400 terminal传输，不误标供应商文本。正常新任务不被历史unknown全局冻结。

验证：预算状态单测覆盖上述矩阵与拒绝后不变；Bridge identity单测区分turn/body/run；真实Codex5项包含finite/unlimited未知断流、输出拒绝与额度终态；Engine完整套件由主集成。五视角：架构 APPROVE；技术A APPROVE WITH CONDITIONS（可信身份与持久围栏）；技术B APPROVE WITH CONDITIONS（边界单测和真实provider次数）；产品 APPROVE（保护真实费用、继续新attempt）；UI/UX APPROVE（无新确认，无界面变更）。

## 后审
生产仅变更 Engine bridge_exchange、bridge_failure、budget。逻辑身份摘要在策略改写前计算，不传递完整来源到计量定义；作用域含原执行run与Core身份，供应商请求不改。Budget 持久定义内识别未知/在途，不复用旧请求收费；单测证明持久恢复后未知仍拒绝、拒绝账本不变、已知0和5token再次请求取得新id并加请求数、not-sent保留退款回执且可再试。

Engine Core 50/50（/tmp/engine-logical-request-core.log）；6个预算相关Node文件27/27（/tmp/engine-logical-request-node.log）；真实Codex 5/5（/tmp/host-budget-terminal-fixed.log），finite/unlimited每轮均1次供应商访问、保留未知费用、自动重试被固定本地终态拒绝。原始6次重派证据保留在 /tmp/host-budget-unlimited-evidence.log。architecture:check 无诊断，diffcheck通过。未扩充权限，不把历史未知当无限额度耗尽，不清零用量。五视角后审均APPROVE，前审条件关闭；完整Engine套件交主集成。


## 完整套件调度 fixture 前审
完整 verify 的两个 request-scheduling budget分支失败：测试模拟七个独立竞争请求却全部使用相同 run、thread、无 turn 身份和完全相同 body。provider单并发分支先串行结束所以已知结算后可再计量，budget多并发分支在第一请求仍在途时正确被同identity围栏拒绝。修 fixture 为每个独立请求分配稳定不同 turn_id，仍保留原排队/取消/容量复用/次数/预留归零断言；不改生产围栏。五视角均同意，技术条件是原定向6例及完整Engine套件通过。

调度补充后审：定向6/6通过（/tmp/engine-scheduling-current.log）；完整 npm test（含build:native）405/405通过（/tmp/engine-complete-logical-fence.log）。原同unknown/在途围栏未改，独立turn竞争预算排队与取消断言完整保留；diffcheck通过，五视角条件关闭。
