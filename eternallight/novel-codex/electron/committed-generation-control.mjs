// @ts-check
// A tool result is an observation, not control authority. Re-read Native's
// committed run and Work before interrupting only the lease that owns it.
export async function interruptCommittedGenerationPause(message, { execution, flow, runtimeRequest, currentExecution, interrupt }) {
  const params = message?.params, item = params?.item, task = execution?.task;
  if (message?.method !== "item/completed" || item?.type !== "mcpToolCall" || item.server !== "novel_workspace"
    || item.tool !== "pause_generation_run" || item.status !== "completed" || !task || task.operation !== "advisory"
    || params.threadId !== task.threadId || !task.target?.runId || flow?.projectId !== task.projectId
    || flow.id !== `work-${task.target.runId}`) return false;
  const content = item.result?.content;
  if (!Array.isArray(content) || content.length !== 1 || content[0]?.type !== "text" || typeof content[0].text !== "string") return false;
  let result;
  try { result = JSON.parse(content[0].text); } catch { return false; }
  const receipt = result?.receipt;
  if (receipt?.authority !== "committedGenerationControl" || receipt.status !== "paused" || receipt.sourceTaskId !== task.id
    || receipt.runId !== task.target.runId || !Number.isSafeInteger(receipt.version) || receipt.version < 0) return false;
  const base = `/api/projects/${encodeURIComponent(task.projectId)}`;
  const run = await runtimeRequest(`${base}/generation-runs/${encodeURIComponent(receipt.runId)}`);
  const work = await runtimeRequest(`${base}/work-tasks/${encodeURIComponent(flow.id)}`);
  if (currentExecution() !== execution || execution.committedPause || run.id !== receipt.runId || run.projectId !== task.projectId
    || run.status !== "paused" || run.version !== receipt.version || work.id !== flow.id || work.projectId !== task.projectId
    || work.threadId !== task.threadId || work.status !== "paused") return false;
  execution.committedPause = receipt;
  await interrupt(execution);
  return true;
}
