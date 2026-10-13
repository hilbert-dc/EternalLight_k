// @ts-check
export class AdvisorySessions {
  #threads = new Map();

  preferredThread(projectId) {
    return this.#threads.get(projectId) ?? null;
  }

  remember(projectId, threadId) {
    this.#threads.set(projectId, threadId);
  }

  canReuseCurrent(projectId, threadId, activeProjectId, activePurpose) {
    if (!threadId || activeProjectId !== projectId || activePurpose !== "advisory") return false;
    const preferred = this.preferredThread(projectId);
    return preferred === null || preferred === threadId;
  }
}
