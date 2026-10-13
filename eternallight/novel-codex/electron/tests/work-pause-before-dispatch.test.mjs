import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { mainHandlers, rendererEvent, runtimeFixture } from "./main-handler-harness.mjs";

const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;

for (const status of ["paused", "cancelled"]) for (const entry of ["advisory", "candidate"]) {
  test(`${status} committed before Host binding prevents ${entry} inference and preserves Native control`, async () => {
    const runtime = runtimeFixture();
    const calls = [];
    const state = { budgets: [], leases: [] };
    let issuedTask;
    const { handlers, adapter } = await mainHandlers(async (path, body, options) => {
      calls.push({ path, body, method: options?.method ?? "GET" });
      const value = await runtime.request(path, body, options);
      if (path.endsWith("/tasks") && body) issuedTask = value;
      if (path.endsWith("/work-task")) {
        // Native control commits Work and Attempt fences in one transaction.
        Object.assign(value, { status, version: value.version + 1 });
        Object.assign(issuedTask, { status: "cancelled", stage: "cancelled",
          capabilityRevokedAt: Math.floor(Date.now() / 1000),
          ...(status === "paused" ? { error: "author_paused" } : {}) });
        for (const step of value.steps) step.status = status === "paused" ? "blocked" : "cancelled";
      }
      return value;
    }, command, state);
    let turns = 0;
    const send = adapter.sendTurn.bind(adapter);
    adapter.sendTurn = async (...args) => { turns++; return send(...args); };
    try {
      const result = entry === "advisory"
        ? handlers.get("agent:send-prompt")(rendererEvent(), "project", "请先整理创作方向")
        : handlers.get("agent:generate-planning")(rendererEvent(), "project", "generateBrief", "请整理创作方向候选");
      await assert.rejects(result, error => error.code === "TASK_CANCELLED");
      assert.equal(turns, 0, "no model input is dispatched after committed control");
      assert.equal(runtime.workTasks.size, 1, "no replacement root or budget is created");
      assert.equal([...runtime.workTasks.values()][0].status, status);
      assert.equal(calls.filter(call => call.path.endsWith("/tasks") && call.method === "POST").length, 1);
      assert.ok(!calls.some(call => call.path.endsWith("/cancel") || call.path.endsWith("/control") || call.method === "PATCH" && call.path.includes("/tasks/")),
        "Host cannot overwrite the committed paused/cancelled state");
      assert.equal(issuedTask.status, "cancelled", "the original Attempt remains terminal");
      assert.deepEqual(state.budgets, []);
      assert.deepEqual(state.leases, []);
    } finally { adapter.close(); }
  });
}
