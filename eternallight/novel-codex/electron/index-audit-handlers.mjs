// @ts-check
// Index audit jobs share one IPC shape: read the job, start it, or control a
// specific job version. Native Runtime owns job state and version conflicts.
const AUDITS = [
  ["search-audit", "索引校验控制参数无效"],
  ["entity-audit", "实体索引校验控制参数无效"],
  ["relation-audit", "关系索引校验控制参数无效"],
  ["knowledge-audit", "知识索引校验控制参数无效"],
  ["snapshot-knowledge-audit", "历史知识校验控制参数无效"],
  ["fact-audit", "确认事实校验控制参数无效"],
];

const validProject = projectId => typeof projectId === "string" && !!projectId.trim();

export function registerIndexAuditHandlers(ipcMain, request) {
  for (const [audit, controlError] of AUDITS) {
    const jobPattern = new RegExp(`^${audit}-[a-f0-9]{32}$`);
    const path = projectId => `/api/projects/${encodeURIComponent(projectId)}/${audit}s`;
    ipcMain.handle(`runtime:get-${audit}`, (_event, projectId) => {
      if (!validProject(projectId)) throw new Error("作品标识无效");
      return request(path(projectId));
    });
    ipcMain.handle(`runtime:start-${audit}`, (_event, projectId) => {
      if (!validProject(projectId)) throw new Error("作品标识无效");
      return request(path(projectId), { method: "POST", body: "{}" });
    });
    ipcMain.handle(`runtime:control-${audit}`, (_event, projectId, jobId, command, expectedVersion) => {
      if (!validProject(projectId) || typeof jobId !== "string" || !jobPattern.test(jobId) || !["pause", "resume", "cancel"].includes(command)
        || !Number.isInteger(expectedVersion) || expectedVersion < 0) throw new Error(controlError);
      return request(`${path(projectId)}/${encodeURIComponent(jobId)}/control`, { method: "POST", body: JSON.stringify({ command, expectedVersion }) });
    });
  }
}
