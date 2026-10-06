import "../../../src/styles.css";
import React, { useState } from "react";
import { createRoot } from "react-dom/client";
import { ProposalSetPanel } from "../../../src/ProposalSetPanel";

window.addEventListener("error", (event) => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
const tick = () => new Promise((resolve) => setTimeout(resolve, 30));
const check = (condition, message) => { if (!condition) throw new Error(message); };
const id = `proposal-set-${"a".repeat(32)}`;
const applicationId = `proposal-application-${"b".repeat(32)}`;
const calls = [];
const mixed = window.proposalTestMixed === true;
let status = "proposed";
let savedOperations = [];
let intent = "";
let appliedChoices = {};
const initialWorkspace = {
  revision: 0, headSnapshotId: "snapshot-current",
  artifacts: {
    brief: { candidate: { id: "revision-brief" }, locked: false },
    outline: { candidate: mixed ? null : { id: "revision-outline" }, locked: false },
    plan: { candidate: null, locked: false },
    identity: { candidate: mixed ? { id: "revision-identity" } : null, locked: false },
  },
  chapters: mixed ? [{ id: "first", number: 1, title: "白塔", candidate: { id: "revision-draft", title: "白塔" } }] : [],
  reviews: [
    { id: "review-brief", artifactKind: "brief", artifactRevisionId: "revision-brief", status: "ready", projectRevision: 0, checks: [] },
    { id: "review-outline", artifactKind: "outline", artifactRevisionId: "revision-outline", status: "ready", projectRevision: 0, checks: [] },
    { id: "review-draft", artifactKind: "draft", artifactRevisionId: "revision-draft", status: "ready", projectRevision: 0, checks: [] },
    { id: "review-identity", artifactKind: "identity", artifactRevisionId: "revision-identity", status: "ready", projectRevision: 0, checks: [] },
  ],
};
const detail = (selected = [], choices = {}) => ({
  id, projectId: "project-a", baseSnapshotId: "snapshot-current", baseProjectRevision: 0, intent, createdAt: 1,
  status: status === "proposed" ? "ready" : status,
  operations: savedOperations.map((operation) => ({ ...operation, objectId: operation.operationId === "draft-first" ? "draft-first" : `artifact-${operation.operationId}`, objectKind: operation.operationId === "draft-first" ? "draft" : operation.operationId, candidateRevisionId: `revision-${operation.operationId}`, expectedAcceptedRevisionId: null })),
  reviewOptions: Object.fromEntries(savedOperations.map((operation) => [operation.operationId, operation.operationId === "draft-first" ? { sceneGroupIds: ["scene-first"], factProposals: [
    { id: "fact-first", sceneGroupId: "scene-first", subjectId: "linxi", predicate: "sawLight", value: true, evidence: "林夕看见灯光" },
    { id: "fact-second", sceneGroupId: "scene-first", subjectId: "linxi", predicate: "trusted", value: true, evidence: "林夕仍然犹豫" },
  ], selections: appliedChoices[operation.operationId] ?? {} } : { sceneGroupIds: [], factProposals: [], selections: appliedChoices[operation.operationId] ?? {} }])),
  selection: { operationIds: selected, selectionsByOperationId: choices, missingDependencies: savedOperations.filter((operation) => selected.includes(operation.operationId)).flatMap((operation) => operation.dependsOn.filter((dependency) => !selected.includes(dependency)).map((dependency) => ({ operationId: operation.operationId, requires: dependency }))), declaredClosureReady: selected.length > 0 && status === "proposed" && savedOperations.every((operation) => !selected.includes(operation.operationId) || operation.dependsOn.every((dependency) => selected.includes(dependency))), applyReady: selected.length > 0 && status === "proposed" && savedOperations.every((operation) => !selected.includes(operation.operationId) || operation.dependsOn.every((dependency) => selected.includes(dependency))) },
  staleOperations: [],
  application: status === "proposed" ? null : { id: applicationId, status, resultRevision: 1, resultSnapshotId: "snapshot-applied", undoneByApplicationId: status === "undone" ? "proposal-undo-c" : null, selectedOperationIds: Object.keys(appliedChoices) },
  coverage: { dependencyClosure: "declaredOnly", semanticDependenciesComplete: false },
});
const api = { runtime: {
  listProposalSets: async () => ({ data: savedOperations.length ? [{ id, intent, status, applicationId: status === "proposed" ? null : applicationId }] : [], nextCursor: null }),
  createProposalSet: async (_projectId, request) => {
    calls.push({ create: request });
    check(request.baseSnapshotId === "snapshot-current" && request.operations.length === (mixed ? 3 : 2) && request.operations.some((op) => op.dependsOn.includes("brief")), "Proposal was not bound to the current snapshot, reviews and declared dependency");
    savedOperations = request.operations;
    intent = request.intent;
    return { id, baseSnapshotId: "snapshot-current", replayed: false };
  },
  getProposalSet: async () => detail(),
  previewProposalSet: async (_projectId, _id, selected, choices) => {
    const result = detail(selected, choices);
    if (mixed && choices["draft-first"]?.factDecisions["fact-first"] === "confirmed") await new Promise((resolve) => setTimeout(resolve, 90));
    return result;
  },
  applyProposalSet: async (_projectId, _id, request) => {
    calls.push({ apply: request });
    check(request.selectedOperationIds.length === 2 && Object.keys(request.selectionsByOperationId).length === request.selectedOperationIds.length, "Group selection was not explicit");
    if (mixed) check(request.selectionsByOperationId["draft-first"].factDecisions["fact-first"] === "confirmed" && request.selectionsByOperationId["draft-first"].factDecisions["fact-second"] === "rejected", "Explicit author fact decisions were not sent");
    appliedChoices = request.selectionsByOperationId;
    status = "applied";
    return { applicationId, resultRevision: 1, resultSnapshotId: "snapshot-applied", replayed: false };
  },
  undoProposalSet: async (_projectId, _id, revision, key) => {
    calls.push({ undo: { revision, key } });
    check(revision === 1, "Undo did not require the latest applied revision");
    status = "undone";
    return { undoApplicationId: "proposal-undo-c", resultRevision: 2, resultSnapshotId: "snapshot-undone", replayed: false };
  },
} };
const root = createRoot(document.getElementById("root"));
function Harness() {
  const [workspace, setWorkspace] = useState(initialWorkspace);
  const [refreshToken, setRefreshToken] = useState(0);
  return <ProposalSetPanel api={api} projectId="project-a" workspace={workspace} busy={false} dirty={false} agentRunning={false}
    refreshToken={refreshToken} onFormalChange={async () => { setWorkspace((current) => ({ ...current, revision: status === "applied" ? 1 : 2 })); setRefreshToken((current) => current + 1); }} />;
}
root.render(<Harness />);

async function run() {
  await tick(); await tick();
  document.querySelector(".proposal-set-panel summary").click();
  const input = document.querySelector('.proposal-set-section input:not([type])');
  check(input, "Proposal intent field is missing");
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(input, "一起修订设定与主线");
  input.dispatchEvent(new Event("input", { bubbles: true }));
  await tick();
  const reviews = document.querySelectorAll('.proposal-set-review > label input[type="checkbox"]');
  check(reviews.length === (mixed ? 3 : 2), "Current ready reviews were not offered");
  for (const review of reviews) review.click();
  await tick();
  const dependency = [...document.querySelectorAll('.proposal-set-dependency')].find((label) => label.textContent.includes(mixed ? "白塔依赖创作简报" : "全书大纲依赖创作简报"))?.querySelector('input');
  check(dependency, "Planning dependency selector was not available");
  dependency.click();
  await tick();
  const create = [...document.querySelectorAll("button")].find((button) => button.textContent === "保存提案候选");
  check(create && !create.disabled, "Proposal create action is unavailable");
  create.click();
  await tick(); await tick(); await tick();
  check(calls.length === 1 && calls[0].create.operations.length === (mixed ? 3 : 2), "Proposal was not saved");
  check(document.body.innerText.includes("不保证全部语义依赖已被发现"), "Semantic coverage warning was hidden");
  const apply = [...document.querySelectorAll("button")].find((button) => button.textContent.includes("整组采用"));
  check(apply && !apply.disabled, "Selected reviewed group cannot be applied");
  const firstOperation = document.querySelector('.proposal-set-detail label input[type="checkbox"]');
  firstOperation.click();
  await tick(); await tick();
  check(apply.disabled && document.body.innerText.includes("缺少声明依赖"), "Missing declared dependency did not block application");
  firstOperation.click();
  await tick(); await tick();
  check(!apply.disabled, "Restored dependency closure did not re-enable application");
  if (mixed) {
    const first = document.querySelector('[aria-label="事实决定 fact-first"]');
    const second = document.querySelector('[aria-label="事实决定 fact-second"]');
    const decide = (select, value) => { select.value = value; select.dispatchEvent(new Event("change", { bubbles: true })); };
    check(first.value === "candidate", "Model facts were auto-confirmed");
    decide(first, "confirmed"); await tick();
    decide(first, "candidate"); await tick(); await tick(); await tick();
    check(!apply.disabled && first.value === "candidate", "A late preview replaced the latest author choices");
    decide(first, "confirmed"); await tick(); await tick(); await tick(); await tick();
    const source = document.querySelector('.proposal-fact-choices input'); source.click(); await tick();
    check(first.value === "candidate" && first.querySelector('[value="confirmed"]').disabled, "Removing the source scene retained confirmation");
    source.click(); await tick();
    decide(first, "confirmed"); await tick(); await tick(); await tick(); await tick();
    decide(second, "rejected"); await tick(); await tick(); await tick(); await tick();
    [...document.querySelectorAll('.proposal-set-operation > label')].find((label) => label.textContent.includes("identity"))?.querySelector('input').click();
    await tick(); await tick(); await tick(); await tick();
    check(!apply.disabled && document.body.innerText.includes("正文整份采用"), "Reviewed mixed proposal is not ready or misrepresents scene selection");
    document.querySelector('.proposal-fact-choices').scrollIntoView({ block: "center" });
    window.uiReady = true; await tick(); await tick(); await tick();
  }
  apply.click();
  await tick(); await tick(); await tick(); await tick(); await tick();
  check(calls.length === 2 && calls[1].apply.idempotencyKey.startsWith("proposal-ui-"), "Atomic group apply was not called");
  if (mixed) {
    check(document.querySelector('[aria-label="事实决定 fact-first"]').value === "confirmed", "Applied author choices disappeared from the receipt view");
    const identity = [...document.querySelectorAll('.proposal-set-operation > label')].find((label) => label.textContent.includes("identity"))?.querySelector('input');
    check(identity && !identity.checked, "Unselected identity operation appeared adopted after refresh");
  }
  const undo = [...document.querySelectorAll("button")].find((button) => button.textContent === "整组撤销");
  check(undo && !undo.disabled, "Applied group is not undoable at the latest revision");
  undo.click();
  await tick(); await tick();
  check(calls.length === 3 && calls[2].undo.revision === 1, "Undo was not routed through group compensation");
  return { passed: true };
}
run().then((result) => { window.testResult = result; }).catch((error) => { window.testResult = { passed: false, error: error.stack + "\nDOM:" + document.body.innerText }; });
