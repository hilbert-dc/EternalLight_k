import assert from "node:assert/strict";
import { createHash, randomBytes, randomUUID } from "node:crypto";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { closeProbeHosts, parseRevisionOptions, summarizeObservation } from "./probe-live-revision.mjs";
import { queryFailureDiagnostic } from "./probe-query-diagnostics.mjs";
import { persistTaskFinalReply } from "../electron/agent-final-reply.mjs";

const projectRoot = fileURLToPath(new URL("..", import.meta.url));
const hash = value => createHash("sha256").update(String(value)).digest("hex");
const fail = code => { throw Object.assign(new Error(code), { probeCode: code }); };
const tools = ["read_context", "query_story", "resolve_story_evidence", "request_candidate_task", "submit_commitment_extraction", "complete_task"];
const body = "午夜，林砚打开邮局的信箱，发现一封盖着明日邮戳的信。信纸只有一句：别让钟楼敲响第十三声。她把信藏进外套，听见楼上早已停摆的钟又走了一格。值班册里，失踪三年的师傅刚签下名字，墨迹仍湿。门外的小女孩递来一把铜钥匙，说母亲嘱咐她在涨潮前交给林砚。钥匙齿间卡着蓝色蜡屑，与信封上的封印完全相同。林砚刚想追问，女孩便朝没有桥的河心走去，鞋底却没有沾水。";
const authorIntent = "请提取本章有原文依据的伏笔与悬念，实际阅读整章后保存完整建议报告，说明还有哪些不确定；不要改写正文、发布章节、建立独立承诺或确认设定。";

export function extractionPlan(options) {
  if (options.scope !== "single-sample-conversation-extraction") fail("SCOPE_REQUIRED");
  for (const [key, maximum] of [["maxRequests", 12], ["maxTokens", 100000], ["timeoutSeconds", 240]]) {
    if (!Number.isSafeInteger(options[key]) || options[key] < 1 || options[key] > maximum) fail("INVALID_BUDGET");
  }
  if (![options.evidencePath, options.credentialSaltPath].every(value => typeof value === "string" && value.trim() && value.length <= 4096)) fail("EVIDENCE_REQUIRED");
  return { scope: options.scope, samples: 1, stages: ["advisory", "extractCommitments"],
    sharedWorkLimits: { requests: options.maxRequests, tokens: options.maxTokens, seconds: options.timeoutSeconds },
    costConfirmation: `${options.maxRequests}-${options.maxTokens}-${options.timeoutSeconds}:actual-cost-unknown`,
    qualification: "reuse-current-trusted-evidence-no-publication", actualCost: "unavailable" };
}

// Only ordinary MCP results are inspected; provider reasoning and arguments are never retained.
function resultNodes(value) {
  const queue = [value], nodes = [];
  for (let count = 0; queue.length && count < 4096; count++) {
    const item = queue.shift();
    if (typeof item === "string" && item.length <= 262144) {
      try { queue.push(JSON.parse(item)); } catch { /* Plain public text is not a structured receipt. */ }
    } else if (item && typeof item === "object") { nodes.push(item); queue.push(...Object.values(item)); }
  }
  return nodes;
}
export function toolSummary(item) {
  const name = tools.find(value => [value, `mcp__novel_workspace__${value}`, `mcp__novel_workspace.${value}`].includes(item?.tool ?? item?.name)) ?? "other";
  const nodes = resultNodes(item?.result);
  const completed = item?.status === "completed" && !item?.error && !nodes.some(value => value.isError === true || value.error);
  return { name, completed, failure: !completed && ["query_story", "read_context"].includes(name) ? queryFailureDiagnostic(nodes) : null,
    candidateCount: name === "submit_commitment_extraction"
      ? nodes.find(value => Number.isSafeInteger(value.candidateCount) && value.candidateCount >= 0 && value.candidateCount <= 16)?.candidateCount ?? null : null };
}

export function sourceWasDelivered(item, source) {
  const summary = toolSummary(item);
  if (summary.name !== "read_context" || !summary.completed) return false;
  return resultNodes(item.result).some(value => {
    if (value.sourceRevisionId !== source.revisionId || value.objectId !== source.objectId) return false;
    const content = value.content;
    return content?.body === source.body || (value.textRepresentation === "sceneGroups"
      && Array.isArray(content?.sceneGroups) && content.sceneGroups.length > 0
      && content.sceneGroups.every(group => typeof group.body === "string")
      && content.sceneGroups.map(group => group.body.trim()).join("\n\n---\n\n") === source.body);
  });
}

