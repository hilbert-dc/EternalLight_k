import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { ImpactAnalysisPanel } from "../../../src/ImpactAnalysisPanel";
import { PlanningReviewPanel } from "../../../src/PlanningReviewPanel";

const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (value, message) => { if (!value) throw Error(message); };
const button = label => [...document.querySelectorAll<HTMLButtonElement>("button")].find(node => node.textContent === label);
const until = async predicate => { for (let i = 0; i < 120 && !predicate(); i++) await tick(); check(predicate(), "Discussion entry did not settle"); };
const field = (element, value) => { Object.getOwnPropertyDescriptor(Object.getPrototypeOf(element), "value").set.call(element, value); element.dispatchEvent(new Event("input", { bubbles: true })); };
const workspace = { project: { id: "A" }, chapters: [{ id: "c1", number: 3, title: "原章名", candidate: { id: "saved-A", title: "已保存标题", parentRevisionId: "old-A" }, accepted: null }], modeling: [] };
let dirty = false, running = false, available = true, showDiscuss = true, starts = 0, cancels = 0, reads = 0;
let draft = "我已写好下一条想法";
const prompts = [], snapshots = [];
const discuss = prompt => { prompts.push(prompt); if (!draft.trim()) draft = prompt; };
const api = { runtime: { getImpactAnalysis: async () => { reads++; throw Error("Discussion invoked manual impact analysis"); }, createHypothesis: async () => { reads++; throw Error("Discussion invoked a hypothesis"); } } };
let request = null, runs = [], cancelFails = true;
const root = createRoot(document.getElementById("root"));
const render = () => root.render(<main className="main-panel"><ImpactAnalysisPanel api={api as any} projectId="A" workspace={workspace as any} chapterId="c1" dirty={dirty} refreshToken={0}
  agentRunning={running} onDiscuss={showDiscuss ? discuss : undefined} /><PlanningReviewPanel runs={runs} artifactKind="outline" revisionId="outline-A" outlineNodes={[{ id: "volume-A", kind: "volume", title: "第一卷" }]}
  busy={dirty || running} agentAvailable={available} onDiscuss={showDiscuss ? discuss : undefined}
  onStart={async value => { starts++; request = value; }} onCancel={async id => { check(id === "review-running", "Stop targeted a different review"); cancels++; if (cancelFails) throw Error("停止尚未确认"); runs[0].status = "cancelled"; render(); }} /></main>);
