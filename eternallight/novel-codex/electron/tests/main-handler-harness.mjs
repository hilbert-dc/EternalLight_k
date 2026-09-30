import { readFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after } from "node:test";
import { runInNewContext } from "node:vm";
import { EventEmitter } from "node:events";
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
  bindings.app = { isPackaged: false, requestSingleInstanceLock: () => false, quit() {}, setPath() {} };
  bindings.ipcMain = {};
  bindings.BrowserWindow = { fromWebContents: () => ({ isDestroyed: () => false }) };
  bindings.dialog = { showMessageBox: async (_owner, options) => {
    fixtureState.confirmations?.push(options);
    return { response: fixtureState.confirmationResponse ?? 1 };
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
    codexAdapter.configure({command:fixtureCommand,apiKey:null,provider:'deepseek',model:'deepseek-flash'});
    activeGenerationRunId = fixtureState.activeGenerationRunId ?? null;
    activeAgentExecution = fixtureState.activeAgentExecution ?? null;
    recoveryIndex = fixtureState.recoveryIndex ?? null;
    const handlers = new Map();
    registerIpcHandlers({handle:(name,handler)=>handlers.set(name,handler)});
    ({handlers, adapter:codexAdapter, modelingConsumer, sendAgentTurn, providerConfig:agentProviderConfig, modelBridge});
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
  return {
    tasks, intents, workTasks, completions, lookup,
    async request(path, body) {
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
      if (path.endsWith('/actions')) return { actions: [] };
      if (path.endsWith('/next-step')) return { action: null };
      if (path.endsWith('/creative-workspace')) return {revision:0,headSnapshotId:'snapshot-fixture',sourceDigest:'source-fixture',tasks:[]};
      if (path.endsWith('/provider-budget')) {
        const taskId = path.split('/').at(-2);
        const owner = workTasks.get(attemptParents.get(taskId));
        return { taskId, workTaskId: owner.id, budgetId: `budget-${taskId}`,
          maxRequests: owner.initialBudget.maxRequests, maxTokens: owner.initialBudget.maxTokens,
          deadlineAt: owner.initialBudget.deadlineAt };
      }
      if (path.endsWith('/tasks')) {
        const task = {...body,id:`task-${String(nextTaskSequence++).padStart(32,'0')}`,projectId:'project',purpose:body.operation === 'advisory' ? 'advisory' : 'planner'};
        tasks.set(body.threadId,task);
        const source = body.target?.requestedByTaskId ?? body.target?.sourceTaskId;
        const resumedWork = body.target?.resumeStepId ? [...workTasks.values()].find(work => work.steps.some(step => step.id === body.target.resumeStepId)) : null;
        if (body.target?.resumeStepId && !resumedWork) throw new Error('fixture resume step is not persisted');
        const parent = resumedWork?.id ?? (source ? attemptParents.get(source) ?? `work-${task.id}` : body.target?.runId ? `work-${body.target.runId}` : `work-${task.id}`);
        attemptParents.set(task.id, parent);
        if (!workTasks.has(parent)) workTasks.set(parent, { id: parent, projectId: 'project', status: 'running', version: 0, kind: 'conversation', initialBudget: body.initialBudget, steps: [] });
        if (resumedWork) Object.assign(resumedWork.steps.find(step => step.id === body.target.resumeStepId), { currentAttemptId: task.id, status: 'running' });
        else workTasks.get(parent).steps.push({ id: `step-${task.id}`, currentAttemptId: task.id });
        return task;
      }
      if (path.endsWith('/complete')) {
        if (Array.from(body.summary).length > 4000) throw new Error('summary too long');
        completions.push(body);
        return {status:'completed'};
      }
      return {};
    },
  };
}

export function rendererEvent() {
  const sender = new EventEmitter();
  sender.isDestroyed = () => false;
  sender.send = () => {};
  return {sender};
}
