import { randomBytes } from "node:crypto";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

// Product-neutral Codex core tool-loop integration runner.

import {
  CAPABILITIES,
  CodexAppServerClient,
  ModelBridgeProcess,
  TaskBudget,
  compileCodexConfig,
  compileCodexModelCatalog,
  connectionDefinitionDigest,
  contractHash,
  createExecutionLease,
  credentialRealmId,
  defineQualificationSuiteResult,
  digest,
  getCodec,
  resolveSessionPlan,
} from "@eternallight/agent-engine";

const DEFAULT_MCP_PATH = fileURLToPath(new URL("./core-conformance-mcp.mjs", import.meta.url));
const ENVIRONMENT_KEYS = ["PATH", "PATHEXT", "SystemRoot", "WINDIR", "ComSpec", "TEMP", "TMP", "TMPDIR", "LANG", "LC_ALL", "SSL_CERT_FILE", "SSL_CERT_DIR", "HTTP_PROXY", "HTTPS_PROXY", "NO_PROXY"];

function environmentFrom(source) {
  return Object.fromEntries(ENVIRONMENT_KEYS.filter((key) => typeof source[key] === "string" && source[key]).map((key) => [key, source[key]]));
}

function within(promise, timeoutMs, message) {
  let timer;
  return Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(message)), timeoutMs); })]).finally(() => clearTimeout(timer));
}

async function waitForClose(child, timeoutMs = 2_000) {
  if (!child || child.exitCode !== null || child.signalCode !== null) return;
  await Promise.race([new Promise((resolve) => child.once("exit", resolve)), new Promise((resolve) => setTimeout(resolve, timeoutMs))]);
}

export class CodexCoreIntegrationRunner {
  constructor({ baseEnvironment = process.env, baseInstructions, command, cwd, nodeCommand = process.execPath, mcpPath = DEFAULT_MCP_PATH, timeoutMs = 60_000 }) {
    if (!Array.isArray(command) || command.length === 0) throw new Error("core integration runner requires the current Codex App Server command");
    if (typeof baseInstructions !== "string" || !baseInstructions.trim()) throw new Error("core integration runner requires upstream Codex instructions");
    this.baseEnvironment = environmentFrom(baseEnvironment);
    this.baseInstructions = baseInstructions;
    this.command = command;
    this.cwd = cwd;
    this.nodeCommand = nodeCommand;
    this.mcpPath = mcpPath;
    this.timeoutMs = timeoutMs;
  }

