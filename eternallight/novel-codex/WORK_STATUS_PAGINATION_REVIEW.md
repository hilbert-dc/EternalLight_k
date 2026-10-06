# Work 决策证据分页

目标：智能体按需读完当前 Work 的合法行动、必要依赖和真实执行回执；不因首屏截断误判没有后续工作。

接口：`read_work_status` 保持工具名称，增加可选 `cursor`，省略或 JSON null 表示第一页。返回 `pagination.page/nextCursor/hasMore`；后续复制 nextCursor。每页最多 6 个 requirements、6 个 readyActions、3 个 receipts。原 coverage 表明本页是否包含整个集合，offset 标明集合位置。所有页共享 Work/version/sourceDigest/decisionTask 身份；当前状态变化时旧游标报错，不能静默跳回首页。

游标由页码与 project/work/decisionTask/version/sourceDigest 及回执状态摘要组成，长度不超过 100；不是授权凭证。每次读取仍验证当前任务与作者来源。页码越界、跨任务与来源变化均拒绝。只读快照保证一页内部一致。非终态执行进度可能不递增 Work 版本，因此还绑定 taskId/status/error 的稳定摘要；动作按 required/actionKey、依赖按 pending/规范 JSON、回执按 ordinal/id 稳定排序。输出仍遵守 6500 字节上限。

前置评审：架构专家、技术 A/B、产品与 UX 已批准小页、严格来源绑定、不扩权、无全书注入的方案。内部 complete() 仍使用首屏入口读取全局 canFinish，不改变交付接口。

零兼容：PASS。当前工具契约直接添加分页能力，不保留旧模式、双读写、迁移或降级路径。

验收：真实多页回执完整无重复；空页/末页与计数正确；过期版本、来源、项目、Work、决策任务游标拒绝；MCP 编码转发与输入上界；现行 HTTP 契约通过。测试结果由主代理统一运行后补充。

## 最终生成交付补充评审

架构与技术 A/B、产品与 UX 认可：仅最终章 `finishChapter` 的完成提交 Work 总目标自评；中间章不重复全篇证据，不增加机械审阅流水线。Runtime 从持久 run 的 completed/current/target 判断最终章，模型不可声明自己是最后一章。`complete_task` 在原事务保存自评，最后 checkpoint 要求同一决策已存自评才能完成 run。保留原来源、预算、授权及所有当前正文证据验证；普通选行动仍不提交自评。状态以 `finalDeliveryReviewRequired` 提示最终章交付职责。
