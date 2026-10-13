# Multimedia HTTP 合同覆盖

前审：Core既有CreativeEdition/MultimediaWorkspace/CanonWorkspace及嵌套媒体类型是事实权威，Runtime只增加可反序列化校验的响应根包装；对Core现Value分页投影按实际producer建立具名DTO（目录、来源影响、文本分页）。10个直接HTTP入口登记明确路由；6个Host文件/预览/导出/连接组合入口登记本地职责。TS消费生成根类型，Host仅以Omit派生资产previewUrl投影，不独立复制HTTP字段。响应校验不授权写入或重试，缺字段/错误枚举应拒绝，合法null按producer接受。

五视角自审：架构 APPROVE WITH CONDITIONS（复用Core而非再造媒体schema）；技术A APPROVE WITH CONDITIONS（实际HTTP含空目录、分页、null及保存回执）；技术B APPROVE WITH CONDITIONS（生成check、TS消费与gate）；产品 APPROVE（不新增作者步骤）；UI/UX APPROVE（预览URL仍由Host发放）。验证由合同集成代理统一接线/生成/定向真实HTTP，我负责类型与元数据。

## 后审
10个HTTP根已登记并生成，版本列表/创建、workspace读写/patch与canon直接复用Core DTO；目录/来源影响/text-page按实际producer建立分页合同。6项本地Host组合API有明确职责说明。TS公共媒体实体、来源选择器和文档直接引用Core生成类型；HostMediaWorkspace用Omit只派生assets预览能力。Core有8个字段使用skip_serializing_if，本来就应省略；补齐#[ts(optional)]生成标注，保留三个媒体组件原来的省略region/range与删除creativeDesign行为，不引入any或断言。来源影响未固定canon的snapshotId确实可null，严格接收null但拒绝字段遗漏；作者text页只能partialView=true、version=source和非负坐标。

集成证据：生成966份Rust合同；109个Runtime/media方法覆盖门禁通过；应用tsc零诊断。两项真实媒体HTTP目标共8/8通过（含保存/重开、权限隔离、原子patch、Unicode文本页、真实本地生成asset与seal、快照目录和历史来源不变），两项DTO结构负例通过。日志/tmp/media-{contract-generation,http-contract-tests,root-contract-tests,tsc-current,contract-gate}.log。第一次沙箱HTTP有1项本地监听被拒，已沙箱外同目标全部复验通过，没有改变断言。

五视角后审：架构/技术A/技术B/产品/UI-UX均APPROVE；真实HTTP条件关闭；最后8个skip字段标注须由主集成再次生成及TS检查后关闭生成/TS条件。该证据验证传输与权限，不是媒体模型输出质量或真实付费供应商验收。
