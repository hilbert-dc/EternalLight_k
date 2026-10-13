// @ts-check
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

// Presentation only: these reasons come from Native admission, never grant execution.
export function continuationWaiting(reason) {
  const values = {
    snapshotChanged: ["condition", "任务或作品已更新，正在重新核对接续条件"],
    conversationBusy: ["condition", "同一对话的当前执行结束后，再核对后续安排"],
    activeOwner: ["condition", "当前执行仍持有任务，等待它完成交接"],
    activeAttempt: ["condition", "当前步骤尚未结束，等待执行结果"],
    unknownWrite: ["condition", "正在等待原保存结果核对，确认前不会重复写入"],
    unsettledUsage: ["condition", "正在等待原请求费用登记，尚未开始下一次执行"],
    authorIntentChanged: ["condition", "作者要求已更新，等待核对当前安排"],
    authorIntentPending: ["condition", "等待新指令完成登记"],
    interlude: ["condition", "等待当前追加讨论结束"],
    authorStopped: ["condition", "任务已暂停或取消，不会自动启动后续步骤"],
    authorResumeRequired: ["author", "本轮已结束失败，自动接续已停止；请核对原任务后明确继续"],
    generationScopeApprovalRequired: ["author", "新的创作范围需要你确认，原成果保留"],
    authorAuthorizationMissing: ["author", "缺少本次创作的作者授权，需要你确认范围"],
    budgetAuthorizationMissing: ["author", "缺少本次执行授权，需要你确认后再继续"],
    budgetExhausted: ["author", "已达到你明确设置的额度，需要你决定是否继续"],
    budgetExpired: ["author", "已超过你明确设置的有效期，需要你决定是否继续"],
    continuationPending: ["condition", "尚未确认下一步实际启动，正在核对原任务"],
    generationNotRunning: ["blocked", "原创作运行暂未满足接续条件，保留成果等待核对"],
    executionBlocked: ["blocked", "本次执行无法安全接续，需解决具体故障；原目标和成果保留"],
    recoveryStalled: ["blocked", "相同条件下未能推进，已停止重复执行；等待相关条件改变"],
    unknownUsage: ["condition", "用量尚未核实，等待核对原请求"],
  };
  const [category, label] = values[reason] ?? ["blocked", "接续条件尚未满足，原因待核对；原目标和成果保留"];
  return { category, label };
}
