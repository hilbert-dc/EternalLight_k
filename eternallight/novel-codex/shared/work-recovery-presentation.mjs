// Presentation of Native reasons only. This module grants no recovery authority.
export const workStopLabels = Object.freeze({
  recoveryStalled: "自动处理仍未能推进，已停止重复尝试。已保存内容仍保留，请查看具体原因；执行条件实际变化后再核对原任务。",
  pendingRecovery: "保存结果尚未核对，确认前不会重复写入。",
  pendingUsage: "原请求的费用登记尚未结束，等待核对；这不表示有未知写入或额度耗尽。",
  pendingWriteAndUsage: "保存结果和费用登记均待核对，确认前不会重复执行。",
  unknownUsage: "服务商未返回完整用量，暂时无法确认剩余额度。已保存内容仍保留；这不代表额度已用完。",
  executionBlocked: "本次执行暂时无法安全继续，已保存内容仍保留，请查看具体原因。",
  budgetExhausted: "本次明确设置的执行额度已用完，已保存内容仍保留。",
  budgetExpired: "本次明确设置的执行有效期已结束，已保存内容仍保留。",
  readerRestart: "请在创作回看中检查已保存报告；尚无报告时可显式重开独立读者任务。",
  partialOutput: "已有部分内容保存，完整结果尚待核对。请先查看成果，避免重复生成。",
  inputChanged: "作品版本已变化，请在原对话中说明需要保留或调整的内容。",
  attemptBudget: "本次自动分析仍未完成，已停止重复尝试。请查看具体原因和已保存结果。",
  workNotReady: "任务尚未就绪，请查看原任务状态。",
  noReadyActions: "当前没有可以安全执行的下一步，请查看未满足的条件。",
  scene_feedback_needs_author: "场景评估需要你决定后续处理。",
  chapter_plan_rebound: "章纲已更新，需要根据当前内容重新规划。",
  agent_task_failed: "本次执行未完成，请查看具体原因与恢复安排。",
  agent_task_cancelled: "本次执行已停止，已保存内容仍保留。",
  agent_task_failed_after_output: "已保存部分内容，本次执行尚未完成。",
  agent_task_cancelled_after_output: "执行已停止，已保存内容仍保留。",
});
export const workStopDescription = reason => workStopLabels[reason] ?? "任务暂时无法继续，原因尚待确认；已保存内容仍保留。";
export const generationUsageBlocked = run => run?.usageCheck?.state === "unavailable";
export function generationUsageDescription(run) {
  if (!generationUsageBlocked(run)) return null;
  return run.usageCheck.phase === "taskRead"
    ? "暂时无法读取原任务状态，尚不能确认执行或结算结果。请重新读取，已保存内容仍保留。"
    : "原任务费用登记尚未完成，请重新核对；这不代表额度耗尽。已保存内容仍保留。";
}
