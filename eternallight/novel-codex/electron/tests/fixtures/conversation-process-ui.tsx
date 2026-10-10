import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { ConversationTimeline } from "../../../src/ConversationTimeline";
import { ExecutionDiagnostics } from "../../../src/ExecutionDiagnostics";
import { createAgentRun } from "../../../src/agent-events";
const check = (value, message) => { if (!value) throw Error(message); };
const tick = () => new Promise(resolve => setTimeout(resolve, 40));
const capture = async (name: string) => {
  window.testScreenshot = name;
  for (let i = 0; i < 20 && window.testScreenshot; i++) await tick();
};
const viewport = async (width: number, height: number) => {
  window.testViewport = { width, height };
  for (let i = 0; i < 30 && window.testViewport; i++) await tick();
  check(!window.testViewport, "Native screenshot viewport did not settle");
  await tick();
};
const colorChannels = (color: string) => {
  const channels = color.match(/[\d.]+/g)?.map(Number);
  check(channels?.length >= 3, `Unsupported computed CSS color: ${color}`);
  return [channels[0], channels[1], channels[2], channels[3] ?? 1];
};
const compositeColor = (foreground: number[], background: number[]) => foreground.slice(0, 3)
  .map((channel, index) => channel * foreground[3] + background[index] * (1 - foreground[3]));
const textContrast = (element: HTMLElement) => {
  const layers = [];
  for (let current: HTMLElement | null = element; current; current = current.parentElement) layers.push(colorChannels(getComputedStyle(current).backgroundColor));
  const background = layers.reverse().reduce((result, layer) => compositeColor(layer, result), [255, 255, 255]);
  const foreground = compositeColor(colorChannels(getComputedStyle(element).color), background);
  const luminance = color => color.map(channel => {
    const srgb = channel / 255;
    return srgb <= .04045 ? srgb / 12.92 : ((srgb + .055) / 1.055) ** 2.4;
  }).reduce((sum, channel, index) => sum + channel * [.2126, .7152, .0722][index], 0);
  const values = [luminance(foreground), luminance(background)].sort((a, b) => b - a);
  return (values[0] + .05) / (values[1] + .05);
};
const steps = Array.from({ length: 35 }, (_, i) => i % 4 === 0
  ? { id: `progress-${i}`, kind: "system", title: "Agent 进度", messagePhase: "commentary", text: `正在核对第${i + 1}处灯税线索与已保存正文。`, status: "completed" }
  : { id: `tool-${i}`, kind: "tool", title: i % 2 ? "读取写作资料" : "查询故事证据", status: i === 1 ? "failed" : "completed", detail: `诊断记录${i}` });
