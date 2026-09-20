import assert from "node:assert/strict";
import test from "node:test";

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
