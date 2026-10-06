import { readFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { after } from "node:test";
import { runInNewContext } from "node:vm";
import { EventEmitter } from "node:events";
import { createHash } from "node:crypto";
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
  const bindings = { process, console, URL, URLSearchParams, Buffer, setTimeout, clearTimeout };
  const { CAPABILITIES } = await import("@eternallight/agent-engine");
  const { verifiedBaselineEvidence } = await import("@eternallight/agent-engine/test-support");
  bindings.CAPABILITIES = CAPABILITIES;
  bindings.verifiedBaselineEvidence = verifiedBaselineEvidence;
  for (const match of source.matchAll(/^import \{ (.+) \} from "(.+)";$/gm)) {
    if (match[2] === "electron") continue;
    const namespace = await import(match[2].startsWith(".") ? new URL(match[2], mainUrl) : match[2]);
    for (const name of match[1].split(", ")) bindings[name] = namespace[name];
  }
  // A generation fixture uses real budget storage while its provider evidence remains controlled.
  if (fixtureState.agentStorage) {
    bindings.loadCodexEvidence = async () => fixtureState.fixtureEvidence;
    bindings.credentialScopesFromEnvironment = () => ({});
  }
  let applicationName = "Electron";
  bindings.app = { setName(name) { applicationName = name; }, getName: () => applicationName, isPackaged: false, requestSingleInstanceLock: () => false, quit() {}, setPath() {},
    getPath: () => dirname(fixtureState.agentStorage.budgetDb) };
  bindings.ipcMain = {};
  bindings.BrowserWindow = { fromWebContents: () => ({ isDestroyed: () => false }) };
  bindings.dialog = { showMessageBox: async (_owner, options) => {
    fixtureState.confirmations?.push(options);
    return { response: 0 };
  } };
  bindings.fetch = async (url, options) => {
    const payload = await runtime(new URL(url).pathname, options?.body ? JSON.parse(options.body) : null, options, new URL(url));
    if (payload?.id && payload.threadId && Number.isInteger(payload.inputRevision) && !payload.inputManifest?.creativeCapabilityBinding) {
      payload.inputManifest = { authorIntent: options?.body ? JSON.parse(options.body).authorIntent : undefined, ...payload.inputManifest };
      payload.operation ??= options?.body ? JSON.parse(options.body).operation : undefined;
      if (new URL(url).pathname.includes("/planning-reviews/")) payload.operation ??= "reviewPlanningSeat";
      withFrozenMethods(payload, decodeURIComponent(new URL(url).pathname.split("/projects/")[1]?.split("/")[0] ?? "project"));
    }
    return { ok: true, json: async () => payload };
  };
  bindings.fixtureCommand = command;
  bindings.fixtureState = fixtureState;
  source = source.replace(/^import .+;\n/gm, "").replaceAll("import.meta.url", JSON.stringify(mainUrl.href));
  return runInNewContext(`${source}\n
    runtimeUrl = 'http://127.0.0.1:8000';
    engineQualificationContextDigest = 'fixture-qualification-context';
    const inspectedConnection = inspectCodexConnection({provider:'deepseek', model:'deepseek-flash'});
    const fixtureEvidence = verifiedBaselineEvidence(inspectedConnection.connection, {
      protocol: 'responses',
      capabilities: [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.TOOL_LOOP, CAPABILITIES.STREAMING, CAPABILITIES.USAGE],
      qualificationContextDigest: engineQualificationContextDigest
    });
    fixtureState.fixtureEvidence = fixtureEvidence;
    agentProviderConfig = resolveCodexProvider({provider:'deepseek', model:'deepseek-flash', evidence:fixtureEvidence, qualificationContextDigest:engineQualificationContextDigest});
    agentStorage = fixtureState.agentStorage ?? null;
    modelBridge = {
      start: async()=>({baseUrl:'http://127.0.0.1:1',token:'fixture'}),
      beginTaskBudget: async(budget)=>{fixtureState.budgets?.push(budget.snapshot());},
      beginExecutionLease: async(lease)=>{fixtureState.leases?.push(lease);}, cancelActiveRequests: async()=>0,
      endExecutionLease: async()=>true, endTaskBudget: async()=>true, close: async()=>{}
    };
    codexAdapter.configure({command:fixtureCommand,apiKey:null,provider:'deepseek',model:'deepseek-flash',
      env:{...process.env,NOVEL_CODEX_TEST_THREAD_STATE:fixtureState.agentStorage.budgetDb+'.fixture-threads.json'}});
    activeGenerationRunId = fixtureState.activeGenerationRunId ?? null;
    activeAgentExecution = fixtureState.activeAgentExecution ?? null;
    recoveryIndex = fixtureState.recoveryIndex ?? null;
    rendererWebContents = fixtureState.rendererWebContents ?? null;
    const handlers = new Map();
    registerIpcHandlers({handle:(name,handler)=>handlers.set(name,handler)});
    ({handlers, adapter:codexAdapter, chapterStructureConsumer, sendAgentTurn, providerConfig:agentProviderConfig, modelBridge, modelConnectionGate});
  `, bindings, { filename: mainUrl.pathname });
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
      if (path.includes('/intents')) {
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
          const record = { taskId, authorMessageId: `author-message-${taskId}-${records.length + 1}`, intentRevision: records.length + 1, authorText: body.authorText, status: 'pending' };
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
          task.status = body.command === 'resume' ? 'running' : body.command === 'pause' ? 'paused' : 'cancelled';
          task.version += 1;
        }
        return task;
      }
      if (path.endsWith('/work-tasks')) return { tasks: [...workTasks.values()], nextCursor: null, activeCount: [...workTasks.values()].filter((task) => ['running', 'paused', 'blocked'].includes(task.status)).length };
      if (path.endsWith('/tasks/lookup')) {
        lookup.push(body.threadIds);
        return {tasks:body.threadIds.map((id)=>tasks.get(id)).filter(Boolean)};
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
        const task = {...body,id,projectId:'project',purpose:body.operation === 'advisory' ? 'advisory' : 'planner'};
        tasks.set(body.threadId,task);
        const source = body.target?.requestedByTaskId ?? body.target?.sourceTaskId;
        const resumedWork = body.target?.resumeStepId ? [...workTasks.values()].find(work => work.steps.some(step => step.id === body.target.resumeStepId)) : null;
        if (body.target?.resumeStepId && !resumedWork) throw new Error('fixture resume step is not persisted');
        const parent = resumedWork?.id ?? (source ? attemptParents.get(source) ?? `work-${task.id}` : body.target?.runId ? `work-${body.target.runId}` : `work-${task.id}`);
        attemptParents.set(task.id, parent);
        if (!workTasks.has(parent)) workTasks.set(parent, { id: parent, projectId: 'project', threadId: body.threadId, status: 'running', version: 0, kind: 'conversation', initialBudget: body.initialBudget, steps: [] });
        if (resumedWork) Object.assign(resumedWork.steps.find(step => step.id === body.target.resumeStepId), { currentAttemptId: task.id, status: 'running' });
        else workTasks.get(parent).steps.push({ id: `step-${task.id}`, currentAttemptId: task.id });
        return task;
      }
      if (path.endsWith('/complete')) {
        if (Array.from(body.summary).length > 4000) throw new Error('summary too long');
        completions.push(body);
        const taskId = path.split('/').at(-2);
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
