import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { CommitmentTaskResult } from "../../../src/CommitmentTaskResult";

const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (value, message) => { if (!value) throw Error(message); };
const until = async (predicate, message) => { for (let i = 0; i < 200 && !predicate(); i++) await tick(); check(predicate(), message); };
const button = label => [...document.querySelectorAll<HTMLButtonElement>("button")].find(node => node.textContent === label);
const text = () => document.body.textContent;
const reads = [], writes = [], adjusted = [];
let formalChanges = 0, claims = 0, releases = 0, acquired = false, allowClaim = true, failEstablish = true;
let holdEstablish = false, releaseEstablish, holdReport = false, releaseReport, returnReplacement = false, supersedeAfterDetail = false;
const proposal = summary => ({ id: "lamp", kind: "foreshadowing", summary, fulfillmentCriteria: "揭示钟声持续时间的原因",
  relatedEntityIds: [], plotThreadId: null, characterArcId: null, targetWindow: { kind: "open" }, status: "open", decisionReason: null,
  setupEvidenceRefs: [{ objectId: "draft-chapter", revisionId: "source", quote: "白塔只亮了四十七秒", start: 0, end: 10 }], fulfillmentEvidenceRefs: [] });
const report = (id, summary, changes = {}) => ({ revisionId: id, snapshotId: `${id}-snapshot`, projectRevision: 3,
  authority: "candidateProposalBatch", reportCurrent: true, sourceCurrent: true, established: [],
  payload: { chapterId: "chapter", sourceRevisionId: "source", examinedRanges: [{ start: 0, end: 10 }], candidates: [proposal(summary)] },
  coverage: { semanticCompleteness: "unknown", modelReportedRanges: true, literaryFulfillmentVerified: false, globalCompletenessClaimed: false }, ...changes });
const reports = new Map([
  ["old", report("old", "旧报告：钟声的来处", { reportCurrent: false })],
  ["new", report("new", "新报告：白塔只亮四十七秒")],
  ["adjust", report("adjust", "需要调整的建议")],
  ["adjust-stale", report("adjust-stale", "调整读取期间失效的建议")],
  ["late", report("late", "迟到操作的建议")],
  ["empty", { ...report("empty", ""), payload: { chapterId: "chapter", sourceRevisionId: "source", examinedRanges: [], candidates: [] } }],
]);
const details = new Map();
let latest = "new";
const api = { runtime: {
  getCommitmentExtraction: async (project, chapter, revision = null) => {
    reads.push([project, chapter, revision]);
    check(project === "project" && chapter === "chapter", "Report escaped its project/chapter");
    const value = structuredClone(reports.get(returnReplacement ? "new" : revision ?? latest));
    if (holdReport && revision === "old") { holdReport = false; return new Promise(resolve => { releaseReport = () => resolve(value); }); }
    return value;
  },
  getCommitment: async (project, id) => {
    check(project === "project" && details.has(id), "Unknown independent commitment");
    if (supersedeAfterDetail) { supersedeAfterDetail = false; reports.get("adjust-stale").reportCurrent = false; }
    return structuredClone(details.get(id));
  },
  establishCommitment: async (project, chapter, request) => {
    check(project === "project" && chapter === "chapter", "Establishment escaped the displayed report");
    const value = reports.get(request.reportRevisionId);
    check(value.reportCurrent && value.sourceCurrent && request.baseSnapshotId === value.snapshotId
      && request.expectedRevision === value.projectRevision && request.idempotencyKey, "Establishment changed the displayed CAS");
    writes.push({ type: "establish", report: request.reportRevisionId, key: request.idempotencyKey });
    if (holdEstablish) { holdEstablish = false; await new Promise(resolve => { releaseEstablish = resolve; }); }
    const id = `independent-${request.reportRevisionId}`;
    value.established = [{ proposalId: "lamp", commitmentId: id }];
    details.set(id, { projectId: project, objectId: `commitment-${id}`, revisionId: `${id}-revision`, snapshotId: `${id}-snapshot`,
      payload: { ...structuredClone(value.payload.candidates[0]), id }, sourceCurrent: true,
      authorControls: { projectRevision: 3, undoAvailable: false }, confirmationState: "candidate", confirmationCommitId: null,
      extractionOrigin: { chapterId: chapter, reportRevisionId: value.revisionId, proposalId: "lamp" },
      interpretation: "sourceBoundNarrativeClaim", literaryFulfillmentVerified: false });
    if (failEstablish) { failEstablish = false; throw Error("建立已保存，但回执断线"); }
    return { commitmentId: id, revisionId: `${id}-revision`, snapshotId: `${id}-snapshot`, confirmationState: "candidate" };
  },
  createCommitmentReview: async (project, id, revision, key) => {
    check(project === "project" && details.get(id).revisionId === revision && key, "Review lost independent revision");
    writes.push({ type: "review", id });
    return { id: `review-${id}`, artifactKind: "commitment", artifactRevisionId: revision, projectRevision: 3, status: "ready" };
  },
  acceptReview: async (project, review, revision) => {
    check(project === "project" && revision === 3, "Acceptance changed expected project revision");
    const id = review.replace(/^review-/, "");
    writes.push({ type: "accept", id });
    Object.assign(details.get(id), { confirmationState: "confirmed", confirmationCommitId: `accepted-${id}` });
    return { commitId: `accepted-${id}`, resultRevision: 4, impacts: [] };
  },
} };
const destination = (id, deliveryState = "recorded") => ({ kind: "commitmentExtraction", projectId: "project", workTaskId: `work-${id}`,
  stepId: `step-${id}`, attemptId: `attempt-${id}`, referenceId: id, chapterId: "chapter", chapterNumber: 2, sourceRevisionId: "source", deliveryState });
