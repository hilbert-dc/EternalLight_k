import assert from "node:assert/strict";
import { createHash, randomBytes, randomUUID } from "node:crypto";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { closeProbeHosts, parseRevisionOptions, summarizeObservation } from "./probe-live-revision.mjs";
import { finalizeProbeStage } from "./probe-live-extraction.mjs";
import { startRuntime, request as runtimeRequest, limited } from "./probe-live-story.mjs";
import { CHAPTER_STRUCTURE_ANALYSIS_PROMPT } from "../electron/chapter-structure-analysis.mjs";
import { createMcpStartupWaiter } from "../electron/tests/helpers/mcp-startup.mjs";

const projectRoot = fileURLToPath(new URL("..", import.meta.url));
const hash = value => createHash("sha256").update(String(value)).digest("hex");
const fail = code => { throw Object.assign(new Error(code), { probeCode: code }); };
const removedBody = "落潮前，守馆人老周在图书馆将铜钥匙交给林澄，约定第二天午夜用它打开钟楼地下室。林澄答应带回母亲留下的账册，老周提醒她，钥匙只能转动一次。";
const remainingBody = "第二天午夜，林澄依照昨天在图书馆与老周的约定来到钟楼。她取出老周交给她的铜钥匙，打开地下室，找到母亲的账册。她想到自己答应带回账册，便将它放进背包。";
const allowedTools = ["read_context", "read_chapter_structure_source", "query_story", "search_story",
  "resolve_story_evidence", "analyze_story_impact", "complete_task"];
const budgetReasons = new Map([["task token budget exhausted", "task_token_budget_exhausted"],
  ["task request budget exhausted", "task_request_budget_exhausted"], ["task duration budget exhausted", "task_duration_budget_exhausted"]]);
const publicCodes = new Set([...budgetReasons.values(), "invalid_tool_arguments", "provider_output_item_limit", "context_item_limit",
  "provider_http_error", "provider_connection_failed", "provider_rate_limited", "provider_server_error", "provider_quota_exhausted",
  "provider_invalid_request", "provider_output_limit", "provider_content_filter", "response_incomplete", "response_stream_disconnected",
  "bridge_error", "MODEL_TURN_NOT_COMPLETED", "CHILD_TASK_NOT_COMPLETED", "MCP_STARTUP_FAILED", "MCP_STARTUP_TIMEOUT", "PROBE_DURATION_LIMIT"]);
const count = value => Number.isSafeInteger(value) && value >= 0 ? value : null;

export function safeStructureFailure(error) {
  // Never classify arbitrary provider prose as a trusted budget error.
  return { type: ["Error", "TypeError", "RangeError", "AssertionError"].includes(error?.name) ? error.name : "unknown",
    code: publicCodes.has(error?.code) ? error.code : publicCodes.has(error?.probeCode) ? error.probeCode : "unknown" };
}

export function observeStructureTransport(diagnostics, kind, value) {
  const safe = { ...summarizeObservation(value, kind), elapsedSeconds: (Date.now() - diagnostics.startedAt) / 1000 };
  if (diagnostics.observations.length < 32) diagnostics.observations.push(safe); else diagnostics.observationsTruncated = true;
  if (kind === "request") diagnostics.localRequestsObserved += 1;
  if (kind === "response") diagnostics.lastResponse = safe;
}

