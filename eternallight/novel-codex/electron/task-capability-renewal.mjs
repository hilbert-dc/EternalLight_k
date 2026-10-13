// @ts-check
export async function renewTaskAuthority({ execution, expectedExpiresAt, runtimeRequest }) {
  const task = execution.task;
  const path = `/api/projects/${encodeURIComponent(task.projectId)}/tasks/${encodeURIComponent(task.id)}`;
  try {
    return await runtimeRequest(`${path}/capability/renew`, { method: "POST", body: JSON.stringify({
      threadId: task.threadId, capabilityHash: execution.capability.hash, expectedExpiresAt,
      operation: task.operation, target: task.target, model: task.model ?? null,
    }) });
  } catch (error) {
    // A model may deliver its candidate and complete_task before its final reply.
    // Completion revoked the capability; it must not be renewed or misreported.
    const latest = await runtimeRequest(path).catch(() => null);
    if (latest?.id === task.id && latest.projectId === task.projectId
      && latest.threadId === task.threadId && latest.status === "completed") return null;
    throw error;
  }
}