export async function finalizeProbeStage({ task, result, runtimeRequest, observation }) {
  observation.modelTurnStatus = ["completed", "failed", "interrupted", "inProgress"].includes(result?.turn?.status) ? result.turn.status : "unknown";
  if (observation.modelTurnStatus !== "completed") fail("MODEL_TURN_NOT_COMPLETED");
  if (typeof result.turn.id !== "string" || !result.turn.id || result.turn.id.length > 160) fail("MODEL_TURN_ID_MISSING");
  const path = `/api/projects/${encodeURIComponent(task.projectId)}/tasks/${encodeURIComponent(task.id)}`;
  const readTerminal = async () => {
    const current = await runtimeRequest(path);
    observation.runtimeTaskStatus = ["queued", "running", "waiting", "completed", "failed", "cancelled"].includes(current.status) ? current.status : "unknown";
    if (current.id !== task.id || current.projectId !== task.projectId || current.threadId !== task.threadId) fail("TASK_TERMINAL_IDENTITY_MISMATCH");
    return current;
  };
  // Candidate tasks must finish through their actual MCP output/complete contract.
  if (task.operation !== "advisory" && (await readTerminal()).status !== "completed") fail("CHILD_TASK_NOT_COMPLETED");
  try {
    await runtimeRequest(`${path}/turn`, { method: "POST", body: JSON.stringify({ threadId: task.threadId, turnId: result.turn.id }) });
    await persistTaskFinalReply(runtimeRequest, task.projectId, task, result);
    if (task.operation === "advisory") await runtimeRequest(`${path}/complete`, {
      method: "POST", body: JSON.stringify({ threadId: task.threadId, summary: Array.from(result.message ?? "").slice(0, 4000).join("") }),
    });
  } catch {
    await readTerminal().catch(() => {});
    fail(task.operation === "advisory" ? "ADVISORY_HOST_FINALIZE_FAILED" : "CHILD_FINAL_REPLY_BINDING_FAILED");
  }
  const terminal = await readTerminal();
  if (terminal.status !== "completed") fail(task.operation === "advisory" ? "ADVISORY_TASK_NOT_COMPLETED" : "CHILD_TASK_NOT_COMPLETED");
  observation.finalReplyRecorded = true;
}

// Bound both stages together; never retain provider prose, arguments or credentials.
export function observeExtraction(report, operation, kind, value) {
  if (!["advisory", "extractCommitments"].includes(operation) || !["request", "response"].includes(kind)) return;
  if (report.observations.length >= 32) { report.observationsTruncated = true; return; }
  report.observations.push({ operation, ...summarizeObservation(value, kind) });
}

