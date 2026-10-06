import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash, randomBytes, randomUUID } from "node:crypto";
import { NOVEL_ROOT_DEVELOPER_INSTRUCTIONS } from "../electron/creative-mission.mjs";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

// Single synthetic repair with the production candidate outcome workflow.
// No qualification publication, author adoption, UI routing or continuation.
// Developer-only source-checkout command: requires electron/tests/helpers and
// current prebuilt native binaries. Not a packaged/distributed product script.
const scope = "single-sample-revision-model-feedback";
const projectRoot = fileURLToPath(new URL("..", import.meta.url));
const toolNames = ["read_context", "query_story", "save_draft_checkpoint", "publish_knowledge_batch", "submit_revision_outcome", "complete_task"];
const hash = value => createHash("sha256").update(Buffer.isBuffer(value) ? value : String(value)).digest("hex");
const count = value => Number.isSafeInteger(value) && value >= 0 ? value : null;
const fail = code => { throw Object.assign(new Error(code), { probeCode: code }); };
const knownTool = name => toolNames.find(tool => name === tool || name === `mcp__novel_workspace__${tool}`) ?? "other";
const failureCodes = ["task_token_budget_exhausted", "task_request_budget_exhausted", "task_duration_budget_exhausted",
  "provider_http_error", "provider_connection_failed", "stream_tool_mismatch", "response_incomplete", "response_stream_disconnected",
  "provider_output_limit", "provider_content_filter", "provider_context_limit", "provider_quota_exhausted", "provider_invalid_request",
  "provider_rate_limited", "provider_server_error", "provider_stream_failed", "invalid_tool_arguments", "context_item_limit", "provider_output_item_limit", "bridge_error"];
const evidenceCodes = ["evidence_shape_invalid", "evidence_quote_invalid", "evidence_quote_not_found", "evidence_quote_ambiguous",
  "evidence_anchor_invalid", "evidence_anchor_mismatch", "evidence_anchor_incomplete"];
const runtimeCodes = ["invalid_request", "conflict", "internal_error", "task_scope_invalid", "task_binding_invalid"];
const modelingReasons = new Map([
  ["entity candidate is invalid", "entity_candidate_invalid"],
  ["knowledge candidate identity or shape is invalid", "knowledge_candidate_invalid"],
  ["too many knowledge or entity candidates", "knowledge_item_limit"],
]);

export function parseRevisionOptions(args) {
  const options = {};
  const values = { "--scope": "scope", "--max-requests": "maxRequests", "--max-tokens": "maxTokens",
    "--timeout-seconds": "timeoutSeconds", "--evidence": "evidencePath", "--credential-salt": "credentialSaltPath",
    "--confirm-cost": "confirmCost" };
  for (let i = 0; i < args.length; i++) {
    const flag = args[i];
    const key = values[flag] ?? ({ "--execute": "execute", "--help": "help" })[flag];
    if (!key || Object.hasOwn(options, key)) fail("INVALID_ARGUMENTS");
    if (flag === "--execute" || flag === "--help") options[key] = true;
    else {
      const value = args[++i];
      if (!value || value.startsWith("--") || value.length > 4096) fail("INVALID_ARGUMENTS");
      if (["maxRequests", "maxTokens", "timeoutSeconds"].includes(key)) {
        if (!/^[1-9][0-9]*$/.test(value)) fail("INVALID_BUDGET");
        options[key] = Number(value);
      } else options[key] = value;
    }
  }
  return options;
}

export function revisionPlan(options) {
  if (options.scope !== scope) fail("SCOPE_REQUIRED");
  for (const [key, maximum] of [["maxRequests", 10], ["maxTokens", 260000], ["timeoutSeconds", 180]]) {
    if (!Number.isSafeInteger(options[key]) || options[key] < 1 || options[key] > maximum) fail("INVALID_BUDGET");
  }
  if (![options.evidencePath, options.credentialSaltPath].every(value => typeof value === "string" && value.trim() && value.length <= 4096)) fail("EVIDENCE_REQUIRED");
  return { scope, samples: 1, stages: ["revision", "modeling", "feedback"],
    limitsPerStage: { requests: options.maxRequests, tokens: options.maxTokens, seconds: options.timeoutSeconds },
    maximumTotal: { requests: options.maxRequests * 3, tokens: options.maxTokens * 3 },
    // Token/request limits are enforceable; a monetary cap is not available.
    costConfirmation: `3x${options.maxRequests}-${options.maxTokens}-${options.timeoutSeconds}:actual-cost-unknown`,
    actualCost: { status: "unavailable", amount: null, currency: null },
    pending: ["revisionRepairDecisionWorkflow", "authorAcceptance", "continuation", "supplierInvoice"],
    qualification: "reuse-existing-trusted-evidence-no-publication" };
}

