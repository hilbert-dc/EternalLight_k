import { randomBytes } from "node:crypto";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

// Product-neutral Codex child-lifecycle conformance runner.

import {
  CAPABILITIES,
  CodexAppServerClient,
  DELEGATION_EXECUTION_CAPABILITIES,
  BoundedExecutionTrace,
  ModelBridgeProcess,
  TaskBudget,
  compileCodexConfig,
  compileCodexModelCatalog,
  classifyConformanceFailure,
  connectionDefinitionDigest,
  contractHash,
  createExecutionLease,
  credentialRealmId,
  defineConnection,
  digest,
  getCodec,
  resolveSessionPlan,
} from "@eternallight/agent-engine";
import { evaluateDelegationCapability, observedTerminalChildWait, summarizeCollaborationEvents } from "./delegation-proof.mjs";
import { acknowledgedMessage, textDigest } from "../src/observability/message-evidence.mjs";
import { directedRouteDependencyDigest } from "../src/session/directed-route-proof.mjs";

const DEFAULT_MCP_PATH = fileURLToPath(new URL("./conformance-mcp.mjs", import.meta.url));
const CONFORMANCE_ENVIRONMENT_KEYS = [
  "PATH",
  "PATHEXT",
  "SystemRoot",
  "WINDIR",
  "ComSpec",
  "TEMP",
  "TMP",
  "TMPDIR",
  "LANG",
  "LC_ALL",
  "SSL_CERT_FILE",
  "SSL_CERT_DIR",
  "HTTP_PROXY",
  "HTTPS_PROXY",
  "NO_PROXY",
];

function isolatedEnvironment(baseEnvironment) {
  return Object.fromEntries(CONFORMANCE_ENVIRONMENT_KEYS
    .filter((key) => typeof baseEnvironment[key] === "string" && baseEnvironment[key])
    .map((key) => [key, baseEnvironment[key]]));
}

function within(promise, timeoutMs, message) {
  let timer;
  return Promise.race([
    promise,
    new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(message)), timeoutMs); }),
  ]).finally(() => clearTimeout(timer));
}

async function waitForConformanceProof({ proofPath, receipt, observedResponses, events, timeoutMs }) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    let proof = null;
    try {
      if (existsSync(proofPath)) proof = JSON.parse(readFileSync(proofPath, "utf8"));
    } catch {
      // The sidecar may still be committing the small proof file.
    }
    const observedTools = new Set(observedResponses.flatMap((response) => response.toolNames ?? []));
    const waitCompleted = events.some((event) => event.method === "item/completed" && event.params?.item?.type === "collabAgentToolCall" && event.params.item.tool === "wait");
    if (proof?.receipt === receipt && proof.callerThreadId !== proof.rootThreadId
      && ["spawn_agent", "send_message", "wait_agent"].every((name) => observedTools.has(name)) && waitCompleted) {
      return proof;
    }
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
  throw new Error("Codex conformance proof timed out");
}

async function waitForObservation(predicate, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (predicate()) return true;
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
  return predicate();
}

function bootstrapPlan(plan) {
  const bootstrapEdge = Object.freeze({ parentConnectionId: plan.rootRoute, childConnectionId: plan.rootRoute });
  const edges = plan.allowedChildEdges?.length ? plan.allowedChildEdges : [bootstrapEdge];
  const childModels = plan.allowedChildModels?.length ? plan.allowedChildModels : [plan.rootRoute];
  return Object.freeze({
    ...plan,
    id: `bootstrap-${plan.id}`,
    // The isolated conformance lease must exercise one exact self-route before
    // that route can earn durable multi-agent evidence. This grant exists only
    // inside the disposable probe plan and is never published as evidence.
    allowedChildModels: Object.freeze([...childModels]),
    allowedChildEdges: Object.freeze([...edges]),
    routes: Object.freeze(plan.routes.map((route) => Object.freeze({
      ...route,
      enabledExtensions: Object.freeze([...new Set([...route.enabledExtensions, CAPABILITIES.TOOL_LOOP, ...DELEGATION_EXECUTION_CAPABILITIES])].sort()),
      authorizedExtensions: Object.freeze([...new Set([...(route.authorizedExtensions ?? []), ...DELEGATION_EXECUTION_CAPABILITIES])].sort()),
    }))),
  });
}

function boundedFailure(error) {
  return String(error instanceof Error ? error.message : error).slice(0, 500);
}

