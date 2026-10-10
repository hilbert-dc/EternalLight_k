import test from "node:test";
import assert from "node:assert/strict";
import { reconcileWorkProviderUsage, resolveTaskBudgetAuthority, taskProviderBudgetOptions } from "../task-provider-budget.mjs";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { PersistentTaskBudget } from "@eternallight/agent-engine";
import { mainHandlers, rendererEvent, runtimeFixture } from "./main-handler-harness.mjs";

const grant = { taskId: "task-one", workTaskId: "work-run", budgetId: "budget-task-one",
  maxRequests: 3, maxTokens: 17, deadlineAt: 20 };
const context = { taskId: "task-one", workTaskId: "work-run", deadlineAt: 20, now: 1000 };

test("host consumes exact domain limits without recomputing or increasing allowance", () => {
  assert.deepEqual(taskProviderBudgetOptions(grant, context), {
    id: "budget-task-one", maxRequests: 3, maxTokens: 17, maxDurationMs: 19000,
  });
  assert.deepEqual(taskProviderBudgetOptions(grant, { ...context, now: 19000 }), {
    id: "budget-task-one", maxRequests: 3, maxTokens: 17, maxDurationMs: 1000,
  });
});

test("foreign, malformed and expired grants never create budget options", () => {
  for (const patch of [
    { taskId: "other" }, { workTaskId: "other" }, { budgetId: "other" },
    { deadlineAt: 21 }, { maxRequests: 0 }, { maxRequests: 1.5 },
    { maxTokens: -1 }, { maxTokens: Number.MAX_SAFE_INTEGER + 1 }, { maxTokens: 1.5 },
  ]) assert.throws(() => taskProviderBudgetOptions({ ...grant, ...patch }, context));
  assert.throws(() => taskProviderBudgetOptions(grant, { ...context, now: 20000 }));
});

const work = { id: "work-run", projectId: "project", kind: "conversation", steps: [{ currentAttemptId: "task-one" }] };
const root = "/api/projects/project";
const input = { projectId: "project", taskId: "task-one", now: 1000 };

test("ordinary and generation tasks consume domain grants and settle with their owner", async () => {
  const calls = [];
  const replies = [work, { authorization: { deadlineAt: 20 } }, grant];
  const runtimeRequest = async (path) => { calls.push(path); return replies.shift(); };
  assert.deepEqual(await resolveTaskBudgetAuthority({ ...input, runtimeRequest }), {
    deadlineAt: 20, usagePath: `${root}/work-tasks/work-run/provider-usage`,
    options: taskProviderBudgetOptions(grant, context),
  });
  assert.deepEqual(calls, [`${root}/tasks/task-one/work-task`, `${root}/work-tasks/work-run/budget-authorization`, `${root}/tasks/task-one/provider-budget`]);
  assert.deepEqual(await resolveTaskBudgetAuthority({ ...input,
    generationRun: { projectId: "project", activeTaskId: "task-one", id: "run", deadlineAt: 20 },
    runtimeRequest: async (path) => { assert.equal(path, `${root}/tasks/task-one/provider-budget`); return grant; },
  }), { deadlineAt: 20, usagePath: `${root}/generation-runs/run/provider-usage`, options: taskProviderBudgetOptions(grant, context) });
});

test("missing, malformed and failed authorization reads cannot grant an execution budget", async () => {
  const values = [work, { authorization: null }];
  await assert.rejects(resolveTaskBudgetAuthority({ ...input, runtimeRequest: async () => values.shift() }), /尚未获执行额度/);
  for (const replies of [
    [{ ...work, projectId: "other" }], [{ ...work, steps: [] }],
    [work, {}], [work, { authorization: {} }, grant],
    [work, { authorization: { deadlineAt: 20 } }, { ...grant, taskId: "other" }],
    [work, { authorization: { deadlineAt: 1 } }, { ...grant, deadlineAt: 1 }],
  ]) await assert.rejects(resolveTaskBudgetAuthority({ ...input, runtimeRequest: async () => replies.shift() }));
  for (let failure = 0; failure < 3; failure++) {
    let index = 0;
    const replies = [work, { authorization: { deadlineAt: 20 } }, grant];
    await assert.rejects(resolveTaskBudgetAuthority({ ...input, runtimeRequest: async () => {
      if (index === failure) throw new Error("unavailable");
      return replies[index++];
    } }), /unavailable/);
  }
});

