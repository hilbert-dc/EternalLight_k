import { novelFixtureMedia } from "../helpers/novel-edition";
import { chapterRuntimeReads } from "../helpers/chapter-runtime";
import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import { saveAuthor } from "../helpers/author-session";

window.addEventListener("error", event => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
const tick = () => new Promise(resolve => setTimeout(resolve, 25));
const check = (condition, message) => { if (!condition) throw new Error(message); };
const until = async predicate => { for (let i = 0; i < 150 && !predicate(); i++) await tick(); check(predicate(), "UI did not settle: " + String(predicate)); };
const project = { id: "project-last-lamp", title: "恢复作品 A", logline: "", updated_at: "" };
const other = { ...project, id: "project-b", title: "恢复作品 B" };
const runtimeState = project => ({ project, canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1.0", scenes: [], anchors: [] }, novelEdition: { chapters: [] } });
const thread = id => ({ id, projectId: project.id, name: id, preview: null, modelProvider: "fixture", status: "idle", createdAt: 100, updatedAt: 100, isPinned: false, ephemeral: false });
saveAuthor({ name: "恢复作者", email: "fixture@example.invalid", passwordSalt: "fixture", passwordHash: "fixture" });
const calls = [], reads = [], resumes = [], heldReads = new Map(), rechecks = [];
const recovery = (runId, owner, threadId) => ({ runId, planId: runId, projectId: owner.id, threadId, operation: "extractCommitments", runtimeStatus: "failed", reconciledAt: null,
  requests: [{ requestId: runId, turnId: null, kind: "write", operation: "submit_commitment_extraction", state: "unknown", receiptRef: null, hasIdempotencyKey: true, recovery: "manual-reconciliation" }] });
const recoveries = [recovery("old-A", project, "A"), recovery("current-B", project, "B"), recovery("other-project", other, "B")];
let stateReads = 0, taskReads = 0, failRecheck = false, clearRecheck = false, manualRecheck = false, holdDiscussion = false, discussionRelease, failState = false, listener, holdThread = false, failThread = false, holdResume, resumeRelease, holdState = false, stateRelease;
const finish = name => {
  const id = `turn-${calls.length}`;
  listener?.({ method: "turn/started", params: { threadId: "execution", turn: { id } } });
  listener?.({ method: "turn/completed", params: { threadId: "execution", turn: { id, status: "completed", items: [{ id: `${id}-answer`, type: "agentMessage", phase: "final_answer", text: `${name}结果已保存` }] } } });
};
const execute = async name => { calls.push(name);
  if (["continue", "retry", "repairDraft"].includes(name)) {
    const frozen = { workTaskId: "work-work", rootThreadId: "execution", originTaskId: null, sourceTaskId: null, decisionStepId: `decision-step-${calls.length}`, decisionTaskId: `decision-${calls.length}` };
    listener?.({ method: "agent/workflow", params: { workflow: { task: { id: frozen.decisionTaskId, projectId: project.id, threadId: frozen.rootThreadId, operation: "advisory",
      target: { workDecision: { workTaskId: frozen.workTaskId, originTaskId: null, sourceTaskId: null } }, inputManifest: { workDecision: frozen } },
      executionBinding: { ...frozen, taskId: frozen.decisionTaskId, stepId: frozen.decisionStepId, actionKey: null } } } });
  }
  finish(name); return { message: `${name}已保存`, status: "completed", run: { status: "completed", roles: [], reports: [] } }; };
const api = { multimedia: novelFixtureMedia,
  app: { onCloseRequested: () => () => {} },
  runtime: { getWorkTask: async (projectId, id) => ({ id, projectId, kind: "generation", threadId: "execution", goal: "继续作品", steps: [] }), ...chapterRuntimeReads(id => id === other.id ? other : project), listWorkTasks: async () => { taskReads++; return { tasks: [], nextCursor: null, activeCount: 0 }; },
    status: async () => ({ state: "online", mode: "local", error: null }),
    listProjects: async () => ({ projects: [project, other] }),
    getState: async id => { stateReads++; if (holdState) await new Promise(resolve => { stateRelease = resolve; }); if (failState) throw Error("controlled read failure"); return runtimeState(id === other.id ? other : project); },
    cancelPlanningReview: async () => { calls.push("stopReview"); return { reports: [], roles: [] }; },
  },
  agent: {
    status: async () => ({ configured: true, state: "ready", mode: "novel-agent-engine", error: null, provider: "fixture", model: "fixture" }),
    listConnections: async () => ({ connections: [] }), recoveryReport: async () => { if (manualRecheck) { rechecks.push(true); if (failRecheck) throw Error("controlled reconciliation failure"); if (clearRecheck) return structuredClone(recoveries.filter(run => run.runId !== "current-B")); } return structuredClone(recoveries); }, listProposedActions: async () => ({ actions: [] }), setWorkspaceContext: async () => {},
    listExecutions: async () => [], readMessage: async () => { throw Error("message receipt not found"); }, beginInterlude: async () => { throw Error("unexpected interlude"); }, returnInterlude: async () => { throw Error("unexpected interlude return"); }, listMessages: async () => [], listThreads: async () => ({ threads: [thread("A"), thread("B"), thread("empty"), thread("error")], nextCursor: null }),
    onEvent: fn => { listener = fn; return () => {}; }, onUserInputRequest: () => () => {},
    generateWork: async () => execute("work"),
    continueWork: async () => execute("continue"), retryWork: async () => execute("retry"),
    resolveGenerationFeedback: async (_id, _run, _revision, choice) => execute(choice),
    generateChapter: async () => execute("chapter"), resumeKnowledge: async () => execute("knowledge"), reviewPlanning: async () => execute("review"),
    sendPrompt: async (projectId, prompt, _repair, _focus, threadId) => {
      calls.push("discussion"); const id = `turn-${calls.length}`;
      listener?.({ method: "agent/workflow", params: { workflow: { task: { id: `task-${id}`, projectId, operation: "advisory", threadId } } } });
      listener?.({ method: "turn/started", params: { threadId, turn: { id } } });
      listener?.({ method: "turn/completed", params: { threadId, turn: { id, status: "completed", items: [{ id: `${id}-answer`, type: "agentMessage", phase: "final_answer", text: "discussion结果已保存" }] } } });
      if (holdDiscussion) await new Promise(resolve => { discussionRelease = resolve; });
      return { message: "discussion已保存", proposedActions: [] };
    },
    resumeThread: async (_project, id) => { resumes.push(id); if (holdResume === id) await new Promise(resolve => { resumeRelease = resolve; }); return { thread: thread(id) }; },
    readThread: async (projectId, id) => {
      reads.push({ projectId, id });
      if (holdThread) await new Promise(resolve => heldReads.set(id, resolve));
      if (failThread) throw Error("controlled history read failure");
      return { thread: thread(id), turns: id === "empty" ? [] : [{ id: `${id}-turn`, status: "completed", startedAt: null, completedAt: null, items: [
        { id: "user", type: "userMessage", phase: null, text: `${projectId}-${id}提问` },
        { id: "process", type: "agentMessage", phase: "commentary", text: `${id}读取过程` },
        { id: "answer", type: "agentMessage", phase: "final_answer", text: `${projectId}-${id}回答` },
        { id: "unphased", type: "agentMessage", phase: null, text: "NOT_THE_FINAL_ANSWER" },
      ] }, { id: `${id}-process-only`, status: "completed", startedAt: null, completedAt: null, items: [{ id: "only-process", type: "agentMessage", phase: "commentary", text: "只有公开过程记录" }] },
      ...["", null, "   "].map((text, index) => ({ id: `${id}-empty-final-${index}`, status: "completed", startedAt: null, completedAt: null, items: [
        { id: "earlier-final", type: "agentMessage", phase: "final_answer", text: "EARLIER_FINAL_MUST_NOT_SURVIVE" },
        { id: "empty-final", type: "agentMessage", phase: "final_answer", text },
        { id: "unphased-after-empty", type: "agentMessage", phase: null, text: "EMPTY_FINAL_MUST_NOT_FALL_BACK" },
      ] })),
      ] };
    },
  },
};
api.agent.readConversation = async (projectId, id) => { const result = await api.agent.readThread(projectId, id); return { ...result, thread: { ...result.thread, projectId } }; };
window.novelCodex = api;
const root = createRoot(document.getElementById("root"));
root.render(<App />);
const selectThread = id => document.querySelector(`.conversation-select-button[title="${id}"]`).click();
const rail = () => document.querySelector(".conversation-reading-area").textContent;
const button = text => [...document.querySelectorAll("button")].find(node => node.textContent === text);
(async () => {
  try {
    await until(() => stateReads >= 2 && document.querySelector('.conversation-select-button[title="A"]'));
    [...document.querySelectorAll(".nav-item")].find(node => node.textContent.includes("作品概况")).click();
    await until(() => document.querySelector('[data-execute="work"]'));
    const executionSnapshots = [];
    for (const name of ["work", "continue", "retry", "feedbackContinue", "feedbackRepair", "chapter", "review", "stopReview"]) {
      failState = true;
      window.executionError = null;
      const before = calls.length;
      document.querySelector(`[data-execute="${name}"]`).click();
      await until(() => document.querySelector('.discussion-sync-notice[aria-label="对话后工作区同步"]'));
      await tick();
      check(calls.length === before + 1, `${name}: duplicate execution`);
      check(!window.executionError, `${name}: refresh failure rejected successful callback`);
      check(!document.querySelector(".transcript-error"), `${name}: successful execution marked failed`);
      const expectedResult = { feedbackContinue: "continue", feedbackRepair: "repairDraft" }[name] ?? name;
      const retainedResult = name === "stopReview" || document.querySelector('.conversation-turn:not(.is-earlier) .conversation-answer')?.textContent.includes(`${expectedResult}结果已保存`);
      check(retainedResult, `${name}: latest successful result was not retained`);
      executionSnapshots.push({ operation: name, retainedResult, retry: button("重新读取工作区")?.textContent });
      failState = false;
      const beforeRetryReads = stateReads;
      button("重新读取工作区").click();
      await until(() => !document.querySelector('.discussion-sync-notice[aria-label="对话后工作区同步"]'));
      check(calls.length === before + 1, `${name}: readonly retry called model`);
      check(stateReads === beforeRetryReads + 1, `${name}: retry did not perform exactly one state read`);
    }
    // History is a read-only detail, independent of the latest execution and composer.
    holdThread = true; selectThread("A"); await until(() => heldReads.has("A"));
    check(rail().includes("正在读取对话"), "Selected history lacked loading state");
    selectThread("B"); await until(() => heldReads.has("B"));
    heldReads.get("B")(); await until(() => document.querySelector(".conversation-reader")?.textContent.includes("project-last-lamp-B回答"));
    heldReads.get("A")(); await tick();
    check(!document.querySelector(".conversation-reader").textContent.includes("project-last-lamp-A回答") && resumes.length === 0, "Late history read changed selected detail or resumed host");
    holdThread = false; selectThread("empty"); await until(() => document.querySelector(".conversation-empty"));
    failThread = true; selectThread("error"); await until(() => button("重新读取"));
    const executions = calls.length; failThread = false; button("重新读取").click();
    await until(() => document.querySelector(".conversation-reader")?.textContent.includes("project-last-lamp-error回答"));
    check(resumes.length === 0 && calls.length === executions, "History retry changed execution");
    heldReads.delete("A"); holdThread = true; selectThread("A"); await until(() => heldReads.has("A"));
    document.querySelector(".project-switcher").click(); await tick();
    [...document.querySelectorAll(".project-menu-item")].find(node => node.textContent.includes(other.title)).click();
    await until(() => document.querySelector(".project-switcher span[title]")?.textContent === other.title);
    heldReads.get("A")(); await tick();
    check(!rail().includes("project-last-lamp"), "Late project A history leaked into B");
    holdThread = false;
    const recoveryPanel = () => document.querySelector<HTMLDialogElement>(".conversation-recovery");
    const openRecovery = async () => {
      document.querySelector<HTMLButtonElement>('[aria-label="对话操作"]').click(); await tick();
      document.querySelector<HTMLButtonElement>('[aria-label="保存记录"]').click();
      await until(() => recoveryPanel()?.open);
    };
    const closeRecovery = async () => { button("关闭保存记录").click(); await tick(); };
    check(!recoveryPanel(), "Historical save audit occupied the default conversation");
    await openRecovery();
    check(recoveryPanel().querySelector('[data-recovery-run="other-project"]') && !recoveryPanel().querySelector('[data-recovery-run="old-A"]'), "Recovery records leaked across projects");
    await closeRecovery();
    document.querySelector(".project-switcher").click(); await tick();
    [...document.querySelectorAll(".project-menu-item")].find(node => node.textContent.includes(project.title)).click();
    await until(() => document.querySelector(".project-switcher span[title]")?.textContent === project.title);
    check(!recoveryPanel() && !document.querySelector('.conversation-find, .conversation-compose-options'), "Auxiliary panels displaced the author workspace after switching projects");
    selectThread("B"); await until(() => document.querySelector(".conversation-reader")?.textContent.includes("project-last-lamp-B回答"));
    check(document.querySelector('[aria-label="核对本次保存"]'), "Current conversation lost its uncertain save warning");
    await openRecovery();
    check(document.querySelectorAll(".conversation-recovery").length === 1 && !recoveryPanel().closest(".conversation-rail"), "Saving records were duplicated or added to the reading flow");
    const rows = () => [...recoveryPanel().querySelectorAll<HTMLElement>("[data-recovery-run]")];
    check(rows().filter(row => row.textContent.includes("当前对话")).length === 1, "Current recovery thread was not precisely identified and prioritized");
    recoveryPanel().style.width = "300px"; await tick();
    check(recoveryPanel().scrollWidth <= recoveryPanel().clientWidth + 2 && rows().every(row => row.scrollWidth <= row.clientWidth + 2), "Saving records overflowed a 300px dialog");
    const selectedRecovery = () => recoveryPanel().querySelector('[data-recovery-run="current-B"]');
    const view = () => selectedRecovery().querySelector<HTMLButtonElement>("button");
    const recheck = () => selectedRecovery().querySelectorAll<HTMLButtonElement>("button")[1];
    const resumeCount = resumes.length, executionCount = calls.length;
    failThread = true; view().click(); await until(() => recoveryPanel().querySelector('[role="alert"]')?.textContent.includes("controlled history read failure"));
    check(rows().length === 2 && resumes.length === resumeCount && calls.length === executionCount, "Recovery read failure removed a record or resumed execution");
    failThread = false; heldReads.delete("B"); holdThread = true; view().click(); await until(() => heldReads.has("B"));
    check(rows().every(row => [...row.querySelectorAll<HTMLButtonElement>(".conversation-recovery-actions button")].every(button => button.disabled)), "Recovery preview allowed a concurrent acknowledgement");
    heldReads.get("B")(); holdThread = false;
    await until(() => recoveryPanel().querySelector(".conversation-recovery-preview")?.textContent.includes("project-last-lamp-B回答"));
    check(reads.at(-1).projectId === project.id && reads.at(-1).id === "B" && resumes.length === resumeCount && calls.length === executionCount
      && !recoveryPanel().textContent.includes("NOT_THE_FINAL_ANSWER"), "Recovery preview did not read the exact public final reply without resume/model execution");
    await closeRecovery();
    const draft = document.querySelector<HTMLTextAreaElement>('[aria-label="创作指令"]');
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(draft, "只读讨论"); draft.dispatchEvent(new Event("input", { bubbles: true })); await tick();
    holdDiscussion = true; discussionRelease = null; document.querySelector<HTMLButtonElement>('[aria-label="发送创作指令"]').click();
    await until(() => discussionRelease && document.querySelector('[aria-label="停止当前执行"]'));
    await openRecovery();
    check(rows().every(row => [...row.querySelectorAll<HTMLButtonElement>(".conversation-recovery-actions button")].every(button => button.disabled)) && rechecks.length === 0, "Pipeline execution did not block recovery controls");
    await closeRecovery();
    discussionRelease(); holdDiscussion = false; await until(() => !document.querySelector('[aria-label="停止当前执行"]'));
    await openRecovery();
    manualRecheck = true; failRecheck = true; recheck().click();
    await until(() => recoveryPanel().querySelector('[role="alert"]')?.textContent.includes("controlled reconciliation failure"));
    check(rows().length === 2, "Failed reconciliation hid an unresolved record");
    failRecheck = false; recheck().click();
    await until(() => recoveryPanel().querySelector('[role="alert"]')?.textContent.includes("暂时还不能确认保存结果"));
    check(rows().length === 2 && calls.length === executionCount + 1 && resumes.length === resumeCount, "Unknown save was removed or replayed");
    clearRecheck = true; const readsBeforeRecheck = taskReads; recheck().click();
    await until(() => rows().length === 1 && taskReads > readsBeforeRecheck);
    check(rows()[0].dataset.recoveryRun === "old-A" && rechecks.length === 3, "Authoritative recheck did not preserve other unresolved run");
    window.testResult = { passed: true, executionSnapshots, recoverySnapshot: { isolatedThreads: true, readonlyHistoryRetry: true, zeroHostSelection: true, isolatedProjects: true,       pendingSaves: { singleDialog: true, hiddenUntilRequested: true, currentConversationWarning: true, title: "保存记录", viewingDoesNotChangeCurrentThread: true, narrowDialog: "300px", publicReadOnlyPreview: true, zeroResume: true, busyControlsDisabled: true, failedReconciliationRetained: true, unknownRetained: true, authoritativeReconciliation: true, tasksRefreshedReadOnly: true, projectIsolated: true } } };
  } catch (error) { window.testResult = { passed: false, error: error.stack, dom: document.body.textContent.slice(-4000) }; }
})();
