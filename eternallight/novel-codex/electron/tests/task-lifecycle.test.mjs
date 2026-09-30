import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { mainHandlers, rendererEvent } from "./main-handler-harness.mjs";

import { assertTaskCompletable, bindActiveTask, bindExecutionTask, createTaskExecution, requestTaskCancellation, shouldMarkTaskFailed } from "@eternallight/agent-engine";

test("a cancellation request prevents completion and failure transitions", () => {
  const task = bindActiveTask({ id: "task-1", status: "running" });
  assert.doesNotThrow(() => assertTaskCompletable(task));
  assert.equal(shouldMarkTaskFailed(task), true);
  requestTaskCancellation(task);
  assert.throws(() => assertTaskCompletable(task), { message: "生成已停止", code: "TASK_CANCELLED" });
  assert.equal(shouldMarkTaskFailed(task), false);
});

test("cancellation before task creation transfers to the created task", () => {
  const execution = createTaskExecution();
  requestTaskCancellation(execution);
  const task = bindExecutionTask(execution, { id: "task-late" });
  assert.throws(() => assertTaskCompletable(execution), /生成已停止/);
  assert.equal(task.cancellationRequested, true);
  assert.equal(shouldMarkTaskFailed(task), false);
});

test("desktop preserves the domain task cursor and author property rule request", async () => {
  const requests = [];
  const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;
  const { handlers, adapter } = await mainHandlers(async (path, body, options, url) => {
    requests.push({ path, body, query: Object.fromEntries(url.searchParams) });
    return { ok: true };
  }, command);
  try {
    const event = rendererEvent();
    await handlers.get("runtime:list-work-tasks")(event, "project-a", "0:31:65:132");
    assert.deepEqual(requests[0], { path: "/api/projects/project-a/work-tasks", body: null, query: { cursor: "0:31:65:132" } });
    assert.throws(() => handlers.get("runtime:list-work-tasks")(event, "project-a", "0:31:-1:132"), /分页/);
    const request = { payload: { eventRules: [], definitions: [] }, expectedRevision: 2, expectedCandidateRevisionId: null, idempotencyKey: "stable-rule-request" };
    await handlers.get("runtime:save-property-rules")(event, "project-a", request);
    assert.deepEqual(requests[1], { path: "/api/projects/project-a/artifacts/propertyRules", body: request, query: {} });
    await handlers.get("runtime:create-review")(event, "project-a", "propertyRules", "rule-1");
    assert.equal(requests[2].body.artifactKind, "propertyRules");
    assert.throws(() => handlers.get("runtime:create-review")(event, "project-a", "propertyRules", "rule-1", [{}]), /规则/);
    assert.equal(requests.length, 3);
  } finally { adapter.close(); }
});
