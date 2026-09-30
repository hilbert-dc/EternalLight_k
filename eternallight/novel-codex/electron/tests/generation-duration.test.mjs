import assert from "node:assert/strict";
import { test } from "node:test";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { remainingGenerationDurationMs } from "../generation-duration.mjs";
import { generationPermitForChapters } from "../../shared/generation-permit.mjs";
import { mainHandlers, rendererEvent } from "./main-handler-harness.mjs";
import { TaskBudget } from "@eternallight/agent-engine";

test("generation start requires an explicit author-selected chapter scope", async () => {
  const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));
  const { handlers, adapter } = await mainHandlers(
    async () => { throw new Error("Invalid permits must not reach Story Runtime"); },
    `${process.execPath} ${fixture}`,
  );
  try {
    for (const invalid of [undefined, 0, 4, 1.5]) {
      await assert.rejects(
        handlers.get("agent:generate-work")(rendererEvent(), "project", "继续调查白塔", invalid, 4),
        /自动创作章节范围无效/,
      );
    }
  } finally { adapter.close(); }
});

test("generation start rejects stale or missing author-reviewed workspace revisions", async () => {
  const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));
  const requests = [];
  const { handlers, adapter } = await mainHandlers(async (path) => {
    requests.push(path);
    if (path.endsWith("/creative-workspace")) return { revision: 5, headSnapshotId: "snapshot-current", sourceDigest: "a".repeat(64) };
    throw new Error("Stale generation must not reach Story Runtime creation");
  }, `${process.execPath} ${fixture}`);
  try {
    for (const revision of [undefined, 4]) {
      await assert.rejects(
        handlers.get("agent:generate-work")(rendererEvent(), "project", "继续调查白塔", 1, revision,
          "snapshot-current", "a".repeat(64)),
        /创作来源版本无效|作品已在确认期间变化/,
      );
    }
    assert.deepEqual(requests, ["/api/projects/project/creative-workspace"]);
  } finally { adapter.close(); }
});

test("generation start rejects candidate or knowledge changes without a formal revision change", async () => {
  const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));
  const requests = [];
  const { handlers, adapter } = await mainHandlers(async (path) => {
    requests.push(path);
    if (path.endsWith("/creative-workspace")) return { revision: 5, headSnapshotId: "snapshot-current", sourceDigest: "b".repeat(64) };
    throw new Error("Changed sources must not reach GenerationRun creation");
  }, `${process.execPath} ${fixture}`);
  try {
    for (const [snapshot, digest] of [["snapshot-old", "b".repeat(64)], ["snapshot-current", "a".repeat(64)]]) {
      await assert.rejects(
        handlers.get("agent:generate-work")(rendererEvent(), "project", "继续调查白塔", 1, 5, snapshot, digest),
        /作品已在确认期间变化/,
      );
    }
    assert.deepEqual(requests, ["/api/projects/project/creative-workspace", "/api/projects/project/creative-workspace"]);
  } finally { adapter.close(); }
});

test("generation start uses the Rust-selected stage for its provider preflight", async () => {
  const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));
  const operations = {
    brief: "generateBrief",
    outline: "generateOutline",
    rollingPlan: "generatePlan",
  };
  const runId = `generation-${"a".repeat(32)}`;
  for (const [firstStage, operation] of Object.entries(operations)) {
    for (const revisionPredecessor of [null, { writerTaskId: `task-${"b".repeat(32)}`,
      assessmentRevisionId: "revision-outcome-one", decisionId: "revision-decision-one" }]) {
    const requests = [];
    const { handlers, adapter, providerConfig } = await mainHandlers(async (path, body) => {
      requests.push({ path, body });
      if (path.endsWith("/creative-workspace")) return { revision: 4, headSnapshotId: "snapshot-reviewed", sourceDigest: "c".repeat(64) };
      if (path.endsWith("/generation-start-position")) return { completedChapters: 0, firstStage };
      if (path.endsWith("/generation-runs")) return { id: runId, version: 1 };
      if (path.endsWith(`/work-tasks/work-${runId}`)) return { status: "running" };
      if (path.endsWith("/next")) return { run: { id: runId, status: "completed" }, action: null };
      throw new Error(`Unexpected Runtime request: ${path}`);
    }, `${process.execPath} ${fixture}`);
    try {
      providerConfig.operations.splice(0, providerConfig.operations.length, operation);
      const result = await handlers.get("agent:generate-work")(
        rendererEvent(), "project", "按照当前作品状态继续", 1, 4, "snapshot-reviewed", "c".repeat(64),
        revisionPredecessor,
      );
      assert.equal(result.status, "completed");
      assert.deepEqual(requests.slice(0, 3).map(({ path }) => path), [
        "/api/projects/project/creative-workspace",
        "/api/projects/project/generation-start-position",
        "/api/projects/project/generation-runs",
      ]);
      assert.equal(requests[2].body.targetChapters, 1);
      assert.equal(requests[2].body.expectedProjectRevision, 4);
      assert.equal(requests[2].body.expectedBaseSnapshotId, "snapshot-reviewed");
      assert.equal(requests[2].body.expectedSourceDigest, "c".repeat(64));
      assert.deepEqual(JSON.parse(JSON.stringify(requests[2].body.revisionPredecessor)), revisionPredecessor);
      assert.equal(requests[2].body.authorIntent, "按照当前作品状态继续");
      assert.equal(requests[2].body.maxTaskAttempts, generationPermitForChapters(1).maxTaskAttempts, "linking provenance cannot inherit the old task budget");
    } finally { adapter.close(); }
    }
  }
});

