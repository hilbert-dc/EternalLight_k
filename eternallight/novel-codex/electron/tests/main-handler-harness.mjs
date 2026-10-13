import { EXECUTION_CHANNELS } from "../execution-requests.mjs";
import { readFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { after } from "node:test";
import { runInNewContext } from "node:vm";
import { EventEmitter } from "node:events";
import { createHash, randomUUID } from "node:crypto";
import { withFrozenMethods } from "./fixtures/frozen-methods.mjs";

// Execute the production main handlers with a real Adapter; only Electron startup
// and the Runtime transport are replaced so the tests run without a display.
export async function mainHandlers(runtime, command, fixtureState = {}) {
  if (!fixtureState.agentStorage) {
    const directory = await mkdtemp(join(tmpdir(), "novel-main-handler-"));
    fixtureState.agentStorage = { budgetDb: join(directory, "engine.sqlite3") };
    after(() => rm(directory, { recursive: true, force: true }));
  }
  const mainUrl = new URL("../main.mjs", import.meta.url);
  let source = await readFile(mainUrl, "utf8");
  // Load the extracted production registrations in the same controlled boundary
  // as main so transport/credential fixtures remain the only replacements.
  const registrationModules = ["workspace-observation-handlers", "model-configuration-handlers", "session-control-handlers", "creative-execution-handlers", "desktop-window", "runtime-service", "model-transport", "agent-turn-runtime", "work-flow-coordinator", "creative-task-coordinator", "generation-coordinator", "planning-review-coordinator", "advisory-coordinator", "desktop-services", "conversation-delivery", "automatic-work-consumers", "trusted-ipc-registry"];
  const registrations = await Promise.all(registrationModules.map(name => readFile(new URL(`../${name}.mjs`, import.meta.url), "utf8")));
  source = source.replace(/^import \{ (?:register(?:WorkspaceObservation|ModelConfiguration|SessionControl|CreativeExecution)Handlers|create(?:DesktopWindow|RuntimeService|ModelTransport|AgentTurnRuntime|WorkFlowCoordinator|CreativeTaskCoordinator|GenerationCoordinator|PlanningReviewCoordinator|AdvisoryCoordinator|DesktopServices|ConversationDelivery|AutomaticWorkConsumers|TrustedIpcRegistry)) \} from .+;\n/gm, "");
  source = registrations.map(text => text.replace(/^export function /gm, "function ")).join("\n") + "\n" + source;
  const bindings = { EXECUTION_CHANNELS, process, console, URL, URLSearchParams, Buffer, structuredClone, setTimeout, clearTimeout };
  const { CAPABILITIES } = await import("@eternallight/agent-engine");
  const { verifiedBaselineEvidence } = await import("@eternallight/agent-engine/test-support");
  bindings.CAPABILITIES = CAPABILITIES;
  bindings.verifiedBaselineEvidence = verifiedBaselineEvidence;
  for (const match of source.matchAll(/^import \{ (.+) \} from "(.+)";$/gm)) {
    if (match[2] === "electron") continue;
    const namespace = await import(match[2].startsWith(".") ? new URL(match[2], mainUrl) : match[2]);
    for (const name of match[1].split(", ")) {
      const [sourceName, localName] = name.split(/\s+as\s+/);
      bindings[localName ?? sourceName] = namespace[sourceName];
    }
  }
  // A generation fixture uses real budget storage while its provider evidence remains controlled.
  if (fixtureState.agentStorage) {
    bindings.loadCodexEvidence = async () => fixtureState.fixtureEvidence;
    bindings.credentialScopesFromEnvironment = () => ({});
  }
  if (fixtureState.providerConfig) bindings.resolveCodexProvider = () => fixtureState.providerConfig;
  let applicationName = "Electron";
  bindings.app = { setName(name) { applicationName = name; }, getName: () => applicationName, isPackaged: false, requestSingleInstanceLock: () => false, quit() {}, setPath() {},
    getPreferredSystemLanguages: () => ["zh-CN"], getPath: () => dirname(fixtureState.agentStorage.budgetDb) };
  bindings.ipcMain = {};
  const powerBlocks = new Set();
  bindings.powerSaveBlocker = { start() { const id = powerBlocks.size + 1; powerBlocks.add(id); return id; }, stop(id) { powerBlocks.delete(id); }, isStarted(id) { return powerBlocks.has(id); } };
  // Scheme registration belongs to Electron bootstrap, which this VM does not start.
  bindings.protocol = { registerSchemesAsPrivileged() {} };
  bindings.BrowserWindow = { fromWebContents: () => ({ isDestroyed: () => false }) };
  bindings.dialog = { showMessageBox: async (_owner, options) => {
    fixtureState.confirmations?.push(options);
    return { response: 0 };
  } };
  const observedWorks = new Map(), fixtureOwners = new Map();
  fixtureState.ownerRequests ??= [];
  // Isolated handler fixtures replace Runtime; their explicit current owner
  // contract stays here rather than allowing an unknown endpoint to return a
  // generic Work. Real HTTP tests set nativeRuntime and bypass this mock.
  const fixtureOwner = (path, body) => {
    if (/^\/api\/projects\/[^/]+\/work-tasks\/[^/]+\/execution-condition$/.test(path)) {
      assertOwner(body && Object.keys(body).join(",") === "digest" && /^[a-f0-9]{64}$/.test(body.digest));
      return { recorded: true };
    }
    const match = path.match(/^\/api\/projects\/([^/]+)\/work-tasks\/([^/]+)\/execution-owner(?:\/(renew|release))?$/);
    if (!match) return null;
    const projectId = decodeURIComponent(match[1]), workId = decodeURIComponent(match[2]);
    const work = observedWorks.get(JSON.stringify([projectId, workId]));
    assertOwner(work?.projectId === projectId && work.id === workId && typeof work.threadId === "string");
    const key = JSON.stringify([projectId, workId]);
    const operation = match[3] ?? "acquire", lease = fixtureOwners.get(key);
    fixtureState.ownerRequests.push({ projectId, workId, operation, body: { ...body } });
    assertOwner(body && typeof body.hostSessionId === "string");
    if (operation === "acquire") {
      assertOwner(Object.keys(body).sort().join(",") === "expectedWorkVersion,hostSessionId" && body.expectedWorkVersion === work.version);
      assertOwner(!["paused", "cancelled"].includes(work.status) && (!lease || lease.hostSessionId === body.hostSessionId));
      const next = lease ?? { leaseId: randomUUID(), hostSessionId: body.hostSessionId, workTaskId: work.id,
        rootThreadId: work.threadId, expiresAt: Math.floor(Date.now() / 1000) + 30 };
      fixtureOwners.set(key, next); return next;
    }
    assertOwner(Object.keys(body).sort().join(",") === "hostSessionId,leaseId" && lease?.leaseId === body.leaseId && lease.hostSessionId === body.hostSessionId);
    if (operation === "release") { fixtureOwners.delete(key); return { released: true }; }
    assertOwner(!["paused", "cancelled"].includes(work.status));
    lease.expiresAt = Math.floor(Date.now() / 1000) + 30; return { ...lease };
  };
  const controlReceipts = new Map(), messages = new Map();
  const fixtureMessage = (path, body) => {
    const match = path.match(/^\/api\/projects\/([^/]+)\/conversations\/([^/]+)\/messages(?:\/([^/]+))?$/);
    if (!match) return null;
    const [, projectId, threadId, id] = match.map(value => value && decodeURIComponent(value));
    if (!id && !body) return { messages: [...messages.values()].filter(m => m.projectId === projectId && m.threadId === threadId && !['delivered','cancelled'].includes(m.status)) };
    if (!id) {
      const existing = messages.get(body.messageId);
      if (existing) return structuredClone(existing);
      const message = { ...body, id: body.messageId, projectId, threadId, revision: 0, position: messages.size, status: 'queued', taskId: null, turnId: null, triggerReady: true };
      messages.set(message.id, message); return structuredClone(message);
    }
    const message = messages.get(id);
    assertOwner(message?.projectId === projectId && message.threadId === threadId);
    if (!body) return structuredClone(message);
    assertOwner(body.expectedRevision === message.revision);
    if (body.action === 'claim') { assertOwner(message.status === 'queued'); message.status = 'claimed'; }
    else if (body.action === 'sending') { assertOwner(message.status === 'claimed' && message.taskId); message.status = 'sending'; }
    else if (body.action === 'delivered') { assertOwner(message.status === 'sending' && body.turnId); Object.assign(message, {status:'delivered', turnId:body.turnId}); }
    else if (body.action === 'fail') message.error = body.error;
    else throw Error('Unsupported message fixture action: ' + body.action);
    message.revision++; return structuredClone(message);
  };
  bindings.fetch = async (url, options) => {
    const pathname = new URL(url).pathname, body = options?.body ? JSON.parse(options.body) : null;
    if (!fixtureState.nativeRuntime && pathname.includes("/control-commands/")) {
      return { ok: true, json: async () => ({ receipt: controlReceipts.get(pathname.split("/").at(-1))?.receipt ?? null }) };
    }
    const isWorkControl = !fixtureState.nativeRuntime && pathname.includes("/work-tasks/") && pathname.endsWith("/control");
    if (isWorkControl) {
      assertOwner(Object.keys(body).sort().join(",") === "command,commandId,expectedControlRevision,threadId");
      assertOwner(typeof body.commandId === "string" && /^[a-f0-9-]{36}$/.test(body.commandId) && typeof body.threadId === "string");
      if (controlReceipts.has(body.commandId)) return { ok: true, json: async () => ({ ...controlReceipts.get(body.commandId), replayed: true }) };
    }
    const mockedOwner = fixtureState.nativeRuntime ? null : fixtureOwner(pathname, body);
    const mockedMessage = fixtureState.nativeRuntime ? null : fixtureMessage(pathname, body);
    const payload = mockedOwner ?? mockedMessage ?? await runtime(pathname, body, options, new URL(url));
    if (!fixtureState.nativeRuntime && pathname.endsWith('/tasks') && body?.target?.messageDispatch) {
      const dispatch = body.target.messageDispatch, message = messages.get(dispatch.messageId);
      assertOwner(message?.status === 'claimed' && !message.taskId && message.revision === dispatch.revision && message.sessionId === dispatch.sessionId && message.input.text === body.authorIntent);
      message.taskId = payload.id; message.revision++;
    }
    if (typeof payload?.id === "string" && payload.id.startsWith("work-") && typeof payload.projectId === "string"
      && Array.isArray(payload.steps)) observedWorks.set(JSON.stringify([payload.projectId, payload.id]), payload);
    if (payload?.id && payload.threadId && Number.isInteger(payload.inputRevision) && !payload.inputManifest?.creativeCapabilityBinding) {
      payload.inputManifest = { authorIntent: options?.body ? JSON.parse(options.body).authorIntent : undefined, ...payload.inputManifest };
      payload.operation ??= options?.body ? JSON.parse(options.body).operation : undefined;
      if (new URL(url).pathname.includes("/planning-reviews/")) payload.operation ??= "reviewPlanningSeat";
      withFrozenMethods(payload, decodeURIComponent(new URL(url).pathname.split("/projects/")[1]?.split("/")[0] ?? "project"));
    }
    if (isWorkControl) {
      const outcome = { work: payload, replayed: false, receipt: { ...body, projectId: decodeURIComponent(pathname.split("/")[3]),
        workTaskId: decodeURIComponent(pathname.split("/")[5]), status: "applied", appliedControlRevision: payload.controlRevision,
        workVersion: payload.version, reason: null, createdAt: Math.floor(Date.now() / 1000) } };
      controlReceipts.set(body.commandId, structuredClone(outcome));
      return { ok: true, json: async () => outcome };
    }
    return { ok: true, json: async () => payload };
  };
  bindings.fixtureCommand = command;
  bindings.fixtureState = fixtureState;
  source = source.replace(/^import .+;\n/gm, "").replaceAll("import.meta.url", JSON.stringify(mainUrl.href));
  return runInNewContext(`${source}\n
    runtimeUrl = fixtureState.runtimeUrl ?? 'http://127.0.0.1:8000';
    multimediaHost = fixtureState.multimediaHost ?? null;
    engineQualificationContextDigest = 'fixture-qualification-context';
    const inspectedConnection = inspectCodexConnection({provider:'deepseek', model:'deepseek-flash'});
    const fixtureEvidence = verifiedBaselineEvidence(inspectedConnection.connection, {
      protocol: 'responses',
      capabilities: [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.TOOL_LOOP, CAPABILITIES.STREAMING, CAPABILITIES.USAGE],
      qualificationContextDigest: engineQualificationContextDigest
    });
    fixtureState.fixtureEvidence = fixtureEvidence;
    baseExecutionDomain.agentProviderConfig = fixtureState.providerConfig ?? resolveCodexProvider({provider:'deepseek', model:'deepseek-flash', evidence:fixtureEvidence, qualificationContextDigest:engineQualificationContextDigest});
    baseExecutionDomain.agentStorage = fixtureState.agentStorage ?? null;
    baseExecutionDomain.modelBridge = fixtureState.modelBridge ?? {
      start: async()=>({baseUrl:'http://127.0.0.1:1',token:'fixture'}),
      beginTaskBudget: async(budget)=>{fixtureState.budgets?.push(budget.snapshot());},
      beginExecutionLease: async(lease)=>{fixtureState.leases?.push(lease);}, cancelActiveRequests: async()=>0,
      endExecutionLease: async()=>true, endTaskBudget: async()=>true, close: async()=>{}
    };
    baseExecutionDomain.codexAdapter.configure({command:fixtureCommand,apiKey:null,provider:'deepseek',model:'deepseek-flash',
      modelRoute:fixtureState.providerConfig?.sessionPlan.defaultModel,
      env:{...process.env,NOVEL_CODEX_TEST_THREAD_STATE:fixtureState.agentStorage.budgetDb+'.fixture-threads.json',...fixtureState.adapterEnv}});
    baseExecutionDomain.activeGenerationRunId = fixtureState.activeGenerationRunId ?? null;
    baseExecutionDomain.activeWorkFlow = fixtureState.activeWorkFlow ?? null;
    baseExecutionDomain.activeAgentExecution = fixtureState.activeAgentExecution ?? null;
    runtimeProcess = fixtureState.runtimeProcess ?? null;
    recoveryIndex = fixtureState.recoveryIndex ?? null;
    rendererWebContents = fixtureState.rendererWebContents ?? null;
    executionDomains.prepare = async domain => {
      domain.agentProviderConfig = structuredClone(baseExecutionDomain.agentProviderConfig);
      domain.agentStorage = baseExecutionDomain.agentStorage;
      domain.modelBridge = { ...baseExecutionDomain.modelBridge };
      domain.codexAdapter = new CodexAppServerAdapter({ command: baseExecutionDomain.codexAdapter.enabled ? fixtureCommand : null, apiKey: null, provider: 'deepseek', model: 'deepseek-flash',
        modelRoute: fixtureState.providerConfig?.sessionPlan.defaultModel,
        env: { ...process.env, NOVEL_CODEX_TEST_THREAD_STATE: fixtureState.agentStorage.budgetDb + '.fixture-threads.json', ...fixtureState.adapterEnv } });
      (fixtureState.executionDomains ??= []).push(domain);
      fixtureState.onPrepareDomain?.(domain);
    };
    const handlers = new Map(), rawHandlers = new Map(), executionTargets = new Map();
    // Test client follows the production preload envelope, without weakening Host validation.
    registerIpcHandlers({handle:(name,handler)=> {
      rawHandlers.set(name, handler);
      handlers.set(name, EXECUTION_CHANNELS.has(name) ? (event, ...args) => {
        const requestId = randomUUID();
        executionTargets.set(event, { requestId, projectId: args[0] });
        return handler(event, { requestId, args });
      } : handler);
    }});
    ({handlers, rawHandlers, executionTargets, replaceActiveExecution: (flow, execution) => { baseExecutionDomain.activeWorkFlow = flow; baseExecutionDomain.activeAgentExecution = execution; }, get adapter() { return executionDomains.active().at(-1)?.codexAdapter ?? baseExecutionDomain.codexAdapter; }, executionDomains, conversationMessages, chapterStructureConsumer, workExecutionConsumer, workExecutionOwners, resumeWorkTask, sendAgentTurn, providerConfig:baseExecutionDomain.agentProviderConfig, modelBridge: baseExecutionDomain.modelBridge, modelConnectionGate});
  `, bindings, { filename: mainUrl.pathname });
}

function assertOwner(condition) {
  if (!condition) throw Object.assign(new Error("fixture current execution-owner contract rejected"), { code: "WORK_EXECUTION_OWNER_INVALID" });
}

export function runtimeFixture() {
  let nextTaskSequence = 0;
  const tasks = new Map();
  const intents = new Map();
  const workTasks = new Map();
  const attemptParents = new Map();
  const completions = [];
  const replies = new Map();
  const lookup = [];
  const conversations = new Map();
  const registrations = new Map();
  const authorTurns = new Map();
  let boundTurnSequence = 0;
  return {
    tasks, intents, workTasks, completions, lookup, conversations,
    async request(path, body, options, url) {
      const conversationPath = path.match(/^\/api\/projects\/([^/]+)\/conversations(?:\/([^/]+))?$/);
      if (conversationPath) {
        const projectId = decodeURIComponent(conversationPath[1]);
        const id = conversationPath[2] ? decodeURIComponent(conversationPath[2]) : null;
        if (!id && !body) {
          const archived = url?.searchParams.get('archived') === 'true';
          return { conversations: [...conversations.values()].filter(record => record.projectId === projectId && record.isArchived === archived)
            .sort((a, b) => Number(b.isPinned) - Number(a.isPinned) || b.updatedAt - a.updatedAt || a.id.localeCompare(b.id)).map(record => ({ ...record })) };
        }
        if (!id) {
          const existing = conversations.get(body.threadId);
          if (existing && existing.projectId !== projectId) throw new Error('对话不属于当前作品');
          if (existing && registrations.get(body.threadId) !== JSON.stringify(body)) throw new Error('登记内容冲突');
          if (existing) return { ...existing };
          if (body.forkedFromId && conversations.get(body.forkedFromId)?.projectId !== projectId) throw new Error('分支来源不属于当前作品');
          const record = { id: body.threadId, projectId, title: body.title, isPinned: false, isArchived: false,
            createdAt: Date.now(), updatedAt: Date.now(), forkedFromId: body.forkedFromId, lastTurnId: body.lastTurnId, version: 0 };
          conversations.set(record.id, record); registrations.set(record.id, JSON.stringify(body));
          return { ...record };
        }
        const record = conversations.get(id);
        if (record?.projectId !== projectId) throw new Error('对话不属于当前作品');
        if (body) {
          if (record.version !== body.expectedVersion) throw new Error('对话版本已变化');
          Object.assign(record, Object.fromEntries(Object.entries(body).filter(([key]) => ['title', 'isPinned', 'isArchived'].includes(key))),
            { version: record.version + 1, updatedAt: Date.now() });
        }
        return { ...record };
      }
      if (path.endsWith('/author-messages')) {
        const threadId = decodeURIComponent(path.split('/threads/')[1].split('/')[0]);
        const projectId = decodeURIComponent(path.split('/projects/')[1].split('/')[0]);
        const ancestry = [], seen = new Set();
        let current = threadId;
        while (current) {
          if (seen.has(current)) throw new Error('cyclic branch fixture');
          seen.add(current);
          const record = conversations.get(current);
          if (record && record.projectId !== projectId) throw new Error('foreign author branch');
          ancestry.unshift({ id: current, boundary: record?.lastTurnId });
          current = record?.forkedFromId;
        }
        let allowed = new Map();
        for (const { id, boundary } of ancestry) {
          const ceiling = allowed.get(boundary)?.ordinal;
          if (ceiling === undefined) allowed.clear();
          else allowed = new Map([...allowed].filter(([, message]) => message.ordinal <= ceiling));
          for (const [turnId, message] of authorTurns.get(id) ?? []) if (message.projectId === projectId) allowed.set(turnId, message);
        }
        return { threadId, messages: body.turnIds.flatMap(turnId => allowed.has(turnId) ? [{ turnId, text: allowed.get(turnId).text }] : []) };
      }
      if (path.endsWith('/turn')) {
        const taskId = path.split('/').at(-2);
        const task = [...tasks.values()].find(item => item.id === taskId);
        if (!task || task.threadId !== body.threadId) throw new Error('fixture task turn mismatch');
        task.turnId = body.turnId;
        if (!authorTurns.has(task.threadId)) authorTurns.set(task.threadId, new Map());
        authorTurns.get(task.threadId).set(body.turnId, { projectId: task.projectId, text: task.authorIntent, ordinal: boundTurnSequence++ });
        return task;
      }
      if (path.endsWith('/final-reply')) {
        const taskId = path.split('/').at(-2);
        if (body) replies.set(taskId, { taskId, projectId: 'project', binding: body });
        return replies.get(taskId) ?? null;
      }
      if (path.includes('/intents') || path.endsWith('/answers')) {
        const taskId = path.split('/tasks/')[1].split('/')[0];
        const records = intents.get(taskId) ?? [];
        if (path.endsWith('/delivered')) {
          const revision = Number(path.split('/').at(-2));
          const record = records.find((item) => item.intentRevision === revision);
          if (!record) throw new Error('missing intent');
          record.status = 'delivered';
          return record;
        }
        if (body) {
          if (body.expectedIntentRevision !== records.length || records.at(-1)?.status === 'pending') throw new Error('stale intent');
          const record = { taskId, authorMessageId: `author-message-${taskId}-${records.length + 1}`, intentRevision: records.length + 1, authorText: body.authorText, status: path.endsWith('/answers') ? 'recorded' : 'pending' };
          intents.set(taskId, [...records, record]);
          return record;
        }
        return { intents: records };
      }
      if (path.endsWith('/work-task')) {
        const id = path.split('/').at(-2);
        return workTasks.get(attemptParents.get(id));
      }
      if (path.includes('/work-tasks/')) {
        const id = path.split('/work-tasks/')[1].split('/')[0];
        const task = workTasks.get(id);
        if (path.endsWith('/decision')) {
          const done = task.steps.every(step => step.status === 'completed');
          if (done) task.status = 'completed';
          return { workTaskId: task.id, threadId: task.threadId, step: null, stopReason: done ? 'completed' : 'workNotReady' };
        }
        if (path.endsWith('/budget-authorization')) {
          if (body) { task.initialBudget = body; task.version += 1; return {}; }
          return { authorization: task?.initialBudget ?? null };
        }
        if (path.endsWith('/provider-usage') && !body) return { pending: null };
        if (path.endsWith('/control')) {
          assertOwner(body.expectedControlRevision === task.controlRevision);
          task.controlRevision += 1;
          task.status = body.command === 'resume' ? 'running' : body.command === 'pause' ? 'paused' : 'cancelled';
          task.version += 1;
        }
        return task;
      }
      if (path.endsWith('/work-tasks')) return { tasks: [...workTasks.values()], nextCursor: null, activeCount: [...workTasks.values()].filter((task) => ['running', 'paused', 'blocked'].includes(task.status)).length };
      if (path.endsWith('/tasks/lookup')) {
        lookup.push(body.threadIds);
        return {tasks:body.threadIds.map((id)=>tasks.get(id)).filter(task => task?.projectId === decodeURIComponent(path.split("/")[3]))};
      }
      if (/\/tasks\/[^/]+\/planning-reviews$/.test(path)) return { runs: [] };
      if (path.endsWith('/generation-handoff')) return { proposal: null };
      if (path.endsWith('/actions')) return { actions: [] };
      if (path.endsWith('/next-step')) return { action: null };
      if (path.endsWith('/creative-workspace')) return {project:{id:decodeURIComponent(path.split('/projects/')[1].split('/')[0])},revision:0,headSnapshotId:'snapshot-fixture',sourceDigest:'source-fixture',tasks:[]};
      if (path.endsWith('/provider-budget')) {
        const taskId = path.split('/').at(-2);
        const owner = workTasks.get(attemptParents.get(taskId));
        return { taskId, workTaskId: owner.id, budgetId: `budget-${taskId}`,
          maxRequests: owner.initialBudget.maxRequests, maxTokens: owner.initialBudget.maxTokens,
          deadlineAt: owner.initialBudget.deadlineAt };
      }
      if (path.endsWith('/tasks')) {
        const id = body.operation === 'assessChapterStructure' && body.initialBudget
          ? `task-${createHash('sha256').update(JSON.stringify(['project', body.initialBudget.authorizationId])).digest('hex').slice(0, 32)}`
          : `task-${String(nextTaskSequence++).padStart(32,'0')}`;
        const task = {...body,id,status:'queued',projectId:'project',purpose:body.operation === 'advisory' ? 'advisory' : 'planner'};
        tasks.set(body.threadId,task);
        const source = body.target?.requestedByTaskId ?? body.target?.sourceTaskId;
        const resumedWork = body.target?.resumeStepId ? [...workTasks.values()].find(work => work.steps.some(step => step.id === body.target.resumeStepId)) : null;
        if (body.target?.resumeStepId && !resumedWork) throw new Error('fixture resume step is not persisted');
        const parent = resumedWork?.id ?? (source ? attemptParents.get(source) ?? `work-${task.id}` : body.target?.runId ? `work-${body.target.runId}` : `work-${task.id}`);
        attemptParents.set(task.id, parent);
        if (!workTasks.has(parent)) workTasks.set(parent, { id: parent, projectId: 'project', threadId: body.threadId, status: 'running', version: 0, controlRevision: 0, kind: 'conversation', initialBudget: body.initialBudget, steps: [] });
        if (resumedWork) Object.assign(resumedWork.steps.find(step => step.id === body.target.resumeStepId), { currentAttemptId: task.id, status: 'running' });
        else workTasks.get(parent).steps.push({ id: `step-${task.id}`, currentAttemptId: task.id });
        return task;
      }
      if (path.includes('/work-decision?') || path.endsWith('/work-decision')) return { deliveryReviewRequired: false };
      const fixtureTask = path.match(/\/tasks\/([^/]+)$/);
      if (fixtureTask && options?.method === 'PATCH') {
        const task = [...tasks.values()].find(task => task.id === fixtureTask[1]);
        if (!task || task.threadId !== body.threadId) throw new Error('fixture task binding mismatch');
        if (!['queued','running','waiting'].includes(task.status)) throw new Error('TaskNotWritable');
        Object.assign(task, { status: body.status, error: body.error });
        return task;
      }
      if (fixtureTask && !body) return [...tasks.values()].find(task => task.id === fixtureTask[1]) ?? {};
      if (path.endsWith('/complete')) {
        if (Array.from(body.summary).length > 4000) throw new Error('summary too long');
        completions.push(body);
        const taskId = path.split('/').at(-2);
        const completed = [...tasks.values()].find(task => task.id === taskId);
        if (completed) completed.status = 'completed';
        const owner = workTasks.get(attemptParents.get(taskId));
        const step = owner?.steps.find(item => item.currentAttemptId === taskId);
        if (step) step.status = 'completed';
        return {status:'completed'};
      }
      return {};
    },
  };
}

export function rendererEvent() {
  const sender = new EventEmitter();
  sender.isDestroyed = () => false;
  sender.getURL = () => "http://127.0.0.1:5173/";
  sender.send = () => {};
  return {sender};
}
