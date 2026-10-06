export function taskProviderBudgetOptions(grant, { taskId, workTaskId, deadlineAt, now = Date.now() }) {
  if (!grant || grant.taskId !== taskId || grant.workTaskId !== workTaskId
    || grant.budgetId !== `budget-${taskId}` || grant.deadlineAt !== deadlineAt
    || !(grant.maxRequests === null || (Number.isSafeInteger(grant.maxRequests) && grant.maxRequests >= 1))
    || !(grant.maxTokens === null || (Number.isSafeInteger(grant.maxTokens) && grant.maxTokens >= 1))
    || !Number.isSafeInteger(now)
    || !(grant.deadlineAt === null || (Number.isSafeInteger(grant.deadlineAt) && Number.isSafeInteger(grant.deadlineAt * 1000) && grant.deadlineAt * 1000 > now))) {
    throw new Error("任务执行预算无效、已过期或与当前任务不匹配");
  }
  return { id: grant.budgetId, maxRequests: grant.maxRequests, maxTokens: grant.maxTokens,
    maxDurationMs: grant.deadlineAt === null ? null : grant.deadlineAt * 1000 - now };
}

export async function resolveTaskBudgetAuthority({ runtimeRequest, projectId, taskId, generationRun = null, now }) {
  const root = `/api/projects/${encodeURIComponent(projectId)}`;
  let workTaskId;
  let deadlineAt;
  let usagePath;
  if (generationRun) {
    if (generationRun.projectId !== projectId || generationRun.activeTaskId !== taskId
      || typeof generationRun.id !== "string" || !generationRun.id) throw new Error("生成预算归属不匹配");
    workTaskId = `work-${generationRun.id}`;
    deadlineAt = generationRun.deadlineAt;
    usagePath = `${root}/generation-runs/${encodeURIComponent(generationRun.id)}/provider-usage`;
  } else {
    const work = await runtimeRequest(`${root}/tasks/${encodeURIComponent(taskId)}/work-task`);
    if (work?.projectId !== projectId || !["conversation", "direct"].includes(work.kind)
      || typeof work.id !== "string" || !work.id
      || !work.steps?.some((step) => step.currentAttemptId === taskId)) throw new Error("普通任务预算归属不匹配");
    workTaskId = work.id;
    const result = await runtimeRequest(`${root}/work-tasks/${encodeURIComponent(workTaskId)}/budget-authorization`);
    if (result?.authorization === null) throw new Error("任务尚未获执行额度，请先由作者确认");
    if (!result?.authorization) throw new Error("任务额度确认响应无效");
    deadlineAt = result.authorization.deadlineAt;
    usagePath = `${root}/work-tasks/${encodeURIComponent(workTaskId)}/provider-usage`;
  }
  const grant = await runtimeRequest(`${root}/tasks/${encodeURIComponent(taskId)}/provider-budget`);
  return { deadlineAt, usagePath, options: taskProviderBudgetOptions(grant, { taskId, workTaskId, deadlineAt, now }) };
}

export async function reconcileWorkProviderUsage(runtimeRequest, projectId, work) {
  if (work?.projectId !== projectId || !["conversation", "direct"].includes(work.kind)
    || typeof work.id !== "string" || !work.id) throw new Error("核账任务归属不匹配");
  const path = `/api/projects/${encodeURIComponent(projectId)}/work-tasks/${encodeURIComponent(work.id)}`;
  const result = await runtimeRequest(`${path}/provider-usage`);
  if (result?.pending === null) return work;
  const pending = result?.pending;
  if (typeof pending?.taskId !== "string" || !pending.taskId || pending.taskId.length > 160
    || !["completed", "failed", "cancelled"].includes(pending.status)) throw new Error("任务费用尚不能结算，不能继续执行");
  await runtimeRequest(`${path}/provider-usage`, { method: "POST", body: JSON.stringify({ taskId: pending.taskId }) });
  const refreshed = await runtimeRequest(path);
  if (refreshed?.id !== work.id || refreshed.projectId !== projectId || refreshed.kind !== work.kind) throw new Error("核账后任务归属不匹配");
  return refreshed;
}
import { PersistentTaskBudget } from "@eternallight/agent-engine";

export async function settleTaskProviderBudget(execution, runtimeRequest, taskId) {
  if (!execution.budgetUsagePath) return;
  if (execution.taskBudget && !execution.taskBudgetBound) execution.taskBudget.complete("failed");
  await runtimeRequest(execution.budgetUsagePath, { method: "POST", body: JSON.stringify({ taskId }) });
}

export async function bindTaskProviderBudget(execution, { runtimeRequest, projectId, taskId, generationRun = null, databasePath }) {
  const authority = await resolveTaskBudgetAuthority({ runtimeRequest, projectId, taskId, generationRun });
  execution.taskDeadlineAt = authority.deadlineAt;
  execution.budgetUsagePath = authority.usagePath;
  if (!databasePath) throw new Error("任务缺少独立引擎预算库");
  execution.taskBudget = new PersistentTaskBudget({ databasePath, runId: taskId, ...authority.options });
}
