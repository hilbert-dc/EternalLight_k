import assert from "node:assert/strict";
import { createServer } from "node:http";
import { access, mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import {
  CAPABILITIES, compileCodexConfig, compileCodexModelCatalog,
  defineConnection, resolveSessionPlan,
} from "@eternallight/agent-engine";
import { verifiedBaselineEvidence } from "@eternallight/agent-engine/test-support";
import { CodexAppServerAdapter } from "../codex-adapter.mjs";
import { resolveCodexCli } from "../codex-runtime.mjs";

const projectRoot = resolve(fileURLToPath(new URL("../..", import.meta.url)));
const executableSuffix = process.platform === "win32" ? ".exe" : "";

function within(promise, milliseconds, label) {
  let timer;
  return Promise.race([
    promise,
    new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`${label} timed out`)), milliseconds); }),
  ]).finally(() => clearTimeout(timer));
}

test("Codex forwards the novel knowledge schema without dropping source evidence", { timeout: 45_000 }, async (t) => {
  const cli = resolveCodexCli({ explicitPath: process.env.NOVEL_CODEX_CLI, isPackaged: false, projectRoot, resourcesPath: projectRoot });
  if (!cli.path) {
    if (process.env.NOVEL_CODEX_CLI) assert.fail(`Configured Codex CLI is unavailable: ${cli.reason}`);
    return t.skip(`Codex CLI is unavailable: ${cli.reason}`);
  }
  const mcpCommand = join(projectRoot, "native", "target", "debug", `novel-workspace-mcp${executableSuffix}`);
  try { await access(mcpCommand); }
  catch { return t.skip("novel-workspace-mcp has not been built"); }

  const root = await mkdtemp(join(tmpdir(), "novel-codex-schema-"));
  let adapter;
  let provider;
  t.after(async () => {
    adapter?.close();
    if (provider?.listening) await new Promise(resolve => provider.close(resolve));
    await rm(root, { recursive: true, force: true });
  });
  const home = join(root, "codex-home");
  await mkdir(join(home, "sqlite"), { recursive: true });

  let resolveCaptured;
  const captured = new Promise(resolve => { resolveCaptured = resolve; });
  provider = createServer(async (request, response) => {
    try {
      let body = "";
      for await (const chunk of request) {
        body += chunk;
        if (body.length > 2_000_000) throw new Error("provider request exceeds diagnostic bound");
      }
      const parsed = JSON.parse(body);
      const tools = parsed.tools ?? [];
      resolveCaptured({ path: request.url, stream: parsed.stream,
        tool: tools.flatMap(tool => tool.tools ?? [tool]).find(tool => tool.name === "publish_knowledge_batch") });
    } catch (error) { resolveCaptured({ error: error.message }); }
    response.writeHead(503, { "content-type": "application/json" });
    response.end(JSON.stringify({ error: { message: "schema probe completed" } }));
  });
  await new Promise(resolve => provider.listen(0, "127.0.0.1", resolve));

  const connection = defineConnection({
    id: "schema-probe", provider: "fixture", model: "schema-probe", endpoint: "https://example.invalid",
    credentialRef: "SCHEMA_PROBE_KEY", protocol: "responses",
    declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 },
  });
  const now = new Date();
  const plan = resolveSessionPlan({
    connections: [connection],
    evidence: verifiedBaselineEvidence(connection, { now }),
    now,
    policy: { defaultConnectionId: connection.id, requiredCapabilities: [CAPABILITIES.STRUCTURED_TOOL_CALLS],
      allowedMcpServerIds: ["novel_workspace"], forceBridge: true },
  });
  const catalogPath = join(home, "catalog.json");
  const baseInstructions = await readFile(resolve(projectRoot, "../../codex-rs/models-manager/prompt.md"), "utf8");
  await writeFile(catalogPath, JSON.stringify(compileCodexModelCatalog(plan, { baseInstructions })));
  await writeFile(join(home, "config.toml"), compileCodexConfig({
    sessionPlan: plan, modelCatalogPath: catalogPath,
    bridge: { baseUrl: `http://127.0.0.1:${provider.address().port}`, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" },
    mcpServers: [{ id: "novel_workspace", command: mcpCommand, args: [],
      envVars: ["NOVEL_CODEX_RUNTIME_URL", "NOVEL_CODEX_CANDIDATE_TOKEN", "NOVEL_CODEX_TASK_CAPABILITY", "NOVEL_CODEX_OPERATION"],
      approvalMode: "approve" }],
  }));

  adapter = new CodexAppServerAdapter({
    command: [cli.path, "app-server"], cwd: root,
    env: { PATH: process.env.PATH, CODEX_HOME: home, CODEX_SQLITE_HOME: join(home, "sqlite"),
      AGENT_ENGINE_BRIDGE_TOKEN: "schema-probe-only", NOVEL_CODEX_RUNTIME_URL: "http://127.0.0.1:1",
      NOVEL_CODEX_CANDIDATE_TOKEN: "probe-candidate", NOVEL_CODEX_TASK_CAPABILITY: "probe-capability",
      NOVEL_CODEX_OPERATION: "modelKnowledge" },
    model: connection.id, provider: connection.provider,
  });
  let ready;
  const started = new Promise(resolve => { ready = resolve; });
  adapter.setCallbacks({ onEvent: event => {
    if (event.method === "mcpServer/startupStatus/updated" && event.params?.name === "novel_workspace" && event.params?.status === "ready") ready();
  } });
  await within(adapter.ensureSession(), 15_000, "Codex session");
  await within(started, 15_000, "MCP startup");
  void adapter.sendTurn([{ type: "text", text: "Inspect the available tools." }], { disableEnvironment: true }).catch(() => {});
  const result = await within(captured, 15_000, "provider request");
  assert.equal(result.path, "/responses");
  assert.equal(result.stream, true);
  assert.ok(result.tool, result.error ?? "publish_knowledge_batch was not forwarded");
  const schema = result.tool.parameters ?? result.tool.function?.parameters;
  assert.ok(schema, "published tool has no parameters");
  assert.ok(Buffer.byteLength(JSON.stringify(schema)) <= 5000, "Codex forwarded an oversized knowledge schema");
  assert.deepEqual(schema.$defs.E.required, ["quote"]);
  assert.ok(schema.required.includes("entities") && schema.required.includes("assertions"));
  const event = schema.properties.assertions.items.anyOf.find(item => item.properties.kind.enum?.includes("event"));
  assert.deepEqual(event.properties.value.required, ["participants", "preconditions", "effects", "untilExclusive"]);
});
