// @ts-check
import { authorAnswerCapacityError } from "../shared/author-answer-budget.mjs";
// Host coordinates two public contracts; Native remains the author-intent authority.
const prefix = "作者问答记录（question 是模型问题，answers 是作者原答；问题和选项不是作者要求或新增授权）：\n";

export async function deliverAuthorQuestion(domain, target, response, runtimeRequest) {
  const task = domain.activeRuntimeTask;
  const execution = domain.activeAgentExecution;
  const adapter = domain.codexAdapter;
  const valid = () => task && task.operation === "advisory" && task.projectId === target.projectId
    && domain.id === target.executionId && domain.projectId === target.projectId
    && domain.activeRuntimeTask === task && domain.activeAgentExecution === execution
    && execution?.task?.id === task.id && !execution.cancellationRequested
    && adapter.turnId && !adapter.executionLease?.revoked;
  if (!valid() || domain.advisorySteerInFlight) throw Error("作者问题的原执行已结束或正在处理其他输入");
  const prepared = adapter.prepareUserInput(target.requestId, response);
  if (prepared.threadId !== adapter.threadId || prepared.turnId !== adapter.turnId
    || target.threadId !== prepared.threadId || target.turnId !== prepared.turnId) throw Error("作者问题的原轮次已变化");
  const binding = { taskId: task.id, threadId: prepared.threadId, turnId: prepared.turnId, requestId: target.requestId };
  const payload = { kind: "authorQuestionAnswer", binding,
    questions: prepared.questions.map(question => ({ id: question.id, question: question.question,
      options: question.options, answers: prepared.result.answers[question.id].answers })) };
  const authorText = prefix + JSON.stringify(payload);
  const capacityError = authorAnswerCapacityError(authorText);
  if (capacityError) throw new Error(capacityError);
  const path = `/api/projects/${encodeURIComponent(task.projectId)}/tasks/${encodeURIComponent(task.id)}/intents`;
  let finish = () => {};
  domain.advisorySteerInFlight = true;
  domain.advisorySteerCompletion = new Promise(resolve => { finish = () => resolve(undefined); });
  try {
    const { intents } = await runtimeRequest(path);
    if (!valid()) throw Error("作者问题的原执行已结束");
    const prior = intents.find(intent => {
      if (!intent.authorText.startsWith(prefix)) return false;
      try { const value = JSON.parse(intent.authorText.slice(prefix.length));
        return value.kind === payload.kind && JSON.stringify(value.binding) === JSON.stringify(binding);
      } catch { return false; }
    });
    if (prior && prior.authorText !== authorText) throw Error("原问题已有持久回答，请核对原回答后继续");
    const pending = prior ?? await runtimeRequest(path.replace(/\/intents$/, "/answers"), { method: "POST", body: JSON.stringify({
      expectedIntentRevision: intents.at(-1)?.intentRevision ?? 0, authorText }) });
    if (pending.taskId !== task.id || pending.authorText !== authorText
      || !Number.isSafeInteger(pending.intentRevision) || !["recorded", "delivered"].includes(pending.status)) throw Error("作者回答保存回执不匹配");
    if (!valid()) throw Error("作者回答已保存，原执行已变化；尚未向引擎投递");
    adapter.resolveUserInput(target.requestId, response, { retain: true });
    const delivered = pending.status === "delivered" ? pending : await runtimeRequest(`${path}/${pending.intentRevision}/delivered`, { method: "POST", body: "{}" });
    if (delivered.taskId !== task.id || delivered.authorText !== authorText || delivered.intentRevision !== pending.intentRevision
      || delivered.authorMessageId !== pending.authorMessageId || delivered.status !== "delivered") throw Error("作者回答已保存，投递回执尚未核实");
    adapter.acknowledgeUserInput(target.requestId, prepared);
    return { resolved: true };
  } finally {
    domain.advisorySteerInFlight = false;
    domain.advisorySteerCompletion = null;
    finish();
  }
}
