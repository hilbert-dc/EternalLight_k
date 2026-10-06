import React from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import "../../../src/styles.css";
import { saveAuthor } from "../helpers/author-session";
import { chapterRuntimeReads } from "../helpers/chapter-runtime";

let renderError: unknown = null;
window.addEventListener("error", event => { renderError ??= event.error ?? Error(event.message); });
window.addEventListener("unhandledrejection", event => { renderError ??= event.reason; });
const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (condition, message) => { if (!condition) throw Error(message); };
const until = async (predicate, message) => { for (let i = 0; i < 200 && !predicate(); i++) { if (renderError) throw renderError; await tick(); } if (renderError) throw renderError; check(predicate(), message); };
const deferred = () => { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; };
const projects = [{ id: "project-last-lamp", title: "规则恢复作品", logline: "", updated_at: "" }, { id: "project-b", title: "另一作品", logline: "", updated_at: "" }];
const projectFor = id => projects.find(project => project.id === id);
const savedChapter = { id: "chapter-1", number: 1, title: "当前章", status: "draft", updated_at: "", summary: "", passages: [] };
const emptyState = id => ({ project: projectFor(id), canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1", scenes: [], anchors: [] }, novelEdition: { chapters: [savedChapter] } });
const task = () => ({ id: "work-rules-original", projectId: projects[0].id, threadId: "rules-thread", kind: "direct", goal: "为本作品拟定人物状态规则，仅保存候选", status: "blocked", version: 1,
  deliveryStatus: "partial", deliveryReport: { evidence: [], pendingActionIds: [], unsettledAttemptIds: [] }, resumeMode: "continueExecution", resumeBlockReason: null,
  steps: [{ id: "rules-step", operation: "generatePropertyRules", status: "blocked", target: {}, attempts: [{ id: "original-rules", threadId: "rules-thread", turnId: "original-turn", status: "failed", error: "曾失败", startedAt: 1, completedAt: 2 }], changes: [], currentAttemptId: null, readingProgress: null, outputRefs: [] }] });
let listener, wrongWork = false;
let work = task(), generationRun = null, configured = true, resumeGate = deferred(), root;
const calls = { resume: [], cancel: [], recover: [], generation: [], send: 0, models: 0 };
window.novelCodex = {
  app: { onCloseRequested: () => () => {} },
  runtime: { ...chapterRuntimeReads(projectFor),
    getCreativeWorkspace: async id => ({ ...await chapterRuntimeReads(projectFor).getCreativeWorkspace(id), chapters: [{ ...savedChapter, candidate: null, accepted: null, working: null, workingSelection: null }] }), status: async () => ({ state: "online", mode: "local", error: null }), listProjects: async () => ({ projects }),
    getState: async id => structuredClone(emptyState(id)),
    getWriterPreview: async () => ({ target: { position: { readingChapter: 1, sceneOrder: 1 } }, plannedEffects: [], relevantText: [], coverage: { complete: true, missingScope: [], truncationReasons: [] } }),
    listGenerationRuns: async () => ({ runs: generationRun ? [structuredClone(generationRun)] : [] }),
    getTaskFinalReply: async () => ({ state: "absent" }),
    getWorkTask: async () => ({ ...structuredClone(work), projectId: wrongWork ? projects[1].id : work.projectId }),
    listWorkTasks: async id => ({ tasks: id === projects[0].id ? [structuredClone(work)] : [], activeCount: id === projects[0].id && work.status === "blocked" ? 1 : 0, nextCursor: null }),
    getHistoricalKnowledgeState: async (id, snapshotId) => ({ projectId: id, snapshotId, entries: [], coverage: { complete: true, truncatedEntries: false, pendingSources: [] } }) },
  agent: { status: async () => ({ configured, state: configured ? "ready" : "offline", mode: "novel-agent-engine", provider: "fixture", model: "fixture", error: null }),
    listConnections: async () => ({ connections: [] }), recoveryReport: async () => [], listThreads: async () => ({ threads: [], nextCursor: null }), listProposedActions: async () => ({ actions: [] }),
    setWorkspaceContext: async () => {}, onEvent: fn => { listener = fn; return () => {}; }, onUserInputRequest: () => () => {},
    sendPrompt: async () => { calls.send++; throw Error("Recovery must not send a new author turn"); },
    generateChapter: async () => { calls.models++; throw Error("Unexpected generation"); },
    retryWork: async (projectId, id) => { calls.generation.push({ projectId, id }); return await resumeGate.promise; },
    resumeWorkTask: async (projectId, id) => { calls.resume.push({ projectId, id }); return await resumeGate.promise; },
    controlWorkTask: async (projectId, id, command) => { check(command === "cancel", "Unexpected control"); calls.cancel.push({ projectId, id, command }); work.status = "cancelled"; },
    recoverWorkTask: async (projectId, id) => { calls.recover.push({ projectId, id }); work.status = "completed"; work.deliveryStatus = "ready_for_review"; },
  },
};
const rail = () => document.querySelector<HTMLElement>(".conversation-rail");
const control = label => [...rail().querySelectorAll<HTMLButtonElement>("button")].find(node => node.textContent === label);
const navigate = async label => {
  const nav = [...document.querySelectorAll<HTMLButtonElement>(".nav-item")].find(node => node.textContent.includes(label === "世界规则" ? "世界与规则" : label));
  if (label === "作品目录") {
    const directory = document.querySelector<HTMLDetailsElement>(".sidebar .work-directory");
    if (!directory.open) directory.querySelector("summary").click();
    await until(() => document.querySelector('[data-directory-chapter-id="chapter-1"]'), "Chapter directory missing");
    document.querySelector<HTMLButtonElement>('[data-directory-chapter-id="chapter-1"]').click();
  } else { check(nav && !nav.disabled, `Navigation unavailable: ${label}`); nav.click(); }
  await until(() => document.querySelector<HTMLElement>(".main-panel")?.dataset.view === ({ "世界规则": "world", "角色": "characters", "作品目录": "chapters" })[label], `Page did not open: ${label}`);
  await tick(); await tick();
};
const mount = async (projectId = projects[0].id) => {
  if (root) { root.unmount(); await tick(); }
  localStorage.setItem("novel-codex-active-project", projectId);
  root = createRoot(document.getElementById("root")); root.render(<App />);
  await until(() => document.querySelector(".project-switcher")?.textContent.includes(projectFor(projectId).title) && (projectId === projects[0].id ? control("继续任务") || control("核验并恢复") : document.querySelector(".creative-workspace-view")), "App task reader did not initialize");
  await tick(); await tick();
};
const snapshots = [];
(async () => { try {
  localStorage.clear(); saveAuthor({ name: "恢复作者", email: "fixture@example.invalid", passwordSalt: "fixture", passwordHash: "fixture" });
  await mount(); await navigate("世界规则");
  check(control("继续任务") && !control("继续任务").disabled, "World task cannot resume");
  control("继续任务").click(); control("继续任务").click();
  await until(() => calls.resume.length === 1 && control("继续任务")?.disabled, "Resume did not become single flight");
  check(document.querySelector<HTMLButtonElement>(".project-switcher").disabled, "Pending task permits project switch");
  for (const page of ["角色", "世界规则"]) {
    await navigate(page);
    check(control("继续任务")?.disabled && !control("取消任务")?.disabled, `${page} lost the shared pending control`);
    control("继续任务").click();
    check(calls.resume.length === 1 && calls.send === 0, "Page navigation resumed or sent another turn");
  }
  control("取消任务").click(); control("取消任务").click();
  await until(() => calls.cancel.length === 1 && rail().textContent.includes("已确认取消任务"), "Cancellation did not finish");
  resumeGate.reject(Error("Cancelled original execution"));
  await until(() => !document.querySelector<HTMLButtonElement>(".project-switcher").disabled, "Cancelled resume left the shared controller locked");
  check(!rail().querySelector('[role="alert"]')?.textContent.includes("Cancelled original execution"), "Cancellation was reported as a new failure");
  await navigate("作品目录"); check(rail().innerText.includes(work.goal), "Chapter page lost the original recovery record after cancellation");
  snapshots.push({ case: "shared-pending-across-pages", pages: ["世界规则", "角色"], originalWork: calls.resume[0], resumeCalls: calls.resume.length, cancelCalls: calls.cancel.length, newTurns: calls.send, notice: "已确认取消任务。" });

  work = task(); resumeGate = deferred(); await mount(); await navigate("世界规则");
  control("继续任务").click(); await until(() => calls.resume.length === 2, "Second original recovery didn't start");
  resumeGate.reject(Error("原规则任务恢复失败，请核对已保存候选"));
  await until(() => [...rail().querySelectorAll('[role="alert"]')].some(node => node.textContent.includes("原规则任务恢复失败")), "Recovery error isn't visible in conversation rail");
  check(!control("继续任务").disabled, "Failed recovery cannot be retried");
  snapshots.push({ case: "visible-recovery-failure", error: [...rail().querySelectorAll('[role="alert"]')].find(node => node.textContent.includes("原规则任务恢复失败")).textContent, newTurns: calls.send });

  resumeGate = deferred(); control("继续任务").click(); await until(() => calls.resume.length === 3, "Owner-isolation request didn't start");
  const oldGate = resumeGate;
  // Normal project switching is locked while execution is pending. Replacing the
  // App owner exercises late settlement after teardown without bypassing that lock.
  await mount(projects[1].id); await navigate("世界规则");
  oldGate.reject(Error("旧作品迟到失败")); await tick(); await tick();
  check(!rail().textContent.includes("旧作品迟到失败") && !rail().textContent.includes(work.goal), "Late original-project receipt polluted another project");
  check(!document.querySelector<HTMLButtonElement>(".project-switcher").disabled, "Old owner relocked the new project");
  snapshots.push({ case: "late-owner-isolation", project: projects[1].title, oldErrorVisible: false, oldTaskVisible: false });


  work = task(); resumeGate = deferred(); await mount(); await navigate("世界规则");
  wrongWork = true; control("继续任务").click();
  await until(() => rail().innerText.includes("任务来源已变化"), "Wrong-project persisted Work was not rejected");
  check(calls.resume.length === 3 && calls.send === 0, "Invalid fresh Work resumed or created a new author turn");
  wrongWork = false;
  snapshots.push({ case: "fresh-work-mismatch", resumeCalls: calls.resume.length, newTurns: calls.send });
  const editor = document.querySelector<HTMLTextAreaElement>('[aria-label="创作指令"]');
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(editor, "恢复之后再讨论的草稿"); editor.dispatchEvent(new Event("input", { bubbles: true })); await tick();
  editor.setSelectionRange(2, 6);
  control("继续任务").click(); await until(() => calls.resume.length === 4, "Bound recovery did not start");
  const authorMessages = () => [...rail().querySelectorAll('.user-message .conversation-request-preview')].filter(node => node.textContent === work.goal);
  check(authorMessages().length === 1, "Recovery preparation invented a duplicate author message before a Native task existed");
  work.status = "paused";
  resumeGate.resolve({ ...structuredClone(work), resumeFailure: { code: "WORK_RESUME_NOT_STARTED", message: "Core 对话启动超时", workTaskId: work.id } });
  await until(() => rail().textContent.includes("原任务尚未启动：Core 对话启动超时") && !control("继续任务")?.disabled, "Host-proven pre-task failure lost the original recoverable Work");
  check(authorMessages().length === 1 && !rail().querySelector('[data-conversation-row^="turn:"]'), "A pre-task failure fabricated another conversation turn");
  check(document.querySelector('[aria-label="创作指令"]') === editor && editor.value === "恢复之后再讨论的草稿" && editor.selectionStart === 2 && editor.selectionEnd === 6, "Pre-task recovery failure discarded draft DOM or selection");
  check(calls.send === 0 && calls.models === 0, "Pre-task recovery failure resubmitted the goal or created a new root");
  snapshots.push({ case: "host-proven-resume-not-started", originalWork: work.id, durableStatus: work.status, duplicateAuthorMessages: false, newConversationTurn: false, error: "原任务尚未启动：Core 对话启动超时", draftPreserved: true, retryOriginalAvailable: true });
  resumeGate = deferred();
  control("继续任务").click(); await until(() => calls.resume.length === 5, "Original paused Work could not be resumed after a pre-task failure");
  const frozen = { workTaskId: work.id, originTaskId: "original-rules", sourceTaskId: "original-rules", rootThreadId: work.threadId, decisionStepId: "recovery-decision-step", decisionTaskId: "recovery-decision" };
  listener({ method: "agent/workflow", params: { workflow: { task: { id: frozen.decisionTaskId, projectId: projects[0].id, threadId: work.threadId, operation: "advisory", target: { workDecision: { workTaskId: work.id, originTaskId: frozen.originTaskId, sourceTaskId: frozen.sourceTaskId } }, inputManifest: { workDecision: frozen } }, executionBinding: { ...frozen, taskId: frozen.decisionTaskId, stepId: frozen.decisionStepId, actionKey: null } } } });
  listener({ method: "turn/started", params: { threadId: work.threadId, turn: { id: "recovery-turn" } } });
  listener({ method: "item/agentMessage/delta", params: { threadId: work.threadId, turnId: "recovery-turn", itemId: "reply", delta: "正在依据原任务检查规则候选。" } }); await tick();
  check(rail().innerText.includes("正在依据原任务检查规则候选。") && editor.value === "恢复之后再讨论的草稿", "Native-bound recovery did not stream into the same conversation or consumed draft");
  listener({ method: "turn/completed", params: { threadId: work.threadId, turn: { id: "recovery-turn", status: "completed", items: [] } } });
  work.status = "completed"; resumeGate.resolve(structuredClone(work)); await until(() => !document.querySelector<HTMLButtonElement>(".project-switcher").disabled, "Bound recovery stayed busy");
  snapshots.push({ case: "exact-native-recovery-stream", originalWork: work.id, rootThread: work.threadId, draftPreserved: editor.value, newTurns: calls.send });

  configured = false; work = { ...task(), resumeMode: "recoverSavedResult" }; await mount(); await navigate("世界规则");
  check(!control("核验并恢复").disabled, "Read-only saved-result recovery requires a model");
  control("核验并恢复").click(); control("核验并恢复").click();
  await until(() => calls.recover.length === 1 && rail().textContent.includes("已核验恢复保存的产物"), "Saved-result recovery did not finish");
  check(calls.resume.length === 5 && calls.send === 0 && calls.models === 0, "Saved-result recovery invoked model execution");
  check(!rail().querySelector(".work-task-changes button"), "Revisionless global task panel fabricated Undo");
  snapshots.push({ case: "saved-result-without-model", task: calls.recover[0], status: "结果已保存", modelCalls: calls.models, newTurns: calls.send });
  configured = true;
  work = { ...task(), id: "work-first-generation", kind: "generation", threadId: "", status: "paused", goal: "继续写两章", steps: [{ ...task().steps[0], operation: "advisory", attempts: [], currentAttemptId: null }] };
  resumeGate = deferred(); await mount(); await navigate("世界规则");
  const firstEditor = document.querySelector<HTMLTextAreaElement>('[aria-label="创作指令"]');
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(firstEditor, "下一轮未发送草稿"); firstEditor.dispatchEvent(new Event("input", { bubbles: true })); await tick();
  control("继续任务").click(); await until(() => calls.resume.length === 6, "Unstarted generation could not resume without inventing a thread");
  check(!rail().querySelector('[data-conversation-row^="turn:"]') && firstEditor.value === "下一轮未发送草稿", "An unstarted generation fabricated an author turn");
  resumeGate.resolve({ ...structuredClone(work), resumeFailure: { code: "WORK_RESUME_NOT_STARTED", message: "MCP 启动超时", workTaskId: work.id } });
  await until(() => rail().textContent.includes("原任务尚未启动：MCP 启动超时") && !control("继续任务")?.disabled, "Unstarted generation lost its original paused recovery");
  check(!rail().querySelector('[data-conversation-row^="turn:"]'), "No-task generation failure invented a turn");
  snapshots.push({ case: "unstarted-generation-host-proof", rootThread: "", authorTurnInvented: false, originalWork: work.id, failure: "原任务尚未启动：MCP 启动超时", draftPreserved: firstEditor.value });
  resumeGate = deferred(); control("继续任务").click(); await until(() => calls.resume.length === 7, "Original unstarted generation retry did not run");
  listener({ method: "agent/workflow", params: { workflow: { task: { id: "unbound-advisory", projectId: work.projectId, threadId: "forged-thread", operation: "advisory", target: {} } } } }); await tick();
  check(!rail().querySelector('[data-conversation-row^="turn:"]') && !rail().textContent.includes("forged-thread"), "A bare same-project workflow consumed the pending generation binding");
  const firstBinding = { workTaskId: work.id, originTaskId: null, sourceTaskId: null, rootThreadId: "first-real-root", decisionStepId: "first-step", decisionTaskId: "first-decision" };
  listener({ method: "agent/workflow", params: { workflow: { task: { id: firstBinding.decisionTaskId, projectId: work.projectId, threadId: firstBinding.rootThreadId, operation: "advisory", target: { workDecision: { workTaskId: work.id, originTaskId: null, sourceTaskId: null } }, inputManifest: { workDecision: firstBinding } }, executionBinding: { ...firstBinding, taskId: firstBinding.decisionTaskId, stepId: firstBinding.decisionStepId, actionKey: null } } } });
  listener({ method: "turn/started", params: { threadId: firstBinding.rootThreadId, turn: { id: "first-turn" } } });
  listener({ method: "item/agentMessage/delta", params: { threadId: firstBinding.rootThreadId, turnId: "first-turn", itemId: "first-answer", delta: "正在根据已批准范围安排两章。" } }); await tick();
  check(rail().textContent.includes("正在根据已批准范围安排两章。") && firstEditor.value === "下一轮未发送草稿", "First generation did not bind its actual Native thread");
  listener({ method: "turn/completed", params: { threadId: firstBinding.rootThreadId, turn: { id: "first-turn", status: "completed", items: [] } } });
  work.threadId = firstBinding.rootThreadId; work.status = "completed"; resumeGate.resolve(structuredClone(work)); await until(() => !document.querySelector<HTMLButtonElement>(".project-switcher").disabled, "First generation did not finish");
  snapshots.push({ case: "unstarted-generation-native-bind", actualRootThread: work.threadId, draftPreserved: firstEditor.value, newAuthorCalls: calls.send });

  work = { ...task(), id: "work-generation-envelope", kind: "generation", goal: "保留现稿继续两章", status: "paused" };
  generationRun = { id: "generation-envelope", status: "blocked", stage: "coordinate", blockedReason: "agent_task_failed", currentChapterId: null, currentChapterNumber: null, activeTaskId: null, pendingCurrentIntentGeneration: null, completedChapters: 0, startChapterNumber: 1, targetChapters: 2, feedbackRepairPrompt: null, maxTaskAttempts: null, usedTaskAttempts: 1, usageUnsettled: false, providerBudget: { maxRequests: null, maxTokens: null, usedRequests: 1, usedTokens: 10, unknownTokens: 0, overrun: false, charges: [] }, chapterDeliveries: [], revisionPredecessor: null };
  resumeGate = deferred(); await mount();
  const briefNav = [...document.querySelectorAll<HTMLButtonElement>('.nav-item')].find(node => node.textContent.includes("作品概况")); briefNav.click();
  await until(() => [...document.querySelectorAll<HTMLButtonElement>('button')].some(node => node.textContent === "重试这一步"), "Generation retry entry missing");
  const genEditor = document.querySelector<HTMLTextAreaElement>('[aria-label="创作指令"]'); const beforeRows = rail().querySelectorAll('[data-conversation-row^="turn:"]').length;
  [...document.querySelectorAll<HTMLButtonElement>('button')].find(node => node.textContent === "重试这一步").click(); await until(() => calls.generation.length === 1, "Generation retry did not dispatch original run");
  generationRun.status = "paused";
  resumeGate.resolve({ ...structuredClone(generationRun), resumeFailure: { code: "WORK_RESUME_NOT_STARTED", message: "Core 首回合未启动", workTaskId: work.id }, pausedWork: { id: work.id, projectId: work.projectId, threadId: work.threadId, status: "paused", version: work.version } });
  await until(() => rail().textContent.includes("原任务尚未启动：Core 首回合未启动") && !document.querySelector<HTMLButtonElement>(".project-switcher").disabled, "Generation envelope failed to restore recoverable state");
  check(rail().querySelectorAll('[data-conversation-row^="turn:"]').length === beforeRows && document.querySelector('[aria-label="创作指令"]') === genEditor && calls.send === 0, "Generation pre-task failure fabricated a duplicate turn or lost editor");
  snapshots.push({ case: "generation-entry-host-proof", originalRun: calls.generation[0], failure: "原任务尚未启动：Core 首回合未启动", newAuthorTurn: false, editorDomPreserved: true });
  window.testResult = { passed: true, calls, snapshots };
} catch(error) { window.testResult = { passed: false, error: error.stack, calls, dom: document.body.innerHTML }; } })();
