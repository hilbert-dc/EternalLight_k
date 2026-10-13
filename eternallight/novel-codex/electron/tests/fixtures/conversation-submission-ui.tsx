import { novelFixtureMedia, fixtureCreatedNovel } from "../helpers/novel-edition";
import { emptyCreationIssueSummary } from "../helpers/creation-issues";
import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import { saveAuthor } from "../helpers/author-session";

window.addEventListener("error", event => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
const tick = () => new Promise(resolve => setTimeout(resolve, 30));
const check = (condition, message) => { if (!condition) throw new Error(message); };
const until = async predicate => { for (let i = 0; i < 80 && !predicate(); i++) await tick(); check(predicate(), "UI state did not settle"); };
const currentTurn = () => document.querySelector<HTMLElement>(".agent-transcript > .conversation-turn:not(.is-earlier)");
const currentTurnOwns = (turnId: string) => currentTurn()?.querySelector(".conversation-request")?.getAttribute("data-conversation-detail") === `turn:ui-thread:${turnId}:prompt`;
const currentTurnError = () => currentTurn()?.querySelector(".transcript-error");
const project = { id: "project-last-lamp", title: "对话恢复夹具", logline: "测试作品", updated_at: "" };
const state = { project, canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1.0", scenes: [], anchors: [] }, novelEdition: { chapters: [] } };
let availableProjects = [project], returnedState = state;
saveAuthor({ name: "夹具作者", email: "fixture@example.invalid", passwordSalt: "fixture", passwordHash: "fixture" });
let configured = true, release, releaseTurn, releaseSteer, releaseRead, eventListener, inputListener, releaseAnswer;
const answerCalls = [];
let failReads = false, holdRead = false, stateReads = 0, workspaceReads = 0, planningSaves = 0;
const workspace = { project, revision: 1, headSnapshotId: "ui-snapshot", sourceDigest: "a".repeat(64), index: { status: "ready" },
  artifacts: Object.fromEntries(["brief", "outline", "plan", "identity"].map(kind => [kind, { candidate: null, accepted: null, locked: false }])),
  chapters: [], workingBaselines: [], continuationIntents: [], planningReviews: [], commits: [], tasks: [], modeling: [], factProposals: [], reviews: [], chapterPlanBindings: [] };
const calls = [], steers = [], historyProjects = [];
let workTasks = [];
const setInput = async value => {
  const input = document.querySelector('textarea[aria-label="创作指令"]');
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(input, value);
  input.dispatchEvent(new Event("input", { bubbles: true }));
  await tick();
  return input;
};
const finishTurn = (id, text) => {
  eventListener?.({ method: "item/completed", params: { threadId: "ui-thread", turnId: id, item: { type: "agentMessage", id: `${id}-reply`, text, phase: "final_answer" } } });
  eventListener?.({ method: "turn/completed", params: { threadId: "ui-thread", turn: { id, status: "completed" } } });
};
const api = { multimedia: novelFixtureMedia,
  app: { onCloseRequested: () => () => {} },
  runtime: { listChapterStructureChanges: async (projectId: string) => ({ projectId, items: [], nextCursor: null }),
  getWorkCatalog: async () => ({ revision: 0, volumes: [], placements: [] }), getIdentityCatalog: async () => ({ entries: [], ambiguousNames: [], identityRevisionId: null, identityDecisions: [], truncated: false, ambiguityScanComplete: true }), status: async () => ({ state: "online", mode: "local", error: null }), listProjects: async () => ({ projects: availableProjects }), getState: async () => {
    stateReads++;
    if (holdRead) await new Promise(resolve => { releaseRead = resolve; });
    if (failReads) throw new Error("fixture workspace read interrupted");
    return returnedState;
  },
    createProject: async (title, logline, medium, key) => {
      check(medium === "novel" && typeof key === "string", "Project medium or idempotency identity missing");
      const created = { ...project, id: "first-work", title, logline };
      availableProjects = [created]; returnedState = { ...state, project: created };
      return fixtureCreatedNovel(created);
    },
    getCreativeWorkspace: async () => { workspaceReads++; return structuredClone({ ...workspace, project: returnedState.project }); },
    saveArtifact: async (projectId, kind, payload, expectedRevision, expectedCandidate) => {
      check(projectId === project.id && kind === "brief" && expectedRevision === workspace.revision && expectedCandidate === (workspace.artifacts.brief.candidate?.id ?? null), "Planning save lost source identity");
      planningSaves++; const revision = { id: `brief-${planningSaves}`, payload: structuredClone(payload), version: planningSaves, source: "author", parentRevisionId: null, baseProjectRevision: 1, createdAt: null };
      workspace.artifacts.brief.candidate = revision;
      return { artifactKind: kind, revisionId: revision.id, version: revision.version, payload: revision.payload, linkedImpactIds: [] };
    },
    listPlanningImpacts: async () => ({ data: [], nextCursor: null, counts: { open: 0 } }),
    listGenerationRuns: async () => ({ runs: [] }),
    listWorkTasks: async () => ({ tasks: structuredClone(workTasks), activeCount: 0, nextCursor: null }),
    ...Object.fromEntries(["Search", "Entity", "Relation", "Knowledge", "SnapshotKnowledge", "Fact"].map(kind => [`get${kind}Audit`, async () => ({ job: null })])),
  },
  agent: {
    status: async () => ({ configured, state: "ready", mode: "novel-agent-engine", error: null, provider: "fixture", model: "fixture-model" }),
    listConnections: async () => ({ connections: [] }), recoveryReport: async () => [], listExecutions: async () => [], readMessage: async () => { throw Error("message receipt not found"); }, beginInterlude: async () => { throw Error("unexpected interlude"); }, returnInterlude: async () => { throw Error("unexpected interlude return"); }, listMessages: async () => [], listThreads: async projectId => {
      historyProjects.push(projectId); return { threads: [], nextCursor: null };
    },
    readConversation: async (projectId, id) => ({ thread: { id, projectId, name: "讨论", status: "idle" }, turns: [] }),
    listProposedActions: async () => ({ actions: [] }), setWorkspaceContext: async () => {},
    onUserInputRequest: listener => { inputListener = listener; return () => { inputListener = null; }; },
    resolveUserInput: async (requestId, response) => {
      answerCalls.push({ requestId, response: structuredClone(response) });
      await new Promise((resolve, reject) => { releaseAnswer = { resolve, reject }; });
      eventListener?.({ method: "serverRequest/resolved", params: { requestId: requestId.requestId, projectId: requestId.projectId, threadId: requestId.threadId, turnId: requestId.turnId } });
    },
    onEvent: listener => { eventListener = listener; return () => { eventListener = null; }; },
    steer: async (projectId, prompt) => {
      steers.push({ projectId, prompt });
      await new Promise(resolve => { releaseSteer = resolve; });
      return steers.length === 1 ? { steered: false, retained: true, messageId: "retained-steer" } : { steered: true };
    },
    sendPrompt: async (projectId, prompt) => {
      calls.push({ projectId, prompt });
      if (calls.length === 1) { await new Promise(resolve => { release = resolve; }); throw new Error("fixture connection interrupted"); }
      const id = `turn-${calls.length}`;
      eventListener?.({ method: "turn/started", params: { threadId: "ui-thread", turn: { id } } });
      if (calls.length === 3) await new Promise(resolve => { releaseTurn = resolve; });
      const text = calls.length === 2 ? "重试已恢复" : "追加轮次已完成";
      finishTurn(id, text);
      return { message: text, proposedActions: [] };
    },
  },
};
window.novelCodex = api;
localStorage.setItem(`novel-codex:selected-conversation:${JSON.stringify(["fixture@example.invalid", project.id])}`, "ui-thread");
let root = createRoot(document.getElementById("root"));
  workTasks = [{ id: "conversation-record", projectId: project.id, threadId: "ui-thread", goal: "已保存的创作结果", status: "completed", deliveryStatus: "ready_for_review", deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], evidence: [], pendingActionIds: [], unsettledAttemptIds: [] }, resumeBlockReason: null, resumeMode: null, steps: [], version: 1 }];
