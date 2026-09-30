# 小说检索与影响集合评估

## 目标与范围

补齐 S2/S4/S5 的可复现领域评估入口，而不是把字面规模基准称为语义召回验收。当前生产检索和显式影响路径作为基线；外部标注中的自然语言问题可原样执行并暴露遗漏，不自动改写问题、扩大查询或用模型自评补齐答案。

运行 `npm run --silent evaluate:story -- <数据库路径> <标注 JSON 路径>`。`--silent` 避免 npm 启动日志混入标准输出的 JSON；Cargo 构建日志仍在标准错误。退出 0 表示两个已测通道及每类结果满足阈值，退出 2 表示低于阈值或通道未测，退出 1 表示输入、冻结或金标无效。任何退出码都不能证明整体架构或文学验收。使用 host-local `PathBuf`/`Path`，不加入 App Server 协议路径类型或旧格式兼容；该选择遵循 path-types 技能的本地路径约定。

## 数据与结果契约

- 标注包含数据集 ID、修订、作品 ID、声明的标注来源（engineeringFixture / humanReviewed、reviewer、reference），以及有 ID、类别、1–24 场景编号的 evidence / impact 用例。输入最多 1 MB、256 用例、每例 64 预期项；未知字段、重复身份、空预期与非法锚点拒绝。
- evidence 查询使用生产正文检索前 20 项；预期证据绑定 objectId、revisionId、JSON Pointer、Unicode 字符起止与原文 quote。返回范围必须覆盖预期锚点才算召回；同一修订里的无关命中、重复命中和错误源不能加分。
- impact 查询使用生产显式影响报告（至多三跳、50 条路径），预期项区分实体与正文对象。路径与字面候选的来源锚点分别验证，实体路径不冒充正文修改集；同一目标多条路径不重复计分。显式路径完整不等于全局影响完备。
- 输出记录标注文件及冻结数据库的 SHA-256、查询方法、用例与分类召回、遗漏身份、查询错误、无效返回锚点、源覆盖与截断、耗时。输出不含原文 quote、查询文本、数据库路径、秘密或全文；仅写本机标准输出，不上传遥测。
- 阈值采用目标文档的 90%；除总分外逐类别/通道判断，缺少通道报告为未测，不能因没有预期项得到 100%。任何查询失败或无效返回证据不能判为通过。
- 标注 provenance 是外部声明，不是本程序证明的人工评审。输出固定声明 `fullArchitectureAcceptanceVerified=false`：覆盖了哪些编号不等于验证了全部二十四类隔离/事务场景，召回达标也不是文学质量、真实模型或 Windows 验收。

## 隔离与一致性

源数据库使用 SQLite read-only 打开；`VACUUM INTO` 在新建私有临时目录生成一致副本，包含已提交 WAL 内容，不用简单文件复制丢失 WAL。仅副本交给生产仓储初始化与查询。全轮没有作者作品写入、模型请求、任务创建、索引重建或凭证读取。未知/旧 schema 不提供升级兼容。所有预期及返回来源核对作品与当前选择/指定快照成员，不以全库存在的 revision 当作本作品证据。

## 单人五视角自检与实施计划

用户要求由单人推进，不组织评审。架构：评估示例不扩大生产 crate API，不新增领域权威；技术 A：冻结副本、Unicode 锚点、范围和硬预算；技术 B：真实生产保存/建模/查询及负向回归；产品：显示遗漏而非优化漂亮分数；UX：本地报告不曝光正文，并明确当前仅测基线。零兼容门禁通过，无新依赖、迁移、模型专用分支或降级。

实现放在 story-core example 及其私有 support 模块，测试单独文件。用当前 schema 的真实保存与发布建立小型工程样本，验证有效召回、自然语言遗漏、重复/空预期、错源、坏锚点、分类阈值及原库不变。该样本明确不作为人工金标。

## 实施后证据

已实现 story_retrieval_evaluation example 及私有 support 模块，无生产 crate API、模型调用或新依赖。Cargo 将此 example 的测试纳入默认 native 测试入口，不只运行专项后留在验证链外。报告只保留覆盖字段中的对象、数值、布尔/null 结构，不透传 searchedNames、原文或 query；返回锚点仍按真实原文核验。输入与报告均有明确数据集身份和摘要。

