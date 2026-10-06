// Resume the verified report history, never renderer-supplied prose or thread IDs.
export async function structureDiscussionThread({ projectId, focus, repairSource, runtimeRequest }) {
  const taskId = focus?.chapterStructureTaskId;
  if (!taskId) return null;
  if (repairSource || focus.reference) throw new Error("删章检查不能与其他讨论来源混用");
  const path = `/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(taskId)}`;
  const task = await runtimeRequest(path);
  const validId = value => typeof value === "string" && value.trim() === value && value.length > 0
    && Buffer.byteLength(value) <= 160 && !/[\p{Cc}]/u.test(value);
  const unavailable = () => new Error("这次删章检查没有可继续的完整结果，请重新读取原任务；未切换到其他对话。");
  if (task?.id !== taskId || task.projectId !== projectId || task.operation !== "assessChapterStructure"
    || task.status !== "completed" || !validId(task.threadId) || !validId(task.turnId)) throw unavailable();
  const reply = await runtimeRequest(`${path}/final-reply`);
  if (reply?.projectId !== projectId || reply.taskId !== taskId || reply.state !== "recorded"
    || reply.binding?.threadId !== task.threadId || reply.binding.turnId !== task.turnId
    || reply.binding.truncated !== false || !validId(reply.binding.itemId)
    || typeof reply.binding.text !== "string" || !reply.binding.text.trim()) throw unavailable();
  return task.threadId;
}
