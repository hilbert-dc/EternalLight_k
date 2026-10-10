import { requestReasoningConfiguration } from "../electron/model-reasoning.mjs";
import { randomBytes } from "node:crypto";
import { mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { ModelBridgeProcess, TaskBudget, compileCodexConfig, compileCodexModelCatalog, connectionDefinitionDigest, createExecutionLease } from "@eternallight/agent-engine";
import { CodexAppServerAdapter } from "../electron/codex-adapter.mjs";
import { CodexCoreIntegrationRunner } from "../electron/codex-core-integration-runner.mjs";
import { NOVEL_ROOT_DEVELOPER_INSTRUCTIONS } from "../electron/creative-mission.mjs";
import { NOVEL_SUBAGENT_DEVELOPER_INSTRUCTIONS, composeNovelCodexPrompt, createQualificationContextDigest,
  credentialScopesFromEnvironment, inspectCodexConnection, loadCodexEvidence, resolveCodexCli, resolveCodexProvider, verifyCodexConnection } from "../electron/codex-runtime.mjs";
import { loadProjectDevelopmentEnvironment, resolveDevelopmentEnvironment } from "../electron/development-credentials.mjs";
import { createMcpStartupWaiter } from "../electron/tests/helpers/mcp-startup.mjs";
import { limited } from "./probe-live-story.mjs";

const projectRoot = resolve(fileURLToPath(new URL("..", import.meta.url)));
// Language judgments are reviewed from actual commentary/final text, never
// inferred from character ratios or promoted to production routing rules.
const scenarios = [
  { id: "zh-term", locale: "zh-CN", expected: "中文进度及讨论，保留英文术语和原句", input: '先读测试素材，说明 cliffhanger 的效果，并引用 "Tomorrow is already here."。只讨论，不修改。' },
  { id: "english-input", locale: "zh-CN", expected: "English progress and discussion despite Chinese locale", input: "Read the sample and briefly discuss why the ending creates suspense. Do not change the story." },
  { id: "japanese", locale: "ja-JP", expected: "日本語の進捗と説明。引用は原文を保持", input: "サンプルを読み、結末の緊張感について短く説明してください。原文は変更しないでください。" },
  { id: "english-artifact", locale: "zh-CN", expected: "中文交流、英文续写、原文不被改写", input: "先读测试素材，简短说明构思，再提供两句英文续写，作为建议即可，不保存。" },
  { id: "explicit-switch", locale: "ja-JP", expected: "English progress and final as explicitly requested", input: "先读测试素材，接下来的交流和进度全部使用英文，简短解释结尾，不改原文。" },
  { id: "bilingual", locale: "en-GB", expected: "Requested Chinese/English bilingual explanation is legitimate", input: "Read the sample. Explain the ending in one short Chinese sentence and its English translation; preserve the quoted original line." },
];

export async function probeLiveLanguage({ connectionId = null, reasoningEffort = null, lifecycle = false } = {}) {
  const environment = resolveDevelopmentEnvironment(loadProjectDevelopmentEnvironment(projectRoot, process.env));
  const selected = inspectCodexConnection({ model: connectionId, reasoningEffort });
  if (!environment[selected.credentialRef]) throw new Error(`Development credential is unavailable for ${selected.id}`);
  const cli = resolveCodexCli({ explicitPath: environment.NOVEL_CODEX_CLI, isPackaged: false, projectRoot, resourcesPath: projectRoot });
  if (!cli.path) throw new Error(cli.reason ?? "Codex CLI unavailable");
  const root = await mkdtemp(join(tmpdir(), "novel-language-probe-"));
  const baseInstructions = (await readFile(join(projectRoot, "..", "..", "codex-rs", "models-manager", "prompt.md"), "utf8")).trim();
  const salt = join(root, "credential-salt");
  await writeFile(salt, `${randomBytes(32).toString("base64url")}\n`, { mode: 0o600 });
  const credentialScopes = credentialScopesFromEnvironment(environment, salt);
  const qualificationContextDigest = createQualificationContextDigest({ baseInstructions, cliPath: cli.path });
  const evidencePath = join(root, "evidence.json");
  process.stdout.write(`${JSON.stringify({ stage: "qualification", root })}\n`);
  const coreRunner = new CodexCoreIntegrationRunner({ baseEnvironment: environment, baseInstructions,
    developerInstructions: NOVEL_ROOT_DEVELOPER_INSTRUCTIONS, subagentDeveloperInstructions: NOVEL_SUBAGENT_DEVELOPER_INSTRUCTIONS,
    command: [cli.path, "app-server"], cwd: root, nodeCommand: process.execPath });
  await verifyCodexConnection({ baseEnvironment: environment, coreRunner, credentialScopes, evidencePath,
    provider: selected.provider, model: selected.id, reasoningEffort, qualificationContextDigest });
  const provider = resolveCodexProvider({ provider: selected.provider, model: selected.id, reasoningEffort, credentialScopes,
    evidence: await loadCodexEvidence(evidencePath), qualificationContextDigest });
  const requests = [];
  const bridge = new ModelBridgeProcess({ sessionPlan: provider.sessionPlan, connections: provider.connections,
    credentialResolver: async ref => environment[ref],
    requestConfigurationProvider: ({ connectionId }) => requestReasoningConfiguration(provider.connections, provider, connectionId),
    requestObserver: request => requests.push({ runId: request.runId, connectionId: request.connectionId, reasoningEffort: request.reasoningEffort }) });
  const results = [];
  try {
    const address = await bridge.start();
    for (const scenario of scenarios) {
      const home = join(root, scenario.id);
      await mkdir(join(home, "sqlite"), { recursive: true, mode: 0o700 });
      const catalog = join(home, "catalog.json");
      await writeFile(catalog, JSON.stringify(compileCodexModelCatalog(provider.sessionPlan, { baseInstructions })), { mode: 0o600 });
      let config = compileCodexConfig({ sessionPlan: provider.sessionPlan,
        developerInstructions: NOVEL_ROOT_DEVELOPER_INSTRUCTIONS, subagentDeveloperInstructions: NOVEL_SUBAGENT_DEVELOPER_INSTRUCTIONS,
        modelCatalogPath: catalog, bridge: { baseUrl: address.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" },
        mcpServers: [{ id: "novel_workspace", command: process.execPath,
          args: [join(projectRoot, "electron", "tests", "fixtures", "language-probe-mcp.mjs")], approvalMode: "approve" }] });
      await writeFile(join(home, "config.toml"), config, { mode: 0o600 });
      const budget = new TaskBudget();
      await bridge.beginTaskBudget(budget);
      const lease = createExecutionLease({ sessionPlan: provider.sessionPlan, runId: `language-${scenario.id}`, budgetId: budget.id });
      await bridge.beginExecutionLease(lease);
      const adapter = new CodexAppServerAdapter({ command: [cli.path, "app-server"], cwd: home,
        languagePreferences: () => ({ preferredLanguages: [scenario.locale] }), modelRoute: provider.sessionPlan.defaultModel,
        env: { PATH: process.env.PATH, CODEX_HOME: home, CODEX_SQLITE_HOME: join(home, "sqlite"), AGENT_ENGINE_BRIDGE_TOKEN: address.token } });
      const messages = [], toolCalls = [];
      let waiter;
      try {
        adapter.startProcess();
        waiter = createMcpStartupWaiter({ serverName: "novel_workspace", process: adapter.process, timeoutMs: 15000 });
        adapter.setCallbacks({ onEvent: event => {
          waiter.onEvent(event);
          const item = event.params?.item;
          if (event.method === "item/completed" && item?.type === "agentMessage") messages.push({ phase: item.phase, text: item.text });
          if (event.method === "item/completed" && item?.type === "mcpToolCall") toolCalls.push({ tool: item.tool, status: item.status });
        } });
        await adapter.ensureSession();
        await waiter.ready;
        const prompt = composeNovelCodexPrompt("", scenario.input);
        const result = await limited(adapter.sendTurn(prompt.input, { disableEnvironment: true, additionalContext: prompt.additionalContext }), 90000, scenario.id);
        results.push({ ...scenario, messages: [...messages], toolCalls: [...toolCalls], final: result.message });
        if (lifecycle && scenario.id === "explicit-switch") {
          for (const followup of [
            { id: "switch-back", expected: "中文过程与答复，引用保持原文", input: "现在改用中文交流。重新读取素材，解释断掉的钟绳为什么制造悬念，引用保留原文。" },
            { id: "artifact-after-switch", expected: "中文说明、英文建议正文，不保存", input: "再读一次素材，用中文说明构思，再给两句英文续写建议。不保存。" },
          ]) {
            const start = messages.length, toolsStart = toolCalls.length;
            const next = composeNovelCodexPrompt("", followup.input);
            const answer = await limited(adapter.sendTurn(next.input, { disableEnvironment: true, additionalContext: next.additionalContext }), 90000, followup.id);
            results.push({ ...followup, parentScenario: scenario.id, messages: messages.slice(start), toolCalls: toolCalls.slice(toolsStart), final: answer.message });
            process.stdout.write(`${JSON.stringify({ stage: "observed", id: followup.id, messages: messages.length - start })}\n`);
          }
        }
        process.stdout.write(`${JSON.stringify({ stage: "observed", id: scenario.id, messages: messages.length, toolCalls: toolCalls.length })}\n`);
      } finally {
        waiter?.dispose();
        await adapter.close();
        await bridge.endExecutionLease(lease);
        await bridge.endTaskBudget(budget);
      }
      await writeFile(join(root, "results.json"), JSON.stringify({ qualificationContextDigest,
        connection: { id: selected.id, provider: selected.provider, model: selected.model, protocol: selected.protocol,
          protocolOptions: selected.connection.protocolOptions, digest: connectionDefinitionDigest(selected.connection) },
        selectedReasoningEffort: selected.reasoning?.effort ?? null, requests, results }, null, 2), { mode: 0o600 });
    }
  } finally {
    await bridge.close();
  }
  return { status: "observed-awaiting-language-review", root, scenarios: results.length };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (!process.argv.includes("--confirm-cost")) {
    process.stderr.write("This opt-in probe uses the selected catalog connection for real inference; pass --confirm-cost [--connection=id].\n");
    process.exitCode = 2;
  } else {
    try { process.stdout.write(`${JSON.stringify(await probeLiveLanguage({ lifecycle: process.argv.includes("--lifecycle"), connectionId: process.argv.find(arg => arg.startsWith("--connection="))?.slice("--connection=".length) ?? null, reasoningEffort: process.argv.find(arg => arg.startsWith("--reasoning-effort="))?.split("=")[1] ?? null }))}\n`); }
    catch (error) { process.stderr.write(`${JSON.stringify({ status: "failed", message: String(error?.message ?? error).replace(/sk-[A-Za-z0-9_-]+/g, "[REDACTED]") })}\n`); process.exitCode = 1; }
  }
}
