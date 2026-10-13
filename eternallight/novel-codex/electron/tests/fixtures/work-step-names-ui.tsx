import { emptyCreationIssueSummary } from "../helpers/creation-issues";
import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { WorkTaskPanel } from "../../../src/WorkTaskPanel";
import { workStepName } from "../../../src/work-step-presentation";
import { ConversationRecovery } from "../../../src/ConversationRecovery";

const tick = () => new Promise(resolve => setTimeout(resolve, 25));
const check = (value, message) => { if (!value) throw Error(message); };
const step = (id, operation, target = {}, extra = {}) => ({ id, operation, target, status: "completed", instruction: null,
  currentAttemptId: `${id}-attempt`, attempts: [], outputRefs: [], changes: [], readingProgress: null, ...extra });
const coordinator = (id, source) => step(id, "advisory", { workDecision: { workTaskId: "work", sourceTaskId: source } });
const writer = (number, decision) => step(`writer-${number}`, "generateChapter", { chapterNumber: number,
  workDecisionAction: { workTaskId: "work", decisionStepId: decision, decisionTaskId: `${decision}-attempt` } },
  { attempts: [{ id: `writer-${number}-attempt`, status: "running" }] });
const work = { id: "work", projectId: "A", threadId: "thread", goal: "续写第 2—3 章", kind: "generation", status: "paused",
  deliveryStatus: "pending", deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], evidence: [], pendingActionIds: [], unsettledAttemptIds: [] },
  steps: [coordinator("start", null), writer(2, "start"), coordinator("after-2", "writer-2-attempt"), writer(3, "after-2"),
    coordinator("after-3", "writer-3-attempt"),
    step("discussion", "advisory", {}, { instruction: "核对灯税逾期时间线与人物已知信息，检查章节之间是否存在矛盾。" }),
    step("long", "advisory", {}, { instruction: "ABCDEFGHIJKLMNOPQRSTUVWXYZ".repeat(8) })] };
const root = createRoot(document.getElementById("root"));
const render = async task => {
  root.render(<div className="conversation-task-records" style={{ width: 300 }}><WorkTaskPanel api={undefined} projectId="A" tasks={[task]}
    activeCount={0} nextCursor={null} loadingMore={false} loaded reading={false} readError={null} onRetry={() => {}}
    busy={false} agentRunning={false} dirty={false} readOnly onLoadMore={() => {}} /></div>);
  await tick(); await tick();
};
async function run() {
  await render(work);
  check(!document.querySelector(".work-task-execution, .work-task-controls"), "Secondary execution list returned");
  const names = work.steps.map(step => workStepName(work, step).label);
  check(!names.some(name => name.includes("讨论与规划")), "Recovery labels became generic");
  await render({ ...work, version: 2 });
  check(!document.querySelector(".work-task-execution"), "Refresh revived execution lists");
  const scoped = [
    step("selected", "generateChapter", { chapterNumber: 7, scope: { type: "selectedScenes", sourceRevisionId: "revision", groupIds: ["a"] } }),
    step("queued", "writeNextChapter", { chapterNumber: 8 }),
    ...["structure", "character", "experience"].map(role => step(role, "reviewPlanningSeat", { role, artifactKind: "outline" })),
    step("plan", "generatePlan", { planFromChapter: 7 }),
  ];
  await render({ ...work, steps: scoped });
  const scopedNames = scoped.map(step => workStepName({ ...work, steps: scoped }, step).label);
  check(scopedNames.join("|") === "第 7 章 · 修改选定场景|第 8 章 · 续写章节|大纲 · 结构与因果检查|大纲 · 人物与主题检查|大纲 · 阅读体验检查|从第 7 章起生成近章规划", "Different work scopes still share generic names");
  const boundStep = { ...scoped[0], currentAttemptId: "bound", attempts: [{ id: "bound", threadId: "thread", status: "running" }] };
  const boundWork = { ...work, steps: [boundStep] };
  const run = (runId, operation) => ({ runId, projectId: "A", threadId: "thread", operation, runtimeStatus: "failed", requests: [] });
  root.render(<ConversationRecovery runs={[run("bound", "generateChapter"), run("unloaded", "generateChapter"), run("structure-check", "assessChapterStructure")]}
    tasks={[boundWork]} threadId="thread" busy={false} onClose={() => {}}
    onInspect={async () => { throw Error("Naming inspected a thread"); }} onReconcile={async () => { throw Error("Naming executed recovery"); }} />);
  await tick(); await tick();
  const recovery = document.querySelector<HTMLDialogElement>(".conversation-recovery");
  check(recovery.open && recovery.querySelector('[data-recovery-run="bound"] > strong').textContent === "第 7 章 · 修改选定场景"
    && recovery.querySelector('[data-recovery-run="unloaded"] > strong').textContent === "生成章节"
    && recovery.querySelector('[data-recovery-run="structure-check"] > strong').textContent === "检查删章后的故事衔接", "Saving records lost exact names or invented unloaded chapter scope");
  recovery.style.width = "300px"; await tick();
  check(recovery.scrollWidth <= recovery.clientWidth + 2, "Specific recovery names overflowed the narrow dialog");
  root.unmount();
  return { passed: true, names, scopedNames };
}
run().then(result => { window.testResult = result; }).catch(error => { window.testResult = { passed: false, error: error.stack }; });
