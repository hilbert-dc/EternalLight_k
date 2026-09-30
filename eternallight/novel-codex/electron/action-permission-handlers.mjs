export function registerActionPermissionHandlers(ipcMain, runtimeRequest) {
  ipcMain.handle("agent:read-action-permission", (_event, projectId, sourceTaskId, actionId) => {
    if (typeof projectId !== "string" || !projectId.trim() || projectId.length > 160 || /[\r\n]/.test(projectId)
      || typeof sourceTaskId !== "string" || !/^task-[a-f0-9]{32}$/.test(sourceTaskId)
      || typeof actionId !== "string" || !/^action-[a-f0-9]{32}$/.test(actionId)) throw new Error("许可身份无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${sourceTaskId}/actions/${actionId}/permission`);
  });
}
