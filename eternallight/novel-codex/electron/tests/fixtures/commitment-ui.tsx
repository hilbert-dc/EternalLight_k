import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { CommitmentPanel } from "../../../src/CommitmentPanel";

window.addEventListener("error", (event) => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
const tick = () => new Promise((resolve) => setTimeout(resolve, 25));
const check = (condition, message) => { if (!condition) throw new Error(message); };
const text = () => document.body.textContent;
const button = (label) => [...document.querySelectorAll("button")].find((item) => item.textContent === label);
async function until(predicate, message) {
  const deadline = Date.now() + 3500;
  while (!predicate() && Date.now() < deadline) await tick();
  check(predicate(), message);
}
const proposal = { id: "lamp", kind: "foreshadowing", summary: "白塔只亮四十七秒", fulfillmentCriteria: "揭示灯时的原因",
  relatedEntityIds: [], plotThreadId: null, characterArcId: null, targetWindow: { kind: "open" }, status: "open", decisionReason: null,
  setupEvidenceRefs: [{ objectId: "draft-chapter-one", revisionId: "source-1", quote: "白塔亮了四十七秒", start: 0, end: 9 }], fulfillmentEvidenceRefs: [] };
let report = { revisionId: "report-1", snapshotId: "snapshot-report", projectRevision: 7, authority: "candidateProposalBatch", sourceCurrent: true,
  established: [], payload: { chapterId: "chapter-one", sourceRevisionId: "source-1", examinedRanges: [{ start: 0, end: 9 }], candidates: [proposal] },
  coverage: { semanticCompleteness: "unknown", modelReportedRanges: true, literaryFulfillmentVerified: false, globalCompletenessClaimed: false } };
let detail = { projectId: "project-a", authorControls: { projectRevision: 7, undoAvailable: false }, snapshotId: "snapshot-established", objectId: "commitment-independent", revisionId: "commitment-rev-1",
  payload: { ...proposal, id: "independent" }, extractionOrigin: { chapterId: "chapter-one", reportRevisionId: "report-1", proposalId: "lamp" },
  sourceCurrent: true, confirmationState: "candidate", confirmationCommitId: null, interpretation: "sourceBoundNarrativeClaim", literaryFulfillmentVerified: false };
let failRead = true;
let editor;
const saveCalls = [];
const receipts = new Map();
let failAfterSave = true;
let holdRead = false;
let releaseRead;
const calls = [];
let formalChanges = 0;
let failCatalog = true;
let holdCatalog = false;
let releaseCatalog;
let largeCatalog = false;
const api = { runtime: {
  listCommitments: async (project, cursor, limit) => {
    check(project === "project-a" && limit === 20, "Catalog scope or budget changed");
    if (largeCatalog) {
      const offset = cursor === null ? 0 : Number(cursor);
      return { snapshotId: "large-catalog", data: Array.from({ length: 20 }, (_, i) => ({ id: `bulk-${offset+i}`, summary: `批量承诺 ${offset+i}`, kind: "goal", status: "open", revisionId: `bulk-rev-${offset+i}`, confirmationState: "candidate", sourceCurrent: true })), nextCursor: offset < 200 ? String(offset+20) : null };
    }
    if (failCatalog) { failCatalog = false; throw new Error("目录读取失败"); }
    const data = cursor === null ? Array.from({ length: 20 }, (_, i) => ({ id: `catalog-${i}`, summary: `目录候选 ${i}`, kind: "goal", status: "open", revisionId: `catalog-rev-${i}`, confirmationState: "candidate", sourceCurrent: true }))
      : [{ id: "archived", summary: "旧报告建立的独立承诺", kind: "suspense", status: "open", revisionId: "archived-one", confirmationState: "candidate", sourceCurrent: false }];
    check(cursor === null || cursor === "catalog-next", "Catalog did not preserve its cursor");
    const result = { snapshotId: "catalog-frozen", data, nextCursor: cursor === null ? "catalog-next" : null };
    if (holdCatalog) { holdCatalog = false; result.data[0].summary = "迟到旧目录不应显示"; return await new Promise((resolve) => { releaseCatalog = () => resolve(result); }); }
    return result;
  },
  saveCommitment: async (project, id, request) => {
    check(project === "project-a" && id === "independent" && JSON.stringify(request.extractionOrigin) === JSON.stringify(detail.extractionOrigin), "Save changed identity or extraction origin");
    saveCalls.push(structuredClone(request));
    const stored = receipts.get(request.idempotencyKey);
    if (stored) { check(JSON.stringify(stored.request) === JSON.stringify(request), "Retry changed the frozen request"); return stored.receipt; }
    check(request.baseSnapshotId === detail.snapshotId && request.expectedCandidateRevisionId === detail.revisionId && request.expectedRevision === 9, "Save silently rebound CAS");
    const receipt = { revisionId: `saved-${receipts.size}`, snapshotId: `snapshot-saved-${receipts.size}` };
    receipts.set(request.idempotencyKey, { request: structuredClone(request), receipt });
    detail = { ...detail, payload: structuredClone(request.payload), revisionId: receipt.revisionId, snapshotId: receipt.snapshotId };
    if (failAfterSave) { failAfterSave = false; throw new Error("保存成功但回执断线"); }
    return receipt;
  },
  getCommitmentExtraction: async () => {
    if (failRead) { failRead = false; throw new Error("读取失败：测试断线"); }
    if (holdRead) {
      holdRead = false;
      const stale = structuredClone(report);
      stale.payload.candidates[0].summary = "迟到报告不应显示";
      return await new Promise((resolve) => { releaseRead = () => resolve(stale); });
    }
    return structuredClone(report);
  },
  getCommitment: async (_project, id) => {
    if (id === "archived") return { ...structuredClone(detail), objectId: "commitment-archived", revisionId: "archived-current", payload: { ...structuredClone(proposal), id, summary: "旧承诺的当前版本" }, sourceCurrent: false, extractionOrigin: { chapterId: "chapter-other", reportRevisionId: "old-replaced-report", proposalId: "old-local" } };
    check(id === "independent", "Wrong independent identity"); return structuredClone(detail);
  },
  establishCommitment: async (project, chapter, choice) => {
    calls.push({ establish: choice });
    check(project === "project-a" && chapter === "chapter-one", "Establishment escaped project/chapter");
    check(choice.reportRevisionId === "report-1" && choice.expectedRevision === 7 && choice.baseSnapshotId === "snapshot-report" && choice.idempotencyKey, "Establishment was not frozen to displayed report");
    report.established = [{ proposalId: "lamp", commitmentId: "independent" }];
    throw new Error("已写入但回执断线");
  },
  createCommitmentReview: async (project, id, revision, key) => {
    calls.push({ review: revision });
    check(project === "project-a" && id === "independent" && revision === "commitment-rev-1" && key, "Review was not source-bound");
    return { id: "review-1" };
  },
  acceptReview: async (project, review, expected, selections) => {
    calls.push({ accept: expected });
    check(project === "project-a" && review === "review-1" && expected === 7 && Object.keys(selections).length === 0, "Acceptance lost CAS");
    detail = { ...detail, confirmationState: "confirmed", confirmationCommitId: "accept-1", authorControls: { undoAvailable: true, projectRevision: 8 } };
    report.projectRevision = 8;
    return { commitId: "accept-1", resultRevision: 8, impacts: [] };
  },
  undoCommit: async (project, commit, expected) => {
    calls.push({ undo: commit });
    check(project === "project-a" && commit === "accept-1" && expected === 8, "Undo lost selected confirmation/CAS");
    detail = { ...detail, confirmationState: "candidate", confirmationCommitId: null, authorControls: { undoAvailable: false, projectRevision: 9 } };
    report.projectRevision = 9;
    return { commitId: "undo-1", resultRevision: 9 };
  },
} };
const root = createRoot(document.getElementById("root"));
let props = { api, projectId: "project-a", chapterId: "chapter-one", sourceRevisionId: "source-1", dirty: false, blocked: false, refreshToken: 0,
  registerEditor: (next) => { editor = next; },
  chapters: [{ id: "chapter-one", title: "第一章" }, { id: "chapter-two", title: "第二章" }, { id: "chapter-three", title: "尚未起草的第三章" }],
  sources: [{ chapterId: "chapter-one", title: "第一章", objectId: "draft-chapter-one", revisionId: "source-1", body: "白塔亮了四十七秒。" },
    { chapterId: "chapter-two", title: "第二章", objectId: "draft-chapter-two", revisionId: "source-2", body: "🌙白塔再度亮起。" }],
  onFormalChange: async () => { formalChanges += 1; } };
function render(changes = {}) { props = { ...props, ...changes }; root.render(<CommitmentPanel {...props} />); }
render();

async function run() {
  await until(() => document.querySelector("summary"), "Panel missing");
  check(!text().includes(proposal.summary), "Closed panel fetched or exposed content");
  document.querySelector("summary").click();
  await until(() => text().includes("读取失败"), "Read failure was hidden");
  button("刷新提案").click();
  await until(() => button("建立独立候选") && !button("建立独立候选").disabled, "Report unavailable after retry");
  check(text().includes("语义完整性未知") && text().includes("白塔亮了四十七秒"), "Evidence/coverage hidden");
  button("建立独立候选").click();
  await until(() => button("打开已建立承诺") && !button("打开已建立承诺").disabled, "Unknown write outcome was not recovered from persisted mapping");
  check(calls.length === 1 && !button("确认这份承诺"), "Establishment auto-confirmed or retried with new identity");
  button("打开已建立承诺").click();
  await until(() => button("确认这份承诺") && !button("确认这份承诺").disabled, "Independent candidate not opened");
  check(text().includes("未确认候选"), "Candidate disguised as confirmed");
  button("确认这份承诺").click();
  await until(() => button("撤销当前确认") && !button("撤销当前确认").disabled, "Confirmation not reflected");
  check(formalChanges === 1 && button("确认这份承诺").disabled, "Confirmation refresh/state invalid");
  detail.sourceCurrent = false;
  detail.authorControls.undoAvailable = false;
  button("刷新提案").click();
  await until(() => text().includes("不能直接撤销旧确认"), "Undo restriction not disclosed");
  check(button("撤销当前确认").disabled && button("确认这份承诺").disabled, "Stale source allowed unsupported mutation");
  detail.sourceCurrent = true;
  detail.authorControls.undoAvailable = true;
  button("刷新提案").click();
  await until(() => !button("撤销当前确认").disabled, "Eligible undo not restored");
  button("撤销当前确认").click();
  await until(() => !button("撤销当前确认") && text().includes("已撤销当前确认"), "Latest confirmation could not be withdrawn");
  check(formalChanges === 2 && !button("确认这份承诺").disabled, "Undo refreshed incorrect source state");
  button("调整这份承诺").click();
  await until(() => editor && document.querySelector('[aria-label="承诺摘要"]'), "Editor not registered");
  const input = async (label, value) => {
    const element = document.querySelector(`[aria-label="${label}"]`);
    check(element, `Missing field ${label}`);
    if (element instanceof HTMLSelectElement) { element.value = value; element.dispatchEvent(new Event("change", { bubbles: true })); }
    else { Object.getOwnPropertyDescriptor(element instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype, "value").set.call(element, value); element.dispatchEvent(new Event("input", { bubbles: true })); }
    await tick();
  };
  await input("承诺摘要", "作者修改的伏笔");
  check(editor.dirty, "Editor changes escaped global dirty protection");
  document.querySelector("summary").click(); await tick();
  check(editor?.dirty && document.querySelector('[aria-label="承诺摘要"]').value === "作者修改的伏笔", "Collapsing panel discarded input");
  document.querySelector("summary").click(); await tick();
  document.querySelector(".commitment-editor").scrollIntoView({ block: "start" });
  await new Promise((resolve) => setTimeout(resolve, 400));
  check(await editor.save() === false, "Unknown receipt incorrectly authorized navigation"); await tick();
  check(editor.dirty && document.querySelector('[aria-label="承诺摘要"]').value === "作者修改的伏笔", "Failed save erased input");
  await input("承诺摘要", "回执断线后的新输入");
  check(await editor.save() === false, "Resolving old save silently saved later input"); await tick();
  check(saveCalls.length === 2 && saveCalls[0].idempotencyKey === saveCalls[1].idempotencyKey && editor.dirty, "Unknown save retry lost its frozen identity");
  await input("承诺状态", "fulfilled");
  await input("承诺目标窗口", "chapterRange");
  await input("承诺结束章", "chapter-three");
  check(document.querySelector('[aria-label="承诺结束章"]').value === "chapter-three", "Future chapter without draft could not be selected");
  await input("承诺关联实体", "linxi,zhangheng");
  button("添加兑现证据").click(); await tick();
  await input("兑现证据 1 来源", "source-2");
  await input("兑现证据 1 引用", "白塔再度亮起。");
  await input("兑现证据 1 起始字符", "1");
  check(await editor.save() === true, "New input not saved after original receipt resolution"); await tick();
  const submitted = saveCalls[2].payload;
  check(submitted.summary === "回执断线后的新输入" && submitted.status === "fulfilled" && submitted.relatedEntityIds.length === 2 && submitted.targetWindow.throughChapterId === "chapter-three", "Structured edit omitted fields");
  check(submitted.fulfillmentEvidenceRefs[0].start === 1 && submitted.fulfillmentEvidenceRefs[0].end === 8 && submitted.fulfillmentEvidenceRefs[0].revisionId === "source-2", "Evidence did not use Unicode offsets and explicit source");
  await until(() => !editor.dirty && !editor.busy, "Saved editor remained dirty");
  check(detail.confirmationState === "candidate", "Saving auto-confirmed edited commitment");
  await input("承诺摘要", "保留未保存输入");
  const beforeInspect = saveCalls.length;
  button("核对当前版本并保留输入").click();
  await until(() => text().includes("已明确选用当前服务器版本"), "Explicit rebase not shown");
  check(document.querySelector('[aria-label="承诺摘要"]').value === "保留未保存输入" && editor.dirty && saveCalls.length === beforeInspect, "Explicit read saved or erased local input");
  editor.discard(); await tick();
  check(!editor && !document.querySelector(".commitment-editor"), "Discard did not unregister editor");
  render({ dirty: true });
  await until(() => text().includes("请先保存正文"), "Dirty source guard missing");
  check(!text().includes("白塔亮了四十七秒") && button("刷新提案").disabled, "Dirty source exposed obsolete evidence");
  render({ dirty: false });
  await until(() => button("打开已建立承诺") && !button("刷新提案").disabled, "Return to saved scope failed");
  holdRead = true;
  button("刷新提案").click();
  await until(() => releaseRead, "Delayed request was not issued");
  render({ chapterId: "chapter-two", sourceRevisionId: "source-2" });
  await until(() => text().includes("正文来源已变化"), "New scope did not reject stale source");
  report.sourceCurrent = false;
  render({ chapterId: "chapter-one", sourceRevisionId: "source-1" });
  await until(() => text().includes("正文来源已变化") && !button("刷新提案").disabled, "ABA scope read failed");
  releaseRead(); await tick(); await tick();
  check(!text().includes("迟到报告不应显示") && text().includes("正文来源已变化"), "ABA late response overwrote current scope");
  report = { ...report, established: [], payload: { ...report.payload, candidates: [] } };
  button("刷新提案").click();
  await until(() => text().includes("不代表本章没有伏笔"), "Empty report claimed completeness");
  report = null;
  button("刷新提案").click();
  await until(() => text().includes("本章尚无提取报告"), "Absent report not distinguished");
  document.querySelector(".commitment-catalog summary").click();
  await until(() => text().includes("目录读取失败"), "Catalog failure disguised as empty");
  button("刷新承诺目录").click();
  await until(() => button("加载更多承诺") && !button("加载更多承诺").disabled, "Catalog first page unavailable");
  button("加载更多承诺").click();
  await until(() => button("旧报告建立的独立承诺"), "Archived independent commitment undiscoverable");
  check(text().includes("目录候选 0") && text().includes("catalog-frozen"), "Catalog pagination replaced previous snapshot rows");
  document.querySelector(".commitment-catalog").scrollIntoView({ block: "start" });
  window.uiReady = true; await new Promise((resolve) => setTimeout(resolve, 400));
  button("旧报告建立的独立承诺").click();
  await until(() => text().includes("旧承诺的当前版本") && text().includes("old-replaced-report"), "Catalog did not reopen current independent object from old report");
  check(button("确认这份承诺").disabled && !button("调整这份承诺").disabled, "Stale evidence allowed confirmation or prevented author repair");
  await until(() => button("刷新承诺目录") && !button("刷新承诺目录").disabled, "Catalog did not recover after selection");
  holdCatalog = true;
  button("刷新承诺目录").click();
  await until(() => releaseCatalog, "Late catalog request was not started");
  const oldCatalog = document.querySelector(".commitment-catalog");
  render({ chapterId: "chapter-two", sourceRevisionId: "source-2" });
  await until(() => document.querySelector(".commitment-catalog") !== oldCatalog && !button("刷新提案").disabled, "New chapter did not finish reading");
  document.querySelector(".commitment-catalog summary").click();
  await until(() => button("目录候选 0"), "New chapter catalog not loaded");
  releaseCatalog(); await tick(); await tick();
  check(!text().includes("迟到旧目录不应显示") && text().includes("目录候选 0"), "Late catalog overwrote current scope");
  largeCatalog = true;
  button("刷新承诺目录").click();
  await until(() => button("批量承诺 0"), "Large catalog first page unavailable");
  for (let offset = 20; offset <= 200; offset += 20) {
    await until(() => button("加载更多承诺") && !button("加载更多承诺").disabled, "Large catalog paging stopped");
    button("加载更多承诺").click();
    await until(() => button(`批量承诺 ${offset+19}`), "Large catalog next batch unavailable");
  }
  check(!button("批量承诺 0") && button("批量承诺 219") && text().includes("已收起早期 20 项") && document.querySelectorAll(".commitment-catalog li").length === 200, "Memory cap blocked later objects or accumulated unbounded rows");
  check(calls.filter((call) => call.establish).length === 1 && calls.some((call) => call.undo === "accept-1"), "Write sequence invalid");
  window.testResult = { passed: true };
}
run().catch((error) => { window.testResult = { passed: false, error: error.stack }; });