for (const outcome of ["success", "failure", "preflightFailure"]) {
  test(`production ordinary workflow settles its persisted budget on ${outcome}`, async () => {
    const directory = await mkdtemp(join(tmpdir(), "ordinary-host-budget-"));
    const budgetDb = join(directory, "engine.sqlite3");
    const runtime = runtimeFixture();
    const work = { id: "work-source", projectId: "project", threadId: "thr-test", kind: "conversation", status: "running", version: 1,
      goal: "继续规划", steps: [{ id: "action", operation: "generateBrief", status: "queued",
        sourceTaskId: "source", inputRevision: 0, currentAttemptId: null, attempts: [], instruction: "生成简报", target: { type: "artifact" }, dependsOnStepId: null },
        { id: "source-step", status: "completed", attempts: [{ id: "source", threadId: "thr-test", status: "completed" }] }] };
    const deadlineAt = Math.floor(Date.now() / 1000) + 120;
    const receipts = [];
    const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));
    const { handlers, adapter } = await mainHandlers(async (path, body) => {
      if (path.endsWith("/tasks/source")) return { id: "source", projectId: "project", threadId: "thr-test", operation: "advisory", status: "completed", inputRevision: 0 };
      if (path.endsWith("/tasks/source/actions")) return { actions: [{ id: "action", sourceTaskId: "source", operation: "generateBrief", status: "queued", resultTaskId: null, inputRevision: 0, intentRevision: 0, instruction: "生成简报", target: { type: "artifact" } }] };
      if (path.endsWith("/tasks/source/actions/action/permission")) return { permission: { kind: "exactAction", actionId: "action", workTaskId: work.id, actionStatus: "queued", scopePermitId: null,
        evidence: { projectId: "project", sourceTaskId: "source", inputRevision: 0, intentRevision: 0, operation: "generateBrief", instruction: "生成简报", target: { type: "artifact" }, authorIntent: "继续规划" } } };
      if (path.endsWith("/budget-authorization")) return { authorization: { deadlineAt } };
      if (path.endsWith("/provider-budget")) return { taskId: "child", workTaskId: work.id,
        budgetId: "budget-child", maxRequests: 3, maxTokens: 700, deadlineAt };
      if (path.endsWith("/provider-usage")) {
        if (!body) return { pending: null };
        assert.equal(path, "/api/projects/project/work-tasks/work-source/provider-usage");
        assert.deepEqual(body, { taskId: "child" });
        receipts.push(PersistentTaskBudget.receipt(budgetDb, "child"));
        return {};
      }
      if (path.endsWith("/decision")) return { workTaskId:work.id, threadId:work.threadId,
        step:null, stopReason:work.status === "completed" ? "completed" : "workNotReady" };
      if (path.includes("/work-tasks/") || path.endsWith("/work-task")) return work;
      if (path.endsWith("/tasks")) {
        work.steps[0].currentAttemptId = "child";
        const child = { ...body, id: "child", projectId: "project", purpose: "planner",
          inputManifest: { authorIntent: "继续规划", candidateInstruction: "生成简报",
            authorIntentBinding: { sourceTaskId: "source", intentRevision: 0, actionId: "action" } } };
        if (outcome === "preflightFailure") delete child.inputManifest.authorIntentBinding;
        runtime.tasks.set(child.threadId, child);
        return child;
      }
      if (path.endsWith("/complete")) { work.status = "completed"; work.steps[0].status = "completed"; }
      return runtime.request(path, body);
    }, `${process.execPath} ${fixture}`, { agentStorage: { budgetDb }, onPrepareDomain(domain) {
      if (outcome === "failure") domain.codexAdapter.sendTurn = async () => { throw new Error("fixture model failed"); };
    } });
    try {
      const run = handlers.get("agent:resume-work-task")(rendererEvent(), "project", work.id);
      if (outcome === "failure") await assert.rejects(run, /fixture model failed/);
      else if (outcome === "preflightFailure") await assert.rejects(run, /来源绑定/);
      else assert.equal((await run).status, "completed");
      assert.equal(receipts.length, 1);
      assert.equal(receipts[0].budgetId, "budget-child");
      assert.equal(receipts[0].runId, "child");
      assert.equal(receipts[0].limits.maxRequests, 3);
      assert.equal(receipts[0].limits.maxTokens, 700);
      if (outcome === "preflightFailure") assert.deepEqual(receipts[0].usage,
        { requests:0, tokens:0, unknownTokens:0, overrun:false });
    } finally { adapter.close(); await rm(directory, { recursive: true, force: true }); }
  });
}

