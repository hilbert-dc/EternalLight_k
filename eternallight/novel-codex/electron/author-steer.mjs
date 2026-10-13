// @ts-check
import { composeNovelCodexPrompt } from "./codex-runtime.mjs";

export function authorSteerInput(task, intent) {
  if (intent?.taskId !== task.id || intent.status !== "pending"
    || !Number.isSafeInteger(intent.intentRevision) || intent.intentRevision < 1 || intent.intentRevision > 16
    || typeof intent.authorMessageId !== "string" || !/^[a-zA-Z0-9-]{1,160}$/.test(intent.authorMessageId)
    || typeof intent.authorText !== "string" || !intent.authorText.trim()) throw new Error("作者追加消息来源无效");
  return composeNovelCodexPrompt("", intent.authorText,
    "本条作者追加指令已持久登记。明确修改的部分替换同一作者先前对应要求，其余兼容的原目标继续执行；新旧要求的这种替换本身不是待确认冲突。仅在最新要求仍有影响执行的真实歧义时询问，不重复索要已明确给出的决定。由模型结合原话判断变更范围，不扩大授权。后续行动须使用当前冻结意图代次，旧代次不再允许登记候选动作。",
    { binding: { taskId: task.id, projectId: task.projectId, threadId: task.threadId,
      authorMessageId: intent.authorMessageId, expectedIntentRevision: intent.intentRevision } });
}
