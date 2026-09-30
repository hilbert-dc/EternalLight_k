function scopePath(projectId, sourceTaskId) {
  if (typeof projectId !== "string" || !projectId.trim() || projectId.length > 160 || /[\r\n\0]/.test(projectId)
    || typeof sourceTaskId !== "string" || !/^task-[a-f0-9]{32}$/.test(sourceTaskId)) throw new Error("范围许可身份无效");
  return `/api/projects/${encodeURIComponent(projectId)}/tasks/${sourceTaskId}/execution-scope`;
}

function validateRequest(request) {
  if (!request || typeof request !== "object" || Array.isArray(request)
    || Object.keys(request).sort().join(",") !== "actionIds,expectedIntentRevision,maxActions,maxAdvisoryRounds,maxTaskAttempts"
    || !Array.isArray(request.actionIds) || request.actionIds.length < 1 || request.actionIds.length > 3
    || request.actionIds.some(id => typeof id !== "string" || !/^action-[a-f0-9]{32}$/.test(id))
    || new Set(request.actionIds).size !== request.actionIds.length
    || !Number.isInteger(request.maxActions) || request.maxActions < request.actionIds.length || request.maxActions > 3
    || !Number.isInteger(request.maxTaskAttempts) || request.maxTaskAttempts < 2 || request.maxTaskAttempts > 16
    || !Number.isInteger(request.maxAdvisoryRounds) || request.maxAdvisoryRounds < 0 || request.maxAdvisoryRounds > 3
    || !Number.isSafeInteger(request.expectedIntentRevision) || request.expectedIntentRevision < 0) throw new Error("范围许可参数无效");
}

export function registerExecutionScopeHandlers(ipcMain, { runtimeRequest, withWorkFlow, assertOperationsAvailable, advance }) {
  ipcMain.handle("agent:read-execution-scope", (_event, projectId, sourceTaskId) => runtimeRequest(scopePath(projectId, sourceTaskId)));
  ipcMain.handle("agent:preview-execution-scope", (_event, projectId, sourceTaskId, request) => {
    const path = scopePath(projectId, sourceTaskId);
    validateRequest(request);
    return runtimeRequest(`${path}/preview`, { method: "POST", body: JSON.stringify(request) });
  });
  ipcMain.handle("agent:confirm-execution-scope", (event, projectId, sourceTaskId, request, previewDigest) => {
    const path = scopePath(projectId, sourceTaskId);
    validateRequest(request);
    if (typeof previewDigest !== "string" || !/^[a-f0-9]{64}$/.test(previewDigest)) throw new Error("范围预览摘要无效");
    return withWorkFlow(projectId, async (flow) => {
      const { scope: existing } = await runtimeRequest(path);
      if (existing?.id !== `scope-${previewDigest}`) {
        const preview = await runtimeRequest(`${path}/preview`, { method: "POST", body: JSON.stringify(request) });
        if (preview.previewDigest !== previewDigest) throw new Error("范围预览已变化，请重新预览并确认");
        assertOperationsAvailable([...preview.targets.map(target => target.operation), ...(preview.request.maxAdvisoryRounds > 0 ? ["advisory"] : [])]);
      }
      const permit = await runtimeRequest(path, { method: "POST", body: JSON.stringify({ request, previewDigest }) });
      if (permit.scope.projectId !== projectId || permit.scope.sourceTaskId !== sourceTaskId) throw new Error("范围许可归属不符");
      flow.id = permit.scope.workTaskId;
      const workPath = `/api/projects/${encodeURIComponent(projectId)}/work-tasks/${encodeURIComponent(flow.id)}`;
      const work = await runtimeRequest(workPath);
      if (permit.status !== "active" || work.status !== "running") return { permit, work };
      assertOperationsAvailable([...permit.scope.targets.map(target => target.operation), ...(permit.scope.request.maxAdvisoryRounds > 0 ? ["advisory"] : [])]);
      return { permit, work: await advance(event, projectId, flow.id) };
    });
  });
}
