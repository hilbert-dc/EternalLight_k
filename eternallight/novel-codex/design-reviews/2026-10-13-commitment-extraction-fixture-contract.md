# 伏笔抽取 HTTP fixture 当前合同

## 前审
根因归属：测试直接保存不完整 draft envelope，缺 chapterId 和 sceneGroups；Runtime workspace_projection 原样投影成 null，HTTP DTO 要求章节身份字符串及分组数组。共享 author_task_fixture 仅在作者 workspace 读取成功后填入有限 initialBudget；读取失败使后续 /tasks 返回 TaskBindingInvalid，掩盖了前置数据错误。生产 CandidateStore::save_draft 已写入这两个字段。

按 B01/B03/B04 和零兼容策略修测试数据，不改生产校验、权限、预算、来源或恢复规则。补当前 draft 的 chapterId 与空 sceneGroups（保留 body 作为唯一文本来源），并在创建任务前显式断言作者 workspace 读取成功。原抽取范围、后章不可读、作者报告及候选权限断言全部保留。

|输入|预期|副作用|
|---|---|---|
|完整草稿 envelope|作者 workspace 200，再按现有有限授权创建根任务|不扩大任何权限|
|候选身份、跨章来源或无效绑定|仍由原用例断言拒绝|无非法写入|

五视角：架构 APPROVE（Core producer 为事实权威）；技术A APPROVE（修源 fixture 而非默认填充生产）；技术B APPROVE WITH CONDITIONS（定向完整 integration target）；产品 APPROVE（行为不变）；UI/UX APPROVE（无界面改动）。

## 后审
完整 commitment_extraction_http_tests 通过（1/1；/tmp/commitment-extraction-fixture-test.log），包括新 workspace 前置200及原任务绑定、候选输出、后章不可读、作者报告权限断言。diff --check 通过。只修改 fixture 和本记录，生产零改动。五视角后审均 APPROVE，技术B条件关闭。
