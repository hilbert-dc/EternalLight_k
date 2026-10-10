import { assertNovelTurnBudget } from "./codex-runtime.mjs";
// Run inside the existing Engine lease: corrections share its task capability,
// thread, deadline and persistent budget. Only the model can submit its review.
export async function enforceWorkDeliveryProtocol(adapter, input, options, task, runtimeRequest, assertCurrent) {
  let result = await adapter.sendTurn(input, options);
  if (task.operation !== "advisory") return result;
  const path = `/api/projects/${encodeURIComponent(task.projectId)}/tasks/${encodeURIComponent(task.id)}`;
  for (let correction = 0; correction <= 2; correction++) {
    assertCurrent();
    const current = await runtimeRequest(path);
    if (current.status === "completed") return result;
    if (!["queued", "running", "waiting"].includes(current.status)) throw new Error("工作决策已停止，已保存成果保留。");
    const status = await runtimeRequest(`${path}/work-decision?threadId=${encodeURIComponent(task.threadId)}&expectedRevision=${task.inputRevision}`);
    if (status.deliveryReviewRequired !== true) return result;
    if (correction === 2) throw Object.assign(new Error("最终交付审核尚未提交，任务已停止；已保存成果保留，可从执行记录继续。"), { code: "WORK_DELIVERY_PROTOCOL_INCOMPLETE" });
    const correctionInstructions = "本任务尚未通过最终交付审核。文字回复不能结束任务。保持原作者目标、权限与额度，不重做已保存成果。先调用read_work_status取得最新version/sourceDigest，按需分页核对真实回执和read_work_plan。已授权接续仍待登记时先完成交接，不能以partial和‘下一轮继续’承诺关闭；确有阻碍先把目标记为blocked并说明原因。调用complete_task显式提交deliveryReview；fulfilled只表示本Work前置目标通过及交接登记，不表示原长篇已完成。真正未完成如实填partial和unresolved，不能编造证据或宣称未做的检查。提交成功后再给作者最终回复。";
    const turn = assertNovelTurnBudget({ input: [{ type: "text", text: "应用内部交付核对触发；保持原作者要求。" }],
      additionalContext: { ...options?.additionalContext, novelDeliveryCorrection: { kind: "application", value: correctionInstructions } } });
    result = await adapter.sendTurn(turn.input, { disableEnvironment: true, additionalContext: turn.additionalContext });
  }
}
