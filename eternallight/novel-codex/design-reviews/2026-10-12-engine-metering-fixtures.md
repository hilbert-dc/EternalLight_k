# Engine 计量合同测试收口

## 前审
责任与权威：Engine Rust预算账本（budget.reserve/settle和持久receipt）拥有请求计量与未知状态；本次只修三处过时JS测试，不改生产。B05/B06明确：无token上限允许独立新请求并保留未知费用，同一未知请求不得重派；有限请求数仍有效；读取结算回执不重派也不改账。

|事实|断言|副作用|
|---|---|---|
|同logical id预留参数一致、尚未未知|幂等同id且只计一次|无重复计量|
|同id参数变化|拒绝|账本不变|
|同id已未知，即使0 token预留|拒绝重派，结算回放仅返回原回执|账本不变|
|无token上限且历史未知，新request|允许独立计量、保留历史未知|新增一个request|
|有限request上限已满|拒绝另一request|账本不变|
|持久恢复仅在途预留未知|精确完整receipt含空failureObservations/usageReconciliations|重复读取一致|

五视角自审：架构 APPROVE（Engine唯一事实、不改授权）；技术A APPROVE WITH CONDITIONS（保护断言增强而非删除）；技术B APPROVE WITH CONDITIONS（定向与404完整Engine套件）；产品 APPROVE（不改变行为）；UI/UX APPROVE（无界面改动）。

## 后审
三个失败均为JS断言与现行Rust/规则不一致，生产零改动。持久receipt精确补全两个账本事实数组；media保留重入幂等/参数冲突/request上限断言，新增未知重派拒绝、重复settlement不能覆盖原未知回执和拒绝后账本不变；无限计量用显式request id分别验证同未知请求拒绝与新请求允许，终态精确计量71次/1400000已知token/20000未知token并验证回执重复读取相同。

20项定向通过（/tmp/engine-metering-fixtures.log），完整Engine 404/404通过（/tmp/engine-metering-full.log），diffcheck通过。使用已构建的当前Native binding，无供应商调用。五视角后审：架构、技术A、技术B、产品、UI/UX均APPROVE，前审条件关闭。
