import { emptyCreationIssueSummary } from "../helpers/creation-issues";
import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { WorkTaskPanel } from "../../../src/WorkTaskPanel";
import { ConversationTimeline } from "../../../src/ConversationTimeline";
import { createAgentRun } from "../../../src/agent-events";

const tick = () => new Promise(resolve => setTimeout(resolve, 35));
const check = (value, message) => { if (!value) throw Error(message); };
const until = async (predicate, message = "Presentation did not settle") => { for (let i = 0; i < 100 && !predicate(); i++) await tick(); check(predicate(), message); };
const root = createRoot(document.getElementById("root"));
const controls = [], copied = [], neutralInteractionsFollow = [], explicitReadingPreserved = [];
Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: async text => { copied.push(text); } } });
let holdReplies = false, panelVersion = 0;
const pendingReplies = [];
const api = { runtime: { getTaskFinalReply: async (projectId, taskId) => {
  const receipt = { projectId, taskId, state: "recorded",
    binding: { threadId: `thread-${taskId}`, turnId: `turn-${taskId}`, text: "保存的公开回复。".repeat(45) }, recordedAt: 1 };
  if (holdReplies) return new Promise(resolve => pendingReplies.push(() => resolve(receipt)));
  return receipt;
} } };
const step = id => ({ id: `step-${id}`, operation: "generateChapter", target: { chapterId: "chapter-two" }, status: "completed",
  attempts: [{ id, threadId: `thread-${id}`, turnId: `turn-${id}`, status: "completed", startedAt: id === "live" ? 20 : Number(id),
    completedAt: 21, error: null }], changes: [], outputRefs: [], readingProgress: null });
const task = id => ({ id: `work-${id}`, projectId, threadId: "root-thread", goal: id === "live" ? "作者要修改第二章结尾。" : `作者历史指令${id}`,
  kind: "direct", status: "completed", version: 1, error: null, resumeMode: null, resumeBlockReason: null,
  deliveryStatus: "ready_for_review", deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], evidence: [], pendingActionIds: [], unsettledAttemptIds: [] }, steps: [step(id)] });
let projectId = "A";
let tasks = [task("live"), ...[7, 6, 5, 4, 3, 2, 1].map(id => task(String(id)))];
let run = { ...createAgentRun("internal analysis instruction"), state: "running", threadId: "thread-live", turnId: "turn-live",
  answer: "开始写公开回复。", steps: [
    { id: "commentary", kind: "system", title: "Agent 进度", messagePhase: "commentary", text: "我会保留现有线索，只调整结尾。", status: "completed" },
    { id: "reason", kind: "system", sourceType: "reasoning", title: "正在分析", status: "completed" },
    { id: "tool", kind: "tool", title: "读取本章正文", detail: '{"internalTaskId":"fixture-id"}', status: "running" },
  ] };
