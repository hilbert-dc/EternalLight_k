# 多媒介离线交付与安装资源

日期：2026年10月8日。此文件描述当前交付契约；通过结构和故障测试不等于已经具备合格的安装资源或已生成完整作品。

## 固定版本的可编辑作品包

现有“导出可编辑项目”入口输出一个漫画或动画 Edition 的固定修订。包包含：

- `document.json`：Runtime 保存的确切 JSON 字节，保留浮点数的原始编码；SHA-256 必须与 `documentSource.digest` 相同。
- `workspace.json`：作品、Edition、分支、修订、文档、素材元数据、文档来源、Edition 来源和来源关联。
- `canon.json`：该 Edition 绑定的共同设定快照；没有绑定时不生成此文件。
- `assets/<sha256>`：包内全部素材原始字节，包括固定文档合法引用的同作品其他版本素材；同一 Blob 只写一份，素材修订身份分别保留。
- `manifest.json`：修订身份、素材文件映射和每个输出文件的大小及摘要。

`sourceDigest` 是当前读取集合的指纹，与文档原始字节摘要分开管理。包不会包含私有预览令牌。其 `sourceCoverage` 明确为 `editionAndAssetsWithExternalSourceReferences`：其他作用域的来源原文仍是精确修订引用，不能把这个 Edition 包称为全作品数据库或全历史备份。

`electron/media-archive.mjs` 先在私有临时目录写入、同步并复验，再原子发布新目录。缺少素材、错误的共同设定版本、文档字节不一致、未知文件、越界路径或符号链接都会失败；失败不返回成功回执。打开包时再次验证完整性。

`recoverMediaArchive` 只把验证过的离线数据包复原至新的目录，拒绝覆盖已有目录；它不写应用数据库，不伪造原修订，也不把旧数据转换到当前 schema。应用内导入、全部历史和进行中的作业恢复需要独立的一致数据库快照协议，目前未完成。

## 固定来源构建

macOS 本机运行 `npm run build:media-runtime`。生产脚本独立于消费校验器：按 `scripts/media-runtime-sources.json` 固定 URL 和 SHA256 校验 FFmpeg、x264、Noto Sans SC 字体及许可证，静态链接 x264，关闭外部库自动探测。需要 Xcode 命令行工具、make 和 pkg-config；不会安装系统工具。其他平台需提供独立验证的 bundle，本次未实现 Windows 源码构建。

源码缓存 `.release/media-sources`，临时编译与日志 `.release/media-build`，状态报告 `.release/media-build-report.json`。只有依赖闭包及真实 Electron smoke 通过才原子替换 `.release/media-runtime/<platform>-<arch>`；失败保留前一个合格产物。构建锁防止并行覆盖，损坏缓存拒绝使用。首次构建需要联网，已校验缓存可供后续离线编译。

发布 staging 默认消费上述目标目录，也可通过 `NOVEL_CODEX_RELEASE_MEDIA_RUNTIME` 显式指定外部合格组件。staging 不负责下载或编译，仍验证签名前后文件摘要、依赖和真实执行证据。

## 受控的媒体执行组件

发布资源根目录必须有 `media-runtime`，其中程序和字幕字体入口固定为：

```text
media-runtime/
  ffmpeg                 # Windows 为 ffmpeg.exe
  ffprobe                # Windows 为 ffprobe.exe
  subtitle-font.ttf
  media-runtime-manifest.json
  licenses/
  lib/                   # 仅确实需要且在清单中登记的 macOS 动态库
```

Windows 额外 DLL 放在引用程序所在目录，由 PE 依赖表核对；不能依赖开发机上安装的 VC 运行库。macOS 只接受系统库以及完整交付的相对动态库依赖，拒绝 `/opt/homebrew`、外部绝对路径、越界 RPATH、未登记依赖和架构不符的程序。组件不能是脚本或符号链接。

`scripts/media-runtime-bundle.mjs` 接收明确的来源目录。它不会查找 PATH、下载组件、自动复制 Homebrew 或创建空组件占位。每个程序、动态库和字体必须绑定包内许可证；FFmpeg 的固定构建来源、源码包摘要、版本及构建参数必须登记。清单示意如下，摘要必须替换为对应的真实值：

```json
{
  "product": "novel-codex-media-runtime",
  "target": { "platform": "darwin", "architecture": "arm64" },
  "build": {
    "sourceUrl": "https://ffmpeg.org/releases/ffmpeg-8.1.2.tar.xz",
    "sourceSha256": "实际源码包SHA256",
    "ffmpegVersion": "8.1.2",
    "configureArguments": ["--enable-gpl", "--enable-libx264"],
    "licenseExpression": "GPL-3.0-or-later"
  },
  "entrypoints": { "ffmpeg": "ffmpeg", "ffprobe": "ffprobe", "font": "subtitle-font.ttf" },
  "files": {
    "ffmpeg": { "role": "ffmpeg", "sha256": "实际程序SHA256", "licenseFile": "licenses/ffmpeg.txt" },
    "ffprobe": { "role": "ffprobe", "sha256": "实际程序SHA256", "licenseFile": "licenses/ffmpeg.txt" },
    "subtitle-font.ttf": { "role": "font", "sha256": "实际字体SHA256", "licenseFile": "licenses/font.txt" },
    "licenses/ffmpeg.txt": { "role": "license", "sha256": "实际许可证SHA256" },
    "licenses/font.txt": { "role": "license", "sha256": "实际许可证SHA256" }
  }
}
```