function readMcpTrace(path) {
  if (!existsSync(path)) return [];
  return readFileSync(path, "utf8").split(/\r?\n/).filter(Boolean).flatMap((line) => {
    try { return [JSON.parse(line)]; } catch { return []; }
  });
}

function toolNames(tools) {
  return (tools ?? []).flatMap((tool) => [tool?.name, tool?.namespace && tool?.name ? `${tool.namespace}.${tool.name}` : null]).filter(Boolean);
}

function selectedToolNames(toolCalls) {
  return (toolCalls ?? []).map((call) => call?.namespace && call?.name ? `${call.namespace}.${call.name}` : call?.name).filter(Boolean);
}

function directedEdgeAssertions(observations = {}, traceRef) {
  const evaluated = evaluateDelegationCapability(CAPABILITIES.CHILD_DIRECTED_ROUTE, observations, traceRef);
  return evaluated.assertions;
}

function assertionsPassed(assertions) {
  return assertions.every((entry) => entry.result === "passed");
}

function capabilityMetrics(records) {
  return Object.freeze(records.map((record) => Object.freeze({
    capability: record.capability,
    status: record.status,
    assertions: Object.freeze(record.assertions.map((assertion) => Object.freeze({
      id: assertion.id,
      result: assertion.result,
    }))),
  })));
}

function diagnosticTarget(scenario, gatePath) {
  if (scenario === "spawn-route") return Object.freeze({ phase: "spawn", expectedTool: "spawn_agent", requestScope: "root" });
  if (scenario === "wait") return Object.freeze({ phase: "wait", expectedTool: "wait_agent", requestScope: "root" });
  if (!existsSync(gatePath)) return Object.freeze({ phase: "child-gate", expectedTool: "engine_conformance_gate", requestScope: "child" });
  return Object.freeze({ phase: "child-echo", expectedTool: "engine_conformance_echo", requestScope: "child" });
}

async function waitForProcessClose(child, timeoutMs = 2_000) {
  if (!child || child.exitCode !== null || child.signalCode !== null) return;
  await Promise.race([
    new Promise((resolve) => child.once("exit", resolve)),
    new Promise((resolve) => setTimeout(resolve, timeoutMs)),
  ]);
}