export function attachStructureBudgetDiagnostics(budget, diagnostics) {
  const reserve = budget.reserveRequest.bind(budget), settle = budget.settleReservation.bind(budget);
  const reservations = new Map();
  budget.reserveRequest = (parameters = {}, now = Date.now()) => {
    const before = budget.snapshot();
    const record = { ordinal: ++diagnostics.reservationAttempts, estimatedInputTokens: count(parameters.estimatedInputTokens),
      maxOutputTokens: parameters.maxOutputTokens === undefined ? 0 : count(parameters.maxOutputTokens),
      requestsBefore: count(before.requests), usedTokensBefore: count(before.tokens), unknownTokensBefore: count(before.unknownTokens),
      reservedTokensBefore: count(before.reservedTokens), elapsedSeconds: (now - diagnostics.startedAt) / 1000,
      status: "pending", reason: null, reportedInputTokens: null, reportedOutputTokens: null, reportedTotalTokens: null };
    if (diagnostics.reservations.length < 32) diagnostics.reservations.push(record); else diagnostics.reservationsTruncated = true;
    try { const id = reserve(parameters, now); record.status = "reserved"; reservations.set(id, record); return id; }
    catch (error) {
      record.status = "rejected"; record.reason = budgetReasons.get(error?.message) ?? "unknown";
      diagnostics.budgetFailure = { ...safeStructureFailure(error), reason: record.reason, ordinal: record.ordinal };
      throw error;
    }
  };
  budget.settleReservation = (id, usage = null, options = {}) => {
    const result = settle(id, usage, options), record = reservations.get(id);
    if (record) {
      record.status = ["settled", "overrun", "unknown-reservation"].includes(result?.status) ? result.status : "unknown";
      record.outcome = ["completed", "unknown", "not-sent"].includes(options.outcome ?? "completed") ? options.outcome ?? "completed" : "unknown";
      record.reportedInputTokens = count(usage?.input_tokens ?? usage?.prompt_tokens);
      record.reportedOutputTokens = count(usage?.output_tokens ?? usage?.completion_tokens);
      record.reportedTotalTokens = count(usage?.total_tokens);
    }
    return result;
  };
}

export function verifyObservedStructurePages(pages, expected) {
  const ordered = [...new Map(pages.map(page => [page.start, page])).values()].sort((a, b) => a.start - b.start);
  let end = 0, text = "", continuous = true;
  for (const page of ordered) {
    if (page.start !== end || Array.from(page.quote).length !== page.end - page.start) { continuous = false; break; }
    end = page.end; text += page.quote;
  }
  return { pagesObserved: pages.length, uniquePages: ordered.length, continuous, verifiedCharacters: Array.from(text).length,
    observedHash: hash(text), wholeSourceVerified: continuous && text === expected };
}

export function chapterStructurePlan(options) {
  if (options.scope !== "single-sample-chapter-structure") fail("SCOPE_REQUIRED");
  for (const [key, maximum] of [["maxRequests", 12], ["maxTokens", 120000], ["timeoutSeconds", 240]]) {
    if (!Number.isSafeInteger(options[key]) || options[key] < 1 || options[key] > maximum) fail("INVALID_BUDGET");
  }
  if (![options.evidencePath, options.credentialSaltPath].every(value => typeof value === "string" && value.trim() && value.length <= 4096)) fail("EVIDENCE_REQUIRED");
  return { scope: options.scope, samples: 1, operation: "assessChapterStructure", isolatedCurrentSchema: true,
    limits: { requests: options.maxRequests, tokens: options.maxTokens, seconds: options.timeoutSeconds },
    costConfirmation: `${options.maxRequests}-${options.maxTokens}-${options.timeoutSeconds}:actual-cost-unknown`,
    actualCost: "unavailable", qualification: "trusted-current-evidence-required-no-automatic-verification" };
}

// Loading trusted evidence is read-only; never manufacture qualification or
// silently add the cost of a fresh verification to this single task.
async function qualifiedConnection(options) {
  const api = await import("../electron/codex-runtime.mjs");
  const credentials = await import("../electron/development-credentials.mjs");
  const environment = credentials.resolveDevelopmentEnvironment(credentials.loadProjectDevelopmentEnvironment(projectRoot, {}));
  if (!environment.DEEPSEEK_API_KEY) fail("PROJECT_CREDENTIAL_UNAVAILABLE");
  const cli = api.resolveCodexCli({ explicitPath: process.env.NOVEL_CODEX_CLI, isPackaged: false, projectRoot, resourcesPath: projectRoot });
  if (!cli.path) fail("CODEX_UNAVAILABLE");
  const baseInstructions = (await readFile(resolve(projectRoot, "codex-runtime/engine-prompt.md"), "utf8")).trim();
  const credentialScopes = api.credentialScopesFromEnvironment(environment, resolve(options.credentialSaltPath));
  const qualificationContextDigest = api.createQualificationContextDigest({ baseInstructions, cliPath: cli.path });
  let provider;
  try {
    provider = api.resolveCodexProvider({ provider: "deepseek", model: "deepseek-flash", operation: "assessChapterStructure",
      credentialScopes, evidence: await api.loadCodexEvidence(resolve(options.evidencePath)), qualificationContextDigest,
      minimumEvidenceValidityMs: options.timeoutSeconds * 1000 });
  } catch { fail("QUALIFICATION_REQUIRED"); }
  return { api, environment, cli, baseInstructions, provider };
}