const root = createRoot(document.getElementById("root"));
let props = { api, result: destination("old", "superseded"), disabled: false, refreshToken: 1,
  onFormalChange: async () => { formalChanges++; }, onAdjust: (result, detail) => adjusted.push([result.referenceId, detail]),
  acquireInteraction: () => { claims++; if (!allowClaim || acquired) return false; acquired = true; return true; },
  releaseInteraction: () => { check(acquired, "Lock released twice"); acquired = false; releases++; } };
const render = async (changes = {}) => { props = { ...props, ...changes }; root.render(<React.StrictMode><div style={{ width: 300 }}><CommitmentTaskResult {...props} /></div></React.StrictMode>); await tick(); await tick(); };

async function run() {
  await render();
  await until(() => text().includes("旧报告：钟声的来处"), "Old report not readable");
  check(text().includes("第 2 章 · 伏笔建议"), "Task result chapter scope was lost");
  check(!text().includes("新报告：") && button("确认保留这条伏笔").disabled && writes.length === 0, "Historical report substituted latest or enabled a write");
  reports.get("old").reportCurrent = true;
  await render({ refreshToken: 2 });
  await until(() => !document.querySelector('[aria-busy="true"]'), "Read did not settle");
  check(button("确认保留这条伏笔").disabled, "Superseded delivery used reportCurrent to obtain permission");
  reports.get("old").reportCurrent = false;
  await render({ result: destination("new"), disabled: true });
  await until(() => text().includes("新报告：白塔只亮四十七秒"), "Current report not shown");
  check(button("确认保留这条伏笔").disabled && writes.length === 0, "Parent editing/execution guard was ignored");
  await render({ disabled: false });
  await until(() => !button("确认保留这条伏笔").disabled, "Current report remained blocked");
  const evidence = document.querySelector<HTMLDetailsElement>('.commitment-task-proposal details');
  check(!evidence.open && evidence.dataset.conversationDetail.includes("new:lamp:evidence"), "Evidence lacks stable closed identity");
  evidence.querySelector<HTMLElement>("summary").click();
  await render({ refreshToken: 3 });
  await until(() => !document.querySelector('[aria-busy="true"]'), "Refresh did not settle");
  check(evidence === document.querySelector('.commitment-task-proposal details') && evidence.open, "Refresh destroyed opened evidence");
  check(document.querySelector('.commitment-task-result').scrollWidth <= 301, "Narrow card overflowed");
  const confirm = button("确认保留这条伏笔"); confirm.click(); confirm.click();
  await until(() => writes.length === 1 && !button("确认保留这条伏笔").disabled, "Unknown establish outcome did not recover read-only");
  check(writes[0].type === "establish" && !text().includes("对应伏笔已确认") && formalChanges === 0,
    "Established mapping was mistaken for confirmed or continued writes automatically");
  button("确认保留这条伏笔").click();
  await until(() => button("对应伏笔已确认") && !document.querySelector('[aria-busy="true"]'), "Explicit confirmation did not finish");
  check(writes.map(write => write.type).join() === "establish,review,accept" && formalChanges === 1, "Confirmation replayed establishment or lost its receipt");
  check(button("对应伏笔已确认").disabled && claims === 2 && releases === 2, "Real confirmed status or lock lifecycle wrong");
  latest = "adjust";
  await render({ result: destination("adjust") });
  await until(() => text().includes("需要调整的建议") && !button("调整后再决定").disabled, "Adjustment not available");
  button("调整后再决定").click(); button("调整后再决定").click();
  await until(() => adjusted.length === 1 && !document.querySelector('[aria-busy="true"]'), "Exact adjustment did not open");
  check(adjusted[0][0] === "adjust" && adjusted[0][1].extractionOrigin.reportRevisionId === "adjust"
    && adjusted[0][1].confirmationState === "candidate" && writes.at(-1).type === "establish", "Adjustment auto-confirmed or opened another proposal");
  const adjustmentWrites = writes.length;
  button("调整后再决定").click();
  await until(() => adjusted.length === 2 && !document.querySelector('[aria-busy="true"]'), "Established adjustment could not reopen");
  check(writes.length === adjustmentWrites, "Reopening an established candidate repeated its write");
  latest = "adjust-stale";
  await render({ result: destination("adjust-stale") });
  await until(() => text().includes("调整读取期间失效的建议") && !button("调整后再决定").disabled, "Adjustment race case missing");
  supersedeAfterDetail = true; button("调整后再决定").click();
  await until(() => text().includes("已有后续报告") && !document.querySelector('[aria-busy="true"]'), "Report change while reading detail was not checked");
  check(adjusted.length === 2 && button("调整后再决定").disabled && writes.at(-1).type === "establish",
    "Report superseded during adjustment opened an editor or accepted the candidate");
  await render({ result: destination("adjust") });
  await until(() => text().includes("需要调整的建议"), "Current adjustment report did not recover");
  reports.get("adjust").sourceCurrent = false;
  await render({ refreshToken: 4 });
  await until(() => text().includes("正文来源已变化"), "Stale source disappeared");
  check(text().includes("需要调整的建议") && button("确认保留这条伏笔").disabled && button("调整后再决定").disabled, "Stale source cannot be read safely");
  await render({ result: destination("empty") });
  await until(() => text().includes("本次没有提出可靠候选"), "Empty proposal report missing");
  check(!button("确认保留这条伏笔") && text().includes("不代表本章没有伏笔"), "Empty report overclaimed coverage");
  returnReplacement = true;
  await render({ result: destination("old", "superseded") });
  await until(() => text().includes("未用其他版本替代"), "Replacement was not rejected");
  check(!text().includes("新报告："), "Wrong report leaked into a historical card");
  returnReplacement = false; holdReport = true;
  await render({ result: destination("old", "recorded"), refreshToken: 5 });
  await until(() => releaseReport, "Late read was not held");
  await render({ result: destination("new") });
  await until(() => text().includes("新报告："), "Current owner not restored");
  releaseReport(); await tick(); await tick();
  check(!text().includes("旧报告："), "Late old report replaced another owner");
  latest = "late"; holdEstablish = true;
  await render({ result: destination("late") });
  await until(() => text().includes("迟到操作的建议") && !button("确认保留这条伏笔").disabled, "Late operation source missing");
  button("确认保留这条伏笔").click();
  await until(() => releaseEstablish, "In-flight decision missing");
  const beforeLate = writes.length;
  await render({ result: destination("old", "superseded") });
  await until(() => text().includes("旧报告："), "Replacement owner not visible");
  check(!acquired, "Owner change retained the old lock");
  releaseEstablish(); await tick(); await tick();
  check(writes.length === beforeLate && !text().includes("迟到操作") && !text().includes("对应伏笔已确认"), "Late decision continued confirmation in another owner");
  latest = "adjust"; reports.get("adjust").sourceCurrent = true;
  await render({ result: destination("adjust") });
  await until(() => button("确认保留这条伏笔") && !button("确认保留这条伏笔").disabled, "Lock rejection case missing");
  allowClaim = false; button("确认保留这条伏笔").click();
  await until(() => text().includes("另一条建议正在处理"), "Lock rejection not visible");
  check(writes.length === beforeLate && !acquired, "Rejected interaction wrote or took a lock");
  root.unmount();
  return { passed: true, writes: writes.map(write => write.type), formalChanges, adjustments: adjusted.length,
    preciseReads: reads.filter(read => read[2] !== null).map(read => read[2]), claims, releases };
}
run().then(value => { window.testResult = value; }).catch(error => { window.testResult = { passed: false, error: error.stack, dom: text(), writes }; });