export class CodexConformanceRunner {
  constructor({ baseEnvironment = process.env, baseInstructions, command, cwd, nodeCommand = process.execPath, mcpPath = DEFAULT_MCP_PATH, timeoutMs = 90_000, runProbe = null }) {
    if (!Array.isArray(command) || command.length === 0) throw new Error("conformance runner requires the current Codex App Server command");
    if (typeof baseInstructions !== "string" || !baseInstructions.trim()) throw new Error("conformance runner requires upstream Codex instructions");
    this.baseInstructions = baseInstructions;
    this.baseEnvironment = isolatedEnvironment(baseEnvironment);
    this.command = command;
    this.cwd = cwd;
    this.nodeCommand = nodeCommand;
    this.mcpPath = mcpPath;
    this.timeoutMs = timeoutMs;
    this.runProbe = runProbe ?? ((input) => this.#run(input));
  }

  async verify({ apiKey, connection, credentialScope = connection.credentialRef, protocol, probeEvidence, qualificationContextDigest = "default", now = new Date() }) {
    const selectedConnection = defineConnection(connection);
    const plan = resolveSessionPlan({
      connections: [selectedConnection],
      evidence: probeEvidence,
      now,
      policy: {
        capabilityProfileId: "text-v1",
        requiredCapabilities: [CAPABILITIES.STRUCTURED_TOOL_CALLS],
        credentialScopes: { [selectedConnection.id]: credentialScope },
        defaultConnectionId: selectedConnection.id,
        allowedChildConnectionIds: [selectedConnection.id],
        forceBridge: true,
        qualificationContextDigest,
        allowedMcpServerIds: ["engine_conformance"],
      },
    });
    const environmentDigest = plan.routes[0].environmentDigest;
    const credentialRealm = credentialRealmId(credentialScope);
    const evidenceRef = `conformance:${selectedConnection.id}:${protocol}:${now.toISOString()}:delegation`;
    const scenarioObservations = {};
    const makeRecords = (sample, observations = {}) => DELEGATION_EXECUTION_CAPABILITIES.map((capability) => {
      const scenario = capability === CAPABILITIES.CHILD_WAIT ? "wait" : capability === CAPABILITIES.CHILD_MESSAGE_RUNNING ? "message-running" : "spawn-route";
      const evaluated = evaluateDelegationCapability(capability, { ...scenarioObservations[scenario], terminated: true }, sample?.eventDigest ?? evidenceRef);
      return {
        capability,
        status: evaluated.status,
        scope: {
          protocol,
          suite: "codex-engine-delegation-execution-v1",
          mode: "app-server-mcp",
          toolTypes: ["function"],
          behaviors: [capability],
          ...(capability === CAPABILITIES.CHILD_MESSAGE_RUNNING && observations.forcedChildToolChoice === true ? { parameters: { selectionMode: "forced-child-tool-choice" } } : {}),
          ...(capability === CAPABILITIES.CHILD_DIRECTED_ROUTE ? { routes: [{ parentConnectionId: selectedConnection.id, childConnectionId: selectedConnection.id }] } : {}),
        },
        source: "conformance-suite",
        credentialRealm,
        environmentDigest,
        evidenceRef: `${evidenceRef}:${capability}`,
        checkedAt: now.toISOString(),
        expiresAt: new Date(now.getTime() + 24 * 60 * 60 * 1000).toISOString(),
        sample: { ...sample, outcome: evaluated.status === "verified" ? "passed" : evaluated.status === "failed" ? "failed" : "inconclusive" },
        assertions: evaluated.assertions,
        dependencies: {
          modelProtocol: { digest: digest({ connectionDigest: connectionDefinitionDigest(selectedConnection), protocol, codecVersion: getCodec(protocol).version }), ref: `${selectedConnection.id}:${protocol}` },
          engineBuild: { digest: digest({ engineContractHash: contractHash(), qualificationContextDigest }), ref: qualificationContextDigest },
          access: { digest: credentialRealm, ref: `${selectedConnection.id}:credential-realm` },
          ...(capability === CAPABILITIES.CHILD_DIRECTED_ROUTE ? { route: { digest: digest([{ parentConnectionId: selectedConnection.id, childConnectionId: selectedConnection.id }]), ref: `${selectedConnection.id}->${selectedConnection.id}` } } : {}),
        },
        supersedes: [],
      };
    });
    const stages = [];
    const observations = { selectedOperations: [], completedOperations: [], failedOperations: [], forcedChildToolChoice: false };
    const eventDigests = [];
    const merge = (incoming = {}) => {
      for (const field of ["selectedOperations", "completedOperations", "failedOperations"]) {
        observations[field] = [...new Set([...observations[field], ...(incoming[field] ?? [])])].sort();
      }
      for (const field of ["childIdentityObserved", "childGateObserved", "childResultObserved", "childTerminalResultObserved", "directedRouteObserved", "parentFollowupTokenQueued", "childFollowupVisible"]) {
        observations[field] ||= incoming[field] === true;
      }
      observations.forcedChildToolChoice ||= incoming.forcedChildToolChoice === true;
    };
    for (const scenario of ["spawn-route", "message-running", "wait"]) {
      try {
        const result = await this.runProbe({ credentials: { [selectedConnection.credentialRef]: apiKey }, connections: [selectedConnection], sessionPlan: bootstrapPlan(plan), scenario });
        merge(result.observations);
        scenarioObservations[scenario] = result.observations;
        eventDigests.push(result.eventDigest);
        stages.push({ stageId: scenario, status: "passed", eventDigest: result.eventDigest, budget: result.budget ?? null });
      } catch (error) {
        merge(error?.conformanceObservations);
        scenarioObservations[scenario] = error?.conformanceObservations ?? {};
        stages.push({ stageId: scenario, status: "failed", category: error?.conformanceDiagnostics?.category ?? "conformance", message: boundedFailure(error), budget: error?.budgetSnapshot ?? null, diagnostics: error?.conformanceDiagnostics ?? null });
      }
    }
    observations.terminated = true;
    const failedStages = stages.filter((stage) => stage.status !== "passed");
    const sample = { suiteId: "codex-engine-delegation-execution-v1", suiteVersion: 2, outcome: failedStages.length ? "failed" : "passed", eventDigest: digest(eventDigests) };
    const records = makeRecords(sample, observations);
    const status = failedStages.length === 0 && records.every((record) => record.status === "verified") ? "verified" : "failed";
    return {
      attempt: {
        protocol,
        suite: "codex-engine-delegation-execution-v1",
        status,
        ...(failedStages.length ? { category: failedStages[0].category, message: failedStages.map((stage) => `${stage.stageId}:${stage.message}`).join("; ").slice(0, 500) } : {}),
        diagnostics: { stages },
        metrics: { capabilityResults: capabilityMetrics(records) },
      },
      records,
    };
  }

  async verifyDirectedEdge({ parentApiKey, childApiKey, parentConnection, childConnection, parentCredentialScope = parentConnection.credentialRef, childCredentialScope = childConnection.credentialRef, parentProtocol, childProtocol, parentProbeEvidence, childProbeEvidence, qualificationContextDigest = "default", now = new Date() }) {
    if (parentConnection.id === childConnection.id) throw new Error("directed edge conformance requires distinct connections");
    const parent = defineConnection(parentConnection);
    const child = defineConnection(childConnection);
    const resolveRoute = (connection, evidence, credentialScope) => resolveSessionPlan({
      connections: [connection],
      evidence,
      now,
      policy: { capabilityProfileId: "text-v1", requiredCapabilities: [CAPABILITIES.STRUCTURED_TOOL_CALLS], credentialScopes: { [connection.id]: credentialScope }, defaultConnectionId: connection.id, allowedChildConnectionIds: [connection.id], forceBridge: true, qualificationContextDigest, allowedMcpServerIds: ["engine_conformance"] },
    });
    const parentPlan = resolveRoute(parent, parentProbeEvidence, parentCredentialScope);
    const childPlan = resolveRoute(child, childProbeEvidence, childCredentialScope);
    const edge = Object.freeze({ parentConnectionId: parent.id, childConnectionId: child.id });
    const combined = Object.freeze({
      ...parentPlan,
      id: `bootstrap-edge-${digest([parentPlan.id, childPlan.id, edge])}`,
      routes: Object.freeze([parentPlan.routes[0], childPlan.routes[0]]),
      defaultModel: parent.id,
      rootRoute: parent.id,
      allowedChildModels: Object.freeze([child.id]),
      allowedChildEdges: Object.freeze([edge]),
      expiresAt: new Date(Math.min(Date.parse(parentPlan.expiresAt), Date.parse(childPlan.expiresAt))).toISOString(),
    });
    const evidenceRef = `conformance:${parent.id}:${parentProtocol}:${now.toISOString()}:edge:${child.id}`;
    const environmentDigest = parentPlan.routes[0].environmentDigest;
    const parentCredentialRealm = credentialRealmId(parentCredentialScope);
    const childCredentialRealm = credentialRealmId(childCredentialScope);
    const scope = { protocol: parentProtocol, suite: "codex-engine-directed-edge-v1", mode: "app-server-mcp", toolTypes: ["function"], behaviors: [CAPABILITIES.CHILD_DIRECTED_ROUTE], routes: [edge], parameters: { parentCredentialRealm, childCredentialRealm } };
    const credentialRealm = parentCredentialRealm;
    const dependencies = {
      modelProtocol: { digest: digest({ connectionDigest: connectionDefinitionDigest(parent), protocol: parentProtocol, codecVersion: getCodec(parentProtocol).version }), ref: `${parent.id}:${parentProtocol}` },
      engineBuild: { digest: digest({ engineContractHash: contractHash(), qualificationContextDigest }), ref: qualificationContextDigest },
      route: { digest: directedRouteDependencyDigest({ parentConnection: parent, childConnection: child, parentProtocol, childProtocol, parentCredentialScope, childCredentialScope }), ref: `${parent.id}:${parentProtocol}->${child.id}:${childProtocol}` },
      access: { digest: credentialRealm, ref: `${parent.id}:credential-realm` },
    };
    try {
      const result = await this.runProbe({
        credentials: { [parent.credentialRef]: parentApiKey, [child.credentialRef]: childApiKey },
        connections: [parent, child],
        sessionPlan: bootstrapPlan(combined),
        childModel: child.id,
        scenario: "spawn-route",
      });
      const assertions = directedEdgeAssertions({ ...result.observations, terminated: true }, result.eventDigest);
      const status = assertionsPassed(assertions) ? "verified" : "failed";
      return { attempt: { protocol: parentProtocol, suite: "codex-engine-directed-edge-v1", status }, record: { capability: CAPABILITIES.CHILD_DIRECTED_ROUTE, status, scope, source: "conformance-suite", credentialRealm, environmentDigest, evidenceRef, checkedAt: now.toISOString(), expiresAt: new Date(now.getTime() + 24 * 60 * 60 * 1000).toISOString(), sample: { suiteId: "codex-engine-directed-edge-v1", suiteVersion: 1, outcome: status === "verified" ? "passed" : "failed", eventDigest: result.eventDigest, parentSynthesizedReceipt: result.parentSynthesizedReceipt }, assertions, dependencies, supersedes: [] } };
    } catch (error) {
      const assertions = directedEdgeAssertions(error?.conformanceObservations, evidenceRef);
      return { attempt: { protocol: parentProtocol, suite: "codex-engine-directed-edge-v1", status: "failed", category: "conformance", message: boundedFailure(error) }, record: { capability: CAPABILITIES.CHILD_DIRECTED_ROUTE, status: "failed", scope, source: "conformance-suite", credentialRealm, environmentDigest, evidenceRef, checkedAt: now.toISOString(), expiresAt: new Date(now.getTime() + 24 * 60 * 60 * 1000).toISOString(), sample: { suiteId: "codex-engine-directed-edge-v1", suiteVersion: 1, outcome: "failed", message: boundedFailure(error) }, assertions, dependencies, supersedes: [] } };
    }
  }

  async #run({ credentials, connections, sessionPlan, childModel = null, scenario = "message-running" }) {
    const root = mkdtempSync(join(tmpdir(), "codex-engine-conformance-"));
    const nonce = randomBytes(18).toString("base64url");
    const followup = randomBytes(18).toString("base64url");
    const followupMessage = `FOLLOWUP_TOKEN=${followup}`;
    const receipt = randomBytes(18).toString("base64url");
    const proofPath = join(root, "mcp-proof.json");
    const mcpTracePath = join(root, "mcp-trace.jsonl");
    const gatePath = join(root, "child-gate.json");
    const releasePath = join(root, "child-release.json");
    const runId = `conformance-${nonce}`;
    const trace = new BoundedExecutionTrace({ runId, maxEvents: 320 });
    const selectedConnection = connections.find((connection) => connection.id === (childModel ?? sessionPlan.rootRoute));
    const forceChildToolChoice = scenario === "message-running" && selectedConnection?.protocolOptions?.toolChoiceMode === "full";
    const childInstruction = scenario === "message-running"
      ? "First call engine_conformance_gate exactly once with an empty object. After that tool returns, consume the follow-up queued by the parent, extract only the value immediately after FOLLOWUP_TOKEN=, call engine_conformance_echo exactly once with that value as followup, then return only its exact result. The token is not present in this initial task. Do not call any other tool."
      : `Return exactly CHILD_TERMINAL:${receipt} without calling a tool.`;
    const observedRequests = [];
    const observedResponses = [];
    const events = [];
    let releaseScheduled = false;
    const scheduleGateRelease = () => {
      if (releaseScheduled || existsSync(releasePath)) return;
      releaseScheduled = true;
      void (async () => {
        const deadline = Date.now() + Math.min(this.timeoutMs, 5_000);
        while (Date.now() < deadline && !existsSync(gatePath)) await new Promise((resolve) => setTimeout(resolve, 20));
        if (existsSync(gatePath) && !existsSync(releasePath)) {
          writeFileSync(releasePath, `${JSON.stringify({ releasedAt: new Date().toISOString() })}\n`, { mode: 0o600 });
        }
      })().catch(() => undefined);
    };
    const currentObservations = () => {
      let proof = null;
      try {
        if (existsSync(proofPath)) proof = JSON.parse(readFileSync(proofPath, "utf8"));
      } catch {
        // A partially committed proof is not an observed result.
      }
      let gate = null;
      try {
        if (existsSync(gatePath)) gate = JSON.parse(readFileSync(gatePath, "utf8"));
      } catch {
        // A partially committed gate file is not evidence.
      }
      const collaboration = summarizeCollaborationEvents(events, observedResponses);
      const childRequests = observedRequests.filter((request) => typeof request.parentThreadId === "string" && request.parentThreadId);
      const acknowledged = acknowledgedMessage(events, observedResponses, textDigest(followupMessage));
      const deliveryEvent = acknowledged && events.find((event) => event.params?.item?.id === acknowledged.callId && event.method === "item/completed" && event.params?.threadId === acknowledged.threadId);
      const parentFollowupTokenQueued = Boolean(acknowledged);
      const childFollowupVisible = Boolean(deliveryEvent && childRequests.some((request) => request.threadId === deliveryEvent.params.item.agentThreadId && request.visibleTextDigests?.includes(textDigest(followupMessage))));
      const childIdentityObserved = childRequests.length > 0 || [proof, gate].some((value) => typeof value?.callerThreadId === "string" && value.callerThreadId !== value.rootThreadId);
      const childResultObserved = childIdentityObserved && proof?.receipt === receipt;
      return Object.freeze({
        ...collaboration,
        childIdentityObserved,
        childGateObserved: gate !== null,
        childResultObserved,
        childTerminalResultObserved: observedTerminalChildWait(events, childRequests, `CHILD_TERMINAL:${receipt}`, observedResponses, observedRequests),
        directedRouteObserved: childRequests.some((request) => request.connectionId === (childModel ?? sessionPlan.rootRoute)),
        parentFollowupTokenQueued,
        childFollowupVisible,
        forcedChildToolChoice: forceChildToolChoice,
      });
    };
    const bridge = new ModelBridgeProcess({
      sessionPlan,
      connections,
      toolPolicy: {
        rootAllowed: scenario === "spawn-route" ? ["spawn_agent"] : scenario === "wait" ? ["spawn_agent", "wait_agent"] : ["spawn_agent", "send_message", "wait_agent"],
        childPhaseFile: scenario === "message-running" ? gatePath : null,
        childCompletionFile: scenario === "message-running" ? proofPath : null,
        childBeforePhaseAllowed: scenario === "message-running" ? ["engine_conformance.engine_conformance_gate", "engine_conformance_gate"] : [],
        childAfterPhaseAllowed: scenario === "message-running" ? ["engine_conformance.engine_conformance_echo", "engine_conformance_echo"] : [],
        childToolChoiceRequired: forceChildToolChoice,
      },
      responseBarrier: scenario === "message-running" ? { rootTool: "send_message", releaseFile: gatePath, timeoutMs: Math.min(this.timeoutMs, 30_000) } : null,
      credentialResolver: async (reference) => credentials[reference] ?? null,
      requestObserver: (request) => {
        observedRequests.push({
          requestId: request.requestId ?? null,
          connectionId: request.connectionId ?? null,
          logicalModel: request.logicalModel ?? null,
          threadId: request.threadId ?? null,
          parentThreadId: request.parentThreadId ?? null,
          visibleTextDigests: request.visibleTextDigests ?? [],
          waitResults: request.waitResults ?? [],
          finalAgentMessages: request.finalAgentMessages ?? [],
          toolChoice: request?.toolChoice ?? null,
          tools: Array.isArray(request?.tools) ? request.tools.map((tool) => tool?.name ?? tool?.type ?? "unknown").slice(0, 80) : [],
          encodedToolChoice: request?.encodedToolChoice ?? null,
          encodedTools: Array.isArray(request?.encodedTools) ? request.encodedTools.map((tool) => tool?.name ?? tool?.type ?? "unknown").slice(0, 80) : [],
        });
        trace.record("bridge", "bridge.request.prepared", {
          requestId: request.requestId,
          connectionId: request.connectionId,
          logicalModel: request.logicalModel,
          threadId: request.threadId,
          parentThreadId: request.parentThreadId,
          turnId: request.turnId,
          rootTurnId: request.rootTurnId,
          tools: toolNames(request.tools),
          encodedTools: toolNames(request.encodedTools),
          toolResultCallIds: request.toolResultCallIds ?? [],
          visibleTextDigests: request.visibleTextDigests ?? [],
        });
      },
      responseObserver: (response) => {
        observedResponses.push(response);
        trace.record("bridge", response.status === "failed" ? "bridge.response.failed" : "bridge.response.completed", {
          requestId: response.requestId,
          connectionId: response.connectionId,
          threadId: response.threadId,
          parentThreadId: response.parentThreadId,
          turnId: response.turnId,
          status: response.status,
          errorCategory: response.errorCategory ?? null,
          errorMessage: response.errorMessage ?? null,
          errorStatusCode: response.errorStatusCode ?? null,
          toolCalls: selectedToolNames(response.toolCalls),
          outputTypes: response.outputTypes ?? [],
        });
      },
    });
    let adapter;
    let budget;
    let lease;
    try {
      const bridgeAddress = await bridge.start();
      budget = new TaskBudget({ maxRequests: 16, maxTokens: 200_000, maxDurationMs: this.timeoutMs });
      lease = createExecutionLease({ sessionPlan, runId, budgetId: budget.id, mode: "conformance", maxDurationMs: this.timeoutMs });
      await bridge.beginTaskBudget(budget);
      await bridge.beginExecutionLease(lease);
      const catalogPath = join(root, "model-catalog.json");
      writeFileSync(catalogPath, `${JSON.stringify(compileCodexModelCatalog(sessionPlan, { baseInstructions: this.baseInstructions }), null, 2)}\n`, { mode: 0o600 });
      writeFileSync(join(root, "config.toml"), compileCodexConfig({
        sessionPlan,
        modelCatalogPath: catalogPath,
        bridge: { baseUrl: bridgeAddress.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" },
        mcpServers: [{ id: "engine_conformance", command: this.nodeCommand, args: [this.mcpPath], envVars: ["ELECTRON_RUN_AS_NODE", "ENGINE_CONFORMANCE_FOLLOWUP", "ENGINE_CONFORMANCE_RECEIPT", "ENGINE_CONFORMANCE_PROOF_PATH", "ENGINE_CONFORMANCE_TRACE_PATH", "ENGINE_CONFORMANCE_GATE_PATH", "ENGINE_CONFORMANCE_RELEASE_PATH"], approvalMode: "approve" }],
      }), { mode: 0o600 });
      mkdirSync(join(root, "sqlite"));
      chmodSync(root, 0o700);
      adapter = new CodexAppServerClient({
        command: this.command,
        cwd: this.cwd,
        env: {
          ...this.baseEnvironment,
          CODEX_HOME: root,
          CODEX_SQLITE_HOME: join(root, "sqlite"),
          ELECTRON_RUN_AS_NODE: "1",
          AGENT_ENGINE_BRIDGE_TOKEN: bridgeAddress.token,
          ENGINE_CONFORMANCE_FOLLOWUP: followup,
          ENGINE_CONFORMANCE_RECEIPT: receipt,
          ENGINE_CONFORMANCE_PROOF_PATH: proofPath,
          ENGINE_CONFORMANCE_TRACE_PATH: mcpTracePath,
          ENGINE_CONFORMANCE_GATE_PATH: gatePath,
          ENGINE_CONFORMANCE_RELEASE_PATH: releasePath,
        },
        model: sessionPlan.defaultModel,
        turnTimeoutMs: this.timeoutMs,
      });
      adapter.setCallbacks({ onEvent: (event) => {
        events.push(event);
        const item = event.params?.item;
        if (acknowledgedMessage(events, observedResponses, textDigest(followupMessage))) scheduleGateRelease();
        trace.record("codex", "agent.event", {
          method: event.method,
          itemType: item?.type ?? null,
          itemId: item?.id ?? null,
          status: item?.status ?? event.params?.status?.type ?? null,
          tool: item?.tool ?? item?.action ?? null,
          threadId: event.params?.threadId ?? null,
          turnId: event.params?.turnId ?? null,
          errorMessage: event.params?.error?.message ?? null,
          errorInfo: event.params?.error?.codexErrorInfo ?? null,
          willRetry: event.params?.willRetry ?? null,
        });
      } });
      await within(adapter.ensureSession(), this.timeoutMs, "Codex conformance App Server did not initialize");
      const modelInstruction = childModel ? ` Use the exact child model override ${childModel}.` : "";
      const prompt = scenario === "spawn-route"
        ? `Use spawn_agent exactly once to create one child named engine_conformance with this exact instruction: ${childInstruction}.${modelInstruction} After spawn_agent completes, return exactly SPAWN_ACCEPTED and do not call another tool.`
        : scenario === "wait"
          ? `Use spawn_agent exactly once to create one child named engine_conformance with this exact instruction: ${childInstruction}.${modelInstruction} Then use wait_agent for that child exactly once. After the wait result arrives, return exactly WAIT_ACCEPTED and do not call another tool.`
          : `Use only spawn_agent, send_message, and wait_agent. Spawn exactly one child named engine_conformance with this exact instruction: ${childInstruction}.${modelInstruction} Immediately queue this exact new follow-up to the running child using send_message: ${followupMessage}. Then wait for the child. Return only the exact CONFORMANCE_RECEIPT from the child's real MCP result and end without another tool call.`;
      let result;
      try {
        result = await within(
          adapter.sendTurn([{ type: "text", text: prompt }], { disableEnvironment: true, threadName: "Codex Engine conformance" }),
          this.timeoutMs,
          "Codex conformance turn timed out",
        );
        if (scenario === "message-running") {
          await within(
            waitForConformanceProof({ proofPath, receipt, observedResponses, events, timeoutMs: Math.min(this.timeoutMs, 2_000) }),
            Math.min(this.timeoutMs, 2_500),
            "Codex conformance proof was not complete after the turn ended",
          );
        } else if (scenario === "spawn-route") {
          await waitForObservation(() => currentObservations().childIdentityObserved, Math.min(this.timeoutMs, 2_000));
        }
      } catch (error) {
        const observed = events.slice(-40).map((event) => `${event.method}:${event.params?.item?.type ?? event.params?.status?.type ?? ""}`).join(",");
        throw new Error(`${boundedFailure(error)}; observed=${observed}`);
      }
      const collaboration = events.filter((event) => event.params?.item?.type === "collabAgentToolCall");
      const completedCollaboration = collaboration.filter((event) => event.method === "item/completed");
      const expectedReceipt = `CONFORMANCE_RECEIPT:${receipt}`;
      const parentSynthesizedReceipt = scenario === "message-running" && result.message.includes(expectedReceipt);
      const proof = existsSync(proofPath) ? JSON.parse(readFileSync(proofPath, "utf8")) : null;
      for (const event of readMcpTrace(mcpTracePath)) trace.record("mcp", event.type, event.details);
      const childReceiptObserved = proof?.receipt === receipt && typeof proof?.callerThreadId === "string" && proof.callerThreadId !== proof.rootThreadId;
      if (scenario === "message-running" && !childReceiptObserved) {
        const error = new Error("delegated child MCP proof was not recorded");
        const traceSnapshot = trace.snapshot();
        const target = diagnosticTarget(scenario, gatePath);
        error.conformanceDiagnostics = {
          phase: target.phase,
          expectedTool: target.expectedTool,
          category: classifyConformanceFailure(traceSnapshot, target),
          trace: traceSnapshot,
          collaborationItems: collaboration.map((event) => ({ method: event.method, status: event.params?.item?.status ?? null, tool: event.params?.item?.tool ?? event.params?.item?.action ?? null })).slice(-40),
          requests: observedRequests.slice(-24),
          responses: observedResponses.slice(-24),
        };
        throw error;
      }
      if (scenario === "message-running" && !existsSync(gatePath)) throw new Error("delegated child gate proof was not recorded");
      if (!currentObservations().childIdentityObserved) throw new Error("Codex did not establish a trusted child lineage");
      const collaborationTools = new Set(summarizeCollaborationEvents(events, observedResponses).completedOperations);
      if (childReceiptObserved) {
        collaborationTools.add("spawn_agent");
        collaborationTools.add("send_message");
      }
      if (["message-running", "wait"].includes(scenario) && !collaborationTools.has("wait_agent")) throw new Error(`Codex did not complete required wait_agent operation; items=${JSON.stringify(collaboration.map((event) => event.params.item)).slice(0, 2_000)}`);
      if (childModel && !currentObservations().directedRouteObserved) throw new Error(`directed child route was not observed: ${childModel}`);
      return {
        parentSynthesizedReceipt,
        eventDigest: digest({
          collaboration: completedCollaboration.map((event) => ({
            status: event.params.item.status ?? null,
            tool: event.params.item.tool ?? event.params.item.action ?? null,
          })),
          receiptVerified: true,
          parentSynthesizedReceipt,
          operations: [...collaborationTools].filter(Boolean).sort(),
        }),
        trace: trace.snapshot(),
        observations: currentObservations(),
        budget: budget.snapshot(),
      };
    } catch (error) {
      const appServerProcess = adapter?.process;
      adapter?.close();
      await waitForProcessClose(appServerProcess);
      for (const event of readMcpTrace(mcpTracePath)) trace.record("mcp", event.type, event.details);
      if (error instanceof Error) error.conformanceObservations = currentObservations();
      const snapshot = budget?.snapshot();
      if (snapshot && error instanceof Error) {
        error.budgetSnapshot = snapshot;
        const traceSnapshot = trace.snapshot();
        const target = diagnosticTarget(scenario, gatePath);
        error.conformanceDiagnostics ??= {
          phase: target.phase,
          expectedTool: target.expectedTool,
          category: classifyConformanceFailure(traceSnapshot, {
            ...target,
            timedOut: /timed out/i.test(error.message),
            budgetExhausted: snapshot.overrun || snapshot.requests >= snapshot.maxRequests,
          }),
          trace: traceSnapshot,
          requests: observedRequests.slice(-24),
          responses: observedResponses.slice(-24),
        };
      }
      throw error;
    } finally {
      const appServerProcess = adapter?.process;
      adapter?.close();
      await waitForProcessClose(appServerProcess);
      if (lease) await bridge.endExecutionLease(lease).catch(() => undefined);
      if (budget) await bridge.endTaskBudget(budget).catch(() => undefined);
      await bridge.close().catch(() => undefined);
      rmSync(root, { recursive: true, force: true, maxRetries: 5, retryDelay: 50 });
    }
  }
}
