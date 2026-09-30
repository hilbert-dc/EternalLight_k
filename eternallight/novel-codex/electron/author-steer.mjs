import { composeNovelCodexPrompt } from "./codex-runtime.mjs";

export function authorSteerInput(task, intent) {
  if (intent?.taskId !== task.id || intent.status !== "pending"
    || !Number.isSafeInteger(intent.intentRevision) || intent.intentRevision < 1 || intent.intentRevision > 16
    || typeof intent.authorMessageId !== "string" || !/^[a-zA-Z0-9-]{1,160}$/.test(intent.authorMessageId)
    || typeof intent.authorText !== "string" || !intent.authorText.trim()) throw new Error("作者追加消息来源无效");
  return composeNovelCodexPrompt("", intent.authorText,
    `当前活动对话任务 taskId=${task.id}、projectId=${task.projectId}、threadId=${task.threadId}。本条作者追加指令已登记为消息 ${intent.authorMessageId}、意图代次 ${intent.intentRevision}；request_candidate_task 必须显式传 expectedIntentRevision=${intent.intentRevision}。旧代次不再允许登记候选动作。`);
}