export async function inspectChapterStructureQualification(options) {
  chapterStructurePlan(options);
  try { await qualifiedConnection(options); return { eligible: true, model: "deepseek-flash", modelCalls: 0 }; }
  catch (error) { return { eligible: false, code: error.probeCode ?? "QUALIFICATION_INSPECTION_FAILED", modelCalls: 0 }; }
}

// Ordinary MCP tool results are inspected in memory only. Neither tool
// arguments/results nor provider reasoning are written to the report.
function publicNodes(value) {
  const queue = [value], nodes = [];
  for (let count = 0; queue.length && count < 4096; count++) {
    const item = queue.shift();
    if (typeof item === "string" && item.length <= 262144) {
      try { queue.push(JSON.parse(item)); } catch { /* Public plain text is not a receipt. */ }
    } else if (item && typeof item === "object") { nodes.push(item); queue.push(...Object.values(item)); }
  }
  return nodes;
}

export function summarizeStructureQuery(item) {
  const operations = ["text", "passages", "resolveEntity", "state", "relations"];
  const codes = ["invalid_query_plan", "invalid_request", "conflict", "internal_error", "task_scope_invalid", "task_binding_invalid"];
  const argumentsNodes = publicNodes(item?.arguments);
  const plan = argumentsNodes.find(node => Array.isArray(node.steps));
  const nodes = publicNodes(item?.result);
  const result = nodes.find(node => Array.isArray(node.steps) && node.steps.some(step => Array.isArray(step.results)));
  return { stepTypes: plan?.steps.slice(0, 4).map(step => operations.includes(step.operation?.type) ? step.operation.type : "unknown") ?? [],
    returnedItems: result?.steps.slice(0, 4).map(step => count(step.returnedItems) ?? count(step.results?.length)) ?? [],
    errorCode: nodes.map(node => node.error?.code ?? node.code).find(code => codes.includes(code)) ?? null };
}

