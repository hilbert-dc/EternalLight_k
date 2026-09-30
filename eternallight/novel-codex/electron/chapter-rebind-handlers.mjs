const fields = ["chapterId", "previousPlanRevisionId", "previousChapterPlanId", "sourceRevisionId", "planRevisionId", "chapterPlanId", "expectedRevision", "idempotencyKey"];
const identifier = value => typeof value === "string" && value.trim() === value && value.length > 0 && value.length <= 160 && !/[\0\r\n]/.test(value);

export function registerChapterRebindHandlers(ipcMain, runtimeRequest) {
  ipcMain.handle("runtime:rebind-chapter-plan", (_event, projectId, request) => {
    if (!identifier(projectId) || !request || typeof request !== "object" || Array.isArray(request)
      || Object.keys(request).length !== fields.length || !Object.keys(request).every(key => fields.includes(key))
      || !Number.isSafeInteger(request.expectedRevision) || request.expectedRevision < 0
      || !fields.filter(key => key !== "expectedRevision").every(key => identifier(request[key]))) throw new Error("章纲重新关联确认参数无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/chapters/rebind-plan`, { method: "POST", body: JSON.stringify(request) });
  });
}
