# 分页与发布验证夹具合同校正

## 问题及证据

真实 MCP `source_navigation::present` 将 Runtime 的 `nextCursor` 转换为完整 `nextRead={tool, arguments}`，并删除裸 cursor 字段。规则引擎回环夹具仍检查 `page.nextCursor`，在第一页即错误认为结束并 JSON.parse 未完整拼接的原文。修正消费返回的完整调用，保留多页、字节连续、完整内容、摘要、冻结来源与仅保存候选断言。

发布测试没有拷错 Native 二进制：它显式使用 Node 运行时替身，但 health 只返回 ok/service，遗漏当前发布门禁必需的 creativeMethodDigests。替身构造时固定编译来源技能摘要，健康回执如实返回该模拟编译目录；不修改发布校验器。保留发布资源防篡改断言，并增加编译方法缺失/多余/摘要错误拒绝检查。

## 五视角前审（单执行者自审）

- 架构 APPROVE：只校正两个测试的现行边界，不改生产分页与发布合同。
- 技术A APPROVE：后续请求必须复制 nextRead.arguments，不重算 cursor，不将页末误作源末。
- 技术B APPROVE：运行两个文件的真实定向测试，保留完整来源和发布拒绝负例。
- 产品 APPROVE：候选不自动采用，发布资源来源约束不变。
- UI/UX APPROVE：不涉及界面与用户流程。

## 五视角后审

- 架构 APPROVE：仅两个测试消费当前 MCP/health 合同，生产未改。
- 技术A APPROVE：真实 Codex 回环读取 3 页，连续 byteRange、全量 JSON、contentHash、冻结第一页及候选未采用断言通过；nextRead 的 source/view/version 和末页总字节另外断言。
- 技术B APPROVE：`node --test --test-concurrency=1 electron/tests/property-rule-input-engine.test.mjs electron/tests/release-runtime.test.mjs` 4/4 通过，日志 `/tmp/source-release-fixture-tests.log`；目标 diff whitespace 检查通过。未运行全套。
- 产品 APPROVE：规则候选未成为正式采用内容；发布替身未豁免编译方法目录或摘要校验，缺失/额外/错误摘要负例均通过。
- UI/UX APPROVE：无界面行为变更。

分页最大实际工具文本 2,795 bytes，cl100k_base/o200k_base 最大 token 数 1,265/844。原 6,500-byte raw 页约束和现有 token 上界断言保留；没有通过抬高阈值掩盖截断。
