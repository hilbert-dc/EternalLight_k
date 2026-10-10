import { composeNovelCodexPrompt } from "./codex-runtime.mjs";

export function authorSteerInput(task, intent) {
  if (intent?.taskId !== task.id || intent.status !== "pending"
    || !Number.isSafeInteger(intent.intentRevision) || intent.intentRevision < 1 || intent.intentRevision > 16
    || typeof intent.authorMessageId !== "string" || !/^[a-zA-Z0-9-]{1,160}$/.test(intent.authorMessageId)
    || typeof intent.authorText !== "string" || !intent.authorText.trim()) throw new Error("作者追加消息来源无效");
  return composeNovelCodexPrompt("", intent.authorText,
    "本条作者追加指令已持久登记。后续行动须使用当前冻结意图代次，旧代次不再允许登记候选动作。",
    { binding: { taskId: task.id, projectId: task.projectId, threadId: task.threadId,
      authorMessageId: intent.authorMessageId, expectedIntentRevision: intent.intentRevision } });
}