test("generation execution keeps the original permit deadline across task starts", () => {
  const now = 1_800_000_000_000;
  const deadlineAt = Math.floor(now / 1000) + 45;
  assert.equal(remainingGenerationDurationMs(deadlineAt, now), 45_000);
  assert.equal(remainingGenerationDurationMs(deadlineAt, now + 31_000), 14_000);
  assert.throws(() => remainingGenerationDurationMs(deadlineAt, now + 45_000), {
    code: "GENERATION_DEADLINE_EXPIRED",
  });
});

test("generation execution cannot exceed the per-task ceiling or accept damaged deadlines", () => {
  const now = 1_800_000_000_000;
  assert.equal(remainingGenerationDurationMs(now / 1000 + 3_600, now), 1_800_000);
  for (const invalid of [null, NaN, Infinity, 1.5, Number.MAX_SAFE_INTEGER]) {
    assert.throws(() => remainingGenerationDurationMs(invalid, now));
  }
});

test("production task runner binds both the model budget and engine lease to the generation deadline", async () => {
  const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));
  const state = { budgets: [], leases: [] };
  const { adapter, sendAgentTurn, providerConfig, modelBridge } = await mainHandlers(
    async () => { throw new Error("This turn must not call Story Runtime"); },
    `${process.execPath} ${join("", fixture)}`,
    state,
  );
  const deadlineAt = Math.floor(Date.now() / 1000) + 20;
  try {
    await sendAgentTurn({
      adapter, modelBridge, providerConfig, sessionPlan: providerConfig.sessionPlan,
      operation: "generateBrief", taskDeadlineAt: deadlineAt,
    }, [{ type: "text", text: "测试截止时间" }], { disableEnvironment: true });
    assert.equal(state.budgets.length, 1);
    assert.equal(state.leases.length, 1);
    assert.ok(state.budgets[0].maxDurationMs > 0);
    assert.ok(state.budgets[0].startedAt + state.budgets[0].maxDurationMs <= deadlineAt * 1000);
    assert.ok(new Date(state.leases[0].deadline).getTime() <= deadlineAt * 1000);
  } finally {
    adapter.close();
  }
});

test("domain tasks cannot enter the model engine with a default unbound budget", async () => {
  const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));
  const state = { budgets: [], leases: [] };
  const { adapter, sendAgentTurn, providerConfig, modelBridge } = await mainHandlers(
    async () => { throw new Error("This turn must not call Story Runtime"); },
    `${process.execPath} ${fixture}`, state,
  );
  try {
    await assert.rejects(sendAgentTurn({ adapter, modelBridge, providerConfig,
      sessionPlan: providerConfig.sessionPlan, operation: "advisory", task: { id: "task-unfunded" },
    }, [{ type: "text", text: "不能执行" }], {}), /领域任务缺少持久执行额度/);
    assert.deepEqual(state.budgets, []);
    assert.deepEqual(state.leases, []);
  } finally { adapter.close(); }
});

test("production task runner rejects an expired generation permit before issuing a model lease", async () => {
  const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));
  const state = { budgets: [], leases: [] };
  const { adapter, sendAgentTurn, providerConfig, modelBridge } = await mainHandlers(
    async () => { throw new Error("This turn must not call Story Runtime"); },
    `${process.execPath} ${join("", fixture)}`,
    state,
  );
  try {
    await assert.rejects(sendAgentTurn({
      adapter, modelBridge, providerConfig, sessionPlan: providerConfig.sessionPlan,
      operation: "generateBrief", taskDeadlineAt: Math.floor(Date.now() / 1000) - 1,
    }, [{ type: "text", text: "过期任务" }], { disableEnvironment: true }), {
      code: "GENERATION_DEADLINE_EXPIRED",
    });
    assert.deepEqual(state.budgets, []);
    assert.deepEqual(state.leases, []);
  } finally {
    adapter.close();
  }
});

test("production ordinary turn binds the supplied budget without creating a new allowance", async () => {
  const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));
  const state = { budgets: [], leases: [] };
  const { adapter, sendAgentTurn, providerConfig, modelBridge } = await mainHandlers(
    async () => { throw new Error("Unexpected Runtime request"); }, `${process.execPath} ${fixture}`, state,
  );
  const taskBudget = new TaskBudget({ id: "budget-ordinary-task", maxRequests: 3, maxTokens: 700 });
  const execution = {
    adapter, modelBridge, providerConfig, sessionPlan: providerConfig.sessionPlan,
    task: { id: "ordinary-task" }, operation: "advisory", taskBudget,
    taskDeadlineAt: Math.floor(Date.now() / 1000) + 20,
  };
  try {
    await sendAgentTurn(execution, [{ type: "text", text: "继续分析" }], { disableEnvironment: true });
    assert.equal(execution.taskBudgetBound, true);
    assert.equal(state.budgets.length, 1);
    assert.equal(state.budgets[0].id, taskBudget.id);
    assert.equal(state.budgets[0].maxRequests, 3);
    assert.equal(state.budgets[0].maxTokens, 700);
    assert.equal(state.leases[0].runId, "ordinary-task");
    assert.equal(state.leases[0].budgetId, taskBudget.id);
  } finally { adapter.close(); }
});