const run = { ...createAgentRun(), state: "completed", threadId: "thread", turnId: "turn", startedAt: 1000, completedAt: 51000, steps };
const root = createRoot(document.getElementById("root"));
root.render(<aside className="conversation-rail" style={{ width: 320 }}><ConversationTimeline run={run} /></aside>);
(async () => { try {
  await tick(); await tick();
  const timeline = document.querySelector<HTMLDetailsElement>(".conversation-timeline");
  check(timeline.tagName === "SECTION" && timeline.querySelector(".conversation-process-header").innerText.includes("已处理 · 50秒") && document.body.innerText.includes("核对第1处"), "Completed narrative must remain directly readable");
  const blocks = [...document.querySelector(".conversation-process-flow").children];
  check(blocks.length === 18 && blocks.every((block, i) => block.classList.contains(i % 2 === 0 ? "conversation-progress-notes" : "conversation-operation-details")), "Progress and operations lost chronological order");
  const diagnostics = [...document.querySelectorAll<HTMLDetailsElement>(".conversation-operation-details")];
  check(diagnostics.length === 9 && diagnostics.every(d => !d.open), "Only adjacent calls should share a collapsed summary");
  check(document.body.innerText.includes("核对第1处") && !document.body.innerText.includes("Agent 进度") && document.body.innerText.includes("部分操作未完成"), "Progress and failure summary must remain readable");
  check(document.querySelectorAll(".conversation-progress-notes .conversation-markdown").length === 9, "Public progress text was dropped");
  check(document.querySelectorAll(".conversation-work-item").length === 26 && document.querySelectorAll(".conversation-work-item.failed").length === 1, "Diagnostics lost calls or failure evidence");
  diagnostics[0].querySelector<HTMLElement>("summary").click(); await tick();
  check(diagnostics[0].open && diagnostics[0].innerText.includes("未完成") && diagnostics.slice(1).every(d => !d.open), "Inspection should expand only the selected operation group");
  check(diagnostics[0].querySelectorAll(".conversation-work-item").length === 3, "Grouping crossed a commentary boundary");
  const rail = document.querySelector<HTMLElement>(".conversation-rail");
  if (rail.scrollWidth > rail.clientWidth + 1) await capture("conversation-process-overflow");
  check(rail.scrollWidth <= rail.clientWidth + 1, `Process overflows narrow rail (${rail.scrollWidth}/${rail.clientWidth}): ${[...rail.querySelectorAll<HTMLElement>("*")]
    .filter(node => node.getBoundingClientRect().right > rail.getBoundingClientRect().right + 1)
    .map(node => { const style = getComputedStyle(node); return `${node.className}:${Math.round(node.getBoundingClientRect().width)}:min=${style.minWidth}:flex=${style.flex}:display=${style.display}:overflow=${style.overflowX}`; }).join(",")}`);
  root.render(<ConversationTimeline key="host-records" run={{ ...run, state: "running", steps: [
    { id: "workflow:prepared", kind: "system", title: "准备创作上下文", detail: "planner · 0 条时态事实 · 任务 task-fixture", status: "completed" },
    { id: "arbitrary-host-event", kind: "system", title: "任意内部事件", detail: "raw-host-payload", status: "completed" },
    { id: "meaningful-progress", kind: "system", title: "Agent 进度", messagePhase: "commentary", text: "已核对第六章的保存记录，本轮只说明状态。", status: "completed" },
    { id: "input", kind: "system", title: "作者指令", sourceType: "userMessage", status: "completed" },
    { id: "input-event", kind: "system", title: "执行步骤", detail: "userMessage", status: "completed" },
  ] }} />);
  await tick();
  check(document.body.innerText.includes("已核对第六章的保存记录，本轮只说明状态。"), "Model public explanation must be directly readable");
  check(!/准备创作上下文|任意内部事件|raw-host-payload|已准备本轮创作所需/.test(document.body.innerText), "Host events must not impersonate model explanations");
  check(!document.querySelector(".conversation-operation-details") && !document.querySelector(".conversation-technical-record"), "Normal internal records must leave the conversation");
  root.render(<ExecutionDiagnostics run={{ steps: [
    { id: "workflow:prepared", kind: "system", title: "准备创作上下文", detail: "planner · 任务 task-fixture", status: "completed" },
    { id: "private", kind: "system", title: "正在分析", sourceType: "reasoning", detail: "PRIVATE_REASONING", status: "completed" },
    { id: "author", kind: "system", title: "作者指令", sourceType: "userMessage", detail: "PRIVATE_AUTHOR", status: "completed" },
    { id: "steer:1", kind: "system", title: "已追加创作指令", detail: "PRIVATE_STEER", status: "completed" },
  ], history: [{ id: "history", state: "completed", displayExecution: { steps: [{ id: "compact", kind: "system", title: "压缩上下文", detail: "history-fixture", status: "completed" }], totalPublicSteps: 1, detailsTruncated: false } }] }} />);
  await tick();
  document.querySelector<HTMLElement>(".execution-diagnostics > details > summary").click(); await tick();
  document.querySelector<HTMLElement>(".execution-diagnostic-record > summary").click(); await tick();
  check(document.body.innerText.includes("task-fixture"), "Diagnostic evidence must remain inspectable outside conversation");
  check(!/PRIVATE_REASONING|PRIVATE_AUTHOR|PRIVATE_STEER/.test(document.body.textContent), "Private input leaked into diagnostics");
  const older = document.querySelectorAll<HTMLDetailsElement>(".execution-diagnostics > details")[1];
  older.querySelector<HTMLElement>("summary").click(); await tick();
  older.querySelector<HTMLElement>(".execution-diagnostic-record > summary").click(); await tick();
  check(document.body.innerText.includes("history-fixture"), "Retained history must remain inspectable");
  await capture("execution-diagnostics");
  const maintenance = { id: "context-compaction", kind: "system", sourceType: "contextCompaction", title: "压缩上下文", detail: "raw-maintenance", status: "running" };
  root.render(<ConversationTimeline run={{ ...run, state: "running", completedAt: null, steps: [maintenance] }} />); await tick();
  check(document.querySelector(".conversation-context-notice")?.textContent.includes("正在整理对话上下文") && !document.body.textContent.includes("raw-maintenance"), "Actual maintenance should have one lightweight status without raw payload");
  root.render(<ConversationTimeline run={{ ...run, steps: [{ ...maintenance, status: "completed" }] }} />); await tick();
  check(document.querySelector(".conversation-context-notice")?.textContent.includes("对话上下文已自动整理")
    && !document.querySelector(".conversation-operation-details") && !document.body.textContent.includes("raw-maintenance"), "Completed actual maintenance should remain a plain chronological notice");
  root.render(<ConversationTimeline run={{ ...run, steps: [{ ...maintenance, sourceType: undefined, status: "completed" }] }} />); await tick();
  check(!document.querySelector(".conversation-timeline"), "An arbitrary host title must not impersonate an actual compaction event");
  const failureRaw = "provider_output_item_limit: ERROR_FIXTURE_RAW";
  const failureReason = "模型返回的一条结果超过可处理的长度上限，本次任务未完成。";
  const failureProgress = { id: "failure-progress", kind: "system", title: "Agent 进度", messagePhase: "commentary",
    text: "已检查当前章的保存记录，接下来会核对需要继续处理的位置。", status: "completed" };
  const failureNext = { ...failureProgress, id: "failure-next", text: "本轮还没有完成所有修改，已保存的内容仍可继续查看。" };
  const failureRun = { ...run, state: "error", steps: [failureProgress, { ...maintenance, status: "failed" },
    { id: "failure", kind: "error", title: "执行失败", status: "failed", detail: failureRaw }, failureNext,
  ] };
  root.render(<ConversationTimeline run={failureRun} />); await tick();
  check(document.querySelector(".conversation-context-notice")?.textContent.includes("对话整理未完成"), "Failed compaction lost its actual state");
  const failureBlocks = [...document.querySelector(".conversation-process-flow").children];
  const failureNotes = [...document.querySelectorAll<HTMLElement>(".conversation-progress-notes")];
  check(failureBlocks.length === 4 && failureBlocks[0] === failureNotes[0] && failureBlocks[1].classList.contains("conversation-context-notice")
    && failureBlocks[2].getAttribute("role") === "alert" && failureBlocks[3] === failureNotes[1], "Failure changed public progress/context/error chronology");
  check(failureNotes.length === 2 && failureNotes.every((note, index) => note.checkVisibility() && !note.closest("details")
    && note.textContent === [failureProgress.text, failureNext.text][index])
    && !document.body.textContent.includes("查看未完成的过程说明"), "Failure folded public progress into repeated disclosures");
  const failureAlert = document.querySelector<HTMLElement>('.conversation-process-flow > [role="alert"]');
  check(failureAlert?.checkVisibility() && !failureAlert.closest("details") && failureAlert.innerText.includes(failureReason)
    && document.querySelectorAll('[role="alert"]').length === 1, "Readable failure reason requires expanding an operation or was duplicated");
  const rawDetails = failureAlert.querySelector<HTMLDetailsElement>(":scope > details");
  check(rawDetails && !rawDetails.open && rawDetails.querySelector("pre").textContent === failureRaw
    && !rawDetails.querySelector("pre").checkVisibility(), "Raw failure evidence was lost or shown by default");
  check(document.body.innerText.includes("本轮未完成") && !document.body.textContent.includes("raw-maintenance"), "Failure must show task impact without internal records");
  rawDetails.querySelector<HTMLElement>("summary").click(); await tick();
  check(rawDetails.open && rawDetails.querySelector("pre").checkVisibility() && document.body.innerText.includes(failureRaw)
    && !rawDetails.querySelector("details"), "Error evidence must be available in one level");
  root.render(<ConversationTimeline run={failureRun} showFailure={false} />); await tick();
  const suppressedNotes = [...document.querySelectorAll<HTMLElement>(".conversation-progress-notes")];
  check(!document.querySelector('[role="alert"]') && !document.body.textContent.includes(failureRaw)
    && suppressedNotes.length === 2 && suppressedNotes.every((note, index) => note.checkVisibility() && !note.closest("details")
      && note.textContent === [failureProgress.text, failureNext.text][index]),
    "External failure display suppression hid public progress or retained duplicate error evidence");
  root.render(<ConversationTimeline run={{ ...run, state: "cancelled", steps: [failureProgress, { ...maintenance, status: "cancelled" }, failureNext] }} />); await tick();
  check(document.querySelector(".conversation-context-notice")?.textContent.includes("对话整理已停止")
    && !document.body.textContent.includes("raw-maintenance") && !document.querySelector('[role="alert"]')
    && document.querySelectorAll(".conversation-progress-notes").length === 2
    && [...document.querySelectorAll<HTMLElement>(".conversation-progress-notes")].every(note => note.checkVisibility() && !note.closest("details")),
    "Stopped compaction falsely claimed completion, exposed raw content or hid public progress");
  const readOperation = { category: "read", tool: "read_source_content", object: "章节规划（第 7—15 章）", sourceId: "source-plan", outcome: "read" };
  root.render(<aside className="conversation-rail" style={{ width: 320 }}><ConversationTimeline key="semantic-reads" run={{ ...run, state: "running", startedAt: Date.now() - 50000, completedAt: null, steps: [
    ...Array.from({ length: 7 }, (_, i) => ({ id: `read-${i}`, kind: "tool", title: "读取章节规划（第 7—15 章）", operation: readOperation, status: "completed", detail: `record-${i}` })),
    { id: "failed-read", kind: "tool", title: "读取作品大纲", status: "failed" },
    { id: "boundary", kind: "system", title: "Agent 进度", messagePhase: "commentary", text: "发现结尾动机不一致，需要核对作品大纲。", status: "completed" },
    { id: "next-read", kind: "tool", title: "读取作品大纲", status: "completed" },
  ] }} /></aside>);
  await tick();
  const readBlocks = [...document.querySelectorAll<HTMLDetailsElement>(".conversation-operation-details")];
  check(readBlocks.length === 2 && readBlocks[0].querySelector("summary").innerText.includes("部分操作未完成"), "Summary must show unresolved impact");
  readBlocks[0].querySelector<HTMLElement>("summary").click(); await tick();
  check(readBlocks[0].querySelectorAll(".conversation-work-item").length === 2, "Pagination should produce one material row plus a separate failure");
  check(!/7 次|第 1 次|record-0/.test(document.body.textContent), "Call counts and raw page records must stay outside conversation");
  check(document.body.innerText.includes("第 7—15 章") && document.body.innerText.includes("未完成"), "Actual scope and failure should remain readable");
  const readRail = document.querySelector<HTMLElement>(".conversation-rail");
  check(readRail.scrollWidth <= readRail.clientWidth + 1, `Material scope overflows narrow rail: ${[...readRail.querySelectorAll<HTMLElement>("*")].filter(node => node.getBoundingClientRect().right > readRail.getBoundingClientRect().right + 1).map(node => `${node.className}:${Math.round(node.getBoundingClientRect().width)}`).join(",")}`);
  check(!document.querySelector(".conversation-technical-record"), "Successful calls should not attach technical details");
  await capture("readable-conversation-process");
  const retryOperation = { ...readOperation, retryKey: "same-source-page" };
  const recoveryRun = { ...run, state: "running", completedAt: null, steps: [
    { id: "failed-page", kind: "tool", title: "读取章节规划", operation: retryOperation, status: "failed", detail: "RETRY_RAW" },
    { id: "recovered-page", kind: "tool", title: "读取章节规划", operation: retryOperation, status: "completed" },
  ] };
  root.render(<ConversationTimeline key="recovery-fold" run={recoveryRun} />); await tick();
  const recoveryTimeline = document.querySelector<HTMLDetailsElement>(".conversation-timeline");
  recoveryTimeline.querySelector<HTMLElement>(".conversation-operation-details > summary").click(); await tick();
  check(document.body.innerText.includes("已重试恢复") && !document.body.textContent.includes("RETRY_RAW")
    && !document.body.innerText.includes("部分操作未完成"), "Recovered page must not look like an unresolved failure");
  root.render(<ConversationTimeline key="recovery-fold" run={{ ...recoveryRun, state: "completed", completedAt: 51000 }} />); await tick();
  check(recoveryTimeline.querySelector<HTMLDetailsElement>(".conversation-operation-details").open, "Completion lost the author’s operation detail choice");
  root.render(<ConversationTimeline key="recovery-fold" run={{ ...recoveryRun, state: "completed", completedAt: 52000 }} />); await tick();
  check(recoveryTimeline.querySelector<HTMLDetailsElement>(".conversation-operation-details").open, "Author’s operation detail choice should survive subsequent updates");
  const note = (id, text) => ({ id, kind: "system", title: "Agent 进度", messagePhase: "commentary", text, status: "completed" });
  const narrative = { ...run, steps: [
    note("intent", "我会先核对第七章的行动动机，再检查后续章纲是否承接。"),
    { id: "read-source", kind: "tool", title: "读取章节规划（第 7—15 章）", operation: readOperation, status: "completed" },
    note("finding", "第七章已交代动机，但第八章缺少相应行动。我会补齐这处承接。"),
    { id: "save-plan", kind: "tool", title: "已保存章节规划", status: "completed" },
    note("checked", "已检查后续章节，新增行动与原有结局一致。"),
  ] };
  root.render(<aside className="conversation-rail" style={{ width: 420 }}><div className="conversation-turn is-completed"><ConversationTimeline key="narrative" run={narrative} />
    <article className="conversation-message assistant-message"><div className="conversation-answer">第八章的行动承接已补齐，章纲已保存。</div></article></div></aside>);
  await tick();
  const visibleNarrative = document.body.innerText;
  const expected = ["我会先核对", "读取章节规划", "第七章已交代动机", "已保存章节规划", "已检查后续章节", "第八章的行动承接已补齐"];
  check(expected.every((text, index) => visibleNarrative.indexOf(text) >= 0 && (!index || visibleNarrative.indexOf(text) > visibleNarrative.indexOf(expected[index - 1]))), "Completed narrative lost intent/action/finding/verification/result order");
  check([...document.querySelectorAll<HTMLDetailsElement>(".conversation-operation-details")].every(detail => !detail.open)
    && !document.querySelector(".conversation-progress-notes")?.closest("details"), "Operation folding hid public explanations");
  check(document.querySelector(".conversation-answer").getBoundingClientRect().top >= document.querySelector(".conversation-timeline").getBoundingClientRect().bottom,
    "CSS moved the final result before its narrative");
  await capture("conversation-narrative-flow");

  const unconfirmedWrite = { id: "unconfirmed-write", kind: "tool", title: "保存章节规划", status: "completed",
    operation: { category: "write", tool: "save_chapter_plans", object: "章节规划" } };
  const failedOutline = { id: "outline-failed", kind: "tool", title: "读取作品大纲", status: "failed",
    operation: { category: "read", tool: "read_source_content", object: "作品大纲", sourceId: "source-outline" } };
  const runningRead = { id: "running-read", kind: "tool", title: "读取章节规划（第 7—15 章）", status: "running",
    operation: { ...readOperation, outcome: undefined } };
  root.render(<aside className="conversation-rail" style={{ width: 300 }}><ConversationTimeline key="mixed-operation-truth" run={{ ...run, state: "running", completedAt: null,
    steps: [unconfirmedWrite, failedOutline, runningRead] }} /></aside>); await tick();
  const mixedDetails = document.querySelector<HTMLDetailsElement>(".conversation-operation-details");
  const mixedSummary = mixedDetails.querySelector<HTMLElement>(":scope > summary");
  check(!mixedDetails.open && mixedSummary.innerText.includes("正在") && mixedSummary.innerText.includes("章节规划") && mixedSummary.innerText.includes("部分操作未完成")
    && mixedSummary.innerText.includes("结果待核实") && !mixedSummary.innerText.includes("已保存"), `Active operation summary hid unresolved failure or falsely confirmed a save: open=${mixedDetails.open}; summary=${JSON.stringify(mixedSummary.innerText)}; checks=${JSON.stringify([mixedSummary.innerText.includes("正在"), mixedSummary.innerText.includes("章节规划"), mixedSummary.innerText.includes("部分操作未完成"), mixedSummary.innerText.includes("结果待核实"), !mixedSummary.innerText.includes("已保存")])}`);
  check(mixedSummary.querySelector("svg"), "Operation summaries need a consistent semantic icon");
  mixedSummary.focus();
  check(document.activeElement === mixedSummary, "Operation disclosure cannot receive keyboard focus");
  mixedSummary.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  if (!mixedDetails.open) mixedSummary.click(); await tick();
  check(mixedDetails.open && mixedDetails.innerText.includes("保存结果待核实") && mixedDetails.innerText.includes("未完成"), "Expanded mixed group lost state evidence");
  root.render(<aside className="conversation-rail" style={{ width: 300 }}><ConversationTimeline key="mixed-operation-truth" run={{ ...run,
    steps: [unconfirmedWrite, failedOutline, { ...runningRead, status: "completed", operation: readOperation }] }} /></aside>); await tick();
  check(document.querySelector<HTMLDetailsElement>(".conversation-operation-details").open, "Finishing an operation reset the keyboard disclosure choice");
  check(document.querySelector(".conversation-operation-details > summary").textContent.includes("部分操作未完成")
    && document.querySelector(".conversation-operation-details > summary").textContent.includes("结果待核实"), "Completion erased unresolved state elsewhere in its group");

  const candidateOperation = { category: "write", tool: "save_delegated_draft", object: "第 7 章候选稿",
    outcome: "isolatedCandidate", referenceId: "candidate" };
  const candidateStep = { id: "isolated-save", kind: "tool", title: "已保存第 7 章候选稿，待整合", status: "completed", operation: candidateOperation };
  const formalStep = { id: "formal-save", kind: "tool", title: "已保存第 7 章正文", status: "completed",
    operation: { category: "write", tool: "save_draft_checkpoint", object: "第 7 章正文", outcome: "saved", referenceId: "revision" } };
  root.render(<aside className="conversation-rail" style={{ width: 300 }}><ConversationTimeline key="candidate-boundary" run={{ ...run,
    state: "running", completedAt: null, steps: [candidateStep] }} /></aside>); await tick();
  const candidateDetails = document.querySelector<HTMLDetailsElement>(".conversation-operation-details");
  check(candidateDetails.querySelector("summary").textContent.includes("候选稿，待整合")
    && !document.body.innerText.includes("已保存第 7 章正文") && !candidateDetails.textContent.includes("结果待核实"),
    "Confirmed isolated save must show its boundary without claiming a formal manuscript save");
  candidateDetails.querySelector<HTMLElement>("summary").click(); await tick();
  check(candidateDetails.open && candidateDetails.innerText.includes("候选稿，待整合")
    && !candidateDetails.innerText.includes("保存结果待核实"), "Expanded candidate state disagrees with compact receipt state");
  root.render(<aside className="conversation-rail" style={{ width: 300 }}><ConversationTimeline key="candidate-boundary" run={{ ...run,
    steps: [candidateStep, note("integrate", "候选稿已读回，我将把选定版本保存为本轮正文。"), formalStep] }} /></aside>); await tick();
  check(document.body.innerText.includes("候选稿，待整合") && document.body.innerText.includes("已保存第 7 章正文")
    && document.querySelector(".conversation-activity-label").textContent.includes("已处理")
    && !document.body.innerText.includes("目标已完成"), "Formal save and parent delivery must remain distinct current facts");
  const candidateRail = document.querySelector<HTMLElement>(".conversation-rail");
  check(candidateRail.scrollWidth <= candidateRail.clientWidth + 1, "Candidate and formal save labels overflow the narrow dialogue");

  const briefOperation = { category: "read", tool: "read_source_content", object: "创作简报", sourceId: "source-brief", outcome: "read" };
  const analysisOperation = { category: "analysis", tool: "analyze_story_impact", object: "第 8 章行动承接", outcome: "processed" };
  const savedOperation = { category: "write", tool: "save_chapter_plans", object: "第 8 章规划", outcome: "saved", referenceId: "plan-revision" };
  const comfortableRun = { ...run, steps: [
    note("codex-intent", "我会先核对创作简报和第 7—15 章规划，找出动机与行动没有承接的地方。"),
    { id: "codex-brief", kind: "tool", title: "读取创作简报", status: "completed", operation: briefOperation },
    { id: "codex-plan", kind: "tool", title: "读取章节规划（第 7—15 章）", status: "completed", operation: readOperation },
    { ...maintenance, id: "codex-compaction", status: "completed" },
    note("codex-finding", "第七章已经说明主角的动机，第八章还缺少对应的行动。\n\n我会补上这处承接，再核对它与后续结局是否一致。"),
    { id: "codex-analysis", kind: "tool", title: "核对第 8 章行动承接", status: "completed", operation: analysisOperation },
    note("codex-decision", "后续结局可以保留。需要调整的是第八章的行动起点，我已完成修改并准备保存。"),
    { id: "codex-save", kind: "tool", title: "已保存第 8 章规划", status: "completed", operation: savedOperation },
    note("codex-check", "已读回保存后的规划，行动动机与后续章节一致。接下来可以从这版规划继续写作。"),
  ] };
  const renderComfortable = async width => {
    root.render(<aside className="conversation-rail" style={{ width, margin: 24 }}><div className="conversation-turn is-completed"><ConversationTimeline key="codex-comfortable" run={comfortableRun} />
      <article className="conversation-message assistant-message"><div className="conversation-answer">第八章的行动承接已补齐，规划已保存并读回确认。</div></article></div></aside>);
    await tick(); await tick();
  };
  await viewport(780, 660);
  await renderComfortable(720);
  const comfortableFlow = document.querySelector(".conversation-process-flow");
  const comfortableBlocks = [...comfortableFlow.children];
  const expectedClasses = ["conversation-progress-notes", "conversation-operation-details", "conversation-context-notice", "conversation-progress-notes",
    "conversation-operation-details", "conversation-progress-notes", "conversation-operation-details", "conversation-progress-notes"];
  check(comfortableBlocks.length === expectedClasses.length && comfortableBlocks.every((block, i) => block.classList.contains(expectedClasses[i])), "Comfortable flow lost the exact commentary/action/context chronology");
  const comfortableDetails = [...document.querySelectorAll<HTMLDetailsElement>(".conversation-operation-details")];
  check(comfortableDetails.length === 3 && comfortableDetails.every(detail => !detail.open), "Default Codex-like flow should show only compact adjacent operation summaries");
  check(comfortableDetails[0].querySelector("summary").textContent.includes("已读取创作简报")
    && comfortableDetails[0].querySelector("summary").textContent.includes("第 7—15 章")
    && comfortableDetails[2].querySelector("summary").textContent.includes("已保存第 8 章规划"), "Visible compact rows lost actual source or receipt-backed save scope");
  check(document.querySelectorAll(".conversation-progress-notes .conversation-markdown p").length === 5
    && !document.querySelector(".conversation-progress-notes")?.closest("details"), "Public paragraphs must remain original and directly readable");
  const findingParagraphs = comfortableBlocks[3].querySelectorAll<HTMLElement>("p");
  check(findingParagraphs[1].getBoundingClientRect().top > findingParagraphs[0].getBoundingClientRect().bottom, "Separate public paragraphs lost readable spacing");
  check(getComputedStyle(findingParagraphs[0]).color !== getComputedStyle(comfortableDetails[0].querySelector("summary")).color,
    "Public explanations and secondary tool rows lost their visual hierarchy");
  check(!document.body.textContent.includes("raw-maintenance") && !document.querySelector(".conversation-context-notice details"), "Compaction notice exposed a technical record");
  for (const theme of ["light", "dark"]) {
    document.documentElement.dataset.theme = theme; await tick();
    const secondaryText = [document.querySelector<HTMLElement>(".conversation-activity-label"),
      document.querySelector<HTMLElement>(".conversation-context-notice"), ...comfortableDetails.map(detail => detail.querySelector<HTMLElement>("summary"))];
    for (const element of secondaryText) {
      const ratio = textContrast(element);
      check(ratio >= 4.5, `${theme} process text has insufficient contrast (${ratio.toFixed(2)}:1): ${element.textContent}`);
    }
  }
  document.documentElement.dataset.theme = "light"; await tick();
  await capture("conversation-codex-flow");
  await viewport(360, 840);
  await renderComfortable(300);
  const comfortableRail = document.querySelector<HTMLElement>(".conversation-rail");
  check(comfortableRail.scrollWidth <= comfortableRail.clientWidth + 1, "Codex-like chronological flow overflows at 300 px");
  check(comfortableDetails.every(detail => !detail.open), "Narrow resize reopened details");
  await capture("conversation-codex-flow-narrow");
  window.testResult = { passed: true, compact: true };
} catch (error) { window.testResult = { passed: false, error: error.stack }; } })();
