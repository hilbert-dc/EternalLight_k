// @ts-check
function validId(value) {
  return typeof value === "string" && value.trim().length > 0 && value.length <= 160 && !value.includes("\0");
}

function base(projectId) {
  if (!validId(projectId)) throw new Error("作品标识无效");
  return `/api/projects/${encodeURIComponent(projectId)}/condition-evaluations`;
}

function jobPath(projectId, jobId) {
  if (typeof jobId !== "string" || !/^condition-[a-f0-9]{32}$/.test(jobId)) throw new Error("条件检查编号无效");
  return `${base(projectId)}/${encodeURIComponent(jobId)}`;
}

// Supplied by main: trusted-sender IPC wrapper and author-only transport.
// Renderer data cannot select credentials or arbitrary URLs.
export function registerConditionEvaluationHandlers(ipcMain, runtimeRequest) {
  ipcMain.handle("runtime:start-condition-evaluation", (_event, projectId, target) => {
    const path = base(projectId);
    if (!target || typeof target !== "object" || Array.isArray(target)
      || Object.keys(target).length !== 3
      || !Object.keys(target).every((key) => ["snapshotId", "sourceBatchId", "assertionIndex"].includes(key))
      || !(target.snapshotId === null || validId(target.snapshotId))
      || !validId(target.sourceBatchId) || !Number.isSafeInteger(target.assertionIndex)
      || target.assertionIndex < 0 || target.assertionIndex >= 100) throw new Error("条件检查目标无效");
    return runtimeRequest(path, { method: "POST", body: JSON.stringify(target) });
  });
  ipcMain.handle("runtime:get-condition-evaluation", (_event, projectId, jobId) => runtimeRequest(jobPath(projectId, jobId)));
  ipcMain.handle("runtime:control-condition-evaluation", (_event, projectId, jobId, command, expectedVersion) => {
    const path = jobPath(projectId, jobId);
    if (!["pause", "resume", "cancel"].includes(command) || !Number.isSafeInteger(expectedVersion) || expectedVersion < 0) throw new Error("条件检查控制参数无效");
    return runtimeRequest(`${path}/control`, { method: "POST", body: JSON.stringify({ command, expectedVersion }) });
  });
  ipcMain.handle("runtime:get-condition-explanations", (_event, projectId, jobId, cursor = null) => {
    const path = jobPath(projectId, jobId);
    if (cursor !== null && (typeof cursor !== "string" || !cursor.length || cursor.length > 2048)) throw new Error("条件解释游标无效");
    const query = new URLSearchParams({ limit: "16" });
    if (cursor !== null) query.set("cursor", cursor);
    return runtimeRequest(`${path}/explanations?${query}`);
  });
}
