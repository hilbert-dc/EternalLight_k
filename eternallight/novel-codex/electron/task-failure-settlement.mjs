// @ts-check
import { taskFailureDiagnostic } from "./tool-validation-recovery.mjs";

// A local cancellation stops sending; it does not prove Native has settled the
// Attempt. Always reconcile this exact identity, including owner-loss paths.
export async function settleFailedTask(request, projectId, task, execution, error) {
  if (!task) return;
  const path = `/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}`;
  const read = async () => {
    const current = await request(path);
    if (current?.id !== task.id || current.projectId !== projectId || current.threadId !== task.threadId) {
      throw new Error("中断任务身份无法核对");
    }
    return current;
  };
  const terminal = current => ["completed", "failed", "cancelled"].includes(current.status);
  try {
    const current = await read();
    if (terminal(current)) return;
    if (!["queued", "running", "waiting"].includes(current.status)) throw new Error("中断任务状态无法核对");
    const original = taskFailureDiagnostic(error);
    const diagnostic = execution?.cancellationRequested
      ? "execution_interrupted; 本机执行已中断；原保存结果与用量保留"
      : typeof original === "string" && original.startsWith("execution_interrupted") ? `execution_failed; ${original}` : original;
    try {
      await request(path, { method: "PATCH", body: JSON.stringify({ threadId: task.threadId, status: "failed", error: diagnostic }) });
    } catch (failure) {
      // A concurrent author control or a lost response may already have settled
      // the Attempt. Never overwrite that receipt or touch a newer Attempt.
      if (!terminal(await read())) throw failure;
    }
    if (!terminal(await read())) throw new Error("中断任务终态尚未确认");
  } catch (failure) {
    throw new Error(`${error?.message ?? "执行中断"}；原任务终止状态尚待核对：${failure.message}`, { cause: error });
  }
}