for (const outcome of ["success", "modelFailure", "settlementFailure"]) {
  test(`production advisory consumes its persisted allowance on ${outcome}`, async () => {
    const directory = await mkdtemp(join(tmpdir(), "advisory-host-budget-"));
    const budgetDb = join(directory, "engine.sqlite3");
    const runtime = runtimeFixture();
    const deadlineAt = Math.floor(Date.now() / 1000) + 120;
    const receipts = [];
    const patches = [];
    const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));
    const { handlers, adapter } = await mainHandlers(async (path, body, options) => {
      if (path.endsWith("/budget-authorization")) return { authorization: { deadlineAt } };
      if (path.endsWith("/provider-budget")) {
        const taskId = path.split("/").at(-2);
        const owner = await runtime.request(`${root}/tasks/${taskId}/work-task`);
        return { taskId, workTaskId: owner.id, budgetId: `budget-${taskId}`,
          maxRequests: 2, maxTokens: 500, deadlineAt };
      }
      if (path.endsWith("/provider-usage")) {
        const receipt = PersistentTaskBudget.receipt(budgetDb, body.taskId);
        receipts.push(receipt);
        if (outcome === "settlementFailure") throw new Error("fixture settlement unavailable");
        return {};
      }
      if (options?.method === "PATCH") patches.push(body);
      return runtime.request(path, body);
    }, `${process.execPath} ${fixture}`, { agentStorage: { budgetDb }, onPrepareDomain(domain) {
      if (outcome === "modelFailure") domain.codexAdapter.sendTurn = async () => { throw new Error("fixture model failed"); };
    } });
    try {
      const run = handlers.get("agent:send-prompt")(rendererEvent(), "project", "分析故事");
      if (outcome === "success") await run;
      else await assert.rejects(run, outcome === "modelFailure" ? /fixture model failed/ : /fixture settlement unavailable/);
      const taskId = [...runtime.tasks.values()][0].id;
      assert.equal(receipts.length, outcome === "settlementFailure" ? 2 : 1);
      for (const receipt of receipts) {
        assert.equal(receipt.budgetId, `budget-${taskId}`);
        assert.equal(receipt.runId, taskId);
        assert.equal(receipt.limits.maxRequests, 2);
        assert.equal(receipt.limits.maxTokens, 500);
      }
      assert.equal(patches.length, outcome === "modelFailure" ? 1 : 0);
    } finally { adapter.close(); await rm(directory, { recursive: true, force: true }); }
  });
}

test("ordinary recovery settles terminal usage then reloads the current control version", async () => {
  const original = { ...work, status: "paused", version: 3 };
  const refreshed = { ...original, version: 4 };
  const calls = [];
  const responses = [{ pending: { taskId: "previous", status: "failed" } }, {}, refreshed];
  const result = await reconcileWorkProviderUsage(async (path, options) => {
    calls.push([path, options]); return responses.shift();
  }, "project", original);
  assert.deepEqual(result, refreshed);
  assert.deepEqual(calls, [
    [`${root}/work-tasks/work-run/provider-usage`, undefined],
    [`${root}/work-tasks/work-run/provider-usage`, { method: "POST", body: JSON.stringify({ taskId: "previous" }) }],
    [`${root}/work-tasks/work-run`, undefined],
  ]);
});

test("ordinary recovery never resumes active, malformed or unaccounted execution", async () => {
  for (const response of [{}, { pending: { taskId: "previous", status: "running" } }]) {
    let calls = 0;
    await assert.rejects(reconcileWorkProviderUsage(async () => { calls++; return response; }, "project", work));
    assert.equal(calls, 1);
  }
  let calls = 0;
  await assert.rejects(reconcileWorkProviderUsage(async () => {
    if (++calls === 1) return { pending: { taskId: "previous", status: "cancelled" } };
    throw new Error("missing receipt");
  }, "project", work), /missing receipt/);
  assert.equal(calls, 2);
  assert.equal(await reconcileWorkProviderUsage(async () => ({ pending: null }), "project", work), work);
});

test('generation recovery rereads trusted receipts without submitting caller usage or starting a model', async () => {
  const { reconcileGenerationProviderUsage } = await import('../task-provider-budget.mjs');
  const run = { id: 'run', projectId: 'project', providerBudget: { charges: [{ taskId: 'task', unknownTokens: 7 }] } };
  const refreshed = { ...run, providerBudget: { charges: [{ taskId: 'task', unknownTokens: 0 }] } };
  const calls = [];
  assert.equal(await reconcileGenerationProviderUsage(async (path, options) => {
    calls.push({ path, options });
    return options ? {} : refreshed;
  }, 'project', run), refreshed);
  assert.deepEqual(JSON.parse(calls[0].options.body), { taskId: 'task' });
  assert.equal(calls[0].path, '/api/projects/project/generation-runs/run/provider-usage');
  assert.equal(calls.length, 2);
  await assert.rejects(reconcileGenerationProviderUsage(async () => { throw Error('must not call'); }, 'other', run), /归属/);
});