  async verify({ apiKey, connection, credentialScope = connection.credentialRef, protocol, probeEvidence, qualificationContextDigest = "default", now = new Date() }) {
    const selectedConnection = connection;
    const sessionPlan = resolveSessionPlan({
      connections: [selectedConnection],
      evidence: probeEvidence,
      now,
      policy: { capabilityProfileId: "text-v1", requiredCapabilities: [CAPABILITIES.STRUCTURED_TOOL_CALLS], credentialScopes: { [selectedConnection.id]: credentialScope }, defaultConnectionId: selectedConnection.id, forceBridge: true, qualificationContextDigest, allowedMcpServerIds: ["engine_core"] },
    });
    const root = mkdtempSync(join(tmpdir(), "codex-core-conformance-"));
    const nonce = randomBytes(18).toString("base64url");
    const instruction = `core-${randomBytes(18).toString("base64url")}`;
    const receipt = randomBytes(18).toString("base64url");
    const proofPath = join(root, "proof.json");
    const bridge = new ModelBridgeProcess({ sessionPlan, connections: [selectedConnection], credentialResolver: async () => apiKey });
    const credentialRealm = credentialRealmId(credentialScope);
    const evidenceRecord = (status, assertionIds = []) => ({
      capability: CAPABILITIES.TOOL_LOOP,
      status,
      scope: { protocol, suite: "codex-engine-core-integration-v1", mode: "app-server-mcp", toolTypes: ["function"] },
      source: "conformance-suite",
      credentialRealm,
      environmentDigest: sessionPlan.routes[0].environmentDigest,
      evidenceRef: `core-conformance:${selectedConnection.id}:${protocol}:${now.toISOString()}:tool-loop`,
      checkedAt: now.toISOString(),
      expiresAt: new Date(now.getTime() + 24 * 60 * 60 * 1000).toISOString(),
      sample: { suiteId: "codex-engine-core-integration-v1", suiteVersion: 1, outcome: status === "verified" ? "passed" : "failed" },
      assertions: (assertionIds.length ? assertionIds : ["tool-loop-completion"]).map((id) => ({
        id,
        result: status === "verified" ? "passed" : "failed",
        expected: true,
        observed: status === "verified",
        traceRef: proofPath,
      })),
      dependencies: {
        modelProtocol: { digest: digest({ connectionDigest: connectionDefinitionDigest(selectedConnection), protocol, codecVersion: getCodec(protocol).version }), ref: `${selectedConnection.id}:${protocol}` },
        engineBuild: { digest: digest({ engineContractHash: contractHash(), qualificationContextDigest }), ref: qualificationContextDigest },
        access: { digest: credentialRealm, ref: `${selectedConnection.id}:credential-realm` },
      },
      supersedes: [],
    });
    let adapter;
    let budget;
    let lease;
    try {
      const address = await bridge.start();
      budget = new TaskBudget({ maxRequests: 4, maxTokens: 50_000, maxDurationMs: this.timeoutMs });
      lease = createExecutionLease({ sessionPlan, runId: `core-${nonce}`, budgetId: budget.id, mode: "conformance", maxDurationMs: this.timeoutMs });
      await bridge.beginTaskBudget(budget);
      await bridge.beginExecutionLease(lease);
      const catalogPath = join(root, "model-catalog.json");
      writeFileSync(catalogPath, `${JSON.stringify(compileCodexModelCatalog(sessionPlan, { baseInstructions: this.baseInstructions }), null, 2)}\n`, { mode: 0o600 });
      writeFileSync(join(root, "config.toml"), compileCodexConfig({
        sessionPlan,
        modelCatalogPath: catalogPath,
        bridge: { baseUrl: address.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" },
        mcpServers: [{ id: "engine_core", command: this.nodeCommand, args: [this.mcpPath], envVars: ["ELECTRON_RUN_AS_NODE", "ENGINE_CORE_NONCE", "ENGINE_CORE_INSTRUCTION", "ENGINE_CORE_RECEIPT", "ENGINE_CORE_PROOF_PATH"], approvalMode: "approve" }],
      }), { mode: 0o600 });
      mkdirSync(join(root, "sqlite"));
      chmodSync(root, 0o700);
      adapter = new CodexAppServerClient({
        command: this.command,
        cwd: this.cwd,
        env: { ...this.baseEnvironment, CODEX_HOME: root, CODEX_SQLITE_HOME: join(root, "sqlite"), ELECTRON_RUN_AS_NODE: "1", AGENT_ENGINE_BRIDGE_TOKEN: address.token, ENGINE_CORE_NONCE: nonce, ENGINE_CORE_INSTRUCTION: instruction, ENGINE_CORE_RECEIPT: receipt, ENGINE_CORE_PROOF_PATH: proofPath },
        model: selectedConnection.id,
        turnTimeoutMs: this.timeoutMs,
      });
      await within(adapter.ensureSession(), this.timeoutMs, "Codex core conformance App Server did not initialize");
      const result = await within(adapter.sendTurn([{ type: "text", text: `Call engine_core_echo exactly once with nonce ${nonce} and instruction ${instruction}. Return only its exact result.` }], { disableEnvironment: true, threadName: "Codex core integration conformance" }), this.timeoutMs, "Codex core conformance turn timed out");
      const proof = existsSync(proofPath) ? JSON.parse(readFileSync(proofPath, "utf8")) : null;
      const passed = proof?.receipt === receipt && proof.callerThreadId === proof.rootThreadId && result.message.includes(`CORE_CONFORMANCE_RECEIPT:${receipt}`);
      const suite = defineQualificationSuiteResult({ kind: "core-integration", suiteId: "codex-engine-core-integration-v1", suiteVersion: 1, status: passed ? "passed" : "failed", blockingFailures: passed ? [] : ["core-tool-loop-incomplete"] });
      return {
        ...suite,
        record: evidenceRecord(passed ? "verified" : "failed", passed ? ["tool-loop-completion", "root-tool-selected", "root-tool-executed", "tool-result-consumed", "root-turn-completed"] : []),
      };
    } catch (error) {
      return {
        ...defineQualificationSuiteResult({ kind: "core-integration", suiteId: "codex-engine-core-integration-v1", suiteVersion: 1, status: "failed", blockingFailures: ["core-tool-loop-error"], metrics: { message: String(error?.message ?? error).slice(0, 300) } }),
        record: evidenceRecord("failed"),
      };
    } finally {
      const child = adapter?.process;
      adapter?.close();
      await waitForClose(child);
      if (lease) await bridge.endExecutionLease(lease).catch(() => undefined);
      if (budget) await bridge.endTaskBudget(budget).catch(() => undefined);
      await bridge.close().catch(() => undefined);
      rmSync(root, { recursive: true, force: true, maxRetries: 5, retryDelay: 50 });
    }
  }
}