export async function probeLiveChapterStructure(options) {
  const plan = chapterStructurePlan(options);
  if (options.execute !== true) fail("EXECUTION_REQUIRES_MAIN_AUTHORIZATION");
  if (options.confirmCost !== plan.costConfirmation) fail("COST_CONFIRMATION_REQUIRED");
  const root = await mkdtemp(join(tmpdir(), "novel-live-chapter-structure-"));
  const workspaceRoot = join(root, "workspace"); await mkdir(workspaceRoot, { mode: 0o700 });
  const report = { status: "failed", plan, qualificationAccepted: false, tools: [], toolsTruncated: false,
    removedSourceRead: false, remainingSourceRead: false, finalReplyRecorded: false, usageSettled: false,
    deletedSourceHash: hash(removedBody), retainedSourceHash: hash(remainingBody), manualResultReviewRequired: true,
    cleanup: "pending" };
  const startedAt = Date.now();
  report.diagnostics = { startedAt, observations: [], observationsTruncated: false, localRequestsObserved: 0,
    reservationAttempts: 0, reservations: [], reservationsTruncated: false, adapterErrors: [], adapterErrorsTruncated: false };
  let runtime, bridge, adapter, budget, startup, task, grant, request, base, lease, budgetPath, sourceBaseline;
  let stage = "qualification";
  try {
    const { api, environment, cli, baseInstructions, provider } = await qualifiedConnection(options);
    report.qualificationAccepted = true;
    const engine = await import("@eternallight/agent-engine");
    const { CodexAppServerAdapter } = await import("../electron/codex-adapter.mjs");
    const workflow = await import("../electron/workflow-handoff.mjs");
    const { taskProviderBudgetOptions } = await import("../electron/task-provider-budget.mjs");
    const binary = name => join(projectRoot, "native", "target", "debug", `${name}${process.platform === "win32" ? ".exe" : ""}`);
    const authorToken = randomBytes(32).toString("hex"), candidateToken = randomBytes(32).toString("hex");
    const database = join(workspaceRoot, "story.sqlite3"); budgetPath = `${database}.budget`;
    stage = "runtime_startup"; runtime = await startRuntime(database, authorToken, candidateToken);
    request = (method, path, payload) => runtimeRequest(runtime.origin, authorToken, method, path, payload);
    const { project } = await request("POST", "/api/projects", { title: "隔离章节衔接检查" }); base = `/api/projects/${project.id}`;
    stage = "seed_and_delete";
    async function seed(title, body, key) {
      let ws = await request("GET", `${base}/creative-workspace`);
      const chapter = await request("POST", `${base}/chapters/materialize-standalone`, { title, expectedRevision: ws.revision, idempotencyKey: `${key}-create` });
      ws = await request("GET", `${base}/creative-workspace`);
      const saved = await request("POST", `${base}/drafts/checkpoints`, { expectedRevision: ws.revision,
        expectedCandidateRevisionId: null, idempotencyKey: `${key}-save`, payload: { chapterId: chapter.chapterId,
          chapterNumber: chapter.chapterNumber, title, body, sceneGroups: [{ id: `${key}-scene`, body, locked: false }], completeness: "chapterComplete" } });
      return { chapter, saved };
    }
    const removed = await seed("图书馆的约定", removedBody, "removed");
    const retained = await seed("钟楼地下室", remainingBody, "retained");
    const before = await request("GET", `${base}/creative-workspace`);
    const deletion = await request("POST", `${base}/chapters/${removed.chapter.chapterId}/delete`, { expectedRevision: before.revision, idempotencyKey: "delete-one" });
    assert.equal(deletion.chapterId, removed.chapter.chapterId); assert.equal(deletion.resultRevision, before.revision + 1);
    const eventPath = `${base}/chapter-structure-changes/${deletion.structureChangeId}`;
    const change = await request("GET", eventPath);
    assert.equal(change.assessment.state, "pending"); assert.equal(change.source.savedRevisionId, removed.saved.revisionId);
    const original = await request("GET", `${base}/creative-workspace`), originalState = await request("GET", `${base}/state`);
    assert.equal(original.chapters.length, 1); assert.equal(original.chapters[0].id, retained.chapter.chapterId);
    sourceBaseline = { workspace: original, canon: originalState.canon };
    stage = "adapter_startup";
    bridge = new engine.ModelBridge({ sessionPlan: provider.sessionPlan, connections: provider.connections,
      credentialResolver: async ref => environment[ref], requestObserver: value => observeStructureTransport(report.diagnostics, "request", value),
      responseObserver: value => observeStructureTransport(report.diagnostics, "response", value),
      toolPolicy: { rootAllowed: allowedTools.flatMap(name => [name, `mcp__novel_workspace.${name}`, `mcp__novel_workspace__${name}`]), childAllowed: [] } });
    const address = await bridge.start(), home = join(workspaceRoot, "codex-home");
    await mkdir(join(home, "sqlite"), { recursive: true, mode: 0o700 });
    const catalog = join(home, "catalog.json");
    await writeFile(catalog, JSON.stringify(api.buildCodexModelCatalog({ sessionPlan: provider.sessionPlan, baseInstructions })), { mode: 0o600 });
    await writeFile(join(home, "config.toml"), api.buildCodexConfig({ sessionPlan: provider.sessionPlan, modelCatalogPath: catalog,
      bridge: { baseUrl: address.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" }, mcpServerCommand: binary("novel-workspace-mcp") }), { mode: 0o600 });
    const capability = randomBytes(32).toString("hex");
    const processEnvironment = Object.fromEntries(["PATH", "PATHEXT", "SystemRoot", "WINDIR", "TEMP", "TMP", "TMPDIR"].filter(key => process.env[key]).map(key => [key, process.env[key]]));
    adapter = new CodexAppServerAdapter({ command: [cli.path, "app-server"], cwd: workspaceRoot, model: provider.id, provider: provider.provider,
      env: { ...processEnvironment, CODEX_HOME: home, CODEX_SQLITE_HOME: join(home, "sqlite"), AGENT_ENGINE_BRIDGE_TOKEN: address.token,
        NOVEL_CODEX_RUNTIME_URL: runtime.origin, NOVEL_CODEX_CANDIDATE_TOKEN: candidateToken,
        NOVEL_CODEX_TASK_CAPABILITY: capability, NOVEL_CODEX_OPERATION: "assessChapterStructure" } });
    const removedPages = [];
    adapter.setCallbacks({ onEvent: event => {
      startup?.onEvent(event);
      if (event.method === "turn/completed") {
        report.diagnostics.turnStatus = ["completed", "failed", "interrupted", "inProgress"].includes(event.params?.turn?.status) ? event.params.turn.status : "unknown";
        const error = event.params?.turn?.error;
        if (error) {
          const info = error.codexErrorInfo;
          const variants = ["contextWindowExceeded", "usageLimitExceeded", "httpConnectionFailed", "responseStreamConnectionFailed", "responseStreamDisconnected", "responseTooManyFailedAttempts", "badRequest", "serverOverloaded", "internalServerError", "unauthorized", "other"];
          const variant = typeof info === "string" ? variants.find(value => value === info) : variants.find(value => info && Object.hasOwn(info, value));
          const safe = { ...safeStructureFailure(error), codexErrorInfo: variant ?? "unknown", httpStatus: count(error.httpStatusCode ?? info?.[variant]?.httpStatusCode) };
          if (report.diagnostics.adapterErrors.length < 16) report.diagnostics.adapterErrors.push(safe); else report.diagnostics.adapterErrorsTruncated = true;
        }
      }
      const item = event.params?.item;
      if (event.method !== "item/completed" || item?.type !== "mcpToolCall") return;
      const name = allowedTools.find(value => [value, `mcp__novel_workspace__${value}`, `mcp__novel_workspace.${value}`].includes(item.tool ?? item.name)) ?? "other";
      const nodes = publicNodes(item.result);
      const completed = item.status === "completed" && !item.error && !item.displayTruncated && !nodes.some(node => node.isError === true || node.error);
      if (report.tools.length < 64) report.tools.push({ name, completed,
        ...(name === "query_story" ? { query: summarizeStructureQuery(item) } : {}) }); else report.toolsTruncated = true;
      if (!completed) return;
      if (name === "read_chapter_structure_source") for (const node of nodes) {
        if (node.structureChangeId === change.id && node.sourceRole === "saved" && node.source?.revisionId === removed.saved.revisionId
          && node.source?.objectId === `draft-${removed.chapter.chapterId}` && typeof node.quote === "string"
          && Number.isSafeInteger(node.start) && Number.isSafeInteger(node.end)) {
          removedPages.push({ start: node.start, end: node.end, quote: node.quote });
          report.removedSourceEvidence = verifyObservedStructurePages(removedPages, removedBody);
          report.removedSourceRead = report.removedSourceEvidence.wholeSourceVerified;
        }
      }
      if (["read_context", "query_story", "search_story"].includes(name)) report.remainingSourceRead ||= nodes.some(node =>
        (node.sourceRevisionId === retained.saved.revisionId || node.revisionId === retained.saved.revisionId)
        && (node.objectId === `draft-${retained.chapter.chapterId}` || node.sourceObjectId === `draft-${retained.chapter.chapterId}`)
        && JSON.stringify(node).includes("昨天在图书馆与老周的约定"));
    } });
    adapter.startProcess(); startup = createMcpStartupWaiter({ serverName: "novel_workspace", process: adapter.process, timeoutMs: 30000 });
    await limited(adapter.ensureSession(), 30000, "session startup"); await startup.ready;
    stage = "bind_task";
    const deadlineAt = Math.floor(Date.now() / 1000) + options.timeoutSeconds;
    task = await request("POST", `${base}/tasks`, { operation: "assessChapterStructure", target: { type: "project", structureChangeId: change.id },
      inputRevision: original.revision, threadId: adapter.threadId, authorIntent: CHAPTER_STRUCTURE_ANALYSIS_PROMPT,
      capabilityHash: hash(capability), capabilityExpiresAt: deadlineAt,
      initialBudget: { authorizationId: randomUUID(), authorMessageId: randomUUID(), expectedBaseSnapshotId: original.headSnapshotId,
        expectedSourceDigest: original.sourceDigest, maxRequests: options.maxRequests, maxTokens: options.maxTokens, deadlineAt } });
    grant = await request("GET", `${base}/tasks/${task.id}/provider-budget`);
    const bound = await request("GET", eventPath);
    assert.equal(bound.assessment.state, "bound"); assert.equal(bound.assessment.taskId, task.id); assert.equal(bound.assessment.workTaskId, grant.workTaskId);
    budget = new engine.PersistentTaskBudget({ databasePath: budgetPath, runId: task.id,
      ...taskProviderBudgetOptions(grant, { taskId: task.id, workTaskId: grant.workTaskId, deadlineAt: grant.deadlineAt }) });
    attachStructureBudgetDiagnostics(budget, report.diagnostics);
    await bridge.beginTaskBudget(budget); lease = engine.createExecutionLease({ sessionPlan: provider.sessionPlan, runId: task.id, budgetId: budget.id });
    await bridge.beginExecutionLease(lease);
    const handoff = workflow.createTaskBoundHandoff(project.id, original, task);
    const prompt = api.composeNovelCodexPrompt(workflow.taskMethodText(task), workflow.taskExecutionPrompt(task), workflow.taskBoundInstructions(handoff));
    stage = "model";
    let deadlineTimer;
    const turn = await Promise.race([adapter.sendTurn(prompt, { disableEnvironment: true }), new Promise((_, reject) => {
      deadlineTimer = setTimeout(() => reject(Object.assign(new Error("PROBE_DURATION_LIMIT"), { probeCode: "PROBE_DURATION_LIMIT" })), Math.max(1, deadlineAt * 1000 - Date.now()));
    })]).finally(() => clearTimeout(deadlineTimer));
    await finalizeProbeStage({ task, result: turn, observation: report,
      runtimeRequest: (path, input = {}) => request(input.method ?? "GET", path, input.body ? JSON.parse(input.body) : undefined) });
    await bridge.endExecutionLease(lease);
    report.finalAnswer = turn.finalReply?.text ?? "";
    stage = "verification";
    assert.equal(report.removedSourceRead, true);
    assert.ok(report.remainingSourceRead, "Actual current second-chapter source was not observed");
    assert.ok(report.tools.some(tool => tool.name === "complete_task" && tool.completed));
    assert.match(report.finalAnswer, /钥匙/); assert.match(report.finalAnswer, /约定|来源|交付|获得|缺|断|衔接/); assert.match(report.finalAnswer, /[\p{Script=Han}]/u);
    const reply = await request("GET", `${base}/tasks/${task.id}/final-reply`);
    assert.equal(reply.state, "recorded"); assert.equal(reply.binding.text, report.finalAnswer); assert.equal(reply.binding.truncated, false);
    const after = await request("GET", `${base}/creative-workspace`);
    assert.deepEqual(after.chapters, original.chapters); assert.equal(after.sourceDigest, original.sourceDigest);
    assert.equal(after.revision, original.revision); assert.equal(after.headSnapshotId, original.headSnapshotId);
    assert.deepEqual((await request("GET", `${base}/state`)).canon, originalState.canon);
    const work = await request("GET", `${base}/work-tasks/${grant.workTaskId}`);
    assert.equal(work.steps[0].operation, "assessChapterStructure"); assert.equal(work.steps[0].expectedOutputKind, "completionReport");
    assert.ok(work.deliveryReport.evidence.some(item => item.kind === "completionReport" && item.state === "recorded" && item.referenceId === task.id));
    report.sourceUnchanged = true; report.completionEvidenceRecorded = true; report.automatedContractPassed = true; report.status = "passed";
  } catch (error) {
    report.failure = { stage, code: /^[A-Z_]+$/.test(error.probeCode ?? "") ? error.probeCode : error.code === "ERR_ASSERTION" ? "ASSERTION_FAILED" : "PROBE_FAILED" };
    report.diagnostics.caught = safeStructureFailure(error);
  } finally {
    startup?.dispose(); await closeProbeHosts({ bridge, adapter }, report);
    // Read-only verification also survives a failed model turn. Never infer
    // unchanged story state from a missing final answer or a permission policy.
    if (sourceBaseline) {
      try {
        const after = await request("GET", `${base}/creative-workspace`);
        const afterCanon = (await request("GET", `${base}/state`)).canon;
        report.sourceVerification = { beforeChapterHash: hash(JSON.stringify(sourceBaseline.workspace.chapters)),
          afterChapterHash: hash(JSON.stringify(after.chapters)),
          sourceDigestUnchanged: after.sourceDigest === sourceBaseline.workspace.sourceDigest,
          revisionUnchanged: after.revision === sourceBaseline.workspace.revision,
          snapshotUnchanged: after.headSnapshotId === sourceBaseline.workspace.headSnapshotId,
          formalKnowledgeUnchanged: JSON.stringify(afterCanon) === JSON.stringify(sourceBaseline.canon) };
        report.sourceUnchanged = report.sourceVerification.beforeChapterHash === report.sourceVerification.afterChapterHash
          && Object.values(report.sourceVerification).filter(value => typeof value === "boolean").every(Boolean);
        if (!report.sourceUnchanged) report.status = "failed";
      } catch { report.sourceVerificationFailed = true; report.status = "failed"; }
    }
    if (task && budget) {
      if (report.status !== "passed" && report.runtimeTaskStatus !== "completed") await request("POST", `${base}/tasks/${task.id}/cancel`).catch(() => { report.cancelFailed = true; });
      try {
        budget.complete(report.status === "passed" ? "completed" : "failed");
        await request("POST", `${base}/work-tasks/${grant.workTaskId}/provider-usage`, { taskId: task.id }); report.usageSettled = true;
        const receipt = budget.constructor.receipt(budgetPath, task.id);
        report.diagnostics.receiptStatus = ["terminal", "completed", "failed", "cancelled", "outcome-unknown", "recovered_unknown"].includes(receipt.status) ? receipt.status : "unknown";
        report.diagnostics.receiptOverrun = typeof receipt.usage.overrun === "boolean" ? receipt.usage.overrun : null;
        report.usage = { requests: receipt.usage.requests, tokens: receipt.usage.tokens, unknownTokens: receipt.usage.unknownTokens };
        assert.ok(report.usage.requests <= options.maxRequests && report.usage.tokens <= options.maxTokens);
        assert.equal(report.usage.unknownTokens, 0);
      } catch { report.settlementFailed = true; report.status = "failed"; }
      budget.close();
    }
    if (report.shutdownFailed || report.cancelFailed) report.status = "failed";
    if (runtime?.child && runtime.child.exitCode === null && runtime.child.signalCode === null) {
      const exit = new Promise(done => runtime.child.once("exit", done)); runtime.child.kill();
      await Promise.race([exit, new Promise(done => { const timer = setTimeout(done, 3000); timer.unref(); })]);
      if (runtime.child.exitCode === null && runtime.child.signalCode === null) { report.runtimeStopFailed = true; report.status = "failed"; }
    }
    try { await rm(workspaceRoot, { recursive: true, force: true }); report.cleanup = "completed"; }
    catch { report.cleanup = "failed"; report.status = "failed"; }
  }
  report.elapsedSeconds = (Date.now() - startedAt) / 1000;
  delete report.diagnostics.startedAt;
  const reportPath = join(root, "report.json"); await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`, { mode: 0o600, flag: "wx" });
  return { report, reportPath };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const options = parseRevisionOptions(process.argv.slice(2));
    if (!options.execute) process.stdout.write(`${JSON.stringify(chapterStructurePlan(options))}\n`);
    else {
      const { report, reportPath } = await probeLiveChapterStructure(options);
      process.stdout.write(`${JSON.stringify({ status: report.status, reportPath, failure: report.failure ?? null })}\n`);
      if (report.status !== "passed") process.exitCode = 1;
    }
  } catch (error) { process.stderr.write(`${JSON.stringify({ status: "blocked", code: error.probeCode ?? "PROBE_FAILED" })}\n`); process.exitCode = 2; }
}