字体及全部依赖应由固定来源构建并具备所需交付许可。此清单验证交付文件、构建声明及依赖闭包；构建声明本身不是上游来源真实性的替代证明。发布总清单继续覆盖签名后的文件，媒体清单保留未签名字节摘要。

`stageMediaRuntimeBundle` 在签名前验证来源，复制到新目录，签名本机程序及动态库后重新登记摘要并复验。`verifyMediaRuntimeBundle` 默认进行真实验收：在相同目标系统、空 PATH 下启动当前构建版本的 Electron，调用正式受限 HTML 字体渲染与动画合成链，编码 H.264/AAC 并用 FFprobe 和三个成片帧回读。字体摘要必须等于清单登记值，每个实际字体字形必须是 custom font；字幕的半开帧区间必须对应固定的测试片段。资源结构 fixture 与执行回执 mock 不构成真实媒体验收。可用 `NOVEL_CODEX_RELEASE_ELECTRON` 明确指定同平台、同当前构建版本的 Electron 程序；默认使用当前安装的构建依赖。缺少任一实际证据不能批准媒体安装资源。

开发环境可明确指定 `NOVEL_CODEX_FFMPEG`、`NOVEL_CODEX_FFPROBE` 和 `NOVEL_CODEX_MEDIA_FONT` 绝对路径；非空字幕要求配置覆盖实际字符的字体，纯画面/音轨不要求字幕字体。开发路径不构成便携安装资源，也不满足发布验收。

## 当前证据与未完成项

离线包验证与新目录复原测试 6 项通过；便携资源结构、Mach-O/PE 依赖闭包、摘要、许可证、签名失败和不覆盖及实际 smoke 回执校验测试 7 项通过。它们使用当前格式和受控原生结构 fixture，没有外部下载或付费请求。实际字幕 worker、固定字体、空 PATH 和成片帧另由真实测试验证，当前真实媒体完整组合为 8/8、零跳过。

真实 Host 回归验证：导出回执保存可信的格式，打开包时同时依据该回执、当前归档标识和项目格式决定完整复验。篡改磁盘清单的 `format` 不能跳过校验；清除内存回执后，归档标识仍会要求严格验证。不一致的包会被拒绝，系统打开路径不会被调用。

字幕正式链路统一为固定字体 → 私有 Electron 内存页面 → 透明 PNG → FFmpeg 最上层叠加，不再使用 drawtext。原工具缺少 drawtext 不再阻塞此字幕链。实际开发组件已验证中文字体、H.264/AAC 与字幕前/中/后帧；字体为明确配置的本机测试资源，没有复制到发布包。本机 Homebrew 8.1.2 仍存在 17 个 Homebrew 外部依赖，不能直接作为便携包。2026-10-11 已通过源码构建得到 macOS arm64 合格便携组件，空 PATH 下真实 Electron 38.8.6 渲染使用包内 Noto Sans SC，字幕前/中/后帧可见像素为 0/280/0，H.264/AAC、FFprobe、字体摘要及依赖闭包均通过。证据见 `.release/media-build-report.json`；Windows 构建及实机安装证据仍未完成。

渲染后的 MP4 清单保留正式 renderer 的 `subtitleEvidence`，包括实际字体文件摘要、Chromium 使用的字体字形及字幕半开帧区间。实际 Host 导出已经核对清单、文件摘要及时间边界；空白字幕成片的依据为 `[]`。这些信息属于成片复核依据，不增加对话日志入口，可编辑包也不包含本机字体字节。

GC 不根据当前页面未使用就删除原始素材。历史修订、导出清单、未封存作业、导入 pin、租约和备份引用的完整可达性及数据库/Blob 一致快照协议尚未完成，因此本轮没有删除用户素材。

## 后续改编来源的版本化更新

作者可在“设定与来源”的变化条目上选择“后续改编使用这个新版”。这是作者专用 `replaceEditionSource` 局部操作，按目标作品修订 CAS 保存新的精确基线；每份媒体修订独立持有来源列表，旧任务、旧分支和历史导出继续读取当时的来源。模型 MCP 不提供该作者操作，Native 也拒绝候选执行绕过。

来源切换不重写文档、素材或旧改编映射。旧映射继续显示实际来源变化，来源面板仅保留一个“在对话中分析变化”入口，将当前保存作品的精确身份和读取完整报告的要求追加到原对话草稿，不随来源条数增加讨论按钮。新任务读取被选择的新基线及已保存的旧映射，比较后再按实际授权局部修改。此流程不等于跨版本后台调度或自动生成成片。

仅切换基线时保留旧阅读依赖及其 freshness；内容写入仍检查 `mustBeCurrent`。模型可通过原 `readSet` 合同显式登记历史比较输入和当前制作输入，`sourceImpacts.readFreshness` 返回已有阅读依赖条件。来源选择、依赖核对、内容保存和素材生成分别有自己的事实与回执。


