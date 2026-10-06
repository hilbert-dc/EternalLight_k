const identifier = value => typeof value === "string" && value.trim() === value && value.length > 0
  && Buffer.byteLength(value, "utf8") <= 160 && !/[\p{Cc}]/u.test(value);

export function registerChapterDeletion(ipcMain, request) {
  ipcMain.handle("runtime:delete-chapter", (_event, projectId, input) => {
    if (!identifier(projectId) || input === null || typeof input !== "object" || Array.isArray(input)
      || Object.keys(input).length !== 3 || Object.keys(input).some(key => !["chapterId", "expectedRevision", "idempotencyKey"].includes(key))
      || !identifier(input.chapterId) || !identifier(input.idempotencyKey)
      || !Number.isSafeInteger(input.expectedRevision) || input.expectedRevision < 0) throw new Error("删除章节请求无效");
    return request(`/api/projects/${encodeURIComponent(projectId)}/chapters/${encodeURIComponent(input.chapterId)}/delete`, {
      method: "POST", body: JSON.stringify({ expectedRevision: input.expectedRevision, idempotencyKey: input.idempotencyKey }),
    });
  });
}
