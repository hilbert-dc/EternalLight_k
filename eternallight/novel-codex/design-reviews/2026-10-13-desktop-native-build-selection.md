# 桌面联调的当前 Native 构建选择

## 前审
完整verify在CARGO_TARGET_DIR=/tmp/eternallight-boundaries-target、NOVEL_CODEX_NATIVE_DIR指向其debug目录下构建。production-desktop fixture却固定链接源码树native/target/debug（Oct12旧产物），实际绕过本次构建，导致getState仍用旧DTO。不是允许旧协议或放宽校验：测试应选择当前配置的构建目录。

责任：测试基础设施。复用nativeTestBinary唯一解析器，优先显式NOVEL_CODEX_NATIVE_DIR（绝对路径），否则Cargo target/debug，未设置才默认native/target/debug。desktop runtime链接及MCP OS启动包装均调用同解析器；生产Host在隔离应用内仍使用真实二进制路径。新增优先级测试和真实runtime链接断言，错误只报contract/path/validationIssue，不输出originalResponse正文。

B01/B10：不改DTO、权限、作品或认证缓存，不跳过invalidEvidence用例。五视角前审：架构 APPROVE；技术A APPROVE（单一构建选择）；技术B APPROVE WITH CONDITIONS（解析器与全部5桌面场景）；产品 APPROVE；UI/UX APPROVE（模型不可用时手动作品读写仍可验收）。

## 后审
实际当前Runtime使用 /tmp/eternallight-boundaries-target/debug/novel-codex-story-runtime；desktop每次启动前用realpath断言临时应用链接指向所选构建，MCP启动包装编译时同样调用nativeTestBinary选择当前MCP。相对Cargo target按运行构建/测试的cwd解析，显式NOVEL目录必须绝对。

完整5个真实桌面场景通过，含invalidEvidence的实际createProject→getState wire读取；模型缓存仍拒绝信任、零模型请求、作品手动读写可用。加2个解析器单测共7/7（/tmp/desktop-current-build.log），真实Codex/MCP schema转发1/1（/tmp/native-selection-schema.log）。无DTO/业务生产修改，不放宽校验。autonomous与codex-tool-schema复用解析器消除重复路径逻辑；condition/rebind等由并行代理接同helper。native-binaries.test.mjs已由主代理纳入package.json test:agent常规门禁。diffcheck通过；五视角后审均APPROVE，条件关闭。