root.render(<App />);
(async () => {
try {
  // Wait for both bounded initial loads, so their scheduled read cannot consume
  // a controlled completion intended for the conversation refresh.
  await until(() => stateReads >= 2 && document.querySelector('textarea[aria-label="创作指令"]')?.disabled === false);
  [...document.querySelectorAll(".nav-item")].find(node => node.textContent.includes("作品概况")).click();
  await until(() => document.querySelector(".conversation-rail-scroll .work-task-panel"));
  const record = document.querySelector(".conversation-rail-scroll .work-task-panel");
  check(record.tagName === "SECTION" && !document.querySelector(".main-panel .work-task-panel"), "App conversation displaced the author workspace");
  check(document.querySelector(".conversation-rail-scroll").innerText.includes("已保存的创作结果"), "Conversation record could not be read");
  check(record.scrollWidth <= record.clientWidth + 1, "Conversation record overflowed its narrow pane");
  const taskPlacementSnapshot = { location: "右侧对话栏", presentation: "作者要求与执行过程", mainPanelHasTasks: Boolean(document.querySelector(".main-panel .work-task-panel")), resultVisible: record.innerText.includes("已保存的创作结果") };
  workTasks = [];
  root.unmount(); await tick(); root = createRoot(document.getElementById("root")); root.render(<App />);
  await until(() => document.querySelector('textarea[aria-label="创作指令"]')?.disabled === false);
  [...document.querySelectorAll(".nav-item")].find(node => node.textContent.includes("角色")).click();
  await until(() => !document.querySelector(".work-task-panel"));
  const input = document.querySelector('textarea[aria-label="创作指令"]');
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(input, "分析这个故事");
  input.dispatchEvent(new Event("input", { bubbles: true }));
  await tick();
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await tick();
  check(calls.length === 1, "rapid Enter produced duplicate root submissions");
  release();
  await until(() => document.querySelector(".composer-retry"));
  const failedTurnIdentity = currentTurn()?.querySelector(".conversation-timeline")?.getAttribute("data-conversation-detail");
  check(currentTurn()?.classList.contains("is-error") && currentTurnError() && failedTurnIdentity,
    "Retry control must belong to the current failed root turn");
  const retryInput = document.querySelector('textarea[aria-label="创作指令"]');
  check(!retryInput.disabled, "transient failure disabled configured model's composer");
  check(!retryInput.placeholder.includes("配置"), "transient failure is mislabeled as missing configuration");
  const retrySnapshot = { error: currentTurnError().querySelector("span").textContent, inputDisabled: retryInput.disabled,
    placeholder: retryInput.placeholder, retry: document.querySelector(".composer-retry").getAttribute("aria-label") };
  await setInput("尚未发送的新草稿");
  failReads = true;
  document.querySelector(".composer-retry").click();
  await until(() => document.querySelector(".discussion-sync-notice"));
  check(document.querySelector(".conversation-answer")?.textContent.includes("重试已恢复"), "read failure lost the successful reply");
  check(calls.length === 2, "retry failed to reach the configured model exactly once");
  check(calls[1].prompt === calls[0].prompt, "retry lost the author's original prompt");
  check(retryInput.value === "尚未发送的新草稿", "retry discarded an unrelated unsent draft");
  check(currentTurnOwns("turn-2") && currentTurn()?.classList.contains("is-completed")
    && !currentTurnError() && !document.querySelector(".composer-retry"), "Workspace read failure mislabels the current successful root turn or offers a paid model retry");
  const historicFailure = [...document.querySelectorAll<HTMLElement>(".conversation-turn.is-earlier .conversation-timeline")]
    .find(node => node.getAttribute("data-conversation-detail") === failedTurnIdentity);
  check(historicFailure?.classList.contains("is-error") && historicFailure.querySelector(".transcript-error")
    && historicFailure.querySelector("pre")?.textContent === "fixture connection interrupted",
    "The exact prior root failure must remain inspectable without becoming the current error");
  check(document.querySelector(".save-state").textContent === "工作区待同步", "failed refresh falsely claims the workspace is synchronized");
  const syncSnapshot = { message: document.querySelector(".discussion-sync-notice p").textContent,
    action: document.querySelector(".discussion-sync-notice button").textContent, draft: retryInput.value, modelRetry: false };
  holdRead = true;
  const beforeRead = stateReads;
  const refresh = document.querySelector(".discussion-sync-notice button");
  refresh.click(); refresh.click(); await tick();
  check(stateReads === beforeRead + 1, "rapid refresh started duplicate reads");
  releaseRead();
  await until(() => document.querySelector(".discussion-sync-notice button")?.disabled === false);
  check(currentTurnOwns("turn-2") && currentTurn()?.classList.contains("is-completed") && !currentTurnError()
    && !document.querySelector(".composer-retry") && calls.length === 2, "Failed local refresh changed or reran the current model execution");
  holdRead = false; failReads = false;
  document.querySelector(".discussion-sync-notice button").click();
  await until(() => !document.querySelector(".discussion-sync-notice"));
  check(document.querySelector(".save-state").textContent === "工作区已同步", "successful local refresh left a stale sync warning");
  check(retryInput.value === "尚未发送的新草稿" && calls.length === 2, "local refresh lost draft or invoked model");
  const next = await setInput("开始下一轮讨论");
  next.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", shiftKey: true, bubbles: true }));
  next.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", isComposing: true, bubbles: true }));
  await tick();
  check(calls.length === 2, "newline or input-method confirmation submitted a prompt");
  next.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await until(() => releaseTurn);
  const longQuestion = "请考虑全书结构与人物动机。".repeat(30) + "末尾限定：仅讨论，不改正文。";
  const longSuggestion = "保留原有线索并比较两种人物动机。".repeat(25) + "最后再由作者决定。";
  const question = { requestId: "question-ui", projectId: project.id, executionId: "execution-ui", method: "item/tool/requestUserInput", threadId: "ui-thread", turnId: "turn-3",
    params: { questions: [{ id: "direction", question: longQuestion, options: [{ label: longSuggestion, description: "建议可以继续编辑。".repeat(40) }] }] } };
  inputListener(question);
  eventListener({ id: question.requestId, method: question.method, params: { ...question.params, threadId: question.threadId, turnId: question.turnId } });
  await until(() => document.querySelector(".user-input-card"));
  check(document.querySelector(".approval-question p").textContent === longQuestion, "App truncated the question qualifier");
  document.querySelector<HTMLButtonElement>(".author-question-option").click(); await tick();
  check(document.querySelector<HTMLTextAreaElement>(".approval-input").value === longSuggestion, "App truncated the suggested answer");
  check(document.querySelector(".author-question-option small").textContent === "建议可以继续编辑。".repeat(40), "App truncated the option description");
  // App side effects obey the same turn identity as the transcript reducer.
  eventListener({ method: "agent/error", params: { threadId: "ui-thread", turnId: "turn-2", message: "STALE_ERROR_MUST_NOT_CLEAR_QUESTION" } });
  inputListener({ ...question, requestId: "stale-question", turnId: "turn-2" });
  eventListener({ method: "serverRequest/resolved", params: { threadId: "ui-thread", turnId: "turn-2", requestId: question.requestId } });
  await tick();
  check(document.querySelector(".user-input-card") && currentTurnOwns("turn-3") && currentTurn()?.classList.contains("is-waiting")
    && !currentTurnError() && !document.body.textContent.includes("之后还有"), "Stale event mutated the current root turn, question queue or transcript");
  const waitingDraft = await setInput("等回答提交后再讨论人物动机");
  check(!waitingDraft.disabled && document.querySelector(".composer-draft-notice")?.textContent.includes("本轮"), "waiting interaction blocked drafting or hid its reason");
  waitingDraft.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await tick();
  check(calls.length === 3 && steers.length === 0 && waitingDraft.value === "等回答提交后再讨论人物动机", "waiting draft was submitted or consumed as an answer");
  const answer = document.querySelector('.user-input-card textarea');
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(answer, "从人物误解开始");
  answer.dispatchEvent(new Event("input", { bubbles: true })); await tick();
  eventListener({ method: "structure/error", params: { projectId: "another-project", message: "STALE_BACKGROUND_OTHER_PROJECT" } });
  await tick();
  check(!document.querySelector('.chapter-structure-notice'), "another project's structural check error leaked into the current workspace");
  eventListener({ method: "structure/error", params: { projectId: project.id, message: "fixture structure check interrupted" } });
  await until(() => document.querySelector('.chapter-structure-notice'));
  check(document.querySelector(".user-input-card") && currentTurnOwns("turn-3") && currentTurn()?.classList.contains("is-waiting")
    && !currentTurnError() && document.querySelector(".composer-waiting") && answer.value === "从人物误解开始" && waitingDraft.value === "等回答提交后再讨论人物动机", "Background error changed the active root turn, question, answer or draft");
  check(calls.length === 3 && steers.length === 0 && answerCalls.length === 0, "background error invoked or answered a model task");
  const backgroundSnapshot = { message: document.querySelector('.chapter-structure-notice').textContent,
    waiting: Boolean(document.querySelector(".composer-waiting")), answer: answer.value, draft: waitingDraft.value, modelCalls: calls.length };
  eventListener({ method: "agent/structure-updated", params: { projectId: "another-project", changeId: null, completed: true } }); await tick();
  check(document.querySelector('.chapter-structure-notice'), "another project's success cleared the current structural check notice");
  eventListener({ method: "agent/structure-updated", params: { projectId: project.id, changeId: null, completed: true } });
  await until(() => !document.querySelector('.chapter-structure-notice'));
  const answerButton = document.querySelector('.user-input-card .approval-card-actions button');
  answerButton.click(); answerButton.click(); await tick();
  check(answerCalls.length === 1 && answerButton.disabled && answer.value === "从人物误解开始", "answer acknowledgment was discarded or rapid clicks duplicated submission");
  check(document.querySelector('.user-input-card fieldset').disabled, "pending answer controls remained mutable");
  releaseAnswer.reject(new Error("fixture answer delivery interrupted"));
  await until(() => document.querySelector('.user-input-error'));
  check(document.querySelector('.user-input-error').textContent === 'fixture answer delivery interrupted', 'App swallowed the answer rejection reason');
  check(answer.value === "从人物误解开始" && !answerButton.disabled && waitingDraft.value === "等回答提交后再讨论人物动机", "failed answer lost author text or blocked retry");
  const interactionSnapshot = { message: document.querySelector(".composer-draft-notice").textContent,
    draft: waitingDraft.value, answer: answer.value, error: document.querySelector('.user-input-error').textContent, answerCalls: answerCalls.length };
  answerButton.click(); await tick();
  check(answerCalls.length === 2 && answerCalls[1].response.answers.direction === "从人物误解开始", "answer retry lost the original answer");
  releaseAnswer.resolve();
  await until(() => !document.querySelector(".user-input-card") && !document.querySelector(".composer-draft-notice"));
  check(waitingDraft.value === "等回答提交后再讨论人物动机" && calls.length === 3 && steers.length === 0, "resolving interaction automatically sent or erased the independent draft");
  const steerInput = await setInput("增加人物动机分析");
  steerInput.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  steerInput.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await until(() => releaseSteer);
  check(steers.length === 1 && calls.length === 3, "rapid Enter duplicated steer or started a root turn");
  await setInput("追加等待期间新写的草稿");
  releaseSteer(); await tick();
  check(steerInput.value === "追加等待期间新写的草稿", "retained steer discarded subsequent input");
  check(document.body.textContent.includes("追加要求尚未发送，已保留"), "retained input was presented as delivered or lost");
  await setInput("结束前的追加指令");
  releaseSteer = null;
  steerInput.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await until(() => releaseSteer);
  check(steers.length === 2, "next explicit steer was blocked after the first acknowledgment");
  await setInput("迟到追加回执期间的新草稿");
  holdRead = true;
  releaseRead = null;
  releaseTurn();
  await until(() => [...document.querySelectorAll(".conversation-answer")].some(answer => answer.textContent.includes("追加轮次已完成")));
  releaseSteer(); await tick();
  check(steerInput.value === "迟到追加回执期间的新草稿", "late steer acknowledgment discarded subsequent input");
  check(!steerInput.placeholder.includes("追加指令"), "late steer acknowledgment resurrected a completed turn");
  await until(() => releaseRead);
  [...document.querySelectorAll(".nav-item")].find(button => button.textContent.includes("作品概况")).click();
  await until(() => document.querySelector(".artifact-editor-grid textarea"));
  document.querySelector(".overview-direction-editor > summary").click(); await tick();
  check(document.querySelector(".artifact-editor-grid textarea").checkVisibility(), "Direction editor was not revealed before editing");
  const premise = document.querySelector(".artifact-editor-grid textarea");
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(premise, "读取等待期间修改的故事命题");
  premise.dispatchEvent(new Event("input", { bubbles: true }));
  await until(() => document.body.textContent.includes("创作修改未保存"));
  const beforeWorkspaceReads = workspaceReads;
  releaseRead();
  await until(() => document.querySelector(".discussion-sync-notice"));
  check(workspaceReads === beforeWorkspaceReads, "late formal read triggered a dirty workspace refresh");
  check(premise.value === "读取等待期间修改的故事命题", "late formal read discarded editor input");
  const dirtyDraft = await setInput("保存后再分析最新的故事命题");
  check(!dirtyDraft.disabled && document.querySelector(".composer-draft-notice")?.textContent.includes("发送时先保存规划"), "unsaved planning blocked drafting or hid automatic save semantics");
  check(!document.querySelector('button[aria-label="发送创作指令"]').disabled, "dirty planning could not be submitted through save-and-send");
  check(document.querySelector(".discussion-sync-notice button").disabled, "local refresh bypassed unsaved editor protection");
  check(document.querySelector(".discussion-sync-notice").getBoundingClientRect().top < premise.getBoundingClientRect().top, "sync warning is buried below the editor");
  const dirtySnapshot = { message: document.querySelector(".discussion-sync-notice p").textContent,
    actionDisabled: document.querySelector(".discussion-sync-notice button").disabled, premise: premise.value,
    draft: dirtyDraft.value, reason: document.querySelector(".composer-draft-notice").textContent };
  holdRead = false;
  const readsBeforeSaveSync = stateReads;
  [...document.querySelectorAll(".planning-save-actions button")].find(button => button.textContent === "保存").click();
  await until(() => !document.querySelector(".discussion-sync-notice"));
  await tick(); await tick();
  check(planningSaves === 1 && stateReads === readsBeforeSaveSync + 1 && calls.length === 3 && premise.value === "读取等待期间修改的故事命题" && dirtyDraft.value === "保存后再分析最新的故事命题", "saving dirty planning did not perform exactly one readonly sync or consumed a draft: " + JSON.stringify({planningSaves,stateReads,readsBeforeSaveSync,calls:calls.length,premise:premise.value,draft:dirtyDraft.value}));
  dirtySnapshot.afterSave = { saves: planningSaves, readSyncs: stateReads - readsBeforeSaveSync, modelCalls: calls.length, draft: dirtyDraft.value };
  // A second completed reply is held while the author edits, then discarding
  // also releases exactly one deferred readonly refresh without another turn.
  holdRead = true; releaseRead = null;
  await setInput("只讨论当前简报，不修改作品");
  dirtyDraft.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await until(() => calls.length === 4 && releaseRead);
  await setInput("放弃修改后仍保留的下一条要求");
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(premise, "只在本地存在，稍后放弃的修改");
  premise.dispatchEvent(new Event("input", { bubbles: true })); await tick();
  releaseRead(); await until(() => document.querySelector(".discussion-sync-notice"));
  check(premise.value === "只在本地存在，稍后放弃的修改", "completed response replaced local edits");
  holdRead = false; const readsBeforeDiscardSync = stateReads;
  [...document.querySelectorAll(".nav-item")].find(button => button.textContent.includes("角色")).click();
  await until(() => document.querySelector(".unsaved-dialog"));
  [...document.querySelectorAll(".unsaved-dialog button")].find(button => button.textContent === "放弃修改并离开").click();
  await until(() => !document.querySelector(".discussion-sync-notice") && document.querySelector(".main-panel").dataset.view === "characters");
  await tick(); await tick();
  check(stateReads === readsBeforeDiscardSync + 1 && calls.length === 4 && planningSaves === 1 && dirtyDraft.value === "放弃修改后仍保留的下一条要求", "discarding did not release one readonly sync or caused another model/save");
  dirtySnapshot.afterDiscard = { readSyncs: stateReads - readsBeforeDiscardSync, modelCalls: calls.length, draft: dirtyDraft.value };
  const completedCalls = calls.length;
  root.unmount();
  configured = false;
  const missing = createRoot(document.getElementById("root"));
  missing.render(<App />);
  await until(() => document.querySelector(".composer-draft-notice")?.textContent.includes("模型"));
  const unavailable = document.querySelector('textarea[aria-label="创作指令"]');
  check(!unavailable.disabled, "missing configuration must allow drafting in an existing project");
  await setInput("等模型恢复后继续");
  unavailable.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await tick();
  check(unavailable.value === "等模型恢复后继续" && document.querySelector('button[aria-label="发送创作指令"]').disabled, "missing configuration must still block model sends without discarding the draft");
  check(calls.length === completedCalls, "missing configuration started a model request");
  configured = true;
  window.dispatchEvent(new Event("focus"));
  await until(() => !document.querySelector(".composer-draft-notice"));
  check(unavailable.value === "等模型恢复后继续" && !document.querySelector('button[aria-label="发送创作指令"]').disabled && calls.length === completedCalls, "model recovery erased or automatically submitted the draft");
  missing.unmount();
  configured = true; availableProjects = [];
  const readsBeforeEmpty = stateReads;
  const empty = createRoot(document.getElementById("root"));
  empty.render(<App />);
  await until(() => document.body.textContent.includes("开始你的第一部作品"));
  const emptyComposer = document.querySelector('textarea[aria-label="创作指令"]');
  check(emptyComposer.disabled && emptyComposer.placeholder.includes("创建或打开作品"), "empty workspace allows a model request without a project");
  check(stateReads === readsBeforeEmpty, "empty project library reads a nonexistent placeholder project");
  check(!historyProjects.includes(""), "empty project library reads unowned task history");
  const firstWorkSnapshot = { title: document.querySelector(".project-empty-state strong").textContent,
    composerDisabled: emptyComposer.disabled, placeholder: emptyComposer.placeholder };
  [...document.querySelectorAll("button")].find(button => button.textContent === "新建作品").click(); await tick();
  const title = document.querySelector(".project-dialog input");
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(title, "新建的第一部作品");
  title.dispatchEvent(new Event("input", { bubbles: true })); await tick();
  document.querySelector('.project-dialog button[type="submit"]').click();
  await until(() => document.querySelector('textarea[aria-label="创作指令"]')?.disabled === false);
  check(document.body.textContent.includes("新建的第一部作品") && calls.length === completedCalls, "first project creation failed or started model work");
  empty.unmount();
  localStorage.setItem("novel-codex-active-project", "removed-work");
  const stale = createRoot(document.getElementById("root"));
  stale.render(<App />);
  await until(() => localStorage.getItem("novel-codex-active-project") === "first-work");
  await until(() => document.querySelector('textarea[aria-label="创作指令"]')?.disabled === false);
  check(document.body.textContent.includes("新建的第一部作品"), "stale active selection prevented loading an existing project");
  stale.unmount();
  window.testResult = { passed: true, taskPlacementSnapshot, retrySnapshot, syncSnapshot, dirtySnapshot, firstWorkSnapshot, interactionSnapshot, backgroundSnapshot };
} catch (error) { window.testResult = { passed: false, error: error.stack ?? String(error) }; }
})();
