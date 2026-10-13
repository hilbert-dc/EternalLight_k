import { randomUUID } from "node:crypto";
import { nativeBinding } from "../native.mjs";
const owners = new WeakMap();

export function bindActiveTask(task) {
  const active = { ...task };
  const owner = new (nativeBinding().NativeTaskCancellation)();
  owners.set(active, owner);
  Object.defineProperty(active, "cancellationRequested", { get: () => owners.get(active).cancelled() });
  return active;
}
export function createTaskExecution() { return bindActiveTask({ id: randomUUID(), task: null }); }
export function bindExecutionTask(execution, task) {
  const active = bindActiveTask(task);
  const owner = owners.get(execution);
  if (!owner) throw new Error("unknown task execution handle");
  owners.set(active, owner);
  execution.task = active;
  return active;
}
export function requestTaskCancellation(task) {
  if (task) {
    const owner = owners.get(task);
    if (!owner) throw new Error("unknown task execution handle");
    owner.cancel();
  }
  return task;
}
export function assertTaskCompletable(task) {
  if (!task) return;
  const owner = owners.get(task);
  if (!owner) throw new Error("unknown task execution handle");
  try { owner.assertCompletable(); }
  catch (error) { error.code = "TASK_CANCELLED"; error.message = error.message.replace(/^TASK_CANCELLED:/, ""); throw error; }
}
export function shouldMarkTaskFailed(task) { return Boolean(task && owners.get(task) && !owners.get(task).cancelled()); }
