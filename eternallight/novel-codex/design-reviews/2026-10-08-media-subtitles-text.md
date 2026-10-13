# 字幕成片与长文本局部编辑评审

日期：2026-10-08。当前版本、当前 schema；Root 负责整合，技术评估来自 media_image_dialogue、media_structured_editing、media_delivery 的只读审查。

## 变更概览

- 目标：真实动画字幕导出；大段对白、画格/镜头说明和字幕按 Unicode 字符精确读取、原子局部替换。
- 范围：复用固定文档修订、来源授权、CAS、幂等、Domain/Runtime/MCP 和现有导出入口。字幕使用固定字体在受限 Electron 中生成透明 PNG，再由 FFmpeg 叠加；所有视频画面组合后才叠字幕。
- 验收：中文字形实际使用固定字体；没有系统字体静默替代；透明背景、字幕时间半开边界、相邻/重叠、静音、取消和超限有证据。长文本分页固定修订，坐标以 Unicode scalar 计，局部操作验证全文和切片摘要，批量原子失败、锁与精确来源授权不变。
- 非目标：本批不接入付费供应商、不实现额外小说 Edition、不声明全历史作品库备份或 GC 删除完成。全历史备份必须包含 CLI 会话及真实退出/静止证明，不能由两份数据库复制替代。

## 测试阶段零兼容门禁

- [x] 无旧字段 fallback、迁移、回填、降级、双读双写或版本探测。
- [x] 当前生产接口、类型、fixture、测试和文档同时更新。
- [x] 当前 schema 新建隔离测试数据，不删除用户作品。
- [x] 继续验证权限、原子性、锁、资源上限、取消和跨平台路径。
- [x] 无兼容例外。

## 实施前五角色评审

| 角色 | 结论 | 实施条件 |
| --- | --- | --- |
| 架构专家 | APPROVE WITH CONDITIONS | 字幕只有一条正式渲染链，不保留 drawtext 降级；文字选择是固定修订上的显式字段，不引入第二套作品权威。共同设定仍仅作者保存。 |
| 技术专家 A | APPROVE WITH CONDITIONS | 实际字体/字形、透明度和成片帧验证；不把 fonts.check 当识别证据。精确 Unicode 坐标、摘要、CAS、锁、授权与整批原子性必须覆盖。 |
| 技术专家 B | APPROVE WITH CONDITIONS | 正式导出及发布 smoke 使用同一字幕链；当前 Runtime/MCP/Host/类型一次更新。分页实际字节不超过限制，不静默截断或切到 HEAD。 |
| 产品经理 | APPROVE WITH CONDITIONS | 字幕能真实导出，模型只读取和修改需要的文本；失败保留作品，明确字体或服务缺失。不得用 mock 证明真实成片或所有架构完成。 |
| UI/UX 设计师 | APPROVE WITH CONDITIONS | 复用时间轴与导出入口，预览排版一致；不增加对话底部技术记录、操作模式或冗余按钮。 |

综合：APPROVE WITH CONDITIONS。实施前阻断项已明确为必须验收的条件；未通过条件不得登记完成。

### 实施前补充：大字体页面承载

真实 Electron 验收发现 23 MB 中文字体生成的 data URL 超出 Chromium URL 限制。五角色同意把所有媒体 HTML 统一经私有 session 的精确随机内部 HTTPS URL 从内存提供；不是失败时降级路径。协议适配器与 webRequest 仅放行该 URL 和嵌入 data 资源，不做 DNS/外网读取，不开放 file、权限、页面脚本或新窗口。HTML 字节上限、字体上限、超时、取消和最终清理仍是硬门禁；PNG/PDF 与字幕使用同一私有承载方式。条件：大中文字体正向真实测试及外部请求拒绝必须实际验证。

### 实施前补充：共同设定来源

五角色批准当前来源 quote 上限统一为 8000 Unicode scalar，超限明确拒绝，不截断引用。完整章节可用固定原文档来源并逐段读取；不能把来源 quote 当整篇内容载体。共同设定目录只返回 statement 的 textFields 和 sourceCount，引用原文另用 `canonSources(entryId)` 有界分页，保留每条实际 source。作者设定 API 不授予模型写权。单个 SourceAssociation 双方都含长 quote 仍可能超过 64 KiB，当前严格拒绝；全部来源图叶子分页不在本批登记完成。

### 实施前补充：成片字体依据

