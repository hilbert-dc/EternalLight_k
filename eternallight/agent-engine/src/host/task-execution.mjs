import { randomUUID } from "node:crypto";

export function bindActiveTask(task) {
  const active = { ...task };
  Object.defineProperty(active, "cancellationRequested", { value: false, writable: true });
  return active;
}

export function createTaskExecution() {
  return { id: randomUUID(), cancellationRequested: false, task: null };
}

export function bindExecutionTask(execution, task) {
  const active = bindActiveTask(task);
  execution.task = active;
  if (execution.cancellationRequested) requestTaskCancellation(active);
  return active;
}

export function requestTaskCancellation(task) {
  if (task) task.cancellationRequested = true;
  return task;
}

export function assertTaskCompletable(task) {
  if (!task?.cancellationRequested) return;
  const error = new Error("生成已停止");
  error.code = "TASK_CANCELLED";
  throw error;
}

export function shouldMarkTaskFailed(task) {
  return Boolean(task && !task.cancellationRequested);
}
