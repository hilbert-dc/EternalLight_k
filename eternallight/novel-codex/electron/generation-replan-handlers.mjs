// @ts-check
function runPath(projectId, runId) {
  if (typeof projectId !== "string" || !projectId.trim() || projectId.length > 160 || /[\0\r\n]/.test(projectId)
    || typeof runId !== "string" || !/^generation-[a-f0-9]{32}$/.test(runId)) throw new Error("重规划目标无效");
  return `/api/projects/${encodeURIComponent(projectId)}/generation-runs/${runId}/replan`;
}

// main supplies trusted IPC and author transport. Submission never refreshes the author's preview.
export function registerGenerationReplanHandlers(ipcMain, runtimeRequest, reconcileUsage) {
  if (typeof reconcileUsage !== "function") throw new Error("重规划需要供应商用量核账");
  ipcMain.handle("runtime:preview-generation-replan", async (_event, projectId, runId) => {
    const path = runPath(projectId, runId);
    await reconcileUsage(projectId, runId);
    return runtimeRequest(path);
  });
  ipcMain.handle("runtime:replan-generation", async (_event, projectId, runId, request) => {
    const path = runPath(projectId, runId);
    if (!request || typeof request !== "object" || Array.isArray(request)
      || Object.keys(request).length !== 5
      || !Object.keys(request).every(key => ["expectedVersion", "expectedProjectRevision", "expectedSourceDigest", "restartFrom", "instruction"].includes(key))
      || !Number.isSafeInteger(request.expectedVersion) || request.expectedVersion < 0
      || !Number.isSafeInteger(request.expectedProjectRevision) || request.expectedProjectRevision < 0
      || typeof request.expectedSourceDigest !== "string" || !/^[a-f0-9]{64}$/.test(request.expectedSourceDigest)
      || !["brief", "outline", "rollingPlan", "currentChapter"].includes(request.restartFrom)
      || typeof request.instruction !== "string" || !request.instruction.trim() || Array.from(request.instruction.trim()).length > 1000) throw new Error("重规划确认参数无效");
    await reconcileUsage(projectId, runId);
    return runtimeRequest(path, { method: "POST", body: JSON.stringify(request) });
  });
}
