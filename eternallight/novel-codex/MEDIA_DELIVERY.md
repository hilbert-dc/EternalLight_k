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

字幕正式链路统一为固定字体 → 私有 Electron 内存页面 → 透明 PNG → FFmpeg 最上层叠加，不再使用 drawtext。原工具缺少 drawtext 不再阻塞此字幕链。实际开发组件已验证中文字体、H.264/AAC 与字幕前/中/后帧；字体为明确配置的本机测试资源，没有复制到发布包。本机 Homebrew 8.1.2 仍存在 17 个 Homebrew 外部依赖，不能直接作为便携包。当前还没有合格便携组件和跨平台安装包实机证据。

渲染后的 MP4 清单保留正式 renderer 的 `subtitleEvidence`，包括实际字体文件摘要、Chromium 使用的字体字形及字幕半开帧区间。实际 Host 导出已经核对清单、文件摘要及时间边界；空白字幕成片的依据为 `[]`。这些信息属于成片复核依据，不增加对话日志入口，可编辑包也不包含本机字体字节。

GC 不根据当前页面未使用就删除原始素材。历史修订、导出清单、未封存作业、导入 pin、租约和备份引用的完整可达性及数据库/Blob 一致快照协议尚未完成，因此本轮没有删除用户素材。
