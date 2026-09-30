import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { IdentityDecisionPanel } from "../../../src/IdentityDecisionPanel";

window.addEventListener("error", (event) => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
const tick = () => new Promise((resolve) => setTimeout(resolve, 25));
const check = (value, message) => { if (!value) throw new Error(message); };
const click = async (text) => {
  const button = [...document.querySelectorAll("button")].find((item) => item.textContent === text);
  check(button && !button.disabled, `Unavailable: ${text}`);
  button.click();
  await tick(); await tick();
};

let serial = 0;
let workspace = {
  revision: 0,
  artifacts: { identity: { candidate: null, accepted: null, working: null, locked: false, workingSelection: null } },
  reviews: [], commits: [],
};
const catalog = () => {
  const accepted = workspace.artifacts.identity.accepted?.payload.decisions ?? [];
  const merged = accepted.some((decision) => decision.kind === "same");
  const distinct = accepted.some((decision) => decision.kind === "distinct");
  return {
    entries: ["a", "b"].map((id) => ({ id, canonicalId: merged && id === "b" ? "a" : id, kind: "character", resolutionState: "candidate", claims: [{ displayName: "林夕", aliases: [], sourceObjectId: `draft-${id}`, sourceRevisionId: `revision-${id}`, evidence: { quote: `林夕${id}`, start: 0, end: 2 } }] })),
    ambiguousNames: merged ? [] : [{ name: "林夕", candidateIds: ["a", "b"], canonicalIds: ["a", "b"], authorDecidedDistinct: distinct }],
    identityRevisionId: workspace.artifacts.identity.accepted?.id ?? null,
    identityDecisions: accepted, truncated: false, ambiguityScanComplete: true,
  };
};
const api = { runtime: {
  getIdentityCatalog: async () => structuredClone(catalog()),
  getCreativeWorkspace: async () => structuredClone(workspace),
  saveIdentityDecisions: async (_, decisions, expectedRevision, expectedCandidateRevisionId) => {
    check(expectedRevision === workspace.revision, "Identity save used a stale project revision");
    check(expectedCandidateRevisionId === (workspace.artifacts.identity.candidate?.id ?? null), "Identity save did not CAS the candidate");
    const revisionId = `identity-${++serial}`;
    workspace.artifacts.identity.candidate = { id: revisionId, payload: { decisions } };
    return { artifactKind: "identity", revisionId, payload: { decisions } };
  },
  createReview: async (_, kind, revisionId, proposals, selections) => {
    check(kind === "identity" && revisionId === workspace.artifacts.identity.candidate.id && proposals.length === 0 && Object.keys(selections).length === 0, "Identity review bypassed the author-only boundary");
    workspace.reviews = [{ id: `review-${serial}`, artifactKind: kind, artifactRevisionId: revisionId, status: "ready" }, ...workspace.reviews];
    return { id: `review-${serial}` };
  },
  rejectIdentityReview: async (_, reviewId, expectedRevision) => {
    check(expectedRevision === workspace.revision, "Reject used a stale revision");
    workspace.reviews.find((review) => review.id === reviewId).status = "rejected";
    return { reviewId, status: "rejected" };
  },
  acceptReview: async (_, reviewId, expectedRevision) => {
    check(expectedRevision === workspace.revision, "Accept used a stale revision");
    workspace.reviews.find((review) => review.id === reviewId).status = "accepted";
    workspace.artifacts.identity.accepted = workspace.artifacts.identity.candidate;
    workspace.revision += 1;
    workspace.commits.unshift({ id: `commit-${serial}`, operation_kind: "accept", result_revision: workspace.revision, receipt: { artifactKind: "identity" } });
    return { commitId: `commit-${serial}`, resultRevision: workspace.revision };
  },
  undoCommit: async () => { throw new Error("Undo is not expected in this fixture"); },
} };

const root = createRoot(document.getElementById("root"));
root.render(<IdentityDecisionPanel api={api} projectId="project-identity" refreshToken={0} onFormalChange={async () => {}} />);
async function run() {
  await tick(); await tick();
  window.uiReady = true;
  check(document.body.textContent.includes("人物身份") && document.body.textContent.includes("待确认"), "Identity candidates were not shown");
  await click("确认为同一人物");
  check(document.body.textContent.includes("待审阅") && workspace.artifacts.identity.accepted === null, "Candidate was applied before author adoption");
  await click("撤回候选");
  check(workspace.reviews[0].status === "rejected" && workspace.revision === 0, "Reject changed formal story state");
  await click("确认是不同人物");
  await click("采用决议");
  check(workspace.artifacts.identity.accepted.payload.decisions[0].kind === "distinct", "Author decision was not committed");
  check(document.body.textContent.includes("已确认不同"), "Adopted distinction was not projected");
  await click("移除此决议并审阅");
  check(workspace.artifacts.identity.accepted.payload.decisions.length === 1, "Removing a decision mutated accepted state before review");
  await click("采用决议");
  check(workspace.artifacts.identity.accepted.payload.decisions.length === 0, "Revised identity decision was not committed");
  return { passed: true };
}
run().then((result) => { window.testResult = result; }).catch((error) => { window.testResult = { passed: false, error: error.stack + "\nDOM:" + document.body.innerText }; });
