# 构建产物目录统一

## 前审：目标与归属

- 用户授权：按理想构建目录设计实施；交付出口唯一，验证事实不用目录名表达。
- 根因：设计缺陷。`build-layout.mjs`允许任意输出，`package-release.mjs`直接叠写最终目录并递归选择可执行文件；staging也叠写旧资源。目录存在不能证明本次构建、验证成功。
- 责任模块：产品构建脚本，适用B01/B03/B06/B10；不属于Host或创作领域。不修改作者资料、模型策略、权限与预算。
- 事实权威：构建脚本负责本次执行状态；现有release-manifest负责资源来源；最终目录逐文件摘要和启动检查结果由package-report记录。报告不表示文学质量或完整产品认证。
- 合同：固定`.release/resources`暂存；`.release/package`打包验证；通过后替换`release`，失败保留上一交付；删除输出/资源环境变量入口，同步CLI、CI、文档。Rust三个独立workspace保留各自target，避免改变上游配置、产物发现和构建身份；native/bin继续作为模块加载位置。
- 清理：核验四份旧release变体只包含生成的app和builder文件后，保留最新一份到release，删除其余重复产物。记录其为已有产物、未重新验证，不冒充当前源码构建。
- 验证：临时目录运行真实文件操作覆盖重复构建、失败保留、并发拒绝、摘要绑定、符号链接与替换恢复；已有release-runtime合同回归；architecture:check。当前媒体bundle和构建身份若不满足实际发布前置，记录真实阻断，不降低门禁。

## 状态判定

| 输入 | 结果 | 副作用 |
| --- | --- | --- |
| 本次构建及smoke通过 | 替换release并写绑定产物摘要的报告 | 清理前次输出 |
| staging/build/smoke失败 | 记录失败阶段 | 原release保持；候选仅留在.release |
| 同时发起打包 | 拒绝第二次 | 不清理另一个构建 |
| 替换中断 | 依据备份与最终目录恢复 | 不把未验证候选发布 |
| 历史应用归一化 | 仅调整存放位置 | 不赋予验证通过状态 |

## 五视角前审（同一执行者自审）

| 视角 | 前审 | 后审 |
| --- | --- | --- |
| 架构 | APPROVE：构建脚本统一目录与报告合同 | APPROVE：E1/E2，修改限定构建层 |
| 技术A | APPROVE WITH CONDITIONS：失败保留、锁、摘要及路径边界回归 | APPROVE：E1覆盖原风险；异常杀进程后的锁需确认PID结束再移除 |
| 技术B | APPROVE WITH CONDITIONS：CLI/CI/文档同步，重复构建不残留 | APPROVE：E1/E2合同与调用方同步；实际整包范围见E4 |
| 产品 | APPROVE：只在release查找应用，不以目录名假称通过 | APPROVE：E3唯一出口；E4未冒称新版已生成 |
| UI/UX | APPROVE：不变更产品界面，命令与报告明确成功/失败 | APPROVE：报告区分本次失败、已有产物和通过；无界面改动 |

零兼容：删除被替代的环境变量入口；无fallback、双轨、作者数据迁移。历史证据文件不改写。

## 实施后证据

| 编号 | 实际执行与结果 | 证明范围与限制 |
| --- | --- | --- |
| E1 | `npm run test:release-gate` 21/21；`node --test scripts/tests/package-release.test.mjs electron/tests/release-runtime.test.mjs` 17/17 | 打包流程使用受控命令执行器和真实文件操作；macOS/Windows x64/arm64路径、重复打包、staging/资源校验/build/builder/smoke失败、包内篡改、并发锁、替换失败与中断恢复、符号链接拒绝。资源回归使用真实回环Runtime/MCP fixture，验证重建无陈旧资源、失败保留。不是实际Windows打包或真实媒体渲染。 |
| E2 | `npm run architecture:check`：365文件/7 crates，0 diagnostics；`git diff --check`通过 | 静态边界与差异；保留共享工作区已有修改。CI删除重复staging及旧路径变量，回归加入现有release-gate。 |
| E3 | 五份输出清点仅含app、图标缓存与builder日志；进程检查无相关运行中应用。最新清单时间为2026-10-06 23:21（上海），来自release-conversation；移到release并清理四份旧副本 | `.release/output-consolidation.json`保存原目录/清单摘要/来源；`release/package-report.json`明确unverified。移动后505项文件/链接记录与摘要完全一致，根目录仅release，verifyPackageReport正确拒绝其作为已验证新包。未操作作者资料。 |
| E4 | 实际执行`node scripts/stage-release-runtime.mjs`在写入前因缺少`NOVEL_CODEX_RELEASE_MEDIA_RUNTIME`失败 | 现有发布前置未满足，未生成当前源码的新应用，未执行真实整包与最终应用smoke。无绕过门禁或伪造验证。后续提供合规媒体bundle及匹配源码的构建输入后运行package命令。 |

初次资源回归有两次环境/路径反馈：macOS临时目录经`/var`别名触发普通目录保护，fixture改用realpath；沙箱不允许回环监听，获执行环境批准后原用例通过。未放宽生产目录边界或测试断言。

实现：`build-layout`固定路径；`build-artifacts`集中目录替换、锁和文件摘要；`stage-release-runtime`在新目录生成并替换；`package-release`在候选目录完成资源复制完整性与启动验证，最后替换release；新增`verify:package`核对交付与报告。报告只证明列出的构建检查，不代替模型资格、完整产品或创作质量验收。

后审综合：APPROVE（目录设计与实现）；零兼容PASS，B01–B10相关边界PASS，无新增领域/权限/预算合同。真实整包验证受E4阻断，不能声明当前源码已发布；无额外兼容待办。Rust缓存不合并：独立workspace配置、调用路径及增量构建身份均有现实使用方，不为减少目录数量扩大上游改动。
