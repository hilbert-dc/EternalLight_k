# 核验动作的精确来源入口

根因：第27章实测3802353已报告token中，协调2822761。一次核验读错来源、一次游标误用；压缩只减少携带，不能消除导航步骤。分类为工具设计缺陷：Native已持有writer及精确revision，却要求模型再次搜索。模型误用是触发因素，不能据此降低全文证据要求。

目标：read_work_status.readyActions中verifyChapter返回currentDraftSource，包含该任务绑定、经既有权限校验的immutable source和可直接执行read请求。只返回引用，不内嵌正文、不增加已读证据；仍必须实际读完整稿。复用task_workspace的范围与同Work/生产者证明，不另建事实或模型循环。MCP同步短说明，HTTP场景直接消费此入口验证“仅列出仍拒绝，完整读取才通过”。不改变Core阈值/作者额度/质量/发布权限/UI合同，无迁移兼容层。

五视角前审（同一执行者）：架构APPROVE；技术A/B APPROVE WITH CONDITIONS（同Work与范围校验、实际HTTP读证据）；产品APPROVE（消除已知导航而保留全文）；UI/UX APPROVE（无作者新步骤）。验证：现有直接核验HTTP及生命周期测试、MCP指南测试、架构门禁。运行中的真实任务不热换二进制；部署须在安全维护边界，未部署不报告真实收益。

参考：Cursor https://cursor.com/blog/dynamic-context-discovery 的按需发现；Anthropic https://www.anthropic.com/engineering/writing-tools-for-agents 的高效工具与清晰返回；OpenAI https://developers.openai.com/cookbook/examples/gpt-5/codex_prompting_guide 的压缩与长任务连续性。借鉴公开策略，不声称复制其内部实现。

验证结果：直接核验HTTP 3项通过（真实读取入口、不读拒绝、错误引文/分组拒绝、幂等、独立标准保持）；生命周期HTTP 1项通过；MCP工具合同6项通过；架构369文件/7crates零诊断。首次HTTP在沙箱本机监听EPERM失败，原样在允许回环监听环境通过。Native bins构建成功。无Core修改，无新增兼容或数据库变更。当前运行应用尚未切换，不将构建通过表述为已部署或真实节省。

后审：架构/技术A/技术B/产品/UI五视角通过本次接口实现；实际减量仍须部署后实测。入口注册只产生元数据，状态分页仍保持6500字节合同，实际全文与证据校验不变。构造函数提示词盘点另见 `.release/context-live-validation/2026-10-12-prompt-inventory.json`，统计字符/字节而非供应商token，不能与小说真实用量相加。

部署补记：2026-10-12T03:20:32.204Z 新包六项发布门禁通过，精确稿件入口与提示职责收敛一起部署并启动。原29章成果保持；真实模型使用入口的首试成功率仍须观测，不将HTTP成功替代真实模型收益。
最终包更新为2026-10-12T03:28:05.644Z（六门禁通过），新增编译方法与包内资源一致性门禁。原任务新决策已真实执行，详见 prompt-ownership 同日评审E7。
