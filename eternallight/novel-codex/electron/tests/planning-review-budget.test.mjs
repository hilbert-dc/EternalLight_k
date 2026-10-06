import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { PersistentTaskBudget } from "@eternallight/agent-engine";
import { mainHandlers, rendererEvent } from "./main-handler-harness.mjs";
import { reviewTask } from "./fixtures/planning-review-task.mjs";

for (const outcome of ["success", "successSingle", "modelFailure", "settlementFailure", "missingGrant"]) {
  test(`production planning review shares author allowance: ${outcome}`, async () => {
    const run = { sourceTaskId: null, sourceIntentRevision: null, focus: "", id: "planning-review-fixture", workTaskId: "work-persisted-review-owner", projectId: "project", artifactKind: "outline", revisionId: "outline-one",
      inputRevision: 0, authorIntent: "检查规划", authorMessageId: "author-review", sourcePacketHash: "a".repeat(64),
      roles: outcome === "successSingle" ? ["character"] : ["structure", "character"], status: "running" };
    const work = { id: run.workTaskId, projectId: "project", kind: "direct", goal: "检查规划", status: "running", version: 0, steps: [] };
    const state = { confirmations: [], budgets: [] };
    const tasks = [];
    const charges = [];
    const failed = [];
    let authorization = null;
    let finalized = 0;
    let modelCalls = 0;
    const replies = new Map();
    const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));
    const { handlers, adapter } = await mainHandlers(async (path, body, options) => {
      if (path.endsWith("/final-reply")) {
        const taskId = path.split("/").at(-2);
        if (body) replies.set(taskId, { taskId, projectId: run.projectId, binding: body });
        return replies.get(taskId) ?? null;
      }
      if (path.endsWith("/planning-reviews")) return run;
      if (path.endsWith("/budget-authorization")) {
        if (body) { authorization = body; work.version++; return {}; }
        return { authorization };
      }
      if (path.endsWith(`/work-tasks/${work.id}`) || path.endsWith("/work-task")) return work;
      if (path.endsWith("/creative-workspace")) return { revision: 0, headSnapshotId: "snapshot", sourceDigest: "source" };
      if (path.endsWith("/provider-budget")) {
        if (outcome === "missingGrant") throw new Error("fixture grant missing");
        const id = path.split("/").at(-2);
        // The domain fixture supplies the next reservation; real accounting is covered by Rust HTTP tests.
        return { taskId: id, workTaskId: work.id, budgetId: `budget-${id}`,
          maxRequests: authorization.maxRequests === null ? null : authorization.maxRequests - charges.length,
          maxTokens: authorization.maxTokens === null ? null : authorization.maxTokens - charges.length * 100, deadlineAt: authorization.deadlineAt };
      }
      if (path.endsWith("/provider-usage")) {
        assert.equal(path, `/api/projects/project/work-tasks/${work.id}/provider-usage`);
        charges.push(PersistentTaskBudget.receipt(state.agentStorage.budgetDb, body.taskId));
        if (outcome === "settlementFailure") throw new Error("fixture charge failed");
        return {};
      }
      if (path.endsWith("/tasks")) {
        const role = path.split("/").at(-2);
        const task = reviewTask(run, role, body, `task-${role}`);
        work.steps.push({ id: `step-${role}`, currentAttemptId: task.id });
        tasks.push(task); return task;
      }
      if (options?.method === "PATCH") { failed.push(body); return {}; }
      if (path.endsWith("/finalize")) { finalized++; return { ...run, status: "completed" }; }
      if (path.endsWith("/cancel")) { run.status = "cancelled"; return run; }
      return {};
    }, `${process.execPath} ${fixture}`, state);
    const send = adapter.sendTurn.bind(adapter);
    adapter.sendTurn = async (...args) => {
      modelCalls++;
      assert.ok(JSON.stringify(args).includes(run.authorIntent));
      assert.ok(!JSON.stringify(args).includes("调用方包装不得成为作者原话"));
      if (outcome === "modelFailure") throw new Error("fixture model failed");
      return send(...args);
    };
    try {
      const result = handlers.get("agent:review-planning")(rendererEvent(), "project", {
        artifactKind: "outline", revisionId: "outline-one", authorIntent: "调用方包装不得成为作者原话", roles: run.roles,
      });
      if (outcome.startsWith("success")) {
        assert.equal((await result).run.status, "completed");
        assert.equal(finalized, 1);
        assert.equal(tasks.length, run.roles.length);
        assert.equal(charges.length, run.roles.length);
        assert.deepEqual(state.budgets.map((budget) => [budget.id, budget.maxRequests, budget.maxTokens]), run.roles.map(role => [`budget-task-${role}`, null, null]));
      } else {
        await assert.rejects(result, /fixture/);
        assert.equal(finalized, 0);
        assert.equal(run.status, "running");
        assert.equal(tasks.length, 1);
        assert.equal(modelCalls, outcome === "missingGrant" ? 0 : 1);
      }
      assert.deepEqual(state.confirmations, []);
      assert.equal(failed.length, ["modelFailure", "missingGrant"].includes(outcome) ? 1 : 0);
      for (const receipt of charges) assert.ok(receipt.budgetId.startsWith("budget-task-"));
    } finally { adapter.close(); }
  });
}