六项回归通过：真实生产创建/保存/建模/完成任务后查询；中文与 emoji 字符位置；自然语言改述遗漏与未测通道；总分 90% 但一个类别 0% 时阻断；重复/空预期、错对象/修订/字段和坏锚点；损坏派生文本和查询失败；提交到 WAL 的新作品被副本看见且原主文件/WAL 均不改变。fixture 是执行者编写的短篇工程样本，人工评审声明固定未独立验证，不作为二十四类人工金标或文学样本。

专项与最终 `CARGO_NET_OFFLINE=true npm run test:runtime` 均通过，最终测试包含 95 项核心单测、全部领域/HTTP 集成、8 项 MCP 单测和新增 6 项评估回归。第一次完整运行在受限沙箱无法绑定本地端口；使用相同入口的授权回环环境重跑全部通过，没有删除或跳过测试。独立 native 工作区严格 Clippy（`--workspace --all-targets --locked --offline -- -D warnings`）、`npm run repo:check` 及 `npm run build` 均退出 0。前端大包体警告仍存在；本轮不以这些结果宣称真实模型、文学质量或 Windows 已验证。

工程样本的字面证据与显式因果/正文影响预期均完全召回；原样提问“为什么张衡没能按时抵达？”时，字面基线 evidence recall 为 0。该结果只说明当前直接字面检索不能自行理解改述，不代表使用 Codex Query Planner 分解关键词后的最终对话必然失败。基线 query_ranking 使用 literalOverlap/semantic=false；本入口的结果不能包装成语义质量认证。

后续生产协议已新增 [来源绑定的 Codex 语义候选重排](SEMANTIC_QUERY_RANKING_DESIGN.md)，但当前 evaluator 仍执行直接字面检索与显式影响查询，不运行真实模型规划/打分。重排不能召回基准未返回的证据，因此仍需真正语义召回通道、自主规划/排序的独立端到端评估及人工金标。

生产查询现另有 [无关键词原文召回](PASSAGE_RECALL_DESIGN.md)，可由 Codex 逐页按含义阅读授权正文；协议回环测试不是自然语言模型质量评估，当前 evaluator 不会自动使用这个通道。后续应在独立标注数据上分别记录字面基线、模型自主阅读与组合策略的召回、范围、费用与遗漏，不能由现有搜索结果反生成金标。

实施后单人五视角复核：架构保持生产查询权威且评估不改正式内容；技术 A 只读副本、WAL 一致性与范围核对通过；技术 B 新入口纳入正常测试、未知字段拒绝且未加兼容；产品把遗漏、未测和失败作为结果；UX 输出不带原文并明确声明标注与验收边界。未组织独立专家评审。

仍需经过授权的人工标注作品、语义检索改进、真实模型连续作者旅程及双平台验收，不缩减理想架构的完成条件。人工预期证据应独立标注并绑定源坐标，不能把当前搜索返回结果自动反生成金标，再拿它评估同一检索器。

## 标注示例

以下身份对应工程测试样本，并非随安装附送的作者作品。使用真实数据库时必须替换为人工确认的作品、不可变修订和原文坐标；起止位置是 Unicode 字符而非 UTF-8 字节。预期锚点是召回单位，同修订不同字段不会自动等价。

```json
{
  "datasetId": "port-engineering-v1",
  "revision": "1",
  "projectId": "evaluation-port",
  "annotation": {
    "kind": "engineeringFixture",
    "reviewer": "implementer",
    "reference": "repository-fixture"
  },
  "cases": [
    {
      "id": "literal",
      "category": "literal",
      "scenario": 10,
      "query": {
        "type": "evidence",
        "text": "锁门",
        "snapshotId": null,
        "expected": [
          { "objectId": "draft-chapter-1", "revisionId": "draft-r1",
            "path": "/sceneGroups/0/body", "start": 3, "end": 5, "quote": "锁门" }
        ]
      }
    },
    {
      "id": "causal-impact",
      "category": "explicitDependency",
      "scenario": 17,
      "query": {
        "type": "impact",
        "entityId": "lock",
        "snapshotId": null,
        "worldTime": 2,
        "maxHops": 2,
        "expected": [
          { "type": "entity", "id": "late" },
          { "type": "text", "objectId": "draft-chapter-1" }
        ]
      }
    }
  ]
}
```
