import { novelFixtureMedia } from "../helpers/novel-edition";
import { emptyCreationIssueSummary } from "../helpers/creation-issues";
import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import { saveAuthor } from "../helpers/author-session";

window.addEventListener("error", event => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
const tick = () => new Promise(resolve => setTimeout(resolve, 25));
const check = (condition, message) => { if (!condition) throw Error(message); };
const until = async predicate => { for (let i = 0; i < 120 && !predicate(); i++) await tick(); check(predicate(), "UI did not settle: " + String(predicate)); };
const button = text => [...document.querySelectorAll("button")].find(node => node.textContent === text || node.getAttribute("aria-label") === text);
const navigate = async text => {
  if (text === "作品目录") {
    const directory = document.querySelector<HTMLDetailsElement>(".sidebar .work-directory");
    if (!directory.open) directory.querySelector("summary").click();
    await until(() => document.querySelector('[data-directory-chapter-id="chapter-1"]'));
    document.querySelector<HTMLButtonElement>('[data-directory-chapter-id="chapter-1"]').click();
  } else [...document.querySelectorAll(".nav-item")].find(node => node.textContent.includes(text)).click();
  await until(() => text === "作品目录" ? document.querySelector(".main-panel.chapter-main-panel") : document.querySelector(".breadcrumbs h1")?.textContent === text);
  await tick(); await tick();
};
const input = async (label, value) => {
  const element = document.querySelector(`textarea[aria-label="${label}"]`);
  check(element && !element.disabled, `missing input ${label}`);
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(element, value);
  element.dispatchEvent(new Event("input", { bubbles: true })); await tick();
};
const project = { id: "project-last-lamp", title: "任务恢复作品", logline: "测试", updated_at: "" };
const other = { ...project, id: "project-b", title: "隔离作品" };
const draft = { id: "candidate-1", version: 1, title: "雾港第一章", body: "候选第一场\r\n\r\n---\r\n\r\n候选第二场", sceneGroups: [{ id: "a", body: "候选第一场" }, { id: "b", body: "候选第二场" }, { id: "c", body: "后续段落" }], sourcePlanRefs: { localOnly: true, lineageStatus: "current" } };
let formal = false, noBody = false, tasks = [], stateReads = 0, listener, holdExecution = false, releaseExecution;
let workspaceRevision = 1;
let releaseCoordination, advanceCoordination, advanceWriter, advanceKnowledge, advanceAssessment;
const chapter = () => ({ id: "chapter-1", number: 1, title: draft.title, status: formal ? "accepted" : "draft", updated_at: "", summary: "", passages: noBody ? [] : [{ id: "p1", scene_id: "none", position: 1, body: formal ? "已采用的第一场\n\n---\n\n已采用的第二场" : draft.body, beatIds: [] }, { id: "p2", position: 2, body: "后续段落", scene_id: "none", beatIds: [] }] });
const runtimeState = id => ({ project: id === other.id ? other : project, canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1.0", scenes: [], anchors: [] }, novelEdition: { chapters: id === other.id ? [] : [chapter()] } });
const currentDraft = () => formal ? { ...draft, id: "formal-1", body: "已采用的第一场\n\n---\n\n已采用的第二场\n\n---\n\n后续段落", sceneGroups: [{ id: "a", body: "已采用的第一场" }, { id: "b", body: "已采用的第二场" }, { id: "c", body: "后续段落" }] } : draft;
const workspace = () => ({ project, revision: workspaceRevision, headSnapshotId: `snapshot-${workspaceRevision}`, sourceDigest: "a".repeat(64), index: { status: "ready" },
  artifacts: Object.fromEntries(["brief", "outline", "plan", "identity"].map(kind => [kind, { candidate: null, accepted: null, working: null, locked: false, workingSelection: null }])),
  chapters: [{ ...chapter(), candidate: noBody ? null : currentDraft(), accepted: formal ? currentDraft() : null, working: null, workingSelection: null }],
  workingBaselines: [], continuationIntents: [], planningReviews: [], commits: [], tasks: [], impacts: [],
  modeling: [{ objectId: "draft-chapter-1", sourceRevisionId: draft.id, status: "model_pending", batchId: null }], factProposals: [], reviews: [], chapterPlanBindings: [] });
saveAuthor({ name: "任务恢复作者", email: "fixture@example.invalid", passwordSalt: "fixture", passwordHash: "fixture" });
const calls = [], resumes = [];
const execute = async (operation, projectId, request, sourceTaskId = null, actionId = null) => {
  calls.push({ operation, projectId, request: structuredClone(request) });
  const turnId = `turn-${calls.length}`;
  listener?.({ method: "agent/workflow", params: { workflow: { task: { id: `task-${calls.length}`, projectId, threadId: "execution", operation, target: { requestedByTaskId: sourceTaskId, requestedActionId: actionId } } } } });
  listener?.({ method: "turn/started", params: { threadId: "execution", turn: { id: turnId } } });
  tasks = [{ id: `work-${calls.length}`, projectId, threadId: "discussion", goal: request.prompt ?? `${operation}的冻结指令`, kind: "direct", status: "blocked", version: 1,
    deliveryStatus: "partial", deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], evidence: [], pendingActionIds: [], unsettledAttemptIds: [] },
    resumeMode: "continueExecution", resumeBlockReason: null, steps: [{ id: `step-${calls.length}`, operation,
      target: {}, status: "blocked", changes: [], outputRefs: [], readingProgress: null,
      attempts: [{ id: `task-${calls.length}`, threadId: "execution", turnId, status: "failed", error: `${operation}连接中断`, startedAt: 1, completedAt: 2 }], currentAttemptId: `task-${calls.length}` }] }];
  if (holdExecution) await new Promise(resolve => { releaseExecution = resolve; });
  throw Error(`${operation}连接中断`);
};
let approvalPending = false, approvalCalls = 0;
const approvalAction = { id: `action-${"9".repeat(32)}`, sourceTaskId: `task-${"8".repeat(32)}`, operation: "generateChapter", status: "proposed", intentRevision: 0, inputRevision: 0, instruction: "只修订密函场景", target: { chapterId: "chapter-1", chapterNumber: 1 } };
const api = { multimedia: novelFixtureMedia,
  app: { onCloseRequested: () => () => {} },
  runtime: { listChapterStructureChanges: async (projectId: string) => ({ projectId, items: [], nextCursor: null }),
  getWorkCatalog: async () => ({ revision: 0, volumes: [], placements: [] }),
    getIdentityCatalog: async () => ({ entries: [], ambiguousNames: [], identityRevisionId: null, identityDecisions: [], truncated: false, ambiguityScanComplete: true }),
    status: async () => ({ state: "online", mode: "local", error: null }), listProjects: async () => ({ projects: [project, other] }),
    getState: async id => { stateReads++; return runtimeState(id); },
    getCreativeWorkspace: async id => { const data = structuredClone(workspace()); return id === other.id ? { ...data, project: other, chapters: [], modeling: [] } : data; }, listPlanningImpacts: async () => ({ data: [], nextCursor: null, counts: { open: 0 } }),
    getWorkTask: async (projectId, id) => { const work = tasks.find(task => task.id === id && task.projectId === projectId); check(work, "Exact recovery Work missing"); return structuredClone(work); },
    getTaskFinalReply: async () => ({ state: "absent" }),
    listGenerationRuns: async () => ({ runs: [] }), listWorkTasks: async id => ({ tasks: id === other.id ? [] : structuredClone(approvalPending ? [...tasks, { id: "approval-source-work", projectId: id, threadId: "discussion", goal: "讨论后提出的修改建议", status: "completed", deliveryStatus: "discussion_ended", deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], evidence: [], pendingActionIds: [], unsettledAttemptIds: [] }, steps: [{ id: "approval-source-step", operation: "advisory", target: {}, status: "completed", readingProgress: null, currentAttemptId: approvalAction.sourceTaskId, attempts: [{ id: approvalAction.sourceTaskId, threadId: "discussion", turnId: "approval-source-turn", status: "completed" }], changes: [], outputRefs: [] }] }] : tasks), activeCount: tasks.length, nextCursor: null }),
    getWriterPreview: async () => ({ target: { position: { readingChapter: 1, sceneOrder: 1 } }, plannedEffects: [], relevantText: [], coverage: { complete: true, missingScope: [], truncationReasons: [] } }),
    getCommitmentExtraction: async () => null, saveContinuationIntent: async () => ({}),
    ...Object.fromEntries(["Search", "Entity", "Relation", "Knowledge", "SnapshotKnowledge", "Fact"].map(kind => [`get${kind}Audit`, async () => ({ job: null })])),
  },
  agent: {
    status: async () => ({ configured: true, state: "ready", mode: "novel-agent-engine", provider: "fixture", model: "fixture", error: null }),
    listConnections: async () => ({ connections: [] }), recoveryReport: async () => [], readConversation: async (projectId, id) => ({ thread: { id, projectId, name: "作品讨论", status: "idle" }, turns: [] }), listExecutions: async () => [], readMessage: async () => { throw Error("message receipt not found"); }, beginInterlude: async () => { throw Error("unexpected interlude"); }, returnInterlude: async () => { throw Error("unexpected interlude return"); }, listMessages: async () => [], listThreads: async () => ({ threads: [], nextCursor: null }),
    listProposedActions: async () => ({ actions: approvalPending ? [approvalAction] : [] }),
    approveAction: async () => {
      approvalCalls++; approvalPending = false;
      return { action: approvalAction, taskId: "approved-writer", message: "已保存结果保留，检查未完成。",
        candidateResults: [{ taskId: "approved-writer", action: approvalAction }],
        executionFailure: { message: "本次执行额度已用完，后续处理已暂停。", code: "CONVERSATION_INCOMPLETE", sourceTaskId: "decision-after-save", workTaskId: "work-approved", stepId: null } };
    }, setWorkspaceContext: async () => {},
    onEvent: fn => { listener = fn; return () => {}; }, onUserInputRequest: () => () => {},
    steer: async (projectId, prompt) => { calls.push({ operation: "steer", projectId, request: { prompt } }); return { steered: true }; },
    generateChapter: async (id, request) => execute("generateChapter", id, request), resumeKnowledge: async (id, chapterId) => execute("modelKnowledge", id, { chapterId }),
    sendPrompt: async (id, prompt, source, focus) => {
      calls.push({ operation: "chat", projectId: id, request: { prompt, source, focus } });
      if (["整章修订并检查保存结果", "整章修订并完成自动恢复"].includes(prompt)) {
        const succeeds = prompt === "整章修订并完成自动恢复";
        check(source === null && focus?.chapterId === "chapter-1", "Coordination lost the chapter conversation scope");
        const sourceTaskId = `task-${"c".repeat(32)}`;
        const writerTaskId = `task-${"d".repeat(32)}`;
        const parentTaskId = `task-${"f".repeat(32)}`, recoveredWriterId = `task-${"b".repeat(32)}`;
        const work = { id: "work-original-coordination", projectId: id, threadId: "discussion", goal: prompt,
          kind: "conversation", status: "running", version: 2, deliveryStatus: "pending",
          deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], evidence: [], pendingActionIds: [], unsettledAttemptIds: [] },
          resumeMode: null, resumeBlockReason: null, steps: [] };
        tasks = [work];
        let decisionTaskId = null;
        const workflow = (taskId, threadId, operation, target) => {
          const stepId = `step-${taskId}`;
          let inputManifest = {}, executionBinding = null;
          if (operation === "advisory" && taskId !== sourceTaskId) {
            decisionTaskId = taskId;
            const frozen = { workTaskId: work.id, rootThreadId: work.threadId, originTaskId: sourceTaskId,
              decisionStepId: stepId, decisionTaskId: taskId, sourceTaskId: writerTaskId };
            target = { workDecision: { workTaskId: work.id, originTaskId: sourceTaskId, sourceTaskId: writerTaskId } };
            inputManifest = { workDecision: frozen };
            executionBinding = { ...frozen, stepId, taskId, actionKey: null };
          } else if (taskId !== sourceTaskId && taskId !== writerTaskId) {
            const declared = { workTaskId: work.id, decisionStepId: `step-${decisionTaskId}`,
              decisionTaskId, actionKey: `action-${taskId}` };
            target = { ...target, workDecisionAction: declared };
            inputManifest = { workDecisionAction: { ...declared, rootThreadId: work.threadId, originTaskId: sourceTaskId,
              operation, sourceTaskId: target.sourceTaskId ?? target.writerTaskId ?? null,
              sourceRevisionId: target.draftRevisionId ?? null } };
            executionBinding = { ...declared, rootThreadId: work.threadId, originTaskId: sourceTaskId, stepId, taskId };
          }
          work.steps.push({ id: stepId, operation, status: "running", target, changes: [], readingProgress: null, outputRefs: [], expectedOutputKind: operation === "advisory" ? "completionReport" : null,
            attempts: [{ id: taskId, threadId, turnId: null, status: "running", error: null, startedAt: 1 }], currentAttemptId: taskId });
          listener?.({ method: "agent/workflow", params: { workflow: {
            task: { id: taskId, projectId: id, threadId, operation, target, inputManifest }, executionBinding,
            context: { purpose: operation === "advisory" ? "planner" : "writer", facts: [] } } } });
        };
        workflow(sourceTaskId, "discussion", "advisory", {});
        listener?.({ method: "turn/started", params: { threadId: "discussion", turn: { id: "coordination-turn" } } });
        listener?.({ method: "turn/completed", params: { threadId: "discussion", turn: { id: "coordination-turn", status: "completed" } } });
        listener?.({ method: "agent/candidate-started", params: { projectId: id, sourceTaskId,
          actionId: `action-${"e".repeat(32)}`, operation: "generateChapter", target: { chapterId: "chapter-1" } } });
        workflow(writerTaskId, "writer", "generateChapter", { requestedByTaskId: sourceTaskId, requestedActionId: `action-${"e".repeat(32)}`, chapterId: "chapter-1" });
        listener?.({ method: "turn/started", params: { threadId: "writer", turn: { id: "saved-writer-turn" } } });
        advanceCoordination = () => {
          work.steps.find(step => step.currentAttemptId === writerTaskId).attempts[0].error = "首次写作字段校验失败";
          listener?.({ method: "agent/error", params: { threadId: "writer", turnId: "saved-writer-turn", error: { message: "首次写作字段校验失败" } } });
          workflow(parentTaskId, "discussion", "advisory", { conversationContinuation: { sourceTaskId } });
          listener?.({ method: "turn/started", params: { threadId: "discussion", turn: { id: "adjusting-turn" } } });
        };
        advanceWriter = () => {
          listener?.({ method: "turn/completed", params: { threadId: "discussion", turn: { id: "adjusting-turn", status: "completed" } } });
          workflow(recoveredWriterId, "writer-recovered", "generateChapter", { requestedByTaskId: parentTaskId, requestedActionId: `action-${"a".repeat(32)}`, chapterId: "chapter-1" });
          listener?.({ method: "turn/started", params: { threadId: "writer-recovered", turn: { id: "recovered-writer-turn" } } });
        };
        const knowledgeTaskId = `task-${"1".repeat(32)}`, batchId = `batch-${"2".repeat(32)}`;
        advanceKnowledge = () => {
          listener?.({ method: "turn/completed", params: { threadId: "writer-recovered", turn: { id: "recovered-writer-turn", status: "completed" } } });
          workflow(knowledgeTaskId, "knowledge", "modelKnowledge", { sourceTaskId: recoveredWriterId, batchId });
          listener?.({ method: "turn/started", params: { threadId: "knowledge", turn: { id: "knowledge-turn" } } });
        };
        advanceAssessment = () => {
          listener?.({ method: "turn/completed", params: { threadId: "knowledge", turn: { id: "knowledge-turn", status: "completed" } } });
          workflow(`task-${"3".repeat(32)}`, "assessment", "assessRevisionOutcome", { writerTaskId: recoveredWriterId,
            chapterId: "chapter-1", knowledgeBatchId: batchId, draftRevisionId: "draft-recovered", resumeStepId: "assessment-step" });
          listener?.({ method: "turn/started", params: { threadId: "assessment", turn: { id: "assessment-turn" } } });
        };
        await new Promise(resolve => { releaseCoordination = resolve; });
        draft.body = "真实回执对应的已保存修订";
        draft.sceneGroups = [{ id: "a", body: draft.body }];
        workspaceRevision++;
        work.status = "blocked"; work.resumeMode = "continueExecution"; work.deliveryStatus = "partial"; tasks = [work];
        if (succeeds) {
          tasks = [work]; work.status = "completed"; work.deliveryStatus = "ready_for_review";
          listener?.({ method: "turn/completed", params: { threadId: "assessment", turn: { id: "assessment-turn", status: "completed" } } });
          workflow(`task-${"a".repeat(32)}`, "discussion", "advisory", { conversationContinuation: { sourceTaskId: parentTaskId } });
          listener?.({ method: "turn/started", params: { threadId: "discussion", turn: { id: "delivered-turn" } } });
          listener?.({ method: "turn/completed", params: { threadId: "discussion", turn: { id: "delivered-turn", status: "completed",
            items: [{ id: "delivered-answer", type: "agentMessage", phase: "final_answer", text: "第一章已修改并保存。" }] } } });
          return { message: "第一章已修改并保存。", proposedActions: [], candidateResults: [{ taskId: recoveredWriterId,
            action: { sourceTaskId: parentTaskId, operation: "generateChapter", target: { chapterId: "chapter-1" } } }] };
        }
        return { message: "已有保存结果保留。本轮尚未完成，请从原任务继续或核对执行结果。",
          projectId: id, workflow: { task: { id: sourceTaskId, projectId: id, threadId: "discussion" },
            revision: workspaceRevision, headSnapshotId: `snapshot-${workspaceRevision}`, sourceDigest: "a".repeat(64) },
          proposedActions: [], candidateResults: [{ taskId: writerTaskId,
            action: { sourceTaskId, operation: "generateChapter", target: { chapterId: "chapter-1" } } }],
          executionFailure: { message: "本轮自动处理已达到上限，后续处理已暂停。", code: "CONVERSATION_INCOMPLETE", sourceTaskId,
            workTaskId: "work-original-coordination", stepId: "coordinate-after-save" } };
      }
      const operation = prompt === "章节知识分析" && focus?.chapterId === "chapter-1" ? "modelKnowledge" : noBody && focus?.chapterId === "chapter-1" ? "generateChapter"
        : ["generateBrief", "generateOutline", "generatePlan"].find(value => prompt === `冻结${value}要求`);
      check(operation && source === null, "Planning conversation lost its original instruction or inherited a repair source");
      const sourceTaskId = `task-${String(calls.length).padStart(32, "0")}`;
      listener?.({ method: "agent/workflow", params: { workflow: { task: { id: sourceTaskId, projectId: id, operation: "advisory", target: {}, threadId: "discussion" }, context: { purpose: "advisory", facts: [] } } } });
      listener?.({ method: "turn/started", params: { threadId: "discussion", turn: { id: `chat-${calls.length}` } } });
      listener?.({ method: "turn/completed", params: { threadId: "discussion", turn: { id: `chat-${calls.length}`, status: "completed" } } });
      listener?.({ method: "agent/candidate-started", params: { projectId: id, sourceTaskId,
        actionId: `action-${String(calls.length).padStart(32, "0")}`, operation,
        target: ["generateChapter", "modelKnowledge"].includes(operation) ? { chapterId: focus.chapterId, chapterNumber: focus.chapterNumber } : {} } });
      return execute(operation, id, { prompt }, sourceTaskId, `action-${String(calls.length).padStart(32, "0")}`);
    },
    resumeWorkTask: async (projectId, taskId) => { resumes.push({ projectId, taskId }); const result = { ...tasks.find(task => task.id === taskId), status: "completed" }; tasks = []; return result; },
    controlWorkTask: async () => {},
  },
};
window.novelCodex = api;
createRoot(document.getElementById("root")).render(<App />);
(async () => {
  try {
    await until(() => stateReads >= 2 && document.querySelector(".chapter-document"));
    const publication = [];
    const capturePublication = () => ({ status: document.querySelector('[aria-label="章节发布状态"]')?.textContent,
      title: document.querySelector<HTMLInputElement>('[aria-label="章节标题"]')?.value,
      scenes: [...document.querySelectorAll<HTMLTextAreaElement>('.draft-editor-wrap textarea')].map(node => node.value) });
    check(document.querySelector('[aria-label="章节发布状态"]')?.textContent === "未发布", "unpublished content mislabeled as published");
    check([...document.querySelectorAll<HTMLTextAreaElement>('.draft-editor-wrap textarea')].some(node => node.value === "后续段落"), "editor dropped subsequent scenes");
    publication.push(capturePublication());
    const originalDraft = structuredClone(draft);
    await input("创作指令", "整章修订并检查保存结果");
    const composerNode = document.querySelector<HTMLTextAreaElement>('textarea[aria-label="创作指令"]');
    document.querySelector<HTMLButtonElement>('[aria-label="发送创作指令"]').click();
    await until(() => releaseCoordination);
    await input("创作指令", "下一条尚未发送的构想");
    composerNode.setSelectionRange(3, 7);
    composerNode.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await tick();
    check(calls.length === 1, "Pending coordination sent or steered the next draft");
    advanceCoordination(); await tick();
    await until(() => document.querySelector(".conversation-turn.is-running") && !document.querySelector(".agent-transcript > .conversation-turn:not(.is-earlier) .transcript-error"));
    check(!document.querySelector(".agent-status-bar")?.textContent.includes("本轮任务未完成"), "Parent coordination retained the current failure banner");
    check([...document.querySelectorAll(".conversation-turn.is-earlier")].some(node => node.textContent.includes("首次写作字段校验失败")), "Recovering hid the failed worker history");
    advanceWriter(); await tick();
    await until(() => document.querySelector(".conversation-turn.is-running") && !document.querySelector(".agent-transcript > .conversation-turn:not(.is-earlier) .transcript-error"));
    releaseCoordination();
    await until(() => document.querySelector(".composer-retry")
      && document.querySelector<HTMLTextAreaElement>('[aria-label="场景 1 正文"]')?.value === "真实回执对应的已保存修订");
    check(document.querySelector('textarea[aria-label="创作指令"]') === composerNode
      && composerNode.value === "下一条尚未发送的构想" && composerNode.selectionStart === 3 && composerNode.selectionEnd === 7,
      "Parent failure lost the next draft DOM, input or selection");
    check(document.querySelector(".agent-transcript > .conversation-turn:not(.is-earlier) .transcript-error").textContent.includes("本轮自动处理已达到上限，后续处理已暂停。"), "Parent failure was hidden as successful completion");
    const coordinationRetry = document.querySelector<HTMLButtonElement>(".composer-retry");
    const coordinationCalls = calls.length;
    coordinationRetry.click(); coordinationRetry.click();
    await until(() => resumes.length === 1 && !document.querySelector(".work-task-row"));
    check(resumes[0].taskId === "work-original-coordination" && calls.length === coordinationCalls, "Coordination recovery did not use the original Work");
    check(composerNode.value === "下一条尚未发送的构想", "Original Work recovery replaced the next author draft");
    const coordination = { savedText: "真实回执对应的已保存修订", error: "本轮自动处理已达到上限，后续处理已暂停。",
      recovery: "work-original-coordination", originalPromptCalls: 1, modelCallsOnRecovery: 0,
      draft: composerNode.value, selection: [composerNode.selectionStart, composerNode.selectionEnd] };
    resumes.length = 0;
    await input("创作指令", "整章修订并完成自动恢复");
    document.querySelector<HTMLButtonElement>('[aria-label="发送创作指令"]').click();
    await until(() => document.querySelector(".conversation-turn.is-running"));
    await input("创作指令", "成功后仍保留的下一条草稿");
    advanceCoordination(); await tick(); advanceWriter(); await tick();
    advanceKnowledge(); await tick();
    check(document.querySelector(".conversation-turn.is-running") && !document.querySelector(".agent-transcript > .conversation-turn:not(.is-earlier) .transcript-error"), "Knowledge followup remained on the completed writer");
    advanceAssessment(); await tick();
    check(document.querySelector(".conversation-turn.is-running") && !document.querySelector(".agent-transcript > .conversation-turn:not(.is-earlier) .transcript-error"), "Assessment followup remained on the completed knowledge task");
    releaseCoordination();
    await until(() => document.querySelector('.conversation-turn.is-completed:not(.is-earlier) .conversation-answer')?.textContent.includes("第一章已修改并保存。"));
    check(!document.querySelector(".agent-transcript > .conversation-turn:not(.is-earlier) .transcript-error") && !document.querySelector(".agent-status-bar")?.textContent.includes("本轮任务未完成"), "Successful recovery retained the current error or recovery button");
    check([...document.querySelectorAll(".conversation-turn.is-earlier")].some(node => node.textContent.includes("首次写作字段校验失败")), "Successful recovery erased prior failure details");
    check(document.querySelector('textarea[aria-label="创作指令"]') === composerNode && composerNode.value === "成功后仍保留的下一条草稿", "Successful recovery lost the next author draft");
    await until(() => document.querySelector('[data-work-task-id="work-original-coordination"] .user-message'));
    check([...document.querySelectorAll('.user-message')].filter(node => node.textContent.includes("整章修订并完成自动恢复")).length === 1, "One coordinated Work duplicated the author's message");
    tasks = [];
    const coordinatedSuccess = { currentReply: "第一章已修改并保存。", currentFailure: false, priorFailureRetained: true, authorMessages: 1,
      draft: composerNode.value, sourceSendCalls: 1, automaticUiTransitions: ["调整步骤", "重新执行", "知识分析", "修改核对", "完成"] };
    Object.assign(draft, originalDraft);
    await navigate("角色"); await navigate("作品目录");
    const retries = [];
    for (const [view, operation] of [["作品概况", "generateBrief"], ["全书大纲", "generateOutline"], ["近章规划", "generatePlan"]]) {
      await navigate(view);
      await until(() => button("开始规划") && !button("开始规划").disabled);
      if (view === "作品概况") { document.querySelector(".overview-direction-editor > summary").click(); await tick(); check(button("开始规划").checkVisibility(), "Direction controls stayed hidden"); }
      const beforePrepare = calls.length;
      await input("创作指令", "未发送的独立草稿");
      button("开始规划").click(); await tick();
      check(document.querySelector('textarea[aria-label="创作指令"]').value === "未发送的独立草稿" && calls.length === beforePrepare, "Planning preparation overwrote a draft or executed a model");
      await input("创作指令", ""); button("开始规划").click(); await tick();
      check(document.querySelector('textarea[aria-label="创作指令"]').value.length > 0 && calls.length === beforePrepare, "Planning preparation did not seed the single composer");
      await input("创作指令", `冻结${operation}要求`);
      document.querySelector('[aria-label="发送创作指令"]').click();
      await until(() => document.querySelector(".composer-retry"));
      check(calls.filter(call => call.operation === operation).length === 1, `${operation}: duplicate generation`);
      await input("创作指令", "未发送的独立草稿");
      await navigate("角色");
      const count = calls.length;
      const expectedTask = tasks[0].id, beforeResume = resumes.length;
      const retry = document.querySelector(".composer-retry"); retry.click(); retry.click();
      await until(() => resumes.length === beforeResume + 1 && !document.querySelector(".composer-retry"));
      check(resumes.at(-1).taskId === expectedTask && calls.length === count, `${operation}: retry lost original Work or replayed the prompt`);
      check(document.querySelector('textarea[aria-label="创作指令"]').value === "未发送的独立草稿", "retry discarded unrelated composer draft");
      check(!document.querySelector('[aria-label="原任务恢复"]'), "Retry opened another task management surface");
      retries.push({ operation, action: retry.getAttribute("aria-label"), view: "角色", modelCallsOnRetry: 0 });
    }
    noBody = true;
    await navigate("角色");
    await navigate("作品目录");
    await until(() => button("让 AI 起草") && !button("让 AI 起草").disabled && document.querySelector<HTMLTextAreaElement>('[aria-label="场景 1 正文"]')?.value === "");
    button("让 AI 起草本章").click(); await tick();
    await input("创作指令", "");
    button("取消写作")?.click(); await tick(); button("让 AI 起草本章").click(); await tick();
    const chapterIntent = document.querySelector<HTMLTextAreaElement>('[aria-label="创作指令"]').value;
    check(chapterIntent.includes("2500") && chapterIntent.includes(draft.title), "Empty chapter lacks the exact author-visible default writer intent");
    document.querySelector('[aria-label="发送创作指令"]').click();
    await until(() => calls.at(-1)?.operation === "generateChapter" && document.querySelector(".composer-retry"));
    const originalRequest = structuredClone(calls.at(-1));
    const staleRetry = document.querySelector(".composer-retry");
    await input("创作指令", "不应替换冻结输入的新要求");
    const count = calls.length;
    const chapterResumeCount = resumes.length;
    staleRetry.click(); staleRetry.click();
    await until(() => resumes.length === chapterResumeCount + 1 && !document.querySelector(".work-task-row"));
    check(calls.length === count && JSON.stringify(calls.at(-1)) === JSON.stringify(originalRequest), "chapter retry mutated or replayed frozen operation");
    check(resumes.at(-1).taskId === `work-${count}` && resumes.at(-1).projectId === project.id, "recovery lost original task identity");
    retries.push({ operation: "generateChapter", action: staleRetry.getAttribute("aria-label"), view: "作品目录", modelCallsOnRetry: 0 });
    noBody = false;
    await navigate("角色"); await navigate("作品目录");
    // A new root invalidates an old visible retry callback.
    holdExecution = true;
    check(!button("继续分析本章"), "Optional analysis exposed a manual task on the chapter page");
    await input("创作指令", "章节知识分析");
    document.querySelector<HTMLButtonElement>('[aria-label="发送创作指令"]').click();
    await until(() => releaseExecution);
    staleRetry.click(); await tick();
    check(!document.querySelector('[aria-label="原任务恢复"]'), "stale retry contaminated an active new root");
    releaseExecution(); holdExecution = false;
    await until(() => document.querySelector(".composer-retry"));
    const knowledgeRetry = document.querySelector(".composer-retry");
    const beforeKnowledgeResume = resumes.length; knowledgeRetry.click(); await until(() => resumes.length === beforeKnowledgeResume + 1);
    retries.push({ operation: "modelKnowledge", action: knowledgeRetry.getAttribute("aria-label"), view: "作品目录", modelCallsOnRetry: 0 });
    const finalCount = calls.length;
    document.querySelector(".project-switcher").click(); await tick();
    [...document.querySelectorAll(".project-menu-item")].find(node => node.textContent.includes(other.title)).click();
    await until(() => document.querySelector(".project-switcher span[title]")?.textContent === other.title && !document.querySelector<HTMLTextAreaElement>('[aria-label="创作指令"]')?.disabled);
    knowledgeRetry.click(); await tick();
    check(!document.querySelector('[aria-label="原任务恢复"]') && calls.length === finalCount, "project switch allowed stale retry");
    document.querySelector(".project-switcher").click(); await tick();
    [...document.querySelectorAll(".project-menu-item")].find(node => node.textContent.includes(project.title)).click();
    formal = true;
    await navigate("作品目录");
    await until(() => document.querySelector('[aria-label="章节发布状态"]')?.textContent === "已发布");
    check(![...document.querySelectorAll<HTMLTextAreaElement>('.draft-editor-wrap textarea')].some(node => node.value.includes("候选第一场")), "published-only projection displayed obsolete editing content");
    const versions = document.querySelector<HTMLDetailsElement>('.chapter-versions');
    versions.querySelector<HTMLElement>('summary').click(); await tick();
    check(versions.textContent.includes("已采用的第一场"), "Published version was unavailable for read-only verification");
    publication.push(capturePublication());
    // Reload the project projection to verify an empty chapter is not a candidate.
    noBody = true; formal = false;
    approvalPending = true;
    const existing = document.querySelector("#root");
    // Initial state refresh is read-only and applies the current projection.
    await navigate("作品目录");
    await navigate("角色");
    document.querySelector(".project-switcher").click(); await tick();
    [...document.querySelectorAll(".project-menu-item")].find(node => node.textContent.includes(other.title)).click();
    await until(() => document.querySelector(".project-switcher span[title]")?.textContent === other.title && !document.querySelector<HTMLTextAreaElement>('[aria-label="创作指令"]')?.disabled);
    document.querySelector(".project-switcher").click(); await tick();
    [...document.querySelectorAll(".project-menu-item")].find(node => node.textContent.includes(project.title)).click();
    await navigate("作品目录");
    await until(() => document.querySelector<HTMLTextAreaElement>('[aria-label="场景 1 正文"]')?.value === "");
    check(document.querySelector('[aria-label="章节发布状态"]')?.textContent === "未发布" && !document.querySelector('.chapter-versions'), "empty chapter acquired a published version");
    publication.push(capturePublication());
    const approvalDraft = document.querySelector<HTMLTextAreaElement>('[aria-label="创作指令"]');
    await input("创作指令", "下一条尚未发送的构想");
    await until(() => button("批准生成候选") && !button("批准生成候选").disabled);
    button("批准生成候选").click();
    await until(() => document.querySelector(".agent-transcript > .conversation-turn:not(.is-earlier) .transcript-error")?.textContent.includes("本次执行额度已用完"));
    const callsBeforeRecovery = calls.length;
    check(!document.querySelector(".transcript-error button, .work-task-controls"), "Approval failure exposed duplicate task controls");
    check(approvalCalls === 1 && calls.length === callsBeforeRecovery && document.querySelector('[aria-label="创作指令"]') === approvalDraft && approvalDraft.value === "下一条尚未发送的构想", "Approval failure replayed a request or consumed the next draft");
    const approvalRecovery = { currentError: "本次执行额度已用完，后续处理已暂停。", approvedCalls: approvalCalls, noNewPrompt: true, draftPreserved: approvalDraft.value };
    window.testResult = { passed: true, approvalRecovery, chatCalls: calls.filter(call => call.operation === "chat").length, coordination, coordinatedSuccess, retries, publication,
      isolation: { frozenTaskRecovery: true, doubleClickResumeCount: resumes.length, staleRootBlocked: true, staleProjectBlocked: true, composerDraftRetained: true } };
  } catch (error) { window.testResult = { passed: false, error: error.stack, dom: document.querySelector(".conversation-rail")?.textContent }; }
})();