最终 Host 复核发现正式 renderer 返回的实际字体摘要和字幕帧依据尚未写入成片 manifest。五角色批准把 `subtitleEvidence` 按真实 renderer 回执保存在 MP4 导出清单，不生成推测证据，不增加对话日志或用户控制。无字幕成片为空数组；可编辑归档不冒充包含字体字节。条件是实际 Host 字幕导出清单与当前 renderer 回执一致。

## 实施后复核

| 角色 | 本批结论 | 实际依据与保留条件 |
| --- | --- | --- |
| 架构专家 | APPROVE | 固定修订、Domain 精确来源授权、Runtime/MCP 与正式导出链保持单一权威；没有迁移、HEAD 替换或第二套作品状态。 |
| 技术专家 A | APPROVE | Unicode 范围、实际字节预算、全文/切片摘要、锁、CAS、幂等和原子零写入有 Core/HTTP/整链证据；固定来源无法扩大至兄弟节点或其他范围。实际字体、透明 PNG 与成片帧已回读。 |
| 技术专家 B | APPROVE WITH CONDITIONS | 当前 bins 的工具、宿主、旧/新原字节归档、精确 FieldText 来源及新目录恢复全部通过；Host 清单保留实际字体与帧依据。真实可分发组件、字体许可及跨平台安装验收仍未满足，发布门禁不解除。 |
| 产品经理 | APPROVE | 模型选择需要的文本和范围，工程校验授权、一致性及提交结果；字幕真实预览与导出。超限明确失败并保留内容，不把受控供应商或开发机测试作为作品质量认证。 |
| UI/UX 设计师 | APPROVE | 沿用现有编辑、时间轴、来源折叠和导出入口；来源展示具体字段、字符范围与摘录，没有新增模式、底部技术记录或重复任务控制。 |

综合：本批字幕与长文本闭环通过，完整架构仍为 `APPROVE WITH CONDITIONS`。全历史备份、GC、应用归档导入、额外小说 Edition、全部来源图分页、真实供应商与跨平台安装包未被登记为完成；全仓边界检查问题继续保留。

### 生产结束点与验证范围

Root 的本批 Host、preload、受限字体预览、CSP、发布 worker、发布门禁和来源标签于 2026-10-08 01:21:41 UTC 结束生产补丁，最后一项为实际成片字体依据写入导出清单。字幕生产 slice 冻结于 01:17:38 UTC，实际 Host 验收 fixture 冻结于 01:32:45 UTC，媒体 Native 最新接线修正于 01:17:03 UTC。后续只修实际验收失败及登记文档；另一创作对话的共享源码验收独立记录，不能把本批定向通过说成全仓通过。

当前独立证据：最终生产版本的常规媒体组合 46/46（0 跳过），字幕组合 12/12（0 跳过），真实发布 worker 1/1（0 跳过），Root 固定字体与回执静态组合 14/14（0 跳过），发布 Runtime fixture 3/3（媒体执行是明确 mock）。真实 worker 记录空 PATH、Electron 38.8.6、H.264/AAC、字幕前/中/后亮像素 [0,287,0]、固定字体 SHA 与实际 custom-font 字形。真实 Host 清单同实际 renderer 的 fontEvidence 完全一致，固定帧 [6,18) 的输出按第 5/6/17/18 帧解码核验；空字幕实际 export 清单为 []，没有调用字体 renderer。该 Host 字幕 fixture 的固定 workspace GET 是明确 mock，不能替代 Native 整链。

Native Core 当前 36/36（context 2、patch 11、general 14、text 9），Runtime HTTP 当前 6/6（context 1、multimedia HTTP 5），MCP 7/7，合计 49/49，均为零跳过。正文包含 18k Unicode、实际 JSON 字节预算、精确授权、锁、摘要、范围与整批失败不写入；HTTP 验证固定旧修订、任务自身保存后来源复读、作者接口权限、Canon 写拒绝及重启读回。

最终 `test:multimedia-media` 当前完整 8/8、失败/取消/跳过均为 0，日志 `/tmp/media-subtitles-text-real-final-pass.log`，20.451 秒；实际 Runtime/MCP/Host/归档整链 7.841 秒。它包含真实 16,200 Unicode scalar 分页、局部修改、数据库 readSet、固定 FieldText sourceLink、旧/新文档原字节、共同设定归档及新目录恢复，供应商的 3 次 POST 仍为受控端点，真实外部请求 0。首次 7/8 的失败仅为 fixture 显式空 range 与当前 Rust 省略可选空值的输入差异；修正当前 fixture 后完整复跑，不改生产及来源相等断言。

