// Runs inside the production renderer through its real preload API. These are
// synthetic author inputs, not provider responses or a second agent loop.
export const creationIntakeCases = {
  intakeClarify: {
    prompt: '我想写一部长篇小说，帮我开始。我还没有想好题材、读者和故事方向。',
    answer: '写给成年读者的港城悬疑小说，第三人称限知，主角是抄写员林夕。全书12章，约6万字；密函引出失踪案，结局揭开寄信人身份但保留父亲下落的悬念。请先确定全书故事线与章节结构，这轮不写正文，不采用、不发布。其余可逆的创作选择请你决定。',
    rubric: '需要澄清决定整部作品方向的信息；收到回答后保留作者约束，推进全书设计，不写正文、不重复索取已回答内容。',
  },
  intakePlan: {
    prompt: '请为一部面向成年读者、第三人称限知的港城悬疑小说设计完整故事。全书12章、约6万字：抄写员林夕在雨夜收到密函，调查一宗失踪案；她从隐瞒证据保护自己转为承担揭露真相的代价。结局揭开寄信人身份，但保留父亲下落的悬念。请把全书主要故事线、人物变化、伏笔回收与12章结构保存为创作方案，给出后续写作安排。这轮只做全书方案，不写正文，不采用、不发布。其余可逆的创作选择由你决定，不必逐项征求确认。',
    answer: null,
    rubric: '已有足够约束，直接完成适当全书设计并保存；规划有故事线、人物变化、伏笔回收、12章结构，不擅写正文、不要求重复确认。',
  },
};

export async function runCreationIntake(name, specimen) {
  const api = window.novelCodex;
  const { project } = await api.runtime.createProject('创作准备验收 ' + name, '', 'novel', crypto.randomUUID());
  window.liveProjectId = project.id; window.liveSeed = null; window.liveApprovals = []; window.liveApprovalIds = new Set();
  const events = [], questions = [], answers = [];
  let requestId, settled = false, outcome, answerPending = Promise.resolve();
  const stopEvents = api.agent.onEvent(event => {
    events.push(event);
    if (event.method === 'agent/execution-request') requestId = event.params.requestId;
  });
  const stopQuestions = api.agent.onUserInputRequest(request => {
    questions.push(request);
    // Only the ambiguous case has an authored fixture response. Later questions
    // are preserved for review; no invented approval or default option.
    if (!specimen.answer || questions.length !== 1) return;
    answerPending = (async () => {
      const response = { answers: Object.fromEntries((request.params?.questions ?? []).map(q => [q.id, specimen.answer])) };
      const receipt = await api.agent.resolveUserInput({ projectId: project.id, executionId: request.executionId, requestId: request.requestId, threadId: request.threadId ?? request.params?.threadId, turnId: request.turnId ?? request.params?.turnId }, response);
      answers.push({ response, receipt });
    })().catch(error => { answers.push({ error: String(error?.message ?? error) }); });
  });
  const before = await api.runtime.getCreativeWorkspace(project.id);
  const pending = api.agent.sendPrompt(project.id, specimen.prompt).then(
    value => { outcome = { ok: true, value }; }, error => { outcome = { ok: false, error: String(error?.message ?? error) }; },
  ).finally(() => { settled = true; });
  try {
    const end = Date.now() + 600000;
    while (!settled && Date.now() < end && !(questions.length && (!specimen.answer || questions.length > 1))) {
      await new Promise(resolve => setTimeout(resolve, 100));
    }
    await answerPending;
    if (!settled && requestId) await api.agent.interrupt({ projectId: project.id, requestId });
    await pending;
    return { status: 'needsReview', specimen, before, outcome, questions, answers,
      after: await api.runtime.getCreativeWorkspace(project.id), works: await api.runtime.listWorkTasks(project.id),
      events: events.filter(e => ['item/completed', 'agent/user-input-requested', 'turn/completed'].includes(e.method)),
      limitation: 'Real model behavior sample; requires semantic review and Native output evidence. No automatic literary pass.' };
  } finally {
    if (!settled && requestId) await api.agent.interrupt({ projectId: project.id, requestId }).catch(() => {});
    stopEvents(); stopQuestions();
  }
}

export function intakeObservationFailed(outcome) {
  return outcome?.ok === false || Boolean(outcome?.value?.executionFailure);
}
