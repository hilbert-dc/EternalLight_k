import { createHash } from "node:crypto";
import { isDeepStrictEqual } from "node:util";
import { validateChapterStructureChange } from "./chapter-structure-analysis.mjs";
import { bindTaskProviderBudget } from "./task-provider-budget.mjs";

// Called only when this local task POST threw before returning and before the
// caller could reach sendAgentTurn. Queued status alone never proves zero usage.
export async function bindUnsentStructureTask(execution, { runtimeRequest, projectId, request, databasePath }) {
  if (execution.task || execution.taskBudget || execution.taskBudgetBound || execution.engineHost
    || request?.operation !== "assessChapterStructure" || !request.initialBudget
    || request.target?.type !== "project" || typeof request.target.structureChangeId !== "string") {
    throw new Error("尚未发送任务的本地归属无法核实");
  }
  const taskId = `task-${createHash("sha256").update(JSON.stringify([projectId, request.initialBudget.authorizationId])).digest("hex").slice(0, 32)}`;
  const root = `/api/projects/${encodeURIComponent(projectId)}`;
  const change = validateChapterStructureChange(await runtimeRequest(`${root}/chapter-structure-changes/${encodeURIComponent(request.target.structureChangeId)}`), projectId, request.target.structureChangeId);
  if (change.assessment.state !== "bound" || change.assessment.taskId !== taskId
    || change.assessment.workTaskId !== `work-${taskId}`) throw new Error("章节检查未绑定本次本地授权");
  const task = await runtimeRequest(`${root}/tasks/${encodeURIComponent(taskId)}`);
  if (task?.id !== taskId || task.projectId !== projectId || task.threadId !== request.threadId
    || task.operation !== request.operation || task.inputRevision !== request.inputRevision || task.model !== request.model
    || task.purpose !== "critic" || task.requiredOutputKind !== null || task.status !== "queued" || task.turnId !== null
    || !Array.isArray(task.outputRefs) || task.outputRefs.length !== 0
    || !isDeepStrictEqual(task.target, JSON.parse(JSON.stringify(request.target)))
    || task.inputManifest?.authorMessageId !== request.initialBudget.authorMessageId
    || task.inputManifest?.authorIntent !== request.authorIntent) throw new Error("章节检查任务与未发送请求不一致");
  // PersistentTaskBudget creates a unique run row: an existing receipt/active
  // ledger is rejected rather than overwritten or inferred to have zero usage.
  await bindTaskProviderBudget(execution, { runtimeRequest, projectId, taskId, databasePath });
  return task;
}
