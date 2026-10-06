import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { PropertyRulesPanel } from "../../../src/PropertyRulesPanel";

const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (value, message) => { if (!value) throw Error(message); };
const text = () => document.body.textContent;
const until = async (predicate, message = "Rule result did not settle") => { for (let i = 0; i < 150 && !predicate(); i++) await tick(); check(predicate(), message); };
const button = label => [...document.querySelectorAll<HTMLButtonElement>("button")].find(node => node.textContent === label);
const click = async label => { const node = button(label); check(node && !node.disabled, `Unavailable ${label}`); node.click(); await tick(); };
const blank = id => ({ project: { id, title: id, logline: "" }, revision: 5, artifacts: { propertyRules: { accepted: null, locked: false,
  candidate: { id: `rules-${id}`, version: 1, source: "agent", payload: { definitions: [{ predicate: "alive", subjectKind: "character", schema: { cardinality: "single", valueType: { type: "boolean" } }, description: `${id}人物生命状态`, exceptions: [] }], eventRules: [] } } } }, reviews: [], commits: [] });
const workspaces = Object.fromEntries(["A", "B", "C", "D", "E", "F"].map(id => [id, blank(id)]));
const request = (projectId, requestId, referenceId = `rules-${projectId}`) => ({ requestId, result: { kind: "propertyRules", projectId, workTaskId: `work-${projectId}`, stepId: "step-rule", attemptId: "attempt-rule", referenceId } });
let project = "A", requested = request("A", "first"), running = false, token = 0, dirty = false;
let readFailure = false, failAfterCreate = false, lostReceipt = false, holdNext = false, release = null, foreign = false;
const creates = [], snapshots = []; let accepts = 0, modelCalls = 0, saves = 0;
const api = { runtime: {
  getCreativeWorkspace: async id => {
    const captured = structuredClone(workspaces[id]);
    if (holdNext) { holdNext = false; await new Promise(resolve => { release = resolve; }); }
    if (readFailure) throw Error("controlled rule read failure");
    if (foreign) captured.project.id = "foreign-project";
    return captured;
  },
  createReview: async (id, kind, revisionId, facts, selections) => {
    const workspace = workspaces[id]; creates.push([id, revisionId]);
    check(kind === "propertyRules" && workspace.artifacts.propertyRules.candidate.id === revisionId && facts.length === 0 && Object.keys(selections).length === 0, "Preparation changed source or confirmed facts");
    const review = { id: `review-${creates.length}`, artifactKind: kind, artifactRevisionId: revisionId, projectRevision: workspace.revision, status: "ready", checks: [{ id: "rule-check", passed: true, blocking: true, message: "结构检查完成" }], selections: {} };
    workspace.reviews.push(review);
    if (failAfterCreate) readFailure = true;
    if (lostReceipt) throw Error("saved review receipt lost");
    return review;
  },
  acceptReview: async () => { accepts++; throw Error("Result entry auto accepted rules"); },
  savePropertyRules: async () => { saves++; throw Error("Result entry rewrote rules"); },
}, agent: { sendPrompt: async () => { modelCalls++; throw Error("Rule check invoked a model"); } } };
const root = createRoot(document.getElementById("root"));
const render = () => root.render(<React.StrictMode><PropertyRulesPanel api={api as any} projectId={project} refreshToken={token}
  requestedResult={requested as any} agentRunning={running} onFormalChange={async () => { throw Error("Preparation changed formal state"); }}
  onDirtyChange={value => { dirty = value; }} onSaveHandlerChange={() => {}} onDiscardHandlerChange={() => {}} /></React.StrictMode>);