## 对话中的整体创作设计

漫画和动画文档可保存 `creativeDesign`，用于主 agent 按需组织整体故事线、人物变化、结构、改编取舍与近期安排。它是随作品修订保存的设计文本，不是作者确认、任务完成证据或强制开工表单。作者通过对话阅读和调整，模型用既有工具保存；也可在现有作品编辑区展开“创作设计”直接核对或修订。该文本区默认收起，复用整份作品的草稿、保存和恢复，不设置独立规划流程或保存按钮。

`read_media_workspace` 的 overview 在 `documentHeader.textFields` 返回设计的文本描述符，`section=text`、`textField={kind:"creativeDesign"}` 进行有界分页。`patch_media_document` 的 `setCreativeDesign` 设置或以 null 清除，局部修改复用 `replaceTextRanges`。保存仍受原 CAS、任务身份、来源依赖和幂等校验。下游可引用精确设计选段，选段读取不能扩展到全篇或其他正文。手动编辑与后台设计修改沿原文档合并和保存合同保留；同字段冲突时，既有最新保存版本对照显示只读设计，选择冲突内容不会自动保存。清空文本会移除可选设计字段。


## 改编版本创建的恢复

新建漫画/动画版本在发送前，按本地作者与项目保存精确来源、名称、媒介和幂等键。关闭窗口或重新打开应用后，创建入口恢复原请求，作者点击原提交按钮核对结果；不会自动发送，也不会用最新来源替换已选择的版本。本地存储不是作品权威，仍由 Native 返回原创建回执。成功打开后清除恢复记录；存储不可读/不可写时不发新创建请求。请求已记录后的“关闭”仅关闭窗口，不撤销已可能完成的创建。

创作设计支持直接选段引用：鼠标或键盘选择已保存文字后，现有对话和改编入口使用 `fieldText/creativeDesign` 精确原文与版本。未保存、后台改动导致原文不匹配或完整选区载荷超限时，需保存或重新选择后再引用；应用不截断选段、不自动扩大为全篇。取消对话引用仍使用原 composer 控件。

媒体作者保存回执在替换当前编辑内容前核对项目、作品范围、分支、媒介和修订。来源切换与共同设定采用还需保持原文档、旧映射及无关基线不变。异常回执保留当前内容和原重试请求，不据此宣称保存成功或推断后端未写入。

媒体文档的计划证据与最终交付分开：主任务运行中保存的漫画/动画文档，可用真实 taskId/revisionId 引用为“设计或分镜文档已保存”的成果证据。Native 核对同一 Work 当前 attempt、完整输出、绑定 edition/scope/branch 及分支当前修订；计划更新和新鲜度读取使用同一规则。任务再次修改文档后，旧引用失效，须重新核对并引用新回执；作者改稿还会触发原任务输入过期保护。失败/取消/未启动任务、其他 Work、作者独立保存以及不存在的回执不能冒充该任务的成果。文档保存不能证明图片/视频已生成、作者已采用或全流程已交付；计划本身不授予后续制作权限。

### 同一对话中的依赖改编

`read_adaptation_sources` 返回当前任务可用于改编的真实固定来源，`request_media_adaptation` 原子登记新漫画/动画版本和原 Work 的后继步骤。保存漫画后可以用实际修订登记动画，禁止预造未来来源。原对话由 Host 连续派发，继承原作者要求、累计任务与媒体额度；暂停、只读和来源绑定仍由 Native 校验。同键重放不会新建第二个版本。

改编登记回执不代表已经生成作品；登记步骤必须保存当前完整媒体文档才能结束。小说创作后的改编仍保留原 Work 最终审查，媒体文档进入真实交付证据。文件、素材生成和最终成片仍须各自实际回执，不能由文档保存或模型总结替代。界面只刷新已有版本选择器，不增加改编控制面板。

本链路已有 Native、Host 派发、跨范围累计额度与真实 Runtime/MCP/Host 受控媒体集成证据。真实模型全书规划/创作质量及独立电影生产尚未验收或实现；完整状态见 `design-reviews/2026-10-12-creation-implementation.md` 的“主线实施后审与收口证据”。

### 影视制作目标与成片时长

视听文档可保存可选 `production`：`form` 为 shortFilm / featureFilm / episode，`expression` 为 animation / liveAction / hybrid，`deliverable` 为 screenplay / storyboard / animatic / finishedVideo，`targetDuration` 为正有理时间或 null。模型通过现有 `patch_media_document` 的 `setScreenProduction` 修改，overview 与归档包含同一已保存目标；剧本文本仍保存在 creativeDesign，不另建重复剧本面板。

这是制作方案，不是作者授权或完成认证。目标是 finishedVideo 且声明时长时，渲染前核对时间线时长，生成后核对 FFprobe 实际时长，允许一帧编码误差。短片段不能冒充目标长片；当前单次本机导出仍限一小时，不能据此承诺已支持完整商业长片制作。原作者要求独立保存，模型修改制作目标不能抹去原交付义务。
