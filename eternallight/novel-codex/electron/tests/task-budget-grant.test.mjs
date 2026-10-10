import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { createInitialTaskBudget, grantNewRootBudget } from "../task-budget-grant.mjs";
import { mainHandlers, rendererEvent, runtimeFixture } from "./main-handler-harness.mjs";

test("each author submission freezes its own source and metered authority", () => {
  const workspace = { revision: 2, headSnapshotId: "snapshot", sourceDigest: "source" };
  const result = createInitialTaskBudget({ workspace, now: () => 2000 });
  workspace.headSnapshotId = "changed";
  workspace.sourceDigest = "changed";
  const next = createInitialTaskBudget({ workspace, now: () => 3000 });
  assert.notEqual(result.authorizationId, next.authorizationId);
  assert.notEqual(result.authorMessageId, next.authorMessageId);
  assert.notEqual(result.authorMessageId, result.authorizationId);
  assert.deepEqual([result, next].map(budget => ({ ...budget, authorizationId: "id", authorMessageId: "message" })), [
    { authorizationId: "id", authorMessageId: "message", expectedBaseSnapshotId: "snapshot", expectedSourceDigest: "source", maxRequests: null, maxTokens: null, deadlineAt: null },
    { authorizationId: "id", authorMessageId: "message", expectedBaseSnapshotId: "changed", expectedSourceDigest: "changed", maxRequests: null, maxTokens: null, deadlineAt: null },
  ]);
});

test("malformed source or time cannot grant execution", () => {
  assert.throws(() => createInitialTaskBudget({ workspace: {} }), /来源不完整/);
  for (const timestamp of [-1, NaN, 1.5]) assert.throws(() => createInitialTaskBudget({
    workspace: { revision: 0, headSnapshotId: "snapshot", sourceDigest: "source" }, now: () => timestamp,
  }), /当前时间无效/);
});

const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;
for (const operation of ["advisory", "generateBrief", "generateOutline", "generatePlan", "generateChapter"]) {
  test(`author ${operation} starts directly with a persisted metered budget`, async () => {
    const runtime = runtimeFixture();
    const state = { confirmations: [], budgets: [] };
    const { handlers, adapter } = await mainHandlers(async (path, options) => {
      const result = await runtime.request(path, options);
      // The chapter navigation entry validates its real workspace target.
      if (path.endsWith("/creative-workspace") && operation === "generateChapter") {
        return { ...result, chapters: [{ id: "chapter-one", number: 1 }] };
      }
      return result;
    }, command, state);
    try {
      const event = rendererEvent();
      if (operation === "advisory") await handlers.get("agent:send-prompt")(event, "project", "分析故事");
      else if (operation === "generateChapter") await handlers.get("agent:generate-chapter")(event, "project", { chapterId: "chapter-one", chapterNumber: 1, intent: "继续创作" });
      else await handlers.get("agent:generate-planning")(event, "project", operation, "继续创作");
      const task = [...runtime.tasks.values()][0];
      assert.deepEqual(state.confirmations, []);
      assert.equal(task.initialBudget.expectedBaseSnapshotId, "snapshot-fixture");
      assert.equal(task.initialBudget.expectedSourceDigest, "source-fixture");
      assert.deepEqual(state.budgets.map(budget => ({ id: budget.id, maxRequests: budget.maxRequests, maxTokens: budget.maxTokens })), [
        { id: `budget-${task.id}`, maxRequests: task.initialBudget.maxRequests, maxTokens: task.initialBudget.maxTokens },
      ]);
    } finally { adapter.close(); }
  });
}

test("author submission cannot run without its domain allowance", async () => {
  const runtime = runtimeFixture();
  const { handlers, adapter } = await mainHandlers(async (path, body) => {
    if (path.endsWith("/budget-authorization")) return { authorization: null };
    return runtime.request(path, body);
  }, command);
  let turns = 0;
  adapter.sendTurn = async () => { turns++; throw new Error("must not send"); };
  try {
    await assert.rejects(handlers.get("agent:send-prompt")(rendererEvent(), "project", "分析故事"), /尚未获执行额度/);
    assert.equal(turns, 0);
  } finally { adapter.close(); }
});

test("existing run and step identities keep their original allowance", () => {
  for (const field of ["runId", "resumeStepId", "requestedActionId", "requestedByTaskId", "sourceTaskId"]) {
    for (const value of ["bound", null]) assert.equal(grantNewRootBudget({ target: { [field]: value },
      getWindow: () => assert.fail("derived attempts must not claim new author allowance") }), undefined);
  }
});

test("a closed author window or missing budget store cannot create a root", () => {
  const workspace = { revision: 0, headSnapshotId: "snapshot", sourceDigest: "source" };
  assert.throws(() => grantNewRootBudget({ workspace }), /预算库/);
  assert.throws(() => grantNewRootBudget({ workspace, databasePath: "fixture", getWindow: () => null }), /窗口已关闭/);
});
