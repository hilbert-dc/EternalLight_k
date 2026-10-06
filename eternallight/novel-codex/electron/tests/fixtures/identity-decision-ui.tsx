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

let serial = 0, omitSecond = false;
const snapshots = [], savedDecisions = [];
const chapters = [{ id: "a", number: 1, title: "雾港邮局" }, { id: "b", number: 2, title: "城门灯火" }];
let workspace = {
  project: { id: "project-identity" }, revision: 0,
  artifacts: { identity: { candidate: null, accepted: null, working: null, locked: false, workingSelection: null } },
  reviews: [], commits: [], chapters,
};
const catalog = () => {
  const accepted = workspace.artifacts.identity.accepted?.payload.decisions ?? [];
  const merged = accepted.some((decision) => decision.kind === "same");
  const distinct = accepted.some((decision) => decision.kind === "distinct");
  return {
    entries: ["a", "b"].filter(id => !omitSecond || id !== "b").map((id) => {
      const quote = id === "a" ? "林夕站在邮局门口，握着那封旧信。" : "林夕在城门下熄灭灯火，转身离开。";
      return { id, canonicalId: merged && id === "b" ? "a" : id, kind: "character", resolutionState: merged && id === "b" ? "authorMerged" : "candidate", claims: [{ displayName: "林夕", aliases: id === "a" ? ["小夕"] : [], sourceObjectId: `draft-${id}`, sourceRevisionId: `revision-${id}`, evidence: { quote, start: 0, end: [...quote].length } }] };
    }),
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
    savedDecisions.push(structuredClone(decisions));
    const revisionId = `identity-${++serial}`;
    workspace.artifacts.identity.candidate = { id: revisionId, payload: { decisions } };
    return { artifactKind: "identity", revisionId, payload: { decisions } };
  },
  createReview: async (_, kind, revisionId, proposals, selections) => {
    check(kind === "identity" && revisionId === workspace.artifacts.identity.candidate.id && proposals.length === 0 && Object.keys(selections).length === 0, "Identity review bypassed the author-only boundary");
    workspace.reviews = [{ id: `review-${serial}`, artifactKind: kind, artifactRevisionId: revisionId, status: "ready", projectRevision: workspace.revision }, ...workspace.reviews];
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
root.render(<IdentityDecisionPanel key="project-identity" api={api} projectId="project-identity" refreshToken={0} onFormalChange={async () => {}} />);
const capture = label => snapshots.push({ case: label, people: [...document.querySelectorAll(".identity-person")].map(person => ({ name: person.querySelector(":scope > strong")?.textContent, source: person.querySelector(":scope > small")?.textContent ?? null, quote: person.querySelector(":scope > blockquote")?.textContent ?? null, missing: person.querySelector(".identity-source-missing")?.textContent ?? null, recordsCollapsed: !person.querySelector("details").open })), relation: document.querySelector(".identity-relation")?.textContent ?? null });
async function run() {
  await tick(); await tick();
  window.uiReady = true;
  check(document.body.textContent.includes("人物身份") && document.body.textContent.includes("待确认"), "Identity candidates were not shown");
  const people = [...document.querySelectorAll(".identity-pair > .identity-person")];
  check(people.length === 2 && people.every(person => person.querySelector(":scope > strong").textContent === "林夕") && people[0].querySelector("blockquote").textContent !== people[1].querySelector("blockquote").textContent, "Same-name people weren't distinguished by exact source quotes");
  check(people[0].querySelector(":scope > small").textContent === "第 1 章 · 雾港邮局" && people[1].querySelector(":scope > small").textContent === "第 2 章 · 城门灯火", "Identity source guessed a chapter");
  check(people.every(person => !person.querySelector("details").open && person.querySelector("details dd").textContent === person.getAttribute("data-identity-id")), "Internal identity IDs weren't contained in source details"); capture("same names distinct sources");
  for (const theme of ["light", "dark"]) for (const width of [330, 740]) {
    document.documentElement.dataset.theme = theme; document.getElementById("root").style.width = `${width}px`; await tick();
    const panel = document.querySelector(".identity-decision-panel");
    check(panel.scrollWidth <= panel.clientWidth + 2 && [...panel.querySelectorAll(".identity-person")].every(person => person.scrollWidth <= person.clientWidth + 2), "Identity sources overflow the actual narrow panel");
    snapshots.push({ case: `${theme} ${width}`, fits: true, sourcesReadable: getComputedStyle(people[0].querySelector("blockquote")).whiteSpace === "pre-wrap" });
  }
  const sameButton = [...document.querySelectorAll("button")].find(item => item.textContent === "确认为同一人物");
  sameButton.click(); sameButton.click(); await tick(); await tick();
  check(serial === 1, "Duplicate identity click saved more than one decision");
  check(JSON.stringify(savedDecisions[0]) === JSON.stringify([{ kind: "same", fromId: "b", toId: "a" }]), "Display names replaced exact identity IDs in the save");
  check(document.body.textContent.includes("待审阅") && workspace.artifacts.identity.accepted === null, "Candidate was applied before author adoption");
  await click("撤回候选");
  check(workspace.reviews[0].status === "rejected" && workspace.revision === 0, "Reject changed formal story state");
  await click("确认是不同人物");
  await click("采用决议");
  check(workspace.artifacts.identity.accepted.payload.decisions[0].kind === "distinct", "Author decision was not committed");
  check(document.body.textContent.includes("已确认不同"), "Adopted distinction was not projected");
  check(JSON.stringify(workspace.artifacts.identity.accepted.payload.decisions) === JSON.stringify([{ kind: "distinct", leftId: "a", rightId: "b" }]), "Distinct same-name people weren't kept independent"); capture("author confirmed distinct");
  await click("移除此决议并审阅");
  check(workspace.artifacts.identity.accepted.payload.decisions.length === 1, "Removing a decision mutated accepted state before review");
  await click("采用决议");
  check(workspace.artifacts.identity.accepted.payload.decisions.length === 0, "Revised identity decision was not committed");
  await click("确认为同一人物"); await click("采用决议");
  check(JSON.stringify(workspace.artifacts.identity.accepted.payload.decisions) === JSON.stringify([{ kind: "same", fromId: "b", toId: "a" }]), "Accepted same identity changed its exact source IDs");
  check(document.querySelector('[data-identity-id="b"] > blockquote').textContent.includes("城门下") && document.querySelector('[data-identity-id="a"] > blockquote').textContent.includes("邮局门口"), "Merged canonical identity replaced the original entry's quote"); capture("author confirmed same with original references");
  const beforeMissing = serial; omitSecond = true; await click("刷新");
  check(document.querySelector('[data-identity-id="b"] > strong').textContent === "人物名称暂缺" && document.body.textContent.includes("人物来源暂缺"), "Missing identity was guessed from a matching name");
  check(serial === beforeMissing && workspace.artifacts.identity.accepted.payload.decisions[0].fromId === "b", "Missing-source display rewrote author decisions"); capture("missing source stays explicit");
  omitSecond = false; await click("刷新");
  const oldCatalog = structuredClone(catalog());
  let releaseOld, releaseNew;
  api.runtime.getIdentityCatalog = async project => new Promise(resolve => { if (project === "project-identity") releaseOld = resolve; else releaseNew = resolve; });
  await click("刷新");
  workspace = { project: { id: "project-other" }, revision: 0, artifacts: { identity: { candidate: null, accepted: null, working: null, locked: false, workingSelection: null } }, reviews: [], commits: [], chapters: [] };
  root.render(<IdentityDecisionPanel key="project-other" api={api} projectId="project-other" refreshToken={0} onFormalChange={async () => {}} />);
  await tick(); await tick();
  check(!document.body.textContent.includes("林夕") && !document.body.textContent.includes("撤销最近一次身份决议"), "Changing project showed old identity decisions while reading");
  releaseOld(oldCatalog); await tick();
  check(!document.body.textContent.includes("林夕"), "A late manual refresh installed another project's identities");
  releaseNew({ ...oldCatalog, entries: [], ambiguousNames: [], identityDecisions: [], identityRevisionId: null });
  await tick(); await tick();
  check(!document.querySelector(".identity-decision-panel"), "A complete empty identity catalogue still required manual work");
  return { passed: true, snapshots, saves: serial, savedDecisions };
}
run().then((result) => { window.testResult = result; }).catch((error) => { window.testResult = { passed: false, error: error.stack + "\nDOM:" + document.body.innerText }; });
