# Native 测试启动产物评审

日期：2026-10-08。仅改变本轮隔离验收构建命令的调试信息设置，不修改工程 profile、生产代码、系统安全、权限、断言或业务超时。

## 已观察事实与方案

默认测试二进制约100MB。多个首次启动进程长时间无Rust输出；对sqlite_concurrency测试的只读sample显示96KB footprint、唯一线程停在`_dyld_start`。原默认全量及逐组日志持续保留；未退出不算通过，启动等待不算业务测试失败。

在独立临时`CARGO_TARGET_DIR`使用`CARGO_PROFILE_TEST_DEBUG=0 cargo test --offline --locked`。仍为test profile、同一源码/锁文件/功能与全部断言。先用代表性目标的`--no-run --message-format=json`确认实际artifact的`opt_level=0`、`debuginfo=0`、`debug_assertions=true`、`overflow_checks=true`、`test=true`，再记录大小、SHA、启动及实际完整测试退出。更小产物可能改善启动，是待测假设。不能用bare rustc cfg、去符号构建成功或旧批通过代替当前运行。

Cargo将debug、debug-assertions、overflow-checks分别配置，test默认继承dev，见[官方profiles说明](https://doc.rust-lang.org/cargo/reference/profiles.html)。去调试信息可能减少堆栈符号细节，所有失败日志、测试名和原默认构建证据仍保留。

## 五视角实施前评审

| 视角 | 结论 | 条件 |
| --- | --- | --- |
| 架构 | APPROVE WITH CONDITIONS | 工程合同不变，临时构建策略和当前源码版本分别记录 |
| 技术A | APPROVE WITH CONDITIONS | 实际Cargo test artifact确认两类检查仍开；不关闭系统保护、放大超时或隐藏失败 |
| 技术B | APPROVE WITH CONDITIONS | 离线锁定依赖，当前全量不得筛选/跳过；产物与源码SHA及实际exit匹配 |
| 产品 | APPROVE WITH CONDITIONS | 只降低验收等待；不增加作者输入、预算或权限，不宣称文学验收通过 |
| UI/UX | APPROVE WITH CONDITIONS | 无界面改动；实机应用和实际DeepSeek的未决边界保持 |

零兼容门禁：PASS。没有profile持久修改、fallback、版本探测、迁移、双轨生产接口或安全检查减免。

## 实施后复核

代表性 Engine Core lib30/30、当前 Native Core lib262/262 和 MCP93/93 已默认完整执行并实际exit0。实际 Cargo JSON 均为 opt_level=0、debuginfo=0、debug_assertions=true、overflow_checks=true、test=true；源码和测试二进制前后hash留存。Engine代表性产物约24MB；当前Native Core约30MB、MCP约18MB。当前相关工作区组286/286记录于 `/tmp/autonomous-v3-workspace-current10c-nodebug/manifest.json`，MCP完整记录于 `/tmp/autonomous-v3-workspace-mcp-current10-nodebug.result.json`。首轮失败日志保留。

五视角后审：架构合同与生产配置未改；技术A确认断言和溢出检查仍开；技术B确认默认完整目标、实际exit与hash；产品及UI确认不改变作者成本/权限、不替代实际模型与界面验收。零兼容PASS。这些是临时test profile代表性产物的实测，不证明所有macOS冷启动因果或完整Native/Node通过。默认Node所需Native开发bins继续按工程原dev profile重新构建，本轮未引入CARGO_PROFILE_DEV_DEBUG覆盖。
