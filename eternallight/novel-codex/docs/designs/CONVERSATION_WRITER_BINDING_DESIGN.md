# 对话派生 Writer 任务的绑定范围

2026-09-28 · S2/S3、C03–C05 · 实施者自检（按此前要求，不组织独立评审）。

## 问题与目标

作者在对话中要求修改已有章节时，根 advisory Agent 通过
`request_candidate_task(generateChapter)` 登记候选工作。HTTP 层规范化目标
保留章节与场景范围，却丢掉了 `povCharacterId/storyTime`。对已绑定章纲的
章节，子 Writer 因而可用无 POV 的 Context Pack，读取本应保守遮蔽的
前文。上轮自动 GenerationRun 已绑定该位置，但对话派生任务尚未覆盖。

目标是让对话修改、自动写章和作者预览都服从同一个不可变章节绑定。
允许对话派生的 `wholeChapter` 与 `selectedScenes` 保持原范围；不因为绑定
POV 自动扩大选择场景、授予 Canon 写权或自动采用候选。

## 当前版本契约

1. HTTP 入口保留模型明确提出的有界 POV/整数世界时间，以便领域检查；
   不能在进域前悄悄丢弃不同于章纲的值。
2. Rust `record_task_action` 的同一 SQLite 写事务读取当前章纲绑定和唯一章纲项。
   有绑定时，未指定的 POV/时间自动取绑定值；明确值与绑定不一致则拒绝。
   无绑定独立章保留合法的显式限制，不伪造规划来源。
3. 规范化后的目标写入原 TaskAction/WorkStep，不新建调度状态；后续
   `create_task` 必须精确匹配，模型不可通过删改 POV/时间扩大子任务范围。
   上游 Codex 内核仍只负责执行；小说领域决定权限与来源。
4. 受限 `read_context` 必须使用冻结目标的 POV/时间；遗漏或替换返回拒绝，
   当前章规划来源仍是精确绑定，前文遮蔽不因作者可全局读而放宽。

## 验证门槛

真实 HTTP/SQLite 联合测试创建有前文章节的作品、绑定含 POV/时间的章纲，
以根 advisory 能力登记整章或场景修订。验证规范化 Action、重试回执、
错误显式值的原子拒绝、子任务篡改拒绝、合法 Writer Context Pack 只含
当前章并携带绑定规划，以及无 POV/时间请求被拒绝。测试受控任务能力，
不读取真实用户数据库或调用付费模型。

## 零兼容与边界自检

不添加旧字段 fallback、双读双写、迁移或新的模型执行通道。修改同步覆盖
HTTP、Rust 领域、现行测试和架构索引。此切片不验证作者工作基线选择、
真实模型是否正确理解改稿意图、文学质量、完整 App 或 Windows；不能据此
宣布 C03–C05 或 S1–S5 完成。

## 实施后自检与证据（2026-09-28，macOS）

- `task_action_api` 保留合法显式位置；`story-core::record_task_action` 在同一
  Immediate 写事务中读取冻结章纲并将位置写入原 Action/WorkStep。未绑定
  章节仍保留显式受限位置。没有新表、新调度器或上游 Codex 内核改动。
- 真实 Router/SQLite 测试覆盖整章与限定场景：绑定自动补齐、幂等重试、
  错误显式值无部分写入、子任务删改拒绝、Context Pack 的绑定规划与
  POV 前文遮蔽；另验证无绑定章节的显式 POV/时间不被 HTTP 吞掉。
- Runtime 相关三组集成测试 15 项通过，改动后的 Context/Action 组复跑
  10 项通过；Story Core 库 156 项通过。Native 全工作区、所有 target 的
  离线锁定 Clippy `-D warnings` 通过。
- Story Core 全包测试的 `nextest` 发现阶段出现多个测试二进制 `--list`
  子进程等待数分钟，未完成执行；这不是断言失败，也不把全包标为通过。
  真实供应商、完整桌面交互和跨平台联合验收仍须继续。
