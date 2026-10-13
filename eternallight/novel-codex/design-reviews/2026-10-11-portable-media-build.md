# 便携媒体组件生产链

## 前审

用户授权继续完成媒体构建与打包。根因为设计缺口：现有发布校验器只消费外部完整 bundle，工程没有从固定来源生成该输入的步骤。责任属于构建层（B01/B03/B06/B10），不改变创作权限、领域事实或运行时媒体校验。

目标：为本机 macOS 提供源码锁定的静态 FFmpeg/x264 构建，配套固定版本 Noto SC 字体和各自许可证；输出到 `.release/media-runtime/<platform>-<arch>`，来源缓存与工作目录留在 `.release`。现有校验器仍是发布准入权威，只有通过真实空 PATH、Electron 字幕及 H.264/AAC 帧回读才替换上一个合格 bundle。Windows 保留显式合格 bundle 输入，本次不声称已实现 Windows 源码构建或实机验证。

固定源：FFmpeg 8.1.2（官方 tar.xz，SHA256 由本机发行配方核对）、x264 r3222 固定提交、Noto CJK Sans2.004 官方标签提交；每份下载经仓库锁定摘要校验才提取或使用。编译仅允许系统动态库，x264 静态链接；禁止用开发机共享库代替便携输入。许可证取自对应固定版本，不编写替代文本。

状态：缺源/摘要不符→拒绝且保留旧产物；编译失败→保留日志与失败报告；校验/smoke失败→不发布；全通过→原子替换合格 bundle；并发→构建锁拒绝。来源身份、构建命令及生成文件摘要是构建事实，不代表完整作品验收。

验证：下载摘要和污染缓存反例、失败保留/锁；实际本机编译、动态依赖闭包和真实 smoke；现有 release gate、architecture:check，最终应用隔离启动。没有网络下载或编译完成前不标记证据通过。

五视角自审前审：架构 APPROVE（独立构建生产者+既有验证消费者）；技术 A/B APPROVE WITH CONDITIONS（来源锁定、原子替换及实际运行验证）；产品/UI APPROVE WITH CONDITIONS（只在包及实际启动通过后报告新版可用）。零兼容 PASS，无旧路径fallback或作者数据迁移。

## 后审

实际改动：新增固定来源锁和独立媒体构建脚本；输出布局登记目标平台组件目录；发布 CLI 默认使用该目录；README/MEDIA_DELIVERY 与发布测试入口同步。目标路径拒绝路径片段。没有修改领域权限、作品数据或运行时准入标准。

E1（合同）：来源污染/符号链接、归档越界、目标路径、缓存失败保留、构建锁、发布失败恢复及媒体清单校验共 25 项通过；完整 `test:release-gate` 25 项通过（与前项存在重叠，不相加为独立证据）。测试临时目录初次因 macOS `/var` 符号链接被生产防护拒绝，fixture 改为 realpath 后通过，未放松生产检查。

E2（实际媒体）：`npm run build:media-runtime` 于 2026-10-11 成功，来源锁 SHA256 `9f5b0acbc7afcf4e671678db5247ae80a885e9d3d8f16c469bcd371db9d39465`，媒体清单 SHA256 `54b03a88373b04dc7966e53685d6d98bc0b8b3fd962244e0b6e152ab926ec18b`。空 PATH、Electron 38.8.6、H.264/AAC、FFprobe 与字幕三帧回读通过，像素 0/280/0，字体为包内 Noto Sans SC 且每个字形为 custom font。初次沙箱运行 Electron 无有效回执，保留失败；同一候选在允许桌面启动的环境验证通过，再运行完整生产链成功，未绕过 gate。完整证据 `.release/media-build-report.json`。

E3（真实应用）：`package:dir` 成功，staging、runtime-verification、frontend-build、packaging、packaged-resources、application-smoke 全部通过；应用以隔离用户目录启动。最终 `release/mac-arm64/小说工坊.app`，报告 `release/package-report.json`；`verify:package` 再核对全部产物摘要通过。Core BuildIdentity `16a9358ffa7c5b8cd275d915f126391c5dc4829885f54c12fadb61ac03833f30`，CLI SHA256 `952af09d2eb0e027bbc99951db5ba9b5eea89aa4827c0341c4c560443bf2e961`。本机验证使用已测试 debug Core/Runtime，可运行包不等于正式签名、公证或安装器交付。

E4（静态）：`architecture:check` 扫描 365 文件/7 crates，无诊断；`git diff --check` 通过。

五视角自审后审：架构 APPROVE（E4，构建生产与发布准入职责分开）；技术 A APPROVE（E1/E2，来源、失败保留、锁与实际闭包）；技术 B APPROVE（E2/E3，完整标准入口实际执行）；产品与 UI/UX APPROVE（E3，新包可用且状态由报告证明）。零兼容 PASS，B01/B03/B06/B10 PASS，无新增权限或作者数据迁移。前审要求的本机实际编译、真实媒体、包及隔离启动条件全部关闭。

根因闭环：之前输出目录整理保留了消费/验证链，但缺少便携组件生产者；现已补齐，并由打包默认消费标准目录。本机缺组件阻断已解决。未覆盖：Windows 源码构建/实机验证、远端 CI、Developer ID 签名/公证，以及前 100 章真实创作。此次验收没有调用付费模型或改动作者原作。
