# 工程职责规则与改动门禁

## 变更与实施前评审

目标：把自主创作3.0的职责划分变成当前工程必须遵守的规则，接入父级/产品 AGENTS、评审模板、开发验证及现有小说CI。规则覆盖小说及其他创作媒介；不添加文学流程或新的执行引擎。

范围：`../ARCHITECTURE_RULES.md` 为唯一规则正文；AGENTS和README仅引用。自动依赖检查单独评审于 `2026-10-08-architecture-boundary-check.md`。同步修正已发现的 shared 类型反向依赖：DiscussionExcerpt/ViewId 归共享契约，Renderer 引用该定义。接口字段和行为不变。

| 视角 | 实施前结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE | 一份规则；模型/Engine/Host/Native/传输/UI/方法来源的职责明确；不另建事实库或执行调度层。 |
| 技术A | APPROVE WITH CONDITIONS | AST与Cargo依赖检查有非法导入反例；类型依赖同样受约束；不通过豁免既有越界使检查变绿。 |
| 技术B | APPROVE WITH CONDITIONS | CI和本地入口接线；共享类型与调用方同步；区分静态检查、合同测试、受控provider与真实模型证据。 |
| 产品 | APPROVE | 不规定每轮必需计划/方法/子任务；显式作者控制与模型自主选择并存；不自动增加用户确认。 |
| UI/UX | APPROVE | 不增加产品UI；规则要求准确、简洁的状态投影，内部诊断按需查看。 |

零兼容门禁：PASS。没有旧版本识别、迁移、回填、双读写或fallback；现有类型入口重导出共享定义不产生第二份协议或旧行为路径。设计评审不是额外用户授权流程。

## 实施后复核

| 视角 | 实际复核 | 结论 |
| --- | --- | --- |
| 架构 | 唯一规则正文；父AGENTS及产品AGENTS引用一致；模型选择与确定性控制分开，共享类型下沉至共同层，无第二事实库或新文学流程。 | PASS |
| 技术A | AST检查静态导入/类型/重导出/动态加载，Cargo检查生产和构建路径关系；拒绝测试入口借道与外层符号链接。已补macOS路径别名和检出目录祖先名为tests/fixtures的反例。 | PASS |
| 技术B | 本地repo检查、verify间接入口、CI构建前均接线；类型与引用验证通过。现有Fork失败保持，不伪造整个仓库绿色。 | PASS |
| 产品 | 零方法/零子任务仍合法，短输入验收不由外部人代写；验证按实际影响选择，未要求每次付费模型或全量重跑。 | PASS |
| UI/UX | 无新产品控件；共享类型字段不变；规则明确真实状态与可理解错误，界面行为保持。 | PASS |

实际Root执行：

- `npm run architecture:check`：exit0，332个生产文件、7个Rust crate，0诊断，`/tmp/novel-architecture-rules-check3.log`。
- `npm run test:architecture`：exit0，16/16，0失败/取消/跳过，`/tmp/novel-architecture-rules-tests3.log`。首次3/15的路径别名失败和中间15/15均保留；最终第16项证明检出位置不能造成全量漏扫。
- `tsc --noEmit`：exit0，`/tmp/novel-architecture-rules-types.log`；原有选区引用行为5/5、exit0，`/tmp/novel-architecture-rules-excerpts.log`。
- 使用现有 `js-yaml` 解析CI并验证两个门禁位于集成构建之前：exit0；未新增运行依赖。限定文件 `git diff --check`：exit0。
- `npm run repo:check`：内部架构检查通过，随后既有Fork门禁exit1；仍存在未登记的 `codex-rs` 改动及旧release路径诊断，见 `/tmp/novel-architecture-rules-repo-check.log`。本轮未放宽Fork门禁或删除这些文件；远端CI和分支保护未执行或改动。

零兼容复核PASS。源码身份见 `/tmp/novel-architecture-rules-evidence.json`。语义边界依赖设计评审和有意义的合同测试，自动依赖检查不宣称能证明全部架构或创作质量。当前上一轮工具契约与真实创作验收的未通过结果继续保留，本规则交付不将其改写为通过。
