# DeepSeek 委派消息编码对照（2026-09-20）

## 实现

新增通用连接选项 `protocolOptions.agentMessageMode`：

- `native`（默认）：保留 Responses 的 `agent_message`。
- `message`：将明文 `agent_message` 编码为普通 user message，显式标明它是 Agent 间通信而非新的终端用户指令，保留发送者、接收者和原始文本。遇到加密或不支持的内容拒绝转换，不假装解密。

该选项属于连接协议能力，不按模型名称编写分支；参与现有 connection digest。原有模型目录、认证清单均未修改。

修正 echo 工具过时的“两份随机值”说明。失败诊断增加类型、长度、是否包含正确值、是否包含前缀、首个不同位置等信息，不记录原始 token，不将预期值泄露给模型。严格相等校验未放宽。

## 实测

两组均使用同一 DeepSeek 连接基础设置、相同新版工具说明、相同认证流程和预算；每组各运行一次，每次独立生成随机挑战。因此这是初步对照，不是确定性配对试验或稳定性统计。

| 项目 | native | message |
|---|---|---|
| 协议及核心工具闭环 | 通过 | 通过 |
| spawn / 同模型路由 | 通过 | 通过 |
| 父消息真实投递 | 通过 | 通过 |
| 子请求可读输入 | 通过 | 通过 |
| 子 Agent token 消费 / MCP 回执 | 通过 | 通过 |
| message-running 整体完成 | 超时 | 通过 |
| 独立 wait 场景 | 通过 | 通过 |
| 整套认证 | 失败 | 通过 |

原生组出现两次调用已撤下 gate 工具的授权错误，随后 echo 成功，但场景未在 90 秒内正常结束。因此不能把该失败描述为“模型完全看不到 agent_message”，也不能把总体超时简单归因为编码格式。

普通消息组完整通过，message-running 使用 7 次请求。这证明当前配置能够完成本次被测多 Agent 链路，不代表所有 Codex 能力均通过，也不证明任意模型通用或长期稳定。

报告：

- [原生组](../novel-codex/.certification-reports/2026-09-20T15-06-00.353Z-9cbf05655eda51d3e7a6a575dc6e67ff4cafc8961a56ee0991ae242a5ecc09eb.json)
- [普通消息组](../novel-codex/.certification-reports/2026-09-20T15-06-19.372Z-5e074f80e812d2f2a1849aea43fcba7afad640cdee6663948b185274247ca0bd.json)

## 验证与后续

- 135 项引擎测试通过。
- `npm run test:delegation-integration`：两种消息模式均通过真实 Codex + 本地模拟供应商 + MCP 集成测试。
- `just fmt` 通过。
- 实测使用临时连接目录和 qualify（非发布）入口，未切换应用默认配置或发布认证。
- 下一步固定普通消息配置进行重复 smoke，保留所有失败；确认结果后再更新实际连接配置并按新连接摘要正式认证。若要判断编码差异的因果关系，应增加交错顺序、多样本的对照，避免将随机波动当作结论。

## 后续重复验证与启用

普通消息模式新增两轮完整 smoke 均通过，随后对实际工程配置再运行一轮正式认证并成功发布。包括首次对照，共四轮通过；这是有限样本验证，不是任意任务可靠性保证。

- [第二轮 smoke](../novel-codex/.certification-reports/2026-09-20T15-11-25.041Z-6739e8f8ffea002adc6398c0cb1805484b5882533eb16ceeecbf26df7cd61bf2.json)
- [第三轮 smoke](../novel-codex/.certification-reports/2026-09-20T15-11-42.754Z-1af82c42f35f0fabde7f6662991536baaddea918ac2edd6b24afae15ce4abfb6.json)
- [正式发布报告](../novel-codex/.certification-reports/2026-09-20T15-12-25.245Z-a5da526871fcebcda2e96099b6c04f903bc24993d8a116383af4fda770b1a6f2.json)

实际 `model-catalog.json` 已设置 `agentMessageMode: message`，校验清单同步更新。正式认证 ID：`9394dce029f5743c9024dc77804b215433063148c8db6ff6e97312d700982b90`；profile 为 `directed-delegation-v1`。应用侧认证加载验证返回 `deepseek-flash: verified`，七项已验证能力为 text、structuredToolCalls、toolLoop、childSpawn、childMessageRunning、childWait、childDirectedRoute。真实模型的路由验证仅覆盖同模型父子路由，不代表跨供应商任意组合已认证。

本次仅切换当前连接及其认证，不新增历史兼容路径、降低校验标准、修改权限或暴露密钥。未重启正在运行的应用；已有进程可能需要重启才能加载新的模块级配置。