常规最终日志 `/tmp/media-subtitles-text-suite-frozen.log` 为 46/46，11.320 秒。最终 TypeScript/Vite 构建日志 `/tmp/media-subtitles-text-web-frozen.log` 为 503 模块，构建通过；本批 `git diff --check` 通过。作者 `readTextPage` 的类型只包含四种文档字段，readonly Canon union 仍保留，该共享类型于 02:56:17 UTC 冻结；最后增强 integration fixture 于 03:07:38 UTC 冻结。

最终整链使用当前一次构建的 bins：Runtime SHA-256 `bf2221757e6b448a300ab6762e08440e177badc4cc4f7ee071eee465f3aae093`；MCP SHA-256 `7d3556382787fdadadde5d665a5ef29a8314d6da0dc9bafa30c4d14f575ff971`。Root 独立回读日志及校验两项摘要，未构建或复用另一个 release。

本轮额外执行 `repo:check` 未通过：共享工作区包含 codex-api bridge 与 MCP conversation_history 三个未批准的上游路径改动，同时 Git 引号编码的现存 release-fresh/release-ready/release-verified 输出被边界脚本列入未批准路径。本批未改这些上游文件、未修改 allowlist、未删除发布输出，未把该检查记为通过。原日志为 `/tmp/media-subtitles-text-boundary.log`。这与本批媒体定向结果分开登记。

## Backup / Novel 后续边界（media_delivery 只读审查）

本批只交付字幕渲染与长文本编辑，不把以下项目登记为已完成。

- **全历史作品库备份尚未实现。** 当前可编辑归档只保全固定 Edition 修订、素材和设定快照，其他作用域来源仍是精确引用；不能称为全项目数据库、完整来源图或全历史备份。
- 全历史一致快照必须涵盖 Domain、Engine、媒体 Blob、恢复日志，以及隔离 `CODEX_HOME` 的当前 CLI SQLite 与会话原文。先拒绝新执行并等待本地导入、导出、媒体 IO、Runtime 和 CLI 真正静止/退出，再使用 SQLite backup API；不能复制正在运行的数据库文件或把两份独立快照冒充同一个一致 cut。当前 `codexAdapter.close()` 只发送 kill，不能单独作为退出证明。
- 已做临时原语验证：产品 Electron 38.8.6 / Node 22.22.0 的 `node:sqlite.backup` 可用；两个临时 WAL 库同时持有写栅栏时，已提交 WAL 行被完整备份，两个 `integrity_check=ok`、外键违例 0，竞争写被阻止。这只是备份原语证据，不是产品备份链路通过。
- **GC 删除尚未实现，禁止删除真实用户素材。** 可达性必须覆盖所有历史素材修订、文档及任务冻结输入、跨 Edition 引用、未封存作业、导入、导出和备份 pins。当前 receiving import lease 不绑定 digest，Host 也缺统一维护/drain 门禁，未引用文件只能只读报告为待判断。`outcome_unknown` 和本地租约到期不证明远端作业结束。
- **归档导入应用尚未实现。** `recoverMediaArchive` 是新离线目录复原。应用导入须先校验当前 Rust 类型与语义，生成新项目/Edition/素材/设定身份，完整重映射引用，保留原归档来源证据但不伪造有效本地来源；当前多步基础 API 不能替代持久幂等导入回执与最终原子提交。
- 新目录全库恢复还需当前 CLI 历史绝对路径的严格受控重绑定，以及原作业/预算/未知结果的保全；不自动发送付费请求，不覆盖现有作品，不依赖文件系统 fallback 修复分页历史。
- **额外小说 Edition 尚未实现。** 当前新建独立小说作品与漫画/动画版本能力不等于同项目内第二个小说版本；该能力仍需完整的独立小说内容作用域、分支/任务/来源授权、编辑与导出链及实际验收。
- 后续五角色 PRE 结论为 `APPROVE WITH CONDITIONS`：Host 管字节和快照、Domain 管身份及来源、Engine 管作业及预算；真实静止 cut、持久 pins、当前格式原子恢复及实际读回是实施和完成条件。本批没有旧版本兼容、迁移、回填或素材删除授权。