export function summarizeObservation(value, kind) {
  const tools = kind === "request" ? value?.tools : value?.toolCalls;
  return { kind, requestRef: hash(value?.requestId ?? "missing"),
    connectionMatches: value?.connectionId === "deepseek-flash", modelMatches: (value?.model ?? value?.logicalModel) === "deepseek-flash",
    status: ["completed", "failed", "incomplete"].includes(value?.status) ? value.status : "unknown",
    errorCategory: ["authentication", "timeout", "cancelled", "bridge", "budget", "provider"].includes(value?.errorCategory) ? value.errorCategory : null,
    errorCode: failureCodes.includes(value?.errorCode) ? value.errorCode : null,
    httpStatus: count(value?.errorStatusCode),
    tools: Array.isArray(tools) ? tools.slice(0, 32).map(tool => knownTool(tool?.name)) : [] };
}

export function summarizeToolResult(item, sourceRevisionId) {
  const name = knownTool(item?.tool);
  // Never retain arguments, tool output, raw error, model reasoning or prose.
  const raw = JSON.stringify(item?.result ?? null);
  const result = { name, status: ["completed", "failed"].includes(item?.status) ? item.status : "unknown",
    error: Boolean(item?.status === "failed" || item?.error || item?.result?.isError), errorCode: null,
    sourceSeen: false, candidateSeen: false, bounded: raw.length <= 65536 && !item?.displayTruncated };
  if (!result.bounded) return result;
  const queue = [item?.result];
  for (let nodes = 0; queue.length && nodes < 3000; nodes++) {
    const value = queue.shift();
    if (typeof value === "string") { try { queue.push(JSON.parse(value)); } catch { /* Public prose is omitted. */ } }
    else if (value && typeof value === "object") {
      if (value.error && typeof value.error === "object") {
        result.errorCode = evidenceCodes.find(code => value.error.message === code
          || value.error.message === `InvalidContent(${JSON.stringify(code)})`)
          ?? [...modelingReasons].find(([message]) => value.error.message === `InvalidContent(${JSON.stringify(message)})`)?.[1]
          ?? (runtimeCodes.includes(value.error.code) ? value.error.code : result.errorCode);
      }
      result.sourceSeen ||= value.sourceRevisionId === sourceRevisionId && typeof sourceRevisionId === "string";
      if (Array.isArray(value.steps) && value.evidence && typeof value.evidence === "object") {
        for (const step of value.steps.slice(0, 16)) for (const row of (Array.isArray(step.results) ? step.results.slice(0, 64) : [])) {
          const citation = value.evidence[row.evidenceRef ?? row.assertion?.evidenceRef];
          result.candidateSeen ||= row.confirmationState === "candidate" && row.sourceRevisionId === sourceRevisionId
            && citation?.sourceRevisionId === sourceRevisionId && typeof citation.quote === "string" && citation.quote.length > 0;
        }
      }
      const children = Object.values(value);
      if (children.length > 64) { result.bounded = false; break; }
      queue.push(...children);
    }
  }
  if (queue.length || !result.bounded) { result.bounded = false; result.sourceSeen = false; result.candidateSeen = false; }
  return result;
}

export function summarizeFeedback(report, draft) {
  const value = report?.assessment;
  if (!value || value.draftRevisionId !== draft.id) fail("FEEDBACK_SOURCE_MISMATCH");
  if (report.authority !== "candidateAnalysis" || report.state !== "current") fail("FEEDBACK_BOUNDARY_INVALID");
  if (!Array.isArray(value.outcomes) || value.outcomes.length !== 2
    || JSON.stringify(value.outcomes.map(goal => goal?.id).sort()) !== JSON.stringify(["retain-letter", "testify-motive"])) fail("FEEDBACK_GOALS_INVALID");
  const characters = Array.from(draft.sceneGroups.find(group => group.id === "letter").body);
  return { sourceRef: hash(draft.id), candidate: true, coverage: "partial", goals: value.outcomes.map(goal => {
    if (!["supported", "unmet", "insufficientEvidence"].includes(goal.status)) fail("FEEDBACK_STATUS_INVALID");
    if (!Array.isArray(goal.evidence) || goal.evidence.length > 4
      || goal.status !== "insufficientEvidence" && !goal.evidence.length) fail("FEEDBACK_ANCHOR_MISMATCH");
    for (const anchor of goal.evidence) {
      if (anchor.groupId !== "letter" || !(Number.isSafeInteger(anchor.start) && Number.isSafeInteger(anchor.end)
        && anchor.start >= 0 && anchor.end > anchor.start && anchor.end <= characters.length)
        || characters.slice(anchor.start, anchor.end).join("") !== anchor.quote) fail("FEEDBACK_ANCHOR_MISMATCH");
    }
    return { id: goal.id, status: goal.status, evidenceDigest: goal.evidence.length ? hash(JSON.stringify(goal.evidence)) : null };
  }) };
}