export async function probeLiveExtraction(options) {
  const plan = extractionPlan(options);
  if (options.execute !== true) fail("EXECUTION_REQUIRES_MAIN_AUTHORIZATION");
  if (options.confirmCost !== plan.costConfirmation) fail("COST_CONFIRMATION_REQUIRED");
  const root = await mkdtemp(join(tmpdir(), "novel-live-extraction-"));
  const workspaceRoot = join(root, "workspace"); await mkdir(workspaceRoot, { mode: 0o700 });
  const report = { status: "failed", plan, sourceCharacters: Array.from(body).length, sourceHash: hash(body), stages: [], observations: [], observationsTruncated: false,
    certificationPublished: false, authorAccepted: false, cleanup: "pending" };
  let runtime, request, base, original, sourceRevisionId, parentTask, workId, currentStage = "preflight";
  try {
    const engine = await import("@eternallight/agent-engine");
    const api = await import("../electron/codex-runtime.mjs");
    const credentials = await import("../electron/development-credentials.mjs");
    const workflow = await import("../electron/workflow-handoff.mjs");
    const { executeConversationCandidates } = await import("../electron/conversation-analysis.mjs");
    const { composeAdvisoryPrompt } = await import("../electron/advisory-prompt.mjs");
    const { CodexAppServerAdapter } = await import("../electron/codex-adapter.mjs");
    const { taskProviderBudgetOptions } = await import("../electron/task-provider-budget.mjs");
    const { createMcpStartupWaiter } = await import("../electron/tests/helpers/mcp-startup.mjs");
    const { startRuntime, request: runtimeRequest, limited } = await import("./probe-live-story.mjs");
    const environment = credentials.resolveDevelopmentEnvironment(credentials.loadProjectDevelopmentEnvironment(projectRoot, {}));
    if (!environment.DEEPSEEK_API_KEY) fail("PROJECT_CREDENTIAL_UNAVAILABLE");
    const cli = api.resolveCodexCli({ explicitPath: process.env.NOVEL_CODEX_CLI, isPackaged: false, projectRoot, resourcesPath: projectRoot });
    if (!cli.path) fail("CODEX_UNAVAILABLE");
    const baseInstructions = (await readFile(resolve(projectRoot, "codex-runtime/engine-prompt.md"), "utf8")).trim();
    const credentialScopes = api.credentialScopesFromEnvironment(environment, resolve(options.credentialSaltPath));
    const qualificationContextDigest = api.createQualificationContextDigest({ baseInstructions, cliPath: cli.path });
    const provider = api.resolveCodexProvider({ provider: "deepseek", model: "deepseek-flash", credentialScopes,
      evidence: await api.loadCodexEvidence(resolve(options.evidencePath)), qualificationContextDigest });
    const binary = name => join(projectRoot, "native", "target", "debug", `${name}${process.platform === "win32" ? ".exe" : ""}`);
    const authorToken = randomBytes(32).toString("hex"), candidateToken = randomBytes(32).toString("hex");
    const database = join(workspaceRoot, "story.sqlite3"), budgetPath = `${database}.budget`;
    runtime = await startRuntime(database, authorToken, candidateToken);
    request = (method, path, payload) => runtimeRequest(runtime.origin, authorToken, method, path, payload);
    const { project } = await request("POST", "/api/projects", { title: "隔离真实伏笔探针" }); base = `/api/projects/${project.id}`;
    await request("POST", `${base}/drafts/checkpoints`, { expectedRevision: 0, expectedCandidateRevisionId: null,
      idempotencyKey: "probe-original", payload: { chapterId: "chapter-one", chapterNumber: 1, title: "午夜回信", body,
        sceneGroups: [{ id: "whole-chapter", body, locked: false }], completeness: "complete" } });
    original = await request("GET", `${base}/creative-workspace`); sourceRevisionId = original.chapters[0].candidate.id;
    const originalKnowledge = await request("GET", `${base}/state`);
    const deadlineAt = Math.floor(Date.now() / 1000) + options.timeoutSeconds;
    async function runStage(operation, target, intent) {
      currentStage = operation;
      const record = { operation, status: "failed", tools: [], fullSourceDelivered: false, usageSettled: false };
      report.stages.push(record);
      let bridge, adapter, budget, startup, task, grant, lease;
      try {
        const allowed = operation === "advisory" ? tools.filter(name => !["submit_commitment_extraction"].includes(name))
          : tools.filter(name => name !== "request_candidate_task");
        bridge = new engine.ModelBridge({ sessionPlan: provider.sessionPlan, connections: provider.connections,
          credentialResolver: async ref => environment[ref],
          requestObserver: value => observeExtraction(report, operation, "request", value),
          responseObserver: value => observeExtraction(report, operation, "response", value),
          toolPolicy: { rootAllowed: allowed.flatMap(name => [name, `mcp__novel_workspace.${name}`, `mcp__novel_workspace__${name}`]), childAllowed: [] } });
        const address = await bridge.start(), home = join(workspaceRoot, operation);
        await mkdir(join(home, "sqlite"), { recursive: true, mode: 0o700 });
        const catalog = join(home, "catalog.json");
        await writeFile(catalog, JSON.stringify(api.buildCodexModelCatalog({ sessionPlan: provider.sessionPlan, baseInstructions })), { mode: 0o600 });
        await writeFile(join(home, "config.toml"), api.buildCodexConfig({ sessionPlan: provider.sessionPlan, modelCatalogPath: catalog,
          bridge: { baseUrl: address.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" }, mcpServerCommand: binary("novel-workspace-mcp") }), { mode: 0o600 });
        const capability = randomBytes(32).toString("hex");
        const processEnvironment = Object.fromEntries(["PATH", "PATHEXT", "SystemRoot", "WINDIR", "TEMP", "TMP", "TMPDIR"].filter(key => process.env[key]).map(key => [key, process.env[key]]));
        adapter = new CodexAppServerAdapter({ command: [cli.path, "app-server"], cwd: workspaceRoot, model: provider.id, provider: provider.provider,
          env: { ...processEnvironment, CODEX_HOME: home, CODEX_SQLITE_HOME: join(home, "sqlite"), AGENT_ENGINE_BRIDGE_TOKEN: address.token,
            NOVEL_CODEX_RUNTIME_URL: runtime.origin, NOVEL_CODEX_CANDIDATE_TOKEN: candidateToken, NOVEL_CODEX_TASK_CAPABILITY: capability, NOVEL_CODEX_OPERATION: operation } });
        adapter.setCallbacks({ onEvent: event => {
          startup?.onEvent(event);
          const item = event.params?.item;
          if (event.method !== "item/completed" || item?.type !== "mcpToolCall") return;
          if (record.tools.length < 64) record.tools.push(toolSummary(item));
          record.fullSourceDelivered ||= sourceWasDelivered(item, { revisionId: sourceRevisionId, objectId: "draft-chapter-one", body });
        } });
        adapter.startProcess(); startup = createMcpStartupWaiter({ serverName: "novel_workspace", process: adapter.process, timeoutMs: 30000 });
        await limited(adapter.ensureSession(), 30000, "session startup"); await startup.ready;
        const current = await request("GET", `${base}/creative-workspace`);
        task = await request("POST", `${base}/tasks`, { operation, target, inputRevision: current.revision, threadId: adapter.threadId,
          authorIntent: intent, capabilityHash: hash(capability), capabilityExpiresAt: deadlineAt,
          ...(operation === "advisory" ? { initialBudget: { authorizationId: randomUUID(), authorMessageId: randomUUID(),
            expectedBaseSnapshotId: current.headSnapshotId, expectedSourceDigest: current.sourceDigest,
            maxRequests: options.maxRequests, maxTokens: options.maxTokens, deadlineAt } } : {}) });
        grant = await request("GET", `${base}/tasks/${task.id}/provider-budget`);
        if (workId) assert.equal(grant.workTaskId, workId, "Child must retain the exact parent Work budget"); else workId = grant.workTaskId;
        budget = new engine.PersistentTaskBudget({ databasePath: budgetPath, runId: task.id,
          ...taskProviderBudgetOptions(grant, { taskId: task.id, workTaskId: grant.workTaskId, deadlineAt: grant.deadlineAt }) });
        await bridge.beginTaskBudget(budget); lease = engine.createExecutionLease({ sessionPlan: provider.sessionPlan, runId: task.id, budgetId: budget.id });
        await bridge.beginExecutionLease(lease);
        const handoff = workflow.createTaskBoundHandoff(project.id, current, task);
        const prompt = operation === "advisory"
          ? composeAdvisoryPrompt(task, workflow.taskAuthorPrompt(task), handoff)
          : api.composeNovelCodexPrompt(workflow.taskMethodText(task), workflow.taskAuthorPrompt(task), workflow.taskBoundInstructions(handoff));
        const turn = await limited(adapter.sendTurn(prompt, { disableEnvironment: true }), Math.max(1, deadlineAt * 1000 - Date.now()), "model task");
        await finalizeProbeStage({ task, result: turn, observation: record,
          runtimeRequest: (path, options = {}) => request(options.method ?? "GET", path, options.body ? JSON.parse(options.body) : undefined) });
        await bridge.endExecutionLease(lease);
        record.status = "completed";
        return { workflow: { task }, message: turn.finalReply?.text ?? turn.message ?? "" };
      } finally {
        startup?.dispose();
        await closeProbeHosts({ bridge, adapter }, record);
        if (task && budget) {
          if (record.status !== "completed") await request("POST", `${base}/tasks/${task.id}/cancel`).catch(() => { record.cancelFailed = true; });
          budget.complete(record.status === "completed" ? "completed" : "failed");
          await request("POST", `${base}/work-tasks/${grant.workTaskId}/provider-usage`, { taskId: task.id }).then(() => { record.usageSettled = true; }, () => {});
          const receipt = engine.PersistentTaskBudget.receipt(budgetPath, task.id);
          record.usage = { requests: receipt.usage.requests, tokens: receipt.usage.tokens, unknownTokens: receipt.usage.unknownTokens };
          budget.close();
        }
      }
    }
    const discussion = await runStage("advisory", { type: "project" }, authorIntent); parentTask = discussion.workflow.task;
    const result = await executeConversationCandidates(null, project.id, discussion, {
      runtimeRequest: (path, options = {}) => request(options.method ?? "GET", path, options.body ? JSON.parse(options.body) : undefined),
      currentWorkFlowState: async () => (await request("GET", `${base}/tasks/${parentTask.id}`)).status,
      assertAvailable: operation => { if (operation !== "extractCommitments") fail("UNEXPECTED_OPERATION"); },
      notifyCandidateStart: () => {},
      runCandidateWorkflow: async (_event, _project, intent, method, target) => runStage(method.operation, target, intent),
    });
    assert.equal(result.candidateResults.length, 1, "One ordinary author message must produce one real extraction child");
    currentStage = "verification";
    const saved = await request("GET", `${base}/chapters/chapter-one/commitment-extractions`);
    const payload = saved.payload, characters = Array.from(body);
    assert.ok(payload.candidates.length >= 2, "The real model did not save at least two evidenced proposals; no proposals are synthesized by the probe");
    assert.equal(payload.sourceRevisionId, sourceRevisionId);
    assert.deepEqual(payload.examinedRanges, [{ start: 0, end: characters.length }]);
    const extraction = report.stages.find(stage => stage.operation === "extractCommitments");
    assert.ok(extraction.fullSourceDelivered, "Whole chapter was not observed in the actual MCP delivery");
    assert.ok(extraction.tools.some(tool => tool.name === "resolve_story_evidence" && tool.completed), "No real evidence resolution call completed");
    assert.ok(extraction.tools.some(tool => tool.name === "complete_task" && tool.completed));
    assert.equal(extraction.tools.filter(tool => tool.name === "submit_commitment_extraction" && tool.completed).at(-1)?.candidateCount, payload.candidates.length);
    report.anchors = payload.candidates.flatMap(candidate => {
      assert.ok(candidate.setupEvidenceRefs.length > 0);
      return candidate.setupEvidenceRefs.map(anchor => {
      assert.equal(anchor.objectId, "draft-chapter-one"); assert.equal(anchor.revisionId, sourceRevisionId);
      assert.ok(Number.isSafeInteger(anchor.start) && Number.isSafeInteger(anchor.end) && anchor.start >= 0 && anchor.end > anchor.start && anchor.end <= characters.length);
      assert.equal(characters.slice(anchor.start, anchor.end).join(""), anchor.quote);
      return { start: anchor.start, end: anchor.end, quoteHash: hash(anchor.quote) };
      });
    });
    const after = await request("GET", `${base}/creative-workspace`);
    assert.deepEqual(after.chapters, original.chapters); assert.equal(after.revision, original.revision);
    assert.deepEqual((await request("GET", `${base}/state`)).canon, originalKnowledge.canon);
    assert.equal(after.chapters[0].accepted, null);
    const commitments = await request("GET", `${base}/commitments?limit=20`); assert.equal(commitments.data.length, 0);
    assert.ok(report.stages.every(stage => stage.usageSettled && !stage.shutdownFailed && !stage.cancelFailed));
    report.candidateCount = payload.candidates.length; report.sourceUnchanged = true; report.status = "passed";
  } catch (error) {
    report.failure = { stage: currentStage, code: /^[A-Z_]+$/.test(error.probeCode ?? "") ? error.probeCode : error.code === "ERR_ASSERTION" ? "ASSERTION_FAILED" : "PROBE_FAILED" };
  } finally {
    if (runtime?.child && runtime.child.exitCode === null && runtime.child.signalCode === null) {
      const exit = new Promise(resolveExit => runtime.child.once("exit", resolveExit));
      runtime.child.kill();
      await Promise.race([exit, new Promise(resolveExit => { const timer = setTimeout(resolveExit, 3000); timer.unref(); })]);
      if (runtime.child.exitCode === null && runtime.child.signalCode === null) { report.status = "failed"; report.runtimeStopFailed = true; }
    }
    try { await rm(workspaceRoot, { recursive: true, force: true }); report.cleanup = "completed"; }
    catch { report.cleanup = "failed"; report.status = "failed"; }
  }
  const reportPath = join(root, "report.json"); await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`, { mode: 0o600, flag: "wx" });
  return { report, reportPath };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const options = parseRevisionOptions(process.argv.slice(2));
    if (!options.execute) process.stdout.write(`${JSON.stringify(extractionPlan(options))}\n`);
    else {
      const { report, reportPath } = await probeLiveExtraction(options);
      process.stdout.write(`${JSON.stringify({ status: report.status, reportPath, failure: report.failure ?? null })}\n`);
      if (report.status !== "passed") process.exitCode = 1;
    }
  } catch (error) { process.stderr.write(`${JSON.stringify({ status: "blocked", code: error.probeCode ?? "PROBE_FAILED" })}\n`); process.exitCode = 2; }
}
