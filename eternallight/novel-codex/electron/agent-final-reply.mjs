// @ts-check
// Only completed public agent messages are evidence. Stream deltas, commentary
// and reasoning items cannot become the final reply merely by arriving last.
export function selectFinalReply(previous, item) {
  if (item?.type !== "agentMessage" || ![null, undefined, "final_answer"].includes(item.phase)
    || (item.status != null && item.status !== "completed")
    || typeof item.id !== "string" || !item.id || item.id.length > 160) return previous;
  const text = typeof item.text === "string" ? item.text : Array.isArray(item.content)
    ? item.content.map((part) => typeof part?.text === "string" ? part.text : "").join("") : null;
  if (text === null || (previous?.phase === "final_answer" && item.phase !== "final_answer")) return previous;
  let bytes = 0;
  let bounded = "";
  for (const character of text) {
    const size = Buffer.byteLength(character, "utf8");
    if (bytes + size > 65_536) break;
    bytes += size;
    bounded += character;
  }
  return { itemId: item.id, text: bounded, truncated: item.displayTruncated === true || bounded.length !== text.length,
    phase: item.phase ?? null };
}

export async function persistTaskFinalReply(runtimeRequest, projectId, task, result) {
  if (result?.turn?.status !== "completed" || typeof result.turn.id !== "string") {
    throw new Error("回合没有成功结束，不能登记最终回复");
  }
  const base = `/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}`;
  const history = task.operation === "advisory" ? (await runtimeRequest(`${base}/intents`)).intents : [];
  if (!Array.isArray(history) || history.some((intent) => intent.status !== "delivered")) {
    throw new Error("作者追加要求尚未送达，不能登记最终回复");
  }
  const binding = { threadId: task.threadId, turnId: result.turn.id,
    intentRevision: history.at(-1)?.intentRevision ?? 0, itemId: result.finalReply?.itemId ?? null,
    text: result.finalReply?.text ?? "", truncated: result.finalReply?.truncated ?? false };
  const matches = (receipt) => receipt?.taskId === task.id && receipt.projectId === projectId
    && Object.keys(binding).every((key) => receipt.binding?.[key] === binding[key]);
  try {
    const receipt = await runtimeRequest(`${base}/final-reply`, { method: "POST", body: JSON.stringify(binding) });
    if (!matches(receipt)) throw new Error("最终回复保存结果与本回合不一致");
    return receipt;
  } catch (error) {
    // An unknown HTTP result is reconciled by reading, never by replaying a turn.
    const receipt = await runtimeRequest(`${base}/final-reply`).catch(() => null);
    if (matches(receipt)) return receipt;
    throw error;
  }
}