export function assertRevisionScope(before, after) {
  assert.equal(after.sceneGroups.length, before.sceneGroups.length);
  assert.deepEqual(after.sceneGroups.map(({ body, ...metadata }) => metadata), before.sceneGroups.map(({ body, ...metadata }) => metadata));
  assert.deepEqual(after.sceneGroups.filter(group => group.id !== "letter"), before.sceneGroups.filter(group => group.id !== "letter"));
  assert.equal(after.title, before.title);
  assert.deepEqual(after.sourcePlanRefs, before.sourcePlanRefs);
  assert.equal(after.body, after.sceneGroups.map(group => group.body.trim()).join("\n\n---\n\n"));
  const changed = after.sceneGroups.find(group => group.id === "letter").body;
  assert.notEqual(changed, before.sceneGroups.find(group => group.id === "letter").body);
  return { scopePreserved: true, lexicalGoalSignals: { retainedLetter: /藏|保管/.test(changed), testimonyMotive: /证词|作证/.test(changed) },
    goalVerdict: "requires-author-review", bodyDigest: hash(changed) };
}

export function assertFeedbackIsolation(before, after, beforeKnowledge, afterKnowledge) {
  // sourceDigest includes knowledge heads and analysis artifacts. Publishing
  // knowledge or a candidate report must not be mistaken for manuscript edits.
  assert.equal(after.revision, before.revision);
  assert.deepEqual(after.chapters, before.chapters);
  assert.deepEqual(afterKnowledge, beforeKnowledge);
}

export async function createProbeRevisionScope(request, base, workspace, initial, options) {
  // This scripts the synthetic author's approval, not a model decision or an
  // approval for any real manuscript. It uses the same public scope endpoints.
  const intent = "再次只修改密函场景：保留藏信，明确林夕主动为未来留下证词的动机。其余场景与锁定结尾原样保留。保存候选，不采用。";
  const target = { type: "chapter", chapterId: "chapter-one", chapterNumber: 1, readingChapter: 1, sceneOrder: 1,
    scope: { type: "selectedScenes", sourceRevisionId: initial.id, groupIds: ["letter"] }, desiredOutcomes: [
      { id: "retain-letter", criterion: "密函仍被藏好或保管，没有销毁或交出", groupIds: ["letter"] },
      { id: "testify-motive", criterion: "明确林夕主动为未来留下证词的动机", groupIds: ["letter"] },
    ] };
  const capability = randomBytes(32).toString("hex");
  const threadId = "probe-synthetic-author";
  const deadlineAt = Math.floor(Date.now() / 1000) + options.timeoutSeconds * 3 + 600;
  const source = await request("POST", `${base}/tasks`, { operation: "advisory", target: { type: "project" },
    inputRevision: workspace.revision, threadId, authorIntent: intent, capabilityHash: hash(capability), capabilityExpiresAt: deadlineAt,
    initialBudget: { authorizationId: randomUUID(), authorMessageId: randomUUID(), expectedBaseSnapshotId: workspace.headSnapshotId,
      expectedSourceDigest: workspace.sourceDigest, maxRequests: options.maxRequests * 3, maxTokens: options.maxTokens * 3, deadlineAt } });
  const action = await request("POST", `${base}/tasks/${source.id}/actions`, { taskId: source.id, threadId,
    expectedRevision: workspace.revision, expectedIntentRevision: 0, idempotencyKey: "probe-approved-revision",
    operation: "generateChapter", target, instruction: intent }, capability);
  const scopeRequest = { actionIds: [action.id], maxActions: 1, maxTaskAttempts: 6, maxAdvisoryRounds: 1, expectedIntentRevision: 0 };
  const scopePath = `${base}/tasks/${source.id}/execution-scope`;
  const preview = await request("POST", `${scopePath}/preview`, scopeRequest);
  await request("POST", scopePath, { request: scopeRequest, previewDigest: preview.previewDigest });
  const grant = await request("GET", `${base}/tasks/${source.id}/provider-budget`);
  await request("POST", `${base}/tasks/${source.id}/complete`, { threadId, summary: "临时作者范围已批准" });
  // This author setup has made no provider calls. Settle its real empty ledger
  // before claiming the dependent writer, without inventing a model response.
  const { PersistentTaskBudget } = await import("@eternallight/agent-engine");
  const { taskProviderBudgetOptions } = await import("../electron/task-provider-budget.mjs");
  const empty = new PersistentTaskBudget({ databasePath: options.budgetPath, runId: source.id,
    ...taskProviderBudgetOptions(grant, { taskId: source.id, workTaskId: grant.workTaskId, deadlineAt: grant.deadlineAt }) });
  try { empty.complete("completed"); } finally { empty.close(); }
  await request("POST", `${base}/work-tasks/${grant.workTaskId}/provider-usage`, { taskId: source.id });
  return { sourceTaskId: source.id, intent,
    target: { ...action.target, requestedByTaskId: source.id, requestedActionId: action.id } };
}

