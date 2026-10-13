// @ts-check
import { createInitialTaskBudget } from "./task-budget-grant.mjs";

export async function ensureWorkBudgetAuthorization({ runtimeRequest, projectId, work, origin, databasePath, getWindow }) {
  if (work?.projectId !== projectId || !["conversation", "direct"].includes(work.kind)
    || typeof work.id !== "string" || !work.id || !Number.isSafeInteger(work.version)) {
    throw new Error("任务额度授权归属无效");
  }
  const root = `/api/projects/${encodeURIComponent(projectId)}`;
  const path = `${root}/work-tasks/${encodeURIComponent(work.id)}`;
  const expectedVersion = work.version;
  const result = await runtimeRequest(`${path}/budget-authorization`);
  if (result?.authorization && typeof result.authorization === "object") {
    const deadlineAt = result.authorization.deadlineAt;
    if (!(deadlineAt === null || (Number.isSafeInteger(deadlineAt) && Number.isSafeInteger(deadlineAt * 1000)
      && deadlineAt * 1000 > Date.now()))) throw new Error("任务执行额度无效或已过期");
    return work;
  }
  if (result?.authorization !== null) throw new Error("任务额度授权响应无效");
  if (origin !== "author") throw new Error("此建模任务尚未获执行额度，请由作者确认后继续");
  if (!databasePath) throw new Error("任务缺少独立引擎预算库");
  const workspace = await runtimeRequest(`${root}/creative-workspace`);
  const owner = getWindow();
  if (!owner || owner.isDestroyed()) throw new Error("创作窗口已关闭");
  const budget = createInitialTaskBudget({ workspace });
  await runtimeRequest(`${path}/budget-authorization`, { method: "POST", body: JSON.stringify({
    expectedVersion, expectedProjectRevision: workspace.revision,
    expectedSourceDigest: budget.expectedSourceDigest,
    maxRequests: budget.maxRequests, maxTokens: budget.maxTokens, deadlineAt: budget.deadlineAt,
  }) });
  const refreshed = await runtimeRequest(path);
  if (refreshed?.id !== work.id || refreshed.projectId !== projectId || refreshed.kind !== work.kind
    || !Number.isSafeInteger(refreshed.version) || refreshed.version <= expectedVersion) {
    throw new Error("额度授权后任务状态无效，请刷新任务");
  }
  return refreshed;
}