(async () => { try {
  render(); await until(() => text().includes("规则检查已准备"));
  check(creates.length === 1 && accepts === 0 && button("采用规则") && !button("采用规则").disabled, "Exact result did not prepare one author decision");
  snapshots.push({ flow: "exact result", creates: 1, accepted: false, factsConfirmed: false });
  requested = request("A", "reuse"); render(); await until(() => !dirty && text().includes("规则检查已准备"));
  check(creates.length === 1, "Ready review was duplicated");
  requested = request("A", "stale", "rules-A-old"); render(); await until(() => text().includes("已不是当前规则候选"));
  check(creates.length === 1, "Stale result prepared a different candidate");
  requested = null; render(); await tick(); await click("编辑规则候选");
  requested = request("A", "dirty"); render(); await until(() => text().includes("保存或放弃规则编辑"));
  check(creates.length === 1 && document.querySelector(".property-rule-editor"), "Result preparation discarded rule input");
  await click("放弃本地规则输入"); await until(() => !dirty);
  running = true; requested = request("A", "running"); render(); await tick(); await until(() => text().includes("等待当前创作结束") && button("采用规则")?.disabled);
  check(button("采用规则").disabled && creates.length === 1, "Running generation allowed a rule mutation");
  running = false; project = "B"; requested = request("B", "read-lost"); failAfterCreate = true; render();
  await until(() => text().includes("controlled rule read failure"));
  check(creates.length === 2 && button("采用规则")?.disabled !== false, "Missing read receipt unlocked adoption");
  readFailure = false; failAfterCreate = false; await click("核对对话中的规则检查"); await until(() => text().includes("规则检查已准备"));
  check(creates.length === 2, "Read recovery recreated the saved review");
  project = "C"; requested = request("C", "receipt-lost"); lostReceipt = true; render(); await until(() => text().includes("saved review receipt lost"));
  lostReceipt = false; await click("核对对话中的规则检查"); await until(() => text().includes("规则检查已准备"));
  check(creates.length === 3, "Lost creation receipt duplicated a persisted review");
  snapshots.push({ flow: "read/receipt recovery", creates: 3, readOnlyRetries: true });
  project = "D"; requested = null; render(); await until(() => text().includes("D人物生命状态") && !dirty, "D baseline did not settle");
  holdNext = true; release = null; requested = request("D", "late-running"); render(); await until(() => release);
  running = true; render(); await tick(); release(); await until(() => text().includes("创作或规则编辑状态已变化"), "Late running preflight did not refuse review creation");
  check(creates.length === 3, "Late preflight created a review while the engine was running");
  running = false; requested = null; render(); await tick();
  holdNext = true; release = null; requested = request("D", "late-project"); render(); await until(() => release); const old = release;
  project = "E"; requested = request("E", "new-project"); render(); await until(() => text().includes("规则检查已准备") && text().includes("E人物生命状态"));
  old(); await tick(); check(!creates.some(row => row[0] === "D") && !text().includes("D人物生命状态"), "Late old-project request created or installed rules");
  project = "D"; requested = request("D", "back-D"); render(); await until(() => text().includes("规则检查已准备") && text().includes("D人物生命状态"), "Return-D exact review did not settle");
  check(creates.filter(row => row[0] === "D").length === 1, "A-B-A request incarnation reused a cancelled preparation");
  foreign = true; project = "F"; requested = request("F", "foreign-read"); render(); await until(() => text().includes("读取结果属于其他作品"));
  check(!text().includes("F人物生命状态") && !creates.some(row => row[0] === "F"), "Foreign project response prepared rules");
  snapshots.push({ flow: "scope guards", staleRejected: true, dirtyPreserved: true, runningRejected: true, lateProjectRejected: true, foreignReadRejected: true });
  check(accepts === 0 && modelCalls === 0 && saves === 0 && Object.values(workspaces).every(workspace => workspace.artifacts.propertyRules.accepted === null && workspace.revision === 5), "Preparation changed formal rules, body, or model state");
  root.unmount(); window.testResult = { passed: true, snapshots, creates: creates.length, accepts, modelCalls, saves };
} catch (error) { window.testResult = { passed: false, error: error.stack, dom: document.body.innerText }; } })();
