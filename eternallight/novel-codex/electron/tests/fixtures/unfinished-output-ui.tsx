import { emptyCreationIssueSummary } from "../helpers/creation-issues";
import React, { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { Composer, EarlierTurn, EnhancedAgentTranscript } from "../../../src/App";
import { ConversationOutput } from "../../../src/ConversationOutput";
import { ConversationReader } from "../../../src/ConversationReader";
import { ConversationTimeline } from "../../../src/ConversationTimeline";
import { FailedTaskReply } from "../../../src/SavedTaskReply";
import { WorkTaskPanel } from "../../../src/WorkTaskPanel";
import { createAgentRun } from "../../../src/agent-events";
import "../../../src/styles.css";

const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (condition, label) => { if (!condition) throw Error(label); };
let rendererError = null;
window.addEventListener("error", event => { rendererError = event.error ?? Error(event.message); });
const until = async (predicate, label) => {
  for (let i = 0; i < 150 && !predicate(); i++) { if (rendererError) throw rendererError; await tick(); }
  if (rendererError) throw rendererError;
  check(predicate(), label);
};
const content = "We must inspect sceneGroup before saving. 这是仍未交付的内容。".repeat(60);
const progressText = "我已核对当前章的行动动机，接下来会检查后续章纲能否承接。";
const nextProgressText = "后续章纲还缺少对应行动，这次修订需要补齐这处承接。";
const shortText = "已找到需要修改的位置，本轮还没有完成保存。";
const failureRaw = "provider_output_item_limit: RAW_FAILURE_EVIDENCE";
const failureReason = "模型返回的一条结果超过可处理的长度上限，本次任务未完成。";
const finalText = "已保存的最终回复，不是过程分析。";
const attempt = { id: "task-a", threadId: "thread-a", turnId: "turn-a" };
const receipt = (taskId = "task-a", text = finalText, projectId = "project", threadId = "thread-a", turnId = "turn-a") => ({
  taskId, projectId, state: "recorded", binding: { threadId, turnId, itemId: "final-item", intentRevision: 0, text, truncated: false },
  sourceDigest: "source", receiptDigest: "receipt", recordedAt: 1,
});
const run = { ...createAgentRun("修订当前章"), taskId: attempt.id, threadId: attempt.threadId, turnId: attempt.turnId,
  state: "error", answer: content, error: failureRaw, summary: "本轮未完成", completedAt: 100,
  steps: [{ id: "public-progress", kind: "system", title: "进展", status: "completed", messagePhase: "commentary", text: progressText },
    { id: "agent-message", kind: "assistant", title: "回复", status: "failed", messagePhase: null, text: content },
    { id: "failure", kind: "error", title: "执行失败", status: "failed", detail: failureRaw }],
};
let root, retryCalls = 0, submissionCalls = 0, interruptCalls = 0;
const cases = [];
const availabilityCases = [];
const render = async node => { root.render(<StrictMode>{node}</StrictMode>); await tick(); };
const transcript = (source = run, finalReplyState = undefined) => <EnhancedAgentTranscript projectId="project" run={source} taskBound
  finalReplyState={finalReplyState} userInputRequest={null} queuedUserInputCount={0} onResolveUserInput={() => {}}
  onReview={() => {}} />;
const readyAvailability = { runtimeReady: true, modelReady: true, changingConversation: false,
  historyState: "ready" as const, archived: false, otherConversationRunning: false };
const composer = (overrides: Partial<React.ComponentProps<typeof Composer>> = {}) => <Composer projectId="project" onRetry={() => { retryCalls++; }} retryDisabled={false} retryLabel="重试当前任务"
  queueDisabled={false} planningSaving={false} planningSaveOnSend={false} choosingSource={false} canSteer={false} writing={null}
  writingPending={false} onCancelWriting={() => {}} repairPrepared={false} checkingRepair={false} composer="" setComposer={() => {}}
  onSubmit={() => { submissionCalls++; }} onInterrupt={() => { interruptCalls++; }} composerRef={{ current: null }} activity={[]} agentStatus={{ configured: true, state: "idle" }}
  availability={readyAvailability} blockedByUnsavedChanges={false} projectReady connections={[]} agentConfigSaving={false} onSelectConnection={() => {}} {...overrides} />;
const boundary = (api, identity = attempt, source = run) => <FailedTaskReply api={api} projectId="project" attempt={identity}
  render={state => transcript({ ...source, ...{ taskId: identity.id, threadId: identity.threadId, turnId: identity.turnId } }, state)} />;
const draftElements = (text = content, scope: ParentNode = document) => [...scope.querySelectorAll<HTMLElement>(".conversation-answer")]
  .filter(element => element.textContent.includes(text));
const hiddenDraftContent = (text = content, scope: ParentNode = document) => {
  const elements = draftElements(text, scope);
  return elements.length > 0 && elements.every(element => {
    const detail = element.closest<HTMLDetailsElement>(".conversation-unfinished-content");
    return !element.checkVisibility() && detail && !detail.open;
  });
};
const visibleDraftContent = (text: string, scope: ParentNode = document) => {
  const elements = draftElements(text, scope);
  return elements.length > 0 && elements.every(element => element.checkVisibility() && !element.closest("details"));
};
const visibleProgress = (text = progressText, scope: ParentNode = document) => {
  const elements = [...scope.querySelectorAll<HTMLElement>(".conversation-progress-notes, .conversation-reader-process")]
    .filter(element => element.textContent.includes(text));
  return elements.length > 0 && elements.every(element => element.checkVisibility() && !element.closest("details"));
};

(async () => {
  try {
    root = createRoot(document.getElementById("root"));
    await render(<>{transcript()}{composer()}</>);
    check(hiddenDraftContent(), "failed long draft became a formal visible answer");
    check(visibleProgress(), "failed turn hid its public progress");
    check(document.querySelector(".transcript-error").checkVisibility(), "failure feedback was folded");
    const enhancedFailures = [...document.querySelectorAll<HTMLElement>('[role="alert"]')]
      .filter(element => element.textContent.includes(failureReason));
    check(enhancedFailures.length === 1 && enhancedFailures[0].checkVisibility() && !enhancedFailures[0].closest("details"),
      "Enhanced transcript duplicated or folded its readable failure reason");
    document.querySelector<HTMLButtonElement>('[aria-label="重试当前任务"]').click();
    check(retryCalls === 1, "recovery control was hidden or disabled");
    cases.push("failed draft and public progress");

    const sixHundred = "🕯".repeat(600), sixHundredOne = `${sixHundred}🕯`;
    await render(<ConversationOutput state="error" identity="threshold-600" text={sixHundred}>
      <div className="conversation-answer">{sixHundred}</div>
    </ConversationOutput>);
    check(visibleDraftContent(sixHundred) && !document.querySelector(".conversation-unfinished-content")
      && document.body.textContent.includes("本轮未完成") && document.body.textContent.includes("保存状态尚未核实"),
      "600 Unicode codepoints were folded or promoted to a delivered answer");
    await render(<ConversationOutput state="error" identity="threshold-601" text={sixHundredOne}>
      <div className="conversation-answer">{sixHundredOne}</div>
    </ConversationOutput>);
    check(hiddenDraftContent(sixHundredOne), "601 Unicode codepoints bypassed the long draft boundary");
    cases.push("unicode threshold");

    const shortRun = { ...run, answer: shortText,
      steps: run.steps.map(step => step.kind === "assistant" ? { ...step, text: shortText } : step) };
    for (const replyState of ["unknown", "absent"] as const) {
      await render(transcript(shortRun, replyState));
      check(visibleDraftContent(shortText) && visibleProgress() && !document.querySelector(".conversation-unfinished-content"),
        `${replyState} short unfinished reply or public progress was folded`);
      check(document.querySelector<HTMLElement>(".conversation-unfinished-output .conversation-process-meta")?.textContent.includes("本轮未完成")
        && document.querySelector('[aria-label="复制未完成内容"]')?.checkVisibility()
        && !document.querySelector('[aria-label="复制回复"]'), `${replyState} short unfinished reply lost its status or copy boundary`);
      check(replyState === "unknown" ? document.body.textContent.includes("保存状态尚未核实")
        && !document.body.textContent.includes("未作为最终回复交付") : document.body.textContent.includes("未作为最终回复交付"),
        `${replyState} short reply confused unknown delivery with proven absence`);
      cases.push(`short ${replyState} reply`);
    }

    for (const replyState of ["recorded", "truncated"] as const) {
      const savedText = `已保存的 ${replyState} 最终回复。`;
      const saved = { ...receipt("task-a", savedText), state: replyState,
        binding: { ...receipt("task-a", savedText).binding, truncated: replyState === "truncated" } };
      await render(boundary({ runtime: { getTaskFinalReply: async () => saved } } as any, attempt, shortRun));
      await until(() => document.querySelector(".saved-task-reply")?.textContent.includes(savedText), `${replyState} saved short reply was not read`);
      check(document.querySelector(".saved-task-reply .conversation-answer")?.checkVisibility()
        && hiddenDraftContent(shortText) && visibleProgress(), `${replyState} short stream was mixed with the saved reply or folded public progress`);
    }
    cases.push("short saved streams");

    const continuityRun = { ...shortRun, steps: [run.steps[0],
      { id: "continuity-read", kind: "tool", title: "读取后续章纲", status: "running" },
      { id: "next-public-progress", kind: "system", title: "进展", status: "completed", messagePhase: "commentary", text: nextProgressText },
      shortRun.steps.find(step => step.kind === "assistant")!], completedAt: null };
    let originalProgress: HTMLElement[] = [];
    for (const state of ["running", "error", "cancelled"] as const) {
      await render(transcript({ ...continuityRun, state, error: state === "error" ? failureRaw : null,
        completedAt: state === "running" ? null : 100,
        steps: continuityRun.steps.map(step => step.id === "continuity-read" ? { ...step,
          status: state === "running" ? "running" : state === "error" ? "failed" : "cancelled" } : step) }));
      const flow = document.querySelector(".conversation-process-flow");
      const blocks = [...flow.children];
      const progress = [...flow.querySelectorAll<HTMLElement>(".conversation-progress-notes")];
      check(blocks.length === 3 && blocks[0] === progress[0] && blocks[1].classList.contains("conversation-operation-details")
        && blocks[2] === progress[1] && visibleProgress() && visibleProgress(nextProgressText), `${state} moved or folded public progress`);
      check(!document.querySelector(".conversation-unfinished-content") && visibleDraftContent(shortText), `${state} folded the short unfinished reply`);
      if (state === "running") originalProgress = progress;
      else check(progress.every((element, index) => element === originalProgress[index]), `${state} replaced original public progress nodes`);
      if (state === "error") check(document.querySelector(".conversation-unfinished-output")?.textContent.includes("本轮未完成"), "Failure lost unfinished status");
      if (state === "cancelled") check(document.querySelector(".conversation-unfinished-output")?.textContent.includes("本轮已停止")
        && !document.querySelector(".transcript-error"), "Stop lost its status or invented a failure");
      check(progress[1].compareDocumentPosition(document.querySelector(".conversation-answer")) & Node.DOCUMENT_POSITION_FOLLOWING,
        `${state} moved the reply ahead of the public explanation`);
    }
    cases.push("terminal progress continuity");

    await render(transcript({ ...run, state: "completed", error: null,
      steps: [{ id: "agent-message", kind: "assistant", title: "答复", status: "completed", messagePhase: "final_answer", text: content }] }));
    check(document.querySelector(".conversation-answer").checkVisibility() && !document.querySelector(".conversation-unfinished-content"), "completed final answer was folded by its language or fields");
    cases.push("completed answer");

    let mode = "recorded", reads = 0;
    const api = { runtime: { getTaskFinalReply: async () => { reads++; if (mode === "error") throw Error("回复读取中断");
      if (mode === "absent") return null; if (mode === "mismatch") return receipt("foreign-task"); return receipt(); } } } as any;
    await render(boundary(api));
    await until(() => document.querySelector(".saved-task-reply")?.textContent.includes(finalText), "saved final reply was not preferred");
    check(document.querySelector(".saved-task-reply .conversation-answer").checkVisibility() && hiddenDraftContent()
      && visibleProgress(), "saved final and partial stream were mixed or public progress was folded");
    cases.push("saved final reply");

    await render(<FailedTaskReply api={{ runtime: { getTaskFinalReply: async () => receipt() } } as any}
      projectId="project" attempt={attempt} render={state => transcript({ ...run, state: "cancelled", error: null,
        steps: run.steps.filter(step => step.kind !== "error") }, state)} />);
    await until(() => document.querySelector(".saved-task-reply")?.textContent.includes(finalText), "cancelled saved final was lost");
    check(hiddenDraftContent() && visibleProgress() && document.querySelector(".saved-task-reply .conversation-answer").checkVisibility(),
      "cancelled partial became formal, saved final was folded or public progress was hidden");
    check(!document.querySelector(".transcript-error"), "cancellation created an automatic failure recovery control");
    cases.push("cancelled content");

    mode = "error";
    await render(boundary(api, { ...attempt, id: "task-error" }));
    await until(() => document.querySelector(".conversation-final-reply-error"), "read failure was not exposed");
    check(hiddenDraftContent() && visibleProgress() && !document.querySelector(".saved-task-reply")
      && document.body.textContent.includes("保存状态尚未核实")
      && !document.body.textContent.includes("未作为最终回复交付"), "unknown read was treated as a proven absent final or hid public progress");
    cases.push("unknown read");

    mode = "absent";
    document.querySelector<HTMLButtonElement>(".conversation-final-reply-error button").click();
    await until(() => document.body.textContent.includes("未作为最终回复交付"), "proven absent final was not distinguished");
    check(hiddenDraftContent() && visibleProgress(), "absent final promoted partial stream or hid public progress");
    cases.push("absent reply");

    mode = "mismatch";
    await render(boundary(api, { ...attempt, id: "task-mismatch" }));
    await until(() => document.querySelector(".conversation-final-reply-error")?.textContent.includes("归属不匹配"), "foreign receipt was accepted");
    check(!document.querySelector(".saved-task-reply") && hiddenDraftContent() && visibleProgress(), "foreign final leaked into this turn or hid public progress");
    cases.push("identity mismatch");

    for (const part of ["project", "thread", "turn"]) {
      const wrong = receipt("task-a", finalText, part === "project" ? "other-project" : "project",
        part === "thread" ? "other-thread" : "thread-a", part === "turn" ? "other-turn" : "turn-a");
      await render(boundary({ runtime: { getTaskFinalReply: async () => wrong } } as any));
      await until(() => document.querySelector(".conversation-final-reply-error")?.textContent.includes("归属不匹配"), `${part} identity was not checked`);
      check(!document.querySelector(".saved-task-reply") && hiddenDraftContent() && visibleProgress(), `${part} identity exposed an unrelated final or hid public progress`);
    }

    const pending = new Map();
    const delayedApi = { runtime: { getTaskFinalReply: (_project, taskId) => new Promise(resolve => {
      const waits = pending.get(taskId) ?? []; waits.push(resolve); pending.set(taskId, waits);
    }) } } as any;
    await render(boundary(delayedApi));
    await until(() => pending.has("task-a"), "first identity read did not start");
    const oldDetails = document.querySelector<HTMLDetailsElement>(".conversation-unfinished-content");
    oldDetails.querySelector<HTMLElement>("summary").click();
    check(oldDetails.open, "partial content was not available on demand");
    await render(boundary(delayedApi, { id: "task-b", threadId: "thread-b", turnId: "turn-b" }));
    await until(() => pending.has("task-b"), "new identity read did not start");
    check(!document.querySelector<HTMLDetailsElement>(".conversation-unfinished-content").open, "old open state exposed a new turn's partial content");
    pending.get("task-a").forEach(resolve => resolve(receipt("task-a", "OLD FOREIGN FINAL")));
    await tick();
    check(!document.body.textContent.includes("OLD FOREIGN FINAL") && !document.body.textContent.includes("未作为最终回复交付"), "late old receipt contaminated the new identity");
    pending.get("task-b").forEach(resolve => resolve(receipt("task-b", "CURRENT SAVED FINAL", "project", "thread-b", "turn-b")));
    await until(() => document.querySelector(".saved-task-reply")?.textContent.includes("CURRENT SAVED FINAL"), "current final was lost after identity switch");
    check(hiddenDraftContent() && visibleProgress(), "new partial stream was exposed or public progress was folded after final receipt");
    cases.push("late identity response");

    const detail = { thread: { id: "reader-thread", projectId: "project" }, turns: [{ id: "reader-failed", status: "failed", items: [
      { id: "reader-progress", type: "agentMessage", phase: "commentary", status: "completed", text: progressText },
      { id: "reader-content", type: "agentMessage", phase: null, status: "completed", text: content },
    ] }, { id: "reader-stopped", status: "interrupted", items: [
      { id: "reader-stopped-progress", type: "agentMessage", phase: "commentary", status: "completed", text: nextProgressText },
      { id: "reader-stopped-content", type: "agentMessage", phase: null, status: "completed", text: shortText },
    ] }, { id: "reader-done", status: "completed", items: [
      { id: "reader-final", type: "agentMessage", phase: "final_answer", status: "completed", text: finalText },
    ] }] } as any;
    await render(<><ConversationReader detail={detail} loading={false} error={null} onRetry={() => {}} onFork={() => {}} busy={false} />
      <ConversationTimeline run={run as any} /></>);
    const readerTurns = document.querySelectorAll(".conversation-reader-turn");
    check(hiddenDraftContent(content, readerTurns[0]) && visibleProgress(progressText, readerTurns[0])
      && visibleProgress(progressText, document.querySelector(".conversation-timeline")), "Reader or Timeline folded public progress or exposed a long failed draft");
    check(visibleProgress(nextProgressText, readerTurns[1]) && visibleDraftContent(shortText, readerTurns[1])
      && readerTurns[1].textContent.includes("本轮已停止") && !readerTurns[1].querySelector(".conversation-unfinished-content"),
      "Interrupted reader turn hid public progress or short unfinished output");
    const timelineFailures = [...document.querySelectorAll<HTMLElement>('.conversation-timeline [role="alert"]')]
      .filter(element => element.textContent.includes(failureReason));
    check(timelineFailures.length === 1 && timelineFailures[0].checkVisibility() && !timelineFailures[0].closest("details"),
      "Standalone Timeline folded or duplicated its readable failure reason");
    check([...document.querySelectorAll(".conversation-answer")].find(element => element.textContent === finalText)?.checkVisibility(), "completed reader answer was hidden");
    cases.push("reader and timeline");

    await render(<EarlierTurn projectId="project" taskBound entry={{ ...run, id: "historical-turn", createdAt: 100,
      artifactCount: 0, displayExecution: { steps: run.steps.filter(step => step.kind !== "assistant"),
        startedAt: run.startedAt, completedAt: run.completedAt, totalPublicSteps: 2, detailsTruncated: false } } as any} />);
    check(hiddenDraftContent() && visibleProgress(), "historical failed content exposed its draft or folded public progress");
    cases.push("history");

    await render(<EarlierTurn projectId="project" taskBound entry={{ ...run, id: "historical-commentary", createdAt: 100,
      artifactCount: 0, commentary: progressText, displayExecution: null } as any} />);
    check(hiddenDraftContent() && visibleProgress() && !document.querySelector(".conversation-progress-notes details"),
      "Historical commentary field was folded with the unfinished draft");
    cases.push("history commentary field");

    const task = { id: "work-task", projectId: "project", threadId: "thread-a", kind: "generation", goal: "继续当前章节", status: "blocked",
      deliveryStatus: "pending", error: failureRaw, version: 1, steps: [{ id: "writer-step", operation: "generateChapter", status: "blocked",
        target: { chapterId: "chapter", chapterNumber: 7 }, changes: [], currentAttemptId: "task-a", attempts: [{ ...attempt, status: "failed", error: failureRaw, startedAt: 1 }], readingProgress: null }],
      deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], evidence: [{ stepId: "writer-step", attemptId: "task-a", kind: "draft", referenceId: "saved-revision", state: "recorded" }],
        pendingActionIds: [], unsettledAttemptIds: [], deliveryReview: null }, resumeMode: "continueExecution",
      resumeBlockReason: null, rollbackableCommits: [], } as any;
    const panelApi = { runtime: { getTaskFinalReply: async () => null } } as any;
    await render(<WorkTaskPanel api={panelApi} projectId="project" tasks={[task]} activeCount={1} nextCursor={null} loadingMore={false}
      loaded reading={false} readError={null} onRetry={() => {}} revision={0} busy={false} agentRunning={false}
      dirty={false} onLoadMore={() => {}} onOpenResult={() => {}}
      conversation={{ threadId: "thread-a", run: run as any, renderCurrent: ({ finalReplyState }) => <>{transcript(run, finalReplyState)}{composer()}</>, renderHistory: () => null }} />);
    await until(() => document.body.textContent.includes("未作为最终回复交付"), "WorkPanel did not pass exact attempt reply evidence");
    const continueButton = document.querySelector<HTMLButtonElement>('[aria-label="重试当前任务"]');
    check(continueButton?.checkVisibility() && !continueButton.disabled && hiddenDraftContent() && visibleProgress(),
      "WorkPanel recovery or public progress was hidden by failed content");
    const resultButton = [...document.querySelectorAll<HTMLButtonElement>("button")].find(button => button.textContent === "查看第 7 章正文");
    check(resultButton?.checkVisibility() && !resultButton.closest("details"), "verified saved chapter was folded with failed content");
    const beforeRetry = retryCalls;
    continueButton.click(); check(retryCalls === beforeRetry + 1, "work recovery callback did not run");
    cases.push("work panel recovery");

    const exactDraft = "  生成第7章\n保留未发送草稿  ";
    const availabilityChecks: { id: string; props: Partial<React.ComponentProps<typeof Composer>>; reason: string | null; disabled: boolean }[] = [
      { id: "ready", props: {}, reason: null, disabled: false },
      { id: "qualified after failed turn", props: { agentStatus: { configured: true, state: "error" } }, reason: null, disabled: false },
      { id: "history loading", props: { availability: { ...readyAvailability, historyState: "loading" } }, reason: "正在读取这条对话", disabled: true },
      { id: "history failure with qualified model", props: { availability: { ...readyAvailability, historyState: "error" } }, reason: "这条对话的历史暂时无法读取", disabled: true },
      { id: "conversation changing", props: { availability: { ...readyAvailability, changingConversation: true } }, reason: "正在切换对话", disabled: true },
      { id: "archived conversation", props: { availability: { ...readyAvailability, archived: true } }, reason: "这条对话已归档", disabled: true },
      { id: "other conversation running", props: { availability: { ...readyAvailability, otherConversationRunning: true } }, reason: "另一条对话正在执行", disabled: true },
      { id: "runtime unavailable with qualified model", props: { availability: { ...readyAvailability, runtimeReady: false } }, reason: "创作环境暂未连接", disabled: true },
      { id: "model qualification unavailable", props: { availability: { ...readyAvailability, modelReady: false }, agentStatus: { configured: false, state: "error" } }, reason: "模型连接尚不可用", disabled: true },
      { id: "model configuration applying", props: { agentConfigSaving: true }, reason: "正在应用模型配置", disabled: true },
    ];
    for (const row of availabilityChecks) {
      await render(composer({ composer: exactDraft, retryDisabled: row.disabled, ...row.props }));
      const input = document.querySelector<HTMLTextAreaElement>('[aria-label="创作指令"]');
      const send = document.querySelector<HTMLButtonElement>('[aria-label="发送创作指令"]');
      const reason = document.querySelector<HTMLElement>(".composer-draft-notice");
      check(input && !input.disabled && input.value === exactDraft && send?.disabled === row.disabled,
        `${row.id} changed drafting or send permission`);
      check(row.reason ? reason?.checkVisibility() && reason.textContent.includes(row.reason)
        && input.getAttribute("aria-describedby")?.includes(reason.id) : !reason,
        `${row.id} has a missing, misleading or inaccessible reason`);
      if (row.id !== "model qualification unavailable" && row.reason) {
        check(!reason.textContent.includes("配置或验证连接") && !reason.textContent.includes("模型连接尚不可用"),
          `${row.id} incorrectly asks to verify a usable model`);
      }
      const before = submissionCalls, beforeRetry = retryCalls;
      input.focus(); input.setSelectionRange(2, 5);
      input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
      send.click();
      check(submissionCalls === before + (row.disabled ? 0 : 2) && input.value === exactDraft
        && input.selectionStart === 2 && input.selectionEnd === 5, `${row.id} dispatched a blocked request or changed the draft`);
      const retry = document.querySelector<HTMLButtonElement>('[aria-label="重试当前任务"]');
      check(retry?.disabled === row.disabled, `${row.id} changed the caller's retry gate`);
      retry.click();
      check(retryCalls === beforeRetry + (row.disabled ? 0 : 1), `${row.id} bypassed the retry gate`);
      availabilityCases.push(row.id);
    }
    await render(composer({ composer: exactDraft, canSteer: true, agentStatus: { configured: true, state: "running" },
      availability: { ...readyAvailability, runtimeReady: false, historyState: "error" } }));
    const steer = document.querySelector<HTMLButtonElement>('[aria-label="追加创作指令"]');
    const stop = document.querySelector<HTMLButtonElement>('[aria-label="停止当前执行"]');
    check(steer && !steer.disabled && stop && !stop.disabled && !document.querySelector(".composer-draft-notice"),
      "availability presentation changed active-turn steering or stop permission");
    const beforeSteer = submissionCalls, beforeStop = interruptCalls;
    steer.click(); stop.click();
    check(submissionCalls === beforeSteer + 1 && interruptCalls === beforeStop + 1,
      "active-turn steering or stop no longer dispatches its existing callback");
    availabilityCases.push("active turn steering and stop");
    await render(composer({ composer: exactDraft }));
    check(!document.querySelector<HTMLButtonElement>('[aria-label="发送创作指令"]').disabled
      && !document.querySelector(".composer-draft-notice")
      && document.querySelector<HTMLTextAreaElement>('[aria-label="创作指令"]').value === exactDraft,
      "ready state retained a stale error or lost the draft");
    availabilityCases.push("availability restored");
    window.testResult = { passed: true, cases, availabilityCases };
  } catch (error) { window.testResult = { passed: false, error: String(error), dom: document.body.innerHTML }; }
  finally { root?.unmount(); }
})();
