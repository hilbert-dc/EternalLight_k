// @ts-check
import { workStopDescription } from "../shared/work-recovery-presentation.mjs";
import { isDeepStrictEqual } from "node:util";
import { safeRuntimeDiagnostic } from "./runtime-request-error.mjs";
const genericFailure = "原任务接续未成功，请核对当前任务状态与已保存结果。";
function continuationError(publicMessage) {
  return Object.assign(new Error(publicMessage), { publicMessage });
}
const continuationMessages = {
  generation_handoff_run_changed: "原任务状态或可用额度已变化，需要重新决定接续。",
  generation_handoff_source_changed: "前置资料已变化，需要重新核对后再接续。",
  generation_handoff_binding_invalid: "交接身份已失效，需要重新读取原任务。",
  generation_handoff_unsettled_results: "原任务仍有待核对的保存或结算结果，请先处理后再接续。",
  generation_handoff_prerequisites_incomplete: "前置任务与执行条件尚未满足，请查看未完成事项。",
};

// A direct author click keeps its existing Work authority. It may refresh an
// idle checkpoint, but never manufactures a model proposal or author intent.
export async function rebindIdleContinuation(runtimeRequest, projectId, runId, work, run) {
  if (!["paused", "blocked"].includes(run.status)
    || run.status === "blocked" && (!["pendingRecovery", "pendingUsage", "pendingWriteAndUsage"].includes(run.blockedReason) || run.currentChapterId || run.activeTaskId)) return false;
  if (["pendingRecovery", "pendingUsage", "pendingWriteAndUsage"].includes(work.resumeBlockReason)) throw continuationError(workStopDescription(work.resumeBlockReason));
  const base = `/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(runId)}`;
  const preview = await runtimeRequest(`${base}/replan`);
  const restartFrom = run.currentChapterId ? "currentChapter" : "rollingPlan";
  if (preview.run.id !== runId || preview.run.projectId !== projectId || preview.run.version !== run.version
    || !preview.availableFrom.includes(restartFrom)) throw continuationError("原任务状态已变化，请重新读取");
  if (run.activeTaskId && !["completed", "failed", "cancelled"].includes(preview.activeAttemptStatus)) throw continuationError("原任务仍有执行中的步骤，不能重复调度。");
  if (run.status === "paused" && !run.activeTaskId && run.sourceDigest === preview.sourceDigest) return false;
  await runtimeRequest(`${base}/rebind`, { method: "POST", body: JSON.stringify({
    expectedVersion: run.version, expectedProjectRevision: preview.projectRevision,
    expectedSourceDigest: preview.sourceDigest, restartFrom,
  }) });
  return true;
}

// Shared by new author turns and resumed Work; execute only after the
// preceding Work lifecycle has closed, so the original Work remains the owner.
export async function executeWorkContinuation(runtimeRequest, projectId, result, resumeConversationWork) {
  const originTask = result.workflow?.task;
  const predecessor = result.workTask?.id ?? originTask?.target?.workDecision?.workTaskId
    ?? (originTask?.id ? `work-${originTask.id}` : null);
  if (!predecessor) return result;
  let continuation = null;
  let sourceTaskId = originTask?.id ?? null;
  try {
    // Even if the preceding Work was already completed when the app restarted,
    // its Native decision remains the input. An in-memory result is optional.
    const { handoff } = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/work-tasks/${encodeURIComponent(predecessor)}/generation-handoff`);
    if (!handoff) return result;
    const { task, proposal } = handoff;
    sourceTaskId = task?.id ?? null;
    if (!task || task.projectId !== projectId || task.operation !== "advisory" || task.status !== "completed"
      || task.target?.revisionRepairSource || task.target?.conversationFocus?.readOnly
      || proposal.projectId !== projectId || proposal.sourceTaskId !== task.id || proposal.threadId !== task.threadId
      || originTask && originTask.threadId !== task.threadId
      || result.workTask && (result.workTask.projectId !== projectId || result.workTask.threadId !== task.threadId)
      || result.generationHandoff && !isDeepStrictEqual(result.generationHandoff, proposal)) throw continuationError("原任务恢复决定的来源不匹配");
    const resume = proposal.resumeRun;
    if (typeof resume?.id !== "string" || !/^generation-[a-f0-9]{32}$/.test(resume.id)
      || !Number.isSafeInteger(resume.expectedVersion) || resume.expectedVersion < 0) throw continuationError("原任务恢复身份无效");
    continuation = { workTaskId: `work-${resume.id}`, expectedThreadId: task.threadId,
      origin: "automatic", generationHandoff: { sourceTaskId: task.id, proposalId: proposal.proposalId } };
    // Native commits the first resume/rebind together with the consumption
    // receipt. Repeated requests cannot repeat that transition or override a
    // later author pause, changed intent or exhausted allowance.
    const admission = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/generation-handoff/execute`,
      { method: "POST", body: JSON.stringify({ proposalId: proposal.proposalId }) });
    if (admission.run?.id !== resume.id || admission.receipt?.runId !== resume.id) throw continuationError("原任务交接回执不匹配");
    if (!admission.shouldExecute) {
      const work = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/work-tasks/${encodeURIComponent(continuation.workTaskId)}`);
      return { ...result, generationHandoff: null, workTask: work };
    }
    if (!resumeConversationWork) throw continuationError("原任务恢复执行器不可用");
    const work = await resumeConversationWork(continuation);
    const failure = work.resumeFailure?.message || (["blocked", "paused"].includes(work.status) ? "原任务尚未完成，请查看保存结果与阻塞原因。" : null);
    return { ...result, generationHandoff: null, workTask: work,
    message: failure ?? (work.status === "completed" ? "原任务执行结束，请核对保存结果及目标完成范围。" : "原任务仍在执行，请以最新保存结果为准。"),
    ...(failure ? { executionFailure: { code: "WORK_CONTINUATION_INCOMPLETE", message: failure,
      sourceTaskId, workTaskId: continuation.workTaskId, stepId: null } } : {}) };
  } catch (error) {
    return { ...result, generationHandoff: null, executionFailure: { code: "WORK_CONTINUATION_FAILED",
    message: error?.publicMessage ?? (Object.hasOwn(continuationMessages, error?.code) ? continuationMessages[error.code] : genericFailure), sourceTaskId,
    workTaskId: continuation?.workTaskId ?? predecessor, stepId: null,
    details: { nativeCode: typeof error?.code === "string" ? error.code : null,
      diagnostic: safeRuntimeDiagnostic(error?.details?.diagnostic ?? error?.message) } } };
  }
}
