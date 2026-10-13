// @ts-check
function identifier(value) {
  if (typeof value !== "string" || !value || value.trim() !== value || Buffer.byteLength(value) > 160
    || /[\/\\?#\u0000-\u001f\u007f]/.test(value) || value === "." || value === "..") throw new Error("叙事承诺标识无效");
  return encodeURIComponent(value);
}

export function registerCommitmentHandlers(ipc, runtimeRequest) {
  const reportPath = (projectId, chapterId) => `/api/projects/${identifier(projectId)}/chapters/${identifier(chapterId)}/commitment-extractions`;
  const objectPath = (projectId, id) => `/api/projects/${identifier(projectId)}/commitments/${identifier(id)}`;
  ipc.handle("runtime:list-commitments", (_event, projectId, cursor = null, limit = 20) => {
    if (!Number.isSafeInteger(limit) || limit < 1 || limit > 20 || (cursor !== null && (typeof cursor !== "string" || Buffer.byteLength(cursor) > 1024))) throw new Error("承诺分页参数无效");
    const query = new URLSearchParams({ limit: String(limit) });
    if (cursor !== null) query.set("cursor", cursor);
    return runtimeRequest(`/api/projects/${identifier(projectId)}/commitments?${query}`);
  });
  ipc.handle("runtime:get-commitment-extraction", (_event, projectId, chapterId, reportRevisionId = null) => {
    const path = reportPath(projectId, chapterId);
    return runtimeRequest(`${path}${reportRevisionId === null ? "" : `?reportRevisionId=${identifier(reportRevisionId)}`}`);
  });
  ipc.handle("runtime:establish-commitment", (_event, projectId, chapterId, request) => {
    const path = reportPath(projectId, chapterId);
    if (!request || !Number.isSafeInteger(request.expectedRevision) || request.expectedRevision < 0) throw new Error("承诺建立版本无效");
    for (const name of ["reportRevisionId", "proposalId", "baseSnapshotId", "idempotencyKey"]) identifier(request[name]);
    return runtimeRequest(`${path}/establish`, { method: "POST", body: JSON.stringify(request) });
  });
  ipc.handle("runtime:get-commitment", (_event, projectId, id, snapshotId = null) => {
    const path = objectPath(projectId, id);
    return runtimeRequest(`${path}${snapshotId === null ? "" : `?snapshotId=${identifier(snapshotId)}`}`);
  });
  ipc.handle("runtime:save-commitment", (_event, projectId, id, request) => {
    const path = objectPath(projectId, id);
    if (!request || request.payload?.id !== id || !Number.isSafeInteger(request.expectedRevision) || request.expectedRevision < 0) throw new Error("承诺保存参数无效");
    identifier(request.baseSnapshotId);
    identifier(request.idempotencyKey);
    if (request.expectedCandidateRevisionId !== null) identifier(request.expectedCandidateRevisionId);
    if (Buffer.byteLength(JSON.stringify(request)) > 34_000) throw new Error("承诺保存内容过大");
    return runtimeRequest(path, { method: "POST", body: JSON.stringify(request) });
  });
  ipc.handle("runtime:create-commitment-review", (_event, projectId, id, revisionId, idempotencyKey) => {
    const path = objectPath(projectId, id);
    identifier(revisionId); identifier(idempotencyKey);
    return runtimeRequest(`${path}/reviews`, { method: "POST", body: JSON.stringify({ revisionId, idempotencyKey }) });
  });
}
