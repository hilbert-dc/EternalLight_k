const identifier = value => typeof value === "string" && value.trim().length > 0 && value.length <= 160;
const exactKeys = (value, keys) => value !== null && typeof value === "object" && !Array.isArray(value) && Object.keys(value).every(key => keys.includes(key));
const sourceValid = source => exactKeys(source, ["outlineRevisionId", "nodeId"]) && identifier(source.outlineRevisionId) && identifier(source.nodeId);
export function registerWorkCatalog(ipcMain, request) {
  ipcMain.handle("runtime:get-work-catalog", (_event, projectId) => {
    if (!identifier(projectId)) throw new Error("作品标识无效");
    return request(`/api/projects/${encodeURIComponent(projectId)}/catalog`);
  });
  ipcMain.handle("runtime:update-work-catalog", (_event, projectId, input) => {
    const op = input?.operation;
    if (!identifier(projectId) || !Number.isSafeInteger(input?.expectedRevision) || input.expectedRevision < 0
      || !identifier(input?.idempotencyKey) || !exactKeys(input, ["expectedRevision", "idempotencyKey", "operation"]) || !op) throw new Error("作品目录请求无效");
    if (op.type === "createVolume") {
      if (!exactKeys(op, ["type", "title", "source"]) || typeof op.title !== "string" || !op.title.trim() || Array.from(op.title).length > 160
        || op.source != null && !sourceValid(op.source)) throw new Error("卷名称或来源无效");
    } else if (op.type === "linkOutline") {
      if (!exactKeys(op, ["type", "volumeId", "source"]) || !identifier(op.volumeId) || !sourceValid(op.source)) throw new Error("卷纲关联无效");
    } else if (op.type === "placeChapter") {
      if (!exactKeys(op, ["type", "chapterId", "volumeId"]) || !identifier(op.chapterId) || op.volumeId !== null && !identifier(op.volumeId)) throw new Error("章节归属无效");
    } else throw new Error("不支持的目录操作");
    return request(`/api/projects/${encodeURIComponent(projectId)}/catalog`, { method: "POST", body: JSON.stringify(input) });
  });
}