(async () => { try {
  render(); await until(() => button("在对话中检查本章改动影响"));
  check(!document.querySelector(".impact-analysis-panel > details").open && !document.querySelector(".planning-review-panel > details").open, "Technical forms competed with default discussion");
  button("在对话中检查本章改动影响").click(); button("在对话中讨论这份规划").click(); await tick();
  check(prompts.length === 2 && prompts[0].includes("第 3 章《已保存标题》当前已保存正文") && prompts[0].includes("未覆盖范围") && prompts[1].includes("大纲") && !prompts[0].includes("saved-A"), "Discussion lost readable saved-source context");
  check(draft === "我已写好下一条想法" && starts === 0 && reads === 0, "Preparation overwrote draft or executed analysis/deep review");
  draft = ""; button("在对话中讨论这份规划").click(); check(draft === prompts.at(-1), "Empty composer was not prepared");
  document.querySelector(".planning-review-panel > details > summary").click();
  const intent = document.querySelector<HTMLTextAreaElement>(".planning-review-intent textarea"); field(intent, "只检查第一卷人物动机"); await tick();
  document.querySelector<HTMLInputElement>('.planning-review-depth input[value="deep"]').click(); await tick();
  const scope = document.querySelector<HTMLSelectElement>(".planning-review-scope select");
  Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value").set.call(scope, "volume-A"); scope.dispatchEvent(new Event("change", { bubbles: true })); await tick();
  document.querySelector(".planning-review-panel > details > summary").click(); await tick();
  check(document.querySelector(".planning-review-intent textarea") === intent && intent.value === "只检查第一卷人物动机", "Collapsing manual review lost author inputs");
  button("在对话中讨论这份规划").click(); check(starts === 0, "Ordinary discussion started structured multi-role review");
  document.querySelector(".planning-review-panel > details > summary").click(); button("开始规划深审").click(); await until(() => starts === 1);
  check(request.revisionId === "outline-A" && request.targetNodeId === "volume-A" && request.authorIntent === intent.value && request.roles.join(",") === "structure,character,experience", "Manual multi-role review lost exact scope/roles/intent");
  document.querySelector(".impact-analysis-panel > details > summary").click();
  const entity = document.querySelector<HTMLInputElement>('[aria-label="影响分析实体"]'); field(entity, "known-entity"); await tick();
  document.querySelector(".impact-analysis-panel > details > summary").click(); await tick();
  check(document.querySelector('[aria-label="影响分析实体"]') === entity && entity.value === "known-entity", "Manual impact form lost its input after closing");
  dirty = true; render(); await tick(); check(button("在对话中检查本章改动影响").disabled && button("在对话中讨论这份规划").disabled, "Dirty saved-source discussion was presented as current");
  dirty = false; running = true; render(); await tick(); check(button("在对话中检查本章改动影响").disabled && button("在对话中讨论这份规划").disabled, "Busy author scope could be replaced");
  running = false; available = false; render(); await tick();
  check(!button("在对话中讨论这份规划").disabled && button("开始规划深审").disabled, "Unavailable model blocked preparing a draft or allowed deep review");
  for (const theme of ["light", "dark"]) for (const width of [330, 740]) {
    document.documentElement.dataset.theme = theme; document.getElementById("root").style.width = `${width}px`; await tick();
    check([...document.querySelectorAll<HTMLElement>(".impact-analysis-panel,.planning-review-panel")].every(node => node.scrollWidth <= node.clientWidth + 2), "Discussion entries overflowed a narrow pane");
  }
  const reviewForm = document.querySelector<HTMLFormElement>(".planning-review-form"), reviewDisclosure = reviewForm.closest("details"); reviewDisclosure.open = false;
  runs = [{ id: "review-running", artifactKind: "outline", revisionId: "outline-A", targetNodeId: "volume-A", authorIntent: "核对第一卷", createdAt: "2026-10-04T01:00:00Z", status: "running", roles: ["structure", "character"], reports: [], checksByRole: { structure: ["cause"], character: ["motive"] }, synthesis: null }]; render(); await tick();
  check(!reviewDisclosure.open && button("停止本次评审").checkVisibility() && !document.querySelector<HTMLDetailsElement>(".planning-review-reading").open && document.querySelector(".planning-review-result").textContent.includes("已检查 0 项"), "Closed form hid current status/stop or exposed report content");
  button("停止本次评审").click(); await until(() => cancels === 1 && document.querySelector('[role="alert"]'));
  check(document.querySelector<HTMLElement>('[role="alert"]').checkVisibility() && !reviewDisclosure.open && reviewForm === document.querySelector(".planning-review-form"), "Stop failure disappeared in closed form or lost author inputs");
  cancelFails = false; button("停止本次评审").click(); await until(() => cancels === 2 && document.querySelector(".planning-review-result").textContent.includes("评审已停止"));
  check(starts === 1 && intent.value === "只检查第一卷人物动机", "Stopping restarted review or cleared its form");
  snapshots.push({ impact: "在对话中检查本章改动影响", planning: "在对话中讨论这份规划", manualImpact: "手动检查与假设模拟", multiRole: "多视角规划深审", draftPreserved: true, manualFieldsMounted: true, structuredStarts: starts, runtimeReads: reads, collapsedStop: { attempts: cancels, state: "评审已停止", reportOpen: document.querySelector<HTMLDetailsElement>(".planning-review-reading").open } });
  showDiscuss = false; render(); await tick(); check(!button("在对话中讨论这份规划") && !button("在对话中检查本章改动影响") && button("开始规划深审"), "Missing conversation callback hid real manual capabilities or showed a dead action");
  check(reads === 0 && cancels === 2, "Discussion falsely executed a manual hypothesis/read/cancel");
  root.unmount(); window.testResult = { passed: true, snapshots, starts, reads, prompts: prompts.length };
} catch (error) { window.testResult = { passed: false, error: error.stack, dom: document.body.innerText }; } })();
