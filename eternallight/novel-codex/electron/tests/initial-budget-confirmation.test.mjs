import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { confirmInitialTaskBudget, confirmNewRootBudget } from "../initial-budget-confirmation.mjs";
import { mainHandlers, rendererEvent, runtimeFixture } from "./main-handler-harness.mjs";

test("confirmation freezes reviewed sources and offers bounded execution, not content adoption", async () => {
  const workspace = { revision: 2, headSnapshotId: "snapshot", sourceDigest: "source" };
  const result = await confirmInitialTaskBudget({ workspace, projectId: "project", model: "model", prompt: "分析人物",
    now: () => 2000, showMessageBox: async (options) => {
      assert.equal(options.defaultId, options.cancelId);
      assert.equal(options.buttons[options.defaultId], "取消");
      assert.match(options.detail, /200,000 token/);
      assert.match(options.detail, /不代表正式采用/);
      workspace.headSnapshotId = "changed";
      workspace.sourceDigest = "changed";
      return { response: 1 };
    } });
  assert.match(result.authorizationId, /^[a-f0-9-]{36}$/);
  assert.match(result.authorMessageId, /^[a-f0-9-]{36}$/);
  assert.notEqual(result.authorMessageId, result.authorizationId);
  assert.deepEqual({ ...result, authorizationId: "id", authorMessageId: "message" }, { authorizationId: "id", authorMessageId: "message",
    expectedBaseSnapshotId: "snapshot", expectedSourceDigest: "source", maxRequests: 16,
    maxTokens: 200000, deadlineAt: 902 });
});

test("missing source and non-confirm responses cannot authorize", async () => {
  for (const response of [0, undefined, 2]) {
    await assert.rejects(confirmInitialTaskBudget({
      workspace: { revision: 0, headSnapshotId: "snapshot", sourceDigest: "source" },
      projectId: "project", prompt: "分析", showMessageBox: async () => ({ response }),
    }), { code: "TASK_CANCELLED" });
  }
  await assert.rejects(confirmInitialTaskBudget({ workspace: {}, showMessageBox: async () => {
    assert.fail("invalid sources must not reach confirmation");
  } }), /来源不完整/);
});

test("desktop cancel creates no task and sends no model turn", async () => {
  const runtime = runtimeFixture();
  const state = { confirmationResponse: 0, confirmations: [] };
  const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));
  const { handlers, adapter } = await mainHandlers(runtime.request, `${process.execPath} ${fixture}`, state);
  let turns = 0;
  adapter.sendTurn = async () => { turns++; throw new Error("must not send"); };
  try {
    await assert.rejects(handlers.get("agent:send-prompt")(rendererEvent(), "project", "分析故事"), { code: "TASK_CANCELLED" });
    assert.equal(turns, 0);
    assert.equal(runtime.tasks.size, 0);
    assert.equal(state.confirmations.length, 1);
  } finally { adapter.close(); }
});

test("desktop confirmation reaches task creation and the same persistent budget", async () => {
  const runtime = runtimeFixture();
  const state = { confirmations: [], budgets: [] };
  const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));
  const { handlers, adapter } = await mainHandlers(runtime.request, `${process.execPath} ${fixture}`, state);
  try {
    await handlers.get("agent:send-prompt")(rendererEvent(), "project", "分析故事");
    const task = [...runtime.tasks.values()][0];
    assert.equal(state.confirmations.length, 1);
    assert.equal(task.initialBudget.expectedBaseSnapshotId, "snapshot-fixture");
    assert.equal(task.initialBudget.expectedSourceDigest, "source-fixture");
    assert.equal(state.budgets.length, 1);
    assert.equal(state.budgets[0].id, `budget-${task.id}`);
    assert.equal(state.budgets[0].maxRequests, task.initialBudget.maxRequests);
    assert.equal(state.budgets[0].maxTokens, task.initialBudget.maxTokens);
  } finally { adapter.close(); }
});

test("a confirmed root cannot silently run without its domain allowance", async () => {
  const runtime = runtimeFixture();
  const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));
  const { handlers, adapter } = await mainHandlers(async (path, body) => {
    if (path.endsWith("/budget-authorization")) return { authorization: null };
    return runtime.request(path, body);
  }, `${process.execPath} ${fixture}`);
  let turns = 0;
  adapter.sendTurn = async () => { turns++; throw new Error("must not send"); };
  try {
    await assert.rejects(handlers.get("agent:send-prompt")(rendererEvent(), "project", "分析故事"), /尚未获执行额度/);
    assert.equal(turns, 0);
  } finally { adapter.close(); }
});

test("existing run and step identities never create a new author allowance", async () => {
  for (const field of ["runId", "resumeStepId", "requestedActionId", "requestedByTaskId", "sourceTaskId"]) {
    for (const value of ["bound", null]) {
      assert.equal(await confirmNewRootBudget({ target: { [field]: value },
        getWindow: () => assert.fail("derived attempts must not open a new confirmation") }), undefined);
    }
  }
});

for (const operation of ["generateBrief", "generateOutline", "generatePlan", "generateChapter"]) {
  for (const response of [0, 1]) {
    test(`${operation} ${response ? "uses the confirmed" : "cancels before creating"} root budget`, async () => {
      const runtime = runtimeFixture();
      const state = { confirmationResponse: response, confirmations: [], budgets: [] };
      const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));
      const { handlers, adapter } = await mainHandlers(runtime.request, `${process.execPath} ${fixture}`, state);
      try {
        const run = operation === "generateChapter"
          ? handlers.get("agent:generate-chapter")(rendererEvent(), "project", { chapterId: "chapter-one", chapterNumber: 1, intent: "继续创作" })
          : handlers.get("agent:generate-planning")(rendererEvent(), "project", operation, "继续创作");
        if (!response) {
          await assert.rejects(run, { code: "TASK_CANCELLED" });
          assert.equal(runtime.tasks.size, 0);
          assert.equal(state.budgets.length, 0);
        } else {
          await run;
          const task = [...runtime.tasks.values()][0];
          assert.equal(state.budgets.length, 1);
          assert.equal(state.budgets[0].id, `budget-${task.id}`);
          assert.equal(state.budgets[0].maxRequests, task.initialBudget.maxRequests);
          assert.equal(state.budgets[0].maxTokens, task.initialBudget.maxTokens);
        }
        assert.equal(state.confirmations.length, 1);
      } finally { adapter.close(); }
    });
  }
}
