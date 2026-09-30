// Main supplies the trusted-sender IPC wrapper and Author-only runtime transport.
function outcomePath(projectId, writerTaskId) {
  if (typeof projectId !== "string" || !projectId.trim() || projectId.length > 160 || [".", ".."].includes(projectId) || /[\x00-\x1f\x7f]/.test(projectId)
    || typeof writerTaskId !== "string" || !/^task-[a-f0-9]{32}$/.test(writerTaskId)) throw new Error("修改结果标识无效");
  return `/api/projects/${encodeURIComponent(projectId)}/revision-outcomes/${writerTaskId}`;
}

export function registerRevisionOutcomeHandlers(ipcMain, runtimeRequest) {
  ipcMain.handle("runtime:get-revision-outcome", (_event, projectId, writerTaskId) => runtimeRequest(outcomePath(projectId, writerTaskId)));
}

const decisionFields = ["decisionId", "expectedAssessmentRevisionId", "expectedDecisionId", "choice", "note"];
const decisionId = value => typeof value === "string" && /^[A-Za-z0-9-]{1,128}$/.test(value);
const revisionId = value => typeof value === "string" && value.length > 0 && Buffer.byteLength(value, "utf8") <= 160
  && value.trim() === value && !/[\p{Cc}]/u.test(value);

export function registerRevisionDecisionHandlers(ipcMain, runtimeRequest) {
  ipcMain.handle("runtime:decide-revision-outcome", (_event, projectId, writerTaskId, request) => {
    const path = outcomePath(projectId, writerTaskId);
    if (!request || typeof request !== "object" || Array.isArray(request)
      || Object.keys(request).length !== decisionFields.length || !Object.keys(request).every(key => decisionFields.includes(key))
      || !decisionId(request.decisionId) || !revisionId(request.expectedAssessmentRevisionId)
      || !(request.expectedDecisionId === null || decisionId(request.expectedDecisionId))
      || !["acceptRevision", "repairDraft", "defer"].includes(request.choice)
      || typeof request.note !== "string" || Array.from(request.note).length > 1000) throw new Error("修改结果决策参数无效");
    return runtimeRequest(`${path}/decisions`, { method: "POST", body: JSON.stringify(request) });
  });
}