async function limited(promise, milliseconds) {
  let timer;
  try { return await Promise.race([promise, new Promise((_, reject) => {
    timer = setTimeout(() => reject(Object.assign(new Error("TIMEOUT"), { probeCode: "TIMEOUT" })), milliseconds);
  })]); } finally { clearTimeout(timer); }
}

export async function closeProbeHosts({ bridge, adapter }, record) {
  for (const host of [bridge, adapter]) {
    if (host && host === adapter) {
      try { await adapter.finishTaskOperation(); }
      catch { record.shutdownFailed = true; }
    }
    try { await host?.close(); }
    catch { record.shutdownFailed = true; }
  }
}

export async function probeLiveRevision(options = {}) {
  const plan = revisionPlan(options);
  // These gates precede imports, credential reads, filesystem writes and spawn.
  if (options.execute !== true) fail("EXECUTION_REQUIRES_MAIN_AUTHORIZATION");
  if (options.confirmCost !== plan.costConfirmation) fail("COST_CONFIRMATION_REQUIRED");
  const root = await mkdtemp(join(tmpdir(), "novel-live-revision-"));
  const workspaceRoot = join(root, "workspace");
  await mkdir(workspaceRoot, { mode: 0o700 });
  const report = { version: 2, status: "failed", plan, stages: [], observations: [], observationsTruncated: false,
    certificationPublished: false, authorAccepted: false, continued: false, cleanup: "pending",
    usageBasis: "engine-terminal-receipts-not-supplier-invoice" };
  let runtime, request, base, currentStage = "preflight";
  const tasks = [];
  try {
    const engine = await import("@eternallight/agent-engine");
    const { CodexAppServerAdapter } = await import("../electron/codex-adapter.mjs");
    const api = await import("../electron/codex-runtime.mjs");
    const credentials = await import("../electron/development-credentials.mjs");
    const { taskProviderBudgetOptions } = await import("../electron/task-provider-budget.mjs");
    const workflow = await import("../electron/workflow-handoff.mjs");
    const { createMcpStartupWaiter } = await import("../electron/tests/helpers/mcp-startup.mjs")
      .catch(() => fail("DEVELOPER_HELPER_UNAVAILABLE"));
    const environment = credentials.resolveDevelopmentEnvironment(credentials.loadProjectDevelopmentEnvironment(projectRoot, {}));
    if (!environment.DEEPSEEK_API_KEY) fail("PROJECT_CREDENTIAL_UNAVAILABLE");
    const cli = api.resolveCodexCli({ explicitPath: process.env.NOVEL_CODEX_CLI, isPackaged: false, projectRoot, resourcesPath: projectRoot });
    if (!cli.path) fail("CODEX_UNAVAILABLE");
    const baseInstructions = (await readFile(resolve(projectRoot, "../../codex-rs/models-manager/prompt.md"), "utf8")).trim();
    const credentialScopes = api.credentialScopesFromEnvironment(environment, resolve(options.credentialSaltPath));
    const qualificationContextDigest = api.createQualificationContextDigest({ baseInstructions, cliPath: cli.path });
    const provider = api.resolveCodexProvider({ provider: "deepseek", model: "deepseek-flash", credentialScopes,
      evidence: await api.loadCodexEvidence(resolve(options.evidencePath)), qualificationContextDigest });
    report.model = { connection: "deepseek-flash", protocol: provider.protocol, qualificationContextDigest,
      codexDigest: hash(await readFile(cli.path)) };
    const binary = name => join(projectRoot, "native", "target", "debug", `${name}${process.platform === "win32" ? ".exe" : ""}`);
    const processEnvironment = Object.fromEntries(["PATH", "PATHEXT", "SystemRoot", "WINDIR", "TEMP", "TMP", "TMPDIR"].filter(key => process.env[key]).map(key => [key, process.env[key]]));
    const authorToken = randomBytes(32).toString("hex"), candidateToken = randomBytes(32).toString("hex");
    const budgetPath = join(workspaceRoot, "budget.sqlite3");
    runtime = spawn(binary("novel-codex-story-runtime"), ["--port", "0", "--db", join(workspaceRoot, "story.sqlite3"),
      "--engine-budget-db", budgetPath, "--token-stdin"], { env: processEnvironment, stdio: ["pipe", "pipe", "ignore"] });
    const listening = new Promise((resolveOrigin, reject) => {
      const lines = createInterface({ input: runtime.stdout });
      lines.on("line", line => { if (line.startsWith("Story Runtime listening on ")) resolveOrigin(line.slice(27)); });
      runtime.once("error", () => reject(new Error("RUNTIME_STARTUP")));
      runtime.once("exit", () => reject(new Error("RUNTIME_EXIT")));
    });
    runtime.stdin.on("error", () => {});
    runtime.stdin.end(`${JSON.stringify({ authorToken, candidateToken })}\n`);
    const origin = await limited(listening, 60000);
    const parsedOrigin = new URL(origin);
    if (parsedOrigin.protocol !== "http:" || !["127.0.0.1", "localhost", "[::1]"].includes(parsedOrigin.hostname)) fail("INVALID_RUNTIME_ORIGIN");
    request = async (method, path, payload, capability = null) => {
      const response = await fetch(`${origin}${path}`, { method, signal: AbortSignal.timeout(10000), redirect: "error",
        headers: { authorization: `Bearer ${capability ? candidateToken : authorToken}`, "content-type": "application/json",
          ...(capability ? { "x-novel-task-capability": capability } : {}) },
        ...(payload === undefined ? {} : { body: JSON.stringify(payload) }) });
      if (!response.ok) fail("RUNTIME_REQUEST_FAILED");
      return response.json();
    };
    const { project } = await request("POST", "/api/projects", { title: "临时再次改稿探针" });
    base = `/api/projects/${project.id}`;
    const original = { chapterId: "chapter-one", chapterNumber: 1, title: "密函",
      sceneGroups: [{ id: "opening", sceneId: "plan-opening", body: "林夕走进港口。", locked: false },
        { id: "letter", sceneId: "plan-letter", body: "她将密函藏进衣襟。", locked: false },
        { id: "ending", sceneId: null, body: "白塔熄灭。", locked: true }] };
    original.body = original.sceneGroups.map(group => group.body).join("\n\n---\n\n");
    await request("POST", `${base}/drafts/checkpoints`, { expectedRevision: 0, expectedCandidateRevisionId: null,
      idempotencyKey: "probe-seed", payload: original });
    const before = await request("GET", `${base}/creative-workspace`);
    const initial = before.chapters[0].candidate;
    async function runStage(stage, operation, target, intent, feedbackDraft = null) {
      currentStage = stage;
      const record = { stage, status: "failed", tools: [], usage: null, usageSettled: false };
      report.stages.push(record);
      let bridge, adapter, budget, startup, lease, task, grant, budgetTerminal = false;
      try {
        const output = operation === "modelKnowledge" ? "publish_knowledge_batch"
          : operation === "assessRevisionOutcome" ? "submit_revision_outcome" : "save_draft_checkpoint";
        const allowed = ["read_context", "query_story", "complete_task", output];
        const observe = kind => value => {
          if (report.observations.length < 120) report.observations.push({ stage, ...summarizeObservation(value, kind) });
          else report.observationsTruncated = true;
        };
        bridge = new engine.ModelBridge({ sessionPlan: provider.sessionPlan, connections: provider.connections,
          credentialResolver: async ref => environment[ref], requestObserver: observe("request"), responseObserver: observe("response"),
          toolPolicy: { rootAllowed: allowed.flatMap(name => [name, `mcp__novel_workspace.${name}`, `mcp__novel_workspace__${name}`]), childAllowed: [] } });
        const address = await bridge.start();
        const home = join(workspaceRoot, stage);
        await mkdir(join(home, "sqlite"), { recursive: true, mode: 0o700 });
        const catalog = join(home, "catalog.json");
        await writeFile(catalog, JSON.stringify(engine.compileCodexModelCatalog(provider.sessionPlan, { baseInstructions })), { mode: 0o600 });
        await writeFile(join(home, "config.toml"), engine.compileCodexConfig({ sessionPlan: provider.sessionPlan,
          developerInstructions: NOVEL_ROOT_DEVELOPER_INSTRUCTIONS, subagentDeveloperInstructions: api.NOVEL_SUBAGENT_DEVELOPER_INSTRUCTIONS,
          modelCatalogPath: catalog, bridge: { baseUrl: address.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" },
          mcpServers: [{ id: "novel_workspace", command: binary("novel-workspace-mcp"), args: [], approvalMode: "approve",
            envVars: ["NOVEL_CODEX_RUNTIME_URL", "NOVEL_CODEX_CANDIDATE_TOKEN", "NOVEL_CODEX_TASK_CAPABILITY", "NOVEL_CODEX_OPERATION"] }] }), { mode: 0o600 });
        const capability = randomBytes(32).toString("hex");
        adapter = new CodexAppServerAdapter({ command: [cli.path, "app-server"], cwd: workspaceRoot,
          env: { ...processEnvironment, CODEX_HOME: home, CODEX_SQLITE_HOME: join(home, "sqlite"), AGENT_ENGINE_BRIDGE_TOKEN: address.token,
            NOVEL_CODEX_RUNTIME_URL: origin, NOVEL_CODEX_CANDIDATE_TOKEN: candidateToken, NOVEL_CODEX_TASK_CAPABILITY: capability,
            NOVEL_CODEX_OPERATION: operation }, model: provider.id, provider: provider.provider });
        adapter.setCallbacks({ onEvent: event => {
          startup?.onEvent(event);
          if (event.method === "item/completed" && event.params?.item?.type === "mcpToolCall") {
            if (record.tools.length < 64) record.tools.push(summarizeToolResult(event.params.item, target.sourceRevisionId ?? target.scope?.sourceRevisionId));
            else record.toolsTruncated = true;
          }
        } });
        adapter.startProcess();
        startup = createMcpStartupWaiter({ serverName: "novel_workspace", process: adapter.process, timeoutMs: 30000 });
        await limited(adapter.ensureSession(), 30000);
        await startup.ready;
        const workspace = await request("GET", `${base}/creative-workspace`);
        const deadlineAt = Math.floor(Date.now() / 1000) + options.timeoutSeconds;
        const derived = operation === "modelKnowledge" || target.requestedByTaskId || target.resumeStepId;
        task = await request("POST", `${base}/tasks`, { operation, target, inputRevision: workspace.revision, threadId: adapter.threadId,
          authorIntent: intent, capabilityHash: hash(capability), capabilityExpiresAt: deadlineAt,
          ...(!derived ? { initialBudget: { authorizationId: randomUUID(), authorMessageId: randomUUID(), expectedBaseSnapshotId: workspace.headSnapshotId,
            expectedSourceDigest: workspace.sourceDigest, maxRequests: options.maxRequests * (stage === "revision" ? 2 : 1),
            maxTokens: options.maxTokens * (stage === "revision" ? 2 : 1), deadlineAt: deadlineAt + (stage === "revision" ? options.timeoutSeconds + 600 : 0) } } : {}) });
        tasks.push(task.id);
        record.taskRef = hash(task.id);
        grant = await request("GET", `${base}/tasks/${task.id}/provider-budget`);
        const budgetOptions = taskProviderBudgetOptions(grant, { taskId: task.id, workTaskId: grant.workTaskId, deadlineAt: grant.deadlineAt });
        budget = new engine.PersistentTaskBudget({ databasePath: budgetPath, runId: task.id, ...budgetOptions,
          maxRequests: Math.min(grant.maxRequests, options.maxRequests), maxTokens: Math.min(grant.maxTokens, options.maxTokens),
          maxDurationMs: Math.min(budgetOptions.maxDurationMs, options.timeoutSeconds * 1000) });
        await bridge.beginTaskBudget(budget);
        lease = engine.createExecutionLease({ sessionPlan: provider.sessionPlan, runId: task.id, budgetId: budget.id });
        await bridge.beginExecutionLease(lease);
        const prompt = api.composeNovelCodexPrompt(workflow.taskMethodText(task), workflow.taskAuthorPrompt(task),
          workflow.taskBoundInstructions(workflow.createTaskBoundHandoff(project.id, workspace, task)));
        const turn = await limited(adapter.sendTurn(prompt, { disableEnvironment: true }), options.timeoutSeconds * 1000);
        assert.equal(turn.turn.status, "completed");
        assert.ok(turn.finalReply?.text && !turn.finalReply.truncated);
        if (feedbackDraft) {
          const outcome = await request("GET", `${base}/revision-outcomes/${target.writerTaskId}`);
          record.goalFeedback = summarizeFeedback(outcome, feedbackDraft);
          record.outcomeReceiptRef = hash(outcome.revisionId);
        }
        record.publicReply = { digest: hash(turn.finalReply.text), characters: Array.from(turn.finalReply.text).length,
          candidateBoundaryMentioned: /候选/.test(turn.finalReply.text) };
        assert.equal((await request("GET", `${base}/tasks/${task.id}`)).status, "completed");
        await bridge.endExecutionLease(lease);
        budget.complete("completed");
        budgetTerminal = true;
        await request("POST", `${base}/work-tasks/${grant.workTaskId}/provider-usage`, { taskId: task.id });
        record.usageSettled = true;
        record.status = "completed";
        return task;
      } finally {
        startup?.dispose();
        await closeProbeHosts({ bridge, adapter }, record);
        if (budget) {
          try {
            if (record.status !== "completed") {
              await request("POST", `${base}/tasks/${task.id}/cancel`).catch(() => { record.cancelFailed = true; });
              if (!budgetTerminal) budget.complete("failed");
              await request("POST", `${base}/work-tasks/${grant.workTaskId}/provider-usage`, { taskId: task.id })
                .then(() => { record.usageSettled = true; }, () => { record.usageSettled = false; });
            }
            const receipt = engine.PersistentTaskBudget.receipt(budgetPath, task.id);
            record.budgetReceiptRef = hash(JSON.stringify(receipt));
            record.usage = Object.fromEntries(["requests", "tokens", "unknownTokens", "reservedTokens"].map(key => [key, count(receipt.usage[key])]));
            record.usage.overrun = receipt.usage.overrun === true;
          } catch { record.receiptUnavailable = true; }
          finally { budget.close(); }
        }
      }
    }
    const approval = await createProbeRevisionScope(request, base, before, initial, { ...options, budgetPath });
    tasks.push(approval.sourceTaskId);
    const writer = await runStage("revision", "generateChapter", approval.target, approval.intent);
    currentStage = "revision-check";
    const revised = await request("GET", `${base}/creative-workspace`);
    const draft = revised.chapters[0].candidate;
    report.revision = assertRevisionScope(initial, draft);
    assert.notEqual(draft.id, initial.id);
    assert.equal(revised.chapters[0].accepted, null);
    report.source = { originalRef: hash(initial.id), revisedRef: hash(draft.id), sourceDigest: revised.sourceDigest };
    const next = await request("GET", `${base}/tasks/${writer.id}/next-step`);
    assert.deepEqual(next.action, { kind: "modelKnowledge", chapterId: "chapter-one", sourceRevisionId: draft.id, sourceTaskId: writer.id });
    const batch = await request("POST", `${base}/modeling-batches`, { objectId: "draft-chapter-one", sourceRevisionId: draft.id,
      expectedRevision: revised.revision, extractorVersion: "bounded-live-revision-probe" });
    await runStage("modeling", "modelKnowledge", { batchId: batch.id, sourceTaskId: writer.id }, "只基于绑定新稿提取候选知识与原文证据。");
    assert.equal((await request("GET", `${base}/tasks/${writer.id}/next-step`)).action, null);
    report.modeling = { batchRef: hash(batch.id), sourceRef: hash(draft.id), sourceTaskRef: hash(writer.id), followupCleared: true };
    const feedbackBaseline = await request("GET", `${base}/creative-workspace`);
    const knowledgeBaseline = await request("GET", `${base}/knowledge`);
    const work = await request("GET", `${base}/tasks/${writer.id}/work-task`);
    const step = work.steps.find(item => item.operation === "assessRevisionOutcome" && item.status === "queued"
      && item.target.writerTaskId === writer.id && work.steps.some(prior => prior.id === item.dependsOnStepId && prior.status === "completed"));
    if (!step) fail("FEEDBACK_TASK_NOT_QUEUED");
    await runStage("feedback", "assessRevisionOutcome", { ...step.target, resumeStepId: step.id },
      "按冻结目标核对新稿，只通过 submit_revision_outcome 保存有精确原文依据的候选分析，然后 complete_task；最终回复正常说明结果，不要求 JSON，不修改正文，不采用，不续写。", draft);
    currentStage = "feedback-check";
    const feedback = report.stages.at(-1);
    assert.ok(feedback.tools.some(tool => tool.name === "submit_revision_outcome" && tool.status === "completed" && !tool.error && tool.bounded));
    assertFeedbackIsolation(feedbackBaseline, await request("GET", `${base}/creative-workspace`),
      knowledgeBaseline, await request("GET", `${base}/knowledge`));
    assert.ok(report.observations.some(item => item.kind === "request" && item.connectionMatches && item.modelMatches));
    assert.ok(report.observations.filter(item => item.kind === "request").every(item => item.connectionMatches && item.modelMatches));
    report.status = "passed-single-sample-slice";
  } catch (error) {
    const lastResponse = report.observations.filter(item => item.stage === currentStage && item.kind === "response").at(-1);
    report.failure = { stage: currentStage, code: error.probeCode ?? lastResponse?.errorCode ?? "STAGE_FAILED" };
  } finally {
    if (request && tasks.length) {
      try {
        const ledger = await request("POST", `${base}/tasks/recovery-operations`, { taskIds: tasks, limit: 200 });
        report.receipts = ledger.operations.slice(0, 200).map(row => ({ taskRef: hash(row.run_id), tool: knownTool(row.tool_name),
          state: ["accepted", "committed", "rejected"].includes(row.state) ? row.state : "unknown", receiptRef: row.receipt_ref ? hash(row.receipt_ref) : null }));
        report.ledgerComplete = ledger.complete === true;
        const committed = name => report.receipts.some(row => row.tool === name && row.state === "committed" && row.receiptRef);
        if (!report.ledgerComplete || !committed("save_draft_checkpoint") || !committed("publish_knowledge_batch")
          || !committed("submit_revision_outcome")
          || report.receipts.some(row => !["committed", "rejected"].includes(row.state))) { report.status = "failed"; report.failure ??= { stage: "receipts", code: "RECEIPTS_INCOMPLETE" }; }
        for (const stage of report.stages) {
          if (!report.receipts.some(row => row.taskRef === stage.taskRef && row.tool === "complete_task" && row.state === "committed" && row.receiptRef)) {
            report.status = "failed"; report.failure ??= { stage: "receipts", code: "TASK_COMPLETE_RECEIPT_MISSING" };
          }
        }
      } catch { report.status = "failed"; report.failure ??= { stage: "receipts", code: "RECEIPTS_UNAVAILABLE" }; }
    }
    if (runtime && runtime.exitCode === null && runtime.signalCode === null) {
      const stopped = new Promise(resolveExit => runtime.once("exit", resolveExit));
      runtime.kill();
      await limited(stopped, 5000).catch(() => { report.cleanup = "failed"; });
    }
    try { await rm(workspaceRoot, { recursive: true, force: true }); if (report.cleanup !== "failed") report.cleanup = "completed"; }
    catch { report.cleanup = "failed"; }
    if (report.cleanup === "failed" || report.stages.some(stage => stage.shutdownFailed || stage.receiptUnavailable)) report.status = "failed";
  }
  const reportPath = join(root, "report.json");
  await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`, { mode: 0o600, flag: "wx" });
  return { report, reportPath };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const options = parseRevisionOptions(process.argv.slice(2));
    if (options.help) process.stdout.write("Developer-only source checkout; requires electron/tests/helpers/mcp-startup.mjs and current prebuilt native binaries. Not for packaged distribution. Plan only by default. Required: --scope single-sample-revision-model-feedback --max-requests N(1..10) --max-tokens N(1..260000) --timeout-seconds N(1..180) --evidence PATH --credential-salt PATH. Execution requires main authorization via --execute and --confirm-cost matching plan.costConfirmation. No supplier monetary cap/invoice is available.\n");
    else if (!options.execute) process.stdout.write(`${JSON.stringify(revisionPlan(options))}\n`);
    else {
      const { report, reportPath } = await probeLiveRevision(options);
      process.stdout.write(`${JSON.stringify({ status: report.status, reportPath, failure: report.failure ?? null })}\n`);
      if (report.status === "failed") process.exitCode = 1;
    }
  } catch (error) {
    process.stderr.write(`${JSON.stringify({ status: "blocked", code: error.probeCode ?? "PROBE_FAILED" })}\n`);
    process.exitCode = 2;
  }
}
