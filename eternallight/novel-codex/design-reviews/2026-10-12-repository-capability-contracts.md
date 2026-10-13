# Repository 用例能力合同收口

## 实施前

责任与事实权威：Story Core 保留 SQLite 连接、事务、恢复身份及全部领域校验。Runtime/Authoring 消费具名领域能力接口，不能通过固有方法获得未声明的全部业务能力。B01/B03/B05/B06。现有按目录拆分未消除 SqliteStoryRepository 的公开固有业务 API，是本轮结构缺口。

目标：以现有用例模块为能力粒度，将公开实例业务方法从固有 impl 移到具名 repository trait；原方法体、调用顺序与事务不变。连接初始化和已有借用式 store 构造保留在 composition root。调用方显式导入实际使用的能力，禁止统一 prelude/wildcard 重新引入全部能力；跨领域用例显式依赖所用能力。现有借用式 store 不再包一层代理，亦不把所有方法转发到隐藏的第二个 God object。

SQLite 实现同时实现多个能力是存储适配器职责；拆能力接口不拆事务、不建立第二份事实，也不宣称 Rust crate 级增量编译已解耦。纯构造函数和 crate 内基础 helper 不作为消费者能力公开。Runtime 执行器仍唯一负责同步工作的有界线程隔离。

状态与权限：只改变 Rust 静态能力解析；CAS、租约、恢复身份、执行代次、角色、预算、原子回滚和所有 HTTP 字段保持。零兼容：删除被替代的固有公开业务接口，调用方一次性同步，不留转发桥梁。

## 五视角前审（主代理五视角自审）

|视角|结论与关闭条件|
|---|---|
|架构|APPROVE WITH CONDITIONS：按实际用例能力拆分，调用方明确依赖，无全能力prelude，实例事务不变。|
|技术A|APPROVE WITH CONDITIONS：方法体不变，全部Native编译及事务/恢复回归。|
|技术B|APPROVE WITH CONDITIONS：同步所有production/tests/examples，门禁防公开固有业务接口回潮并有负例。|
|产品|APPROVE：不新增作者步骤，不修改领域行为。|
|UI/UX|APPROVE：该项无UI或协议改变。|

## 验证计划

1. AST 对比移动前后方法体，防纯结构变更意外改变业务；静态能力门禁及正负例。
2. Native 全 workspace 编译、单元与 HTTP 事务回归；类型生成检查，架构检查。
3. 本轮前端/Host/响应合同并行改动完成后统一集成。

## 实施后

待记录实际证据。未验证前不把前审条件视为关闭。

### 实施与首轮实证

- 308 个公开业务实例方法移入 116 个按现有用例归属的 trait，SQL/事务实现直接留在对应 impl，没有另加转发桥梁。公开固有实例方法仅余已有 6 个 store 构造入口。Core/Authoring/Runtime 与所有测试及示例同步显式导入所用能力。
- 移动后、格式化前，使用 syn 读取方法块逐一 SHA-256 比较：308/308 与移动前字节相同。随后仅对新增 trait/impl 段运行 rustfmt；该证据不宣称格式化后仍字节相同。
- `cargo check --workspace --all-targets --offline` 通过，无新增未用能力 import；已有测试 helper dead-code/unused 警告保留，未加全局 allow。
- Core lib 502/502、能力 AST 门禁 3/3 通过，门禁包含真实源码扫描及新增固有业务 API 的正负例。最终全 workspace 与合同回归随并行 DTO 完成后执行。

### 独立复核

`/root/host` 代理只读复核：116 个能力 impl 直接承载 SQL/事务，未发现转发壳或新增 Deref 全能力入口；Runtime/Authoring 无 wildcard 能力导入。308 方法齐全，其中 287 当前块仍与移动前 SHA 相同；其余 21 从较早备份提取方法，先逐一证实 SHA 与本轮移动前记录相同，再将其与当前块以同一 rustfmt 参数格式化，21/21 输出逐字节一致，确认只是格式差异。抽样检查 TaskEvidence/TaskActionPermit/SearchAudit/ProjectRevision 的授权与事务未变；无阻断发现。此为独立实现复核，不能替代下方完整运行验证。