async function render() {
  root.render(<div className="conversation-rail-scroll" style={{ width: 360, height: 240, maxHeight: 240, overflowY: "auto" }}>
    <WorkTaskPanel key={panelVersion} api={api} projectId={projectId} tasks={tasks} activeCount={1} nextCursor={null} loadingMore={false} loaded reading={false}
      readError={null} onRetry={() => {}} revision={1} busy={false} agentRunning dirty={false} onUndo={() => { throw Error("Unexpected undo"); }} onLoadMore={() => {}}
      conversation={{ run, threadId: "root-thread", renderCurrent: ({ taskBound }) => <div className="fixture-live">
        {!taskBound && <article className="user-message">{run.prompt}</article>}
        <ConversationTimeline key={`${run.threadId}:${run.turnId}`} run={run} />
        <article className="assistant-message">{run.answer}<button className="fixture-copy" type="button" onClick={() => void navigator.clipboard.writeText(run.answer)}>复制回复</button><input className="fixture-reading-input" aria-label="下一轮输入" defaultValue="尚未发送的草稿" /></article>{run.error && <p role="alert">{run.error}</p>}
      </div>, renderHistory: entry => <p>{entry.prompt}</p> }} />
  </div>);
  await tick(); await tick();
}
const rail = () => document.querySelector(".conversation-rail-scroll");
const timeline = () => document.querySelector(".conversation-timeline");
const text = () => document.body.innerText;
(async () => {
  try {
    await render(); await until(() => !text().includes("正在读取保存的回复"));
    check(text().split("作者要修改第二章结尾。").length - 1 === 1 && !text().includes("internal analysis instruction"), "Internal prompt replaced or duplicated author input");
    check(timeline().tagName === "SECTION" && timeline().querySelector(".conversation-process-header").innerText.includes("读取本章正文")
      && text().includes("我会保留现有线索"), "Default flow hid current progress");
    const marker = timeline().querySelector(".conversation-state-marker");
    for (const theme of ["light", "dark"]) {
      document.documentElement.dataset.theme = theme;
      for (const width of [320, 420]) {
        rail().style.width = `${width}px`; await tick();
        const bounds = marker.getBoundingClientRect();
        check(bounds.width === 8 && bounds.height === 8 && getComputedStyle(marker).flexGrow === "0"
          && getComputedStyle(marker).animationName === "none", "State marker stretched or animated under global CSS");
        check(getComputedStyle(timeline().querySelector(".conversation-activity-label")).animationName
          === (matchMedia("(prefers-reduced-motion: reduce)").matches ? "none" : "conversation-activity-breathe"),
          "Activity text did not respect motion preferences");
        check(timeline().querySelector(".conversation-process-header").scrollWidth <= timeline().querySelector(".conversation-process-header").clientWidth,
          "Compact activity overflowed the conversation rail");
      }
    }
    delete document.documentElement.dataset.theme; rail().style.removeProperty("width");
    tasks = tasks.map(t => t.id === "work-live" ? { ...t, status: "running", deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], ...t.deliveryReport, unsettledAttemptIds: ["pending-usage"] } } : t);
    await render(); check(!text().includes("用量记录待核对"), "Running turn exposed ordinary accounting noise");
    check(getComputedStyle(timeline().querySelector(".conversation-process-header")).display === "grid"
      && getComputedStyle(timeline().querySelector(".transcript-steps")).overflowY === "visible",
    "Global styles displaced the activity summary or created an inner scroll owner");
    check(!text().includes("fixture-id") && !timeline().querySelector(".transcript-steps").textContent.includes("正在分析"), "Default UI showed raw parameters or reasoning rows");
    check(text().includes("我会保留现有线索"), "Progress should be directly readable");
    const diagnostics = timeline().querySelector<HTMLDetailsElement>(".conversation-operation-details");
    check(!diagnostics.open && timeline().innerText.includes("我会保留现有线索")
      && !timeline().innerText.includes("Agent 进度") && !timeline().innerText.includes("执行记录")
      && timeline().querySelector(".conversation-process-header").innerText.includes("秒"), "Expanded process hid actual progress or exposed internal row labels");
    diagnostics.querySelector<HTMLElement>("summary").click(); await until(() => diagnostics.open);
    check(diagnostics.innerText.includes("读取本章正文"), "Diagnostic operation disappeared");
    diagnostics.querySelector<HTMLElement>("summary").click(); await until(() => !diagnostics.open);
    run = { ...run, state: "completed", completedAt: Date.now(), steps: run.steps.map(s => ({ ...s, status: "completed" })) };
    await render(); check(text().includes("我会保留现有线索") && !diagnostics.open, "Completion hid the narrative or expanded operation details");
    check(getComputedStyle(timeline().querySelector(".conversation-activity-label")).animationName === "none",
      "Finished activity kept animating");
    run = { ...run, state: "error", error: "precise failure" };
    tasks = tasks.map(t => t.id === "work-live" ? { ...t, status: "blocked", error: "Error: precise failure" } : t);
    await render();
    check(text().includes("用量记录待核对"), "Blocked turn hid unsettled usage");
    check(document.querySelectorAll('[role="alert"]').length === 1, "Same bound failure was displayed twice");
    tasks = tasks.map(t => t.id === "work-live" ? { ...t, error: "other durable failure" } : t);
    await render(); check(document.querySelectorAll('[role="alert"]').length === 2, "Different error evidence was hidden");
    check(!diagnostics.open, "Operation details unexpectedly reopened");
    run = { ...run, state: "running", error: null, steps: run.steps.map(s => ({ ...s, status: s.id === "tool" ? "running" : "completed" })) };
    tasks = tasks.map(t => t.id === "work-live" ? { ...t, status: "running", error: null } : t);
    await render(); check(!diagnostics.open && text().includes("我会保留现有线索"), "Retry lost detail choice or narrative");
    run = { ...run, state: "cancelled", steps: run.steps.map(s => ({ ...s, status: s.id === "tool" ? "cancelled" : "completed" })) };
    await render();
    check(timeline().querySelector(".conversation-work-item.cancelled small")?.textContent === "已停止"
      && !timeline().querySelector(".conversation-work-item.completed small"), "Stopped unfinished tool claimed success or erased prior completion");
    run = { ...run, state: "running", steps: run.steps.map(s => ({ ...s, status: s.id === "tool" ? "running" : "completed" })) };
    await render();
    rail().scrollTop = rail().scrollHeight; rail().dispatchEvent(new Event("scroll")); await tick();
    run = { ...run, answer: run.answer + "持续输出的新内容。".repeat(90) }; await render();
    check(rail().scrollHeight - rail().scrollTop - rail().clientHeight < 3, "Bottom did not follow streaming reply");
    const neutralFailures = [];
    const neutral = [
      ["pointer-copy", () => { const button = document.querySelector<HTMLButtonElement>(".fixture-copy"); button.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, pointerType: "mouse" })); button.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, pointerType: "mouse" })); button.click(); }],
      ["Tab", () => document.querySelector(".fixture-copy").dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }))],
      ["Ctrl+C", () => document.querySelector(".fixture-copy").dispatchEvent(new KeyboardEvent("keydown", { key: "c", ctrlKey: true, bubbles: true }))],
      ["Meta+C", () => document.querySelector(".fixture-copy").dispatchEvent(new KeyboardEvent("keydown", { key: "c", metaKey: true, bubbles: true }))],
      ["Ctrl+Shift+Space", () => rail().dispatchEvent(new KeyboardEvent("keydown", { key: " ", ctrlKey: true, shiftKey: true, bubbles: true }))],
      ["Meta+Shift+Space", () => rail().dispatchEvent(new KeyboardEvent("keydown", { key: " ", metaKey: true, shiftKey: true, bubbles: true }))],
      ["input-ArrowUp", () => document.querySelector(".fixture-reading-input").dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowUp", bubbles: true }))],
      ["letter", () => rail().dispatchEvent(new KeyboardEvent("keydown", { key: "a", bubbles: true }))],
      ["wheel-down", () => rail().dispatchEvent(new WheelEvent("wheel", { deltaY: 80, bubbles: true }))],
      ["wheel-horizontal", () => rail().dispatchEvent(new WheelEvent("wheel", { deltaX: 80, bubbles: true }))],
      ["zoom-wheel", () => rail().dispatchEvent(new WheelEvent("wheel", { deltaY: -80, ctrlKey: true, bubbles: true }))],
    ];
    for (const [name, action] of neutral) {
      document.querySelector<HTMLButtonElement>(".conversation-jump-latest")?.click();
      rail().scrollTop = rail().scrollHeight; rail().dispatchEvent(new Event("scroll")); await tick();
      const previousTop = rail().scrollTop; action(); await tick();
      check(Math.abs(rail().scrollTop - previousTop) < 2, `${name} unexpectedly changed the setup reading position`);
      run = { ...run, answer: run.answer + "交互后的新公开内容。".repeat(35) }; await render();
      if (rail().scrollHeight - rail().scrollTop - rail().clientHeight >= 3 || document.querySelector(".conversation-jump-latest")) neutralFailures.push(name);
      else neutralInteractionsFollow.push(name);
    }
    check(copied.length === 1 && copied[0].includes("持续输出的新内容"), "Copy did not preserve the actual visible reply");
    check(!neutralFailures.length, `Neutral interactions stopped streaming follow: ${neutralFailures.join(", ")}`);
    const resumeFollowing = async () => {
      document.querySelector<HTMLButtonElement>(".conversation-jump-latest")?.click();
      rail().scrollTop = rail().scrollHeight; rail().dispatchEvent(new Event("scroll")); await tick();
      check(rail().scrollHeight - rail().scrollTop - rail().clientHeight < 3, "Reading case did not begin at latest");
    };
    const readingDetails = () => document.querySelector<HTMLDetailsElement>("[data-work-task-id='work-live'] .conversation-operation-details");
    for (const method of ["pointer", "keyboard"]) {
      await resumeFollowing(); const summary = readingDetails().querySelector<HTMLElement>(":scope > summary");
      summary.scrollIntoView({ block: "center" }); await tick(); const previousTop = rail().scrollTop;
      const summaryRect = summary.getBoundingClientRect(), railRect = rail().getBoundingClientRect();
      check(summaryRect.top >= railRect.top && summaryRect.bottom <= railRect.bottom, "Details intent setup clicked an offscreen summary");
      check(!readingDetails().open && summary.isConnected, `Visible details ${method} did not begin closed`);
      if (method === "pointer") { summary.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, button: 0, pointerType: "mouse" })); summary.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, button: 0, pointerType: "mouse" })); }
      else { summary.focus({ preventScroll: true }); summary.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); await tick(); }
      if (!readingDetails().open) summary.click(); await until(() => readingDetails().open, `Visible details ${method} did not open`);
      check(Math.abs(rail().scrollTop - previousTop) < 3, `Opening details by ${method} pulled the reading position: before=${previousTop}, after=${rail().scrollTop}, distance=${rail().scrollHeight - rail().scrollTop - rail().clientHeight}`);
      run = { ...run, answer: run.answer + "展开过程中仍保持阅读。".repeat(35) }; await render();
      check(readingDetails().open && Math.abs(rail().scrollTop - previousTop) < 3 && document.querySelector(".conversation-jump-latest"), `Details ${method} lost its reading anchor during output`);
      explicitReadingPreserved.push(`details-${method}`); await tick(); await tick(); readingDetails().querySelector<HTMLElement>(":scope > summary").click(); await until(() => !readingDetails().open, `Visible details ${method} did not close`); await tick(); await tick();
    }
    await resumeFollowing(); const range = document.createRange(), selection = window.getSelection();
    const selectionSource = readingDetails().querySelector(":scope > summary"), selectedText = selectionSource.textContent;
    selectionSource.scrollIntoView({ block: "center" }); await tick(); const selectedTop = rail().scrollTop;
    check(selectionSource.getBoundingClientRect().top >= rail().getBoundingClientRect().top && selectionSource.getBoundingClientRect().bottom <= rail().getBoundingClientRect().bottom, "Selection setup was not visible");
    range.selectNodeContents(selectionSource); selection.removeAllRanges(); selection.addRange(range);
    document.dispatchEvent(new Event("selectionchange")); await tick(); check(selection.toString() === selectedText, "Real noncollapsed selection setup failed");
    run = { ...run, answer: run.answer + "选择原文期间仍保持阅读。".repeat(35) }; await render();
    check(Math.abs(rail().scrollTop - selectedTop) < 3 && selection.toString() === selectedText && document.querySelector(".conversation-jump-latest"), "Streaming pulled the active text selection");
    explicitReadingPreserved.push("selection"); selection.removeAllRanges(); await resumeFollowing();
    rail().scrollTop -= 90; rail().dispatchEvent(new Event("scroll")); await tick(); const scrollOnlyTop = rail().scrollTop;
    run = { ...run, answer: run.answer + "实际滚动阅读位置保持。".repeat(35) }; await render();
    check(Math.abs(rail().scrollTop - scrollOnlyTop) < 3 && document.querySelector(".conversation-jump-latest"), "Scroll-only reading was not authoritative");
    explicitReadingPreserved.push("scroll-only"); await resumeFollowing();
    rail().dispatchEvent(new WheelEvent("wheel", { deltaY: -80 })); rail().scrollTop = 100; rail().dispatchEvent(new Event("scroll")); await tick();
    const top = rail().scrollTop;
    run = { ...run, answer: run.answer + "不会抢走历史阅读位置。".repeat(90) }; await render();
    check(Math.abs(rail().scrollTop - top) < 3 && document.querySelector(".conversation-jump-latest"), "Streaming stole history reading position");
    const anchor = [...document.querySelectorAll('[data-conversation-row]')].find(row => row.getBoundingClientRect().bottom > rail().getBoundingClientRect().top);
    const offset = anchor.getBoundingClientRect().top - rail().getBoundingClientRect().top;
    tasks = [...tasks, task("0")]; await render();
    check(Math.abs(anchor.getBoundingClientRect().top - rail().getBoundingClientRect().top - offset) < 3, "Loading earlier rows lost the visible anchor");
    document.querySelector(".conversation-jump-latest").click(); await tick();
    check(rail().scrollHeight - rail().scrollTop - rail().clientHeight < 3 && !document.querySelector(".conversation-jump-latest"), "Return to latest did not resume following");
    check(!document.querySelector(".work-task-execution > ol"), "Execution numbering was confused with chapter numbers");
    check([...document.querySelectorAll<HTMLDetailsElement>(".work-execution-step")].every(el => !el.open), "Internal step details should start collapsed");
    check(!document.querySelector(".work-task-execution"), "Internal step count dominated task summary");
    const snapshot = { prompt: "作者要修改第二章结尾。", process: timeline().querySelector(".conversation-process-header").innerText.replace(/\d+秒/g, "用时"),
      publicProgressVisible: timeline().tagName === "SECTION" && text().includes("我会保留现有线索"), flow: "对话中的操作原地展开",
      privateArgumentsHidden: !text().includes("fixture-id"), preservedReading: true, controlsExecuted: controls.length, neutralInteractionsFollow, explicitReadingPreserved };
    projectId = "B"; run = createAgentRun(); tasks = []; await render();
    check(!text().includes("作者要修改") && !document.querySelector(".conversation-jump-latest"), "Project switch leaked conversation state");
    projectId = "single"; tasks = [task("single")]; await render();
    await until(() => text().includes("保存的公开回复"));
    rail().dispatchEvent(new WheelEvent("wheel", { deltaY: -60 })); rail().scrollTop = 20; rail().dispatchEvent(new Event("scroll")); await tick();
    check(Math.abs(rail().scrollTop - 20) < 3, "Single-record reading setup did not scroll");
    holdReplies = true; panelVersion++; await render(); await until(() => pendingReplies.length > 0);
    check(text().includes("正在读取保存的回复"), "Remount did not exercise short loading placeholder");
    pendingReplies.splice(0).forEach(resolve => resolve()); await until(() => text().includes("保存的公开回复") && Math.abs(rail().scrollTop - 20) < 3);
    snapshot.restoredAfterShortPlaceholder = true;
    holdReplies = false; projectId = "A"; panelVersion++;
    const writer = step("live");
    tasks = [{ ...task("live"), status: "running", steps: [writer,
      { ...step("followup"), status: "queued", attempts: [] }] }];
    run = { ...createAgentRun(), state: "completed", threadId: "thread-live", turnId: "turn-live", answer: "内部写作已保存，后续检查待执行。" };
    await render();
    check(!document.querySelector(".fixture-live")?.closest("details"), "Public progress was hidden in execution details");
    tasks = [{ ...tasks[0], steps: [{ ...writer, status: "superseded", attempts: [{ ...writer.attempts[0], status: "failed", error: "invalid arguments" }] }, tasks[0].steps[1]] }];
    run = { ...run, state: "error", error: "invalid arguments" };
    await render();
    check(!document.querySelector(".fixture-live")?.closest("details"), "Superseded failure hid its public reply");
    tasks = [{ ...tasks[0], steps: [writer, tasks[0].steps[1]] }];
    run = createAgentRun(); panelVersion++; await render();
    await until(() => document.body.textContent.includes("保存的公开回复"));
    const savedReply = [...document.querySelectorAll(".saved-task-reply")].find(el => el.textContent.includes("保存的公开回复") && !el.closest("details"));
    check(savedReply, "Restored public progress was hidden");
    tasks = [{ ...tasks[0], status: "completed", steps: [writer] }]; panelVersion++; await render();
    await until(() => text().includes("保存的公开回复"));
    check(!document.querySelector(".work-task-execution"), "Last reply was hidden with intermediate records");
    root.unmount(); window.testResult = { passed: true, snapshot };
  } catch (error) { window.testResult = { passed: false, error: error.stack + `\nScroll: ${rail()?.scrollTop}/${rail()?.scrollHeight}; pending: ${pendingReplies.length}; reply: ${text().includes("保存的公开回复")}` + "\nDOM:" + text() }; }
})();
