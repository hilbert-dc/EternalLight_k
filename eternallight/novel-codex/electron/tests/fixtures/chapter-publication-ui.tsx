import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { CreativeWorkspaceView } from "./creative-workspace-harness";

const tick = () => new Promise(resolve => setTimeout(resolve, 15));
const check = (value, message) => { if (!value) throw new Error(message); };
const until = async (predicate, message) => { const deadline = performance.now() + 2500; while (!predicate() && performance.now() < deadline) await tick(); check(predicate(), message); };
const button = label => [...document.querySelectorAll("button")].find(item => item.textContent === label);
const click = async label => { const item = button(label); check(item && !item.disabled, `Unavailable: ${label}`); item.click(); await tick(); await tick(); };
const change = async (selector, value) => {
  const item = document.querySelector(selector); check(item && !item.disabled, `Unavailable: ${selector}`);
  const prototype = item instanceof HTMLSelectElement ? HTMLSelectElement.prototype : item instanceof HTMLInputElement ? HTMLInputElement.prototype : HTMLTextAreaElement.prototype;
  Object.getOwnPropertyDescriptor(prototype, "value").set.call(item, value);
  item.dispatchEvent(new Event(item instanceof HTMLSelectElement ? "change" : "input", { bubbles: true })); await tick();
};
const chapter = id => ({ id, number: 1, title: "待发布章节", status: "draft", summary: "", passages: [] });
const revision = (id, title = "待发布标题") => ({ id, title, version: 2, source: "agent", chapterId: "a", body: "本次正文", sceneGroups: [
  { id: "new-second", sceneId: "stable-second", body: "第二场被重排", locked: false },
  { id: "new-first", sceneId: "stable-first", body: "第一场修订内容", locked: false },
], sourcePlanRefs: { lineageStatus: "current", localOnly: true } });
const makeWorkspace = projectId => ({ project: { id: projectId }, revision: 1, headSnapshotId: "head", sourceDigest: "a".repeat(64),
  artifacts: Object.fromEntries(["brief", "outline", "plan"].map(kind => [kind, { candidate: null, accepted: null, working: null, workingSelection: null, locked: false }])),
  chapters: [{ ...chapter("a"), candidate: revision(`${projectId}-new`), accepted: { ...revision(`${projectId}-published`, "已发布旧标题"), sceneGroups: [{ id: "deleted", sceneId: "deleted", body: "被删除的旧场景" }] }, working: null, workingSelection: null }],
  workingBaselines: [], continuationIntents: [], planningReviews: [], commits: [], tasks: [], impacts: [], factProposals: [], reviews: [], index: { status: "current" },
  modeling: [{ objectId: "draft-a", sourceRevisionId: `${projectId}-new`, status: "ready", batchId: "batch-six", entities: [{ id: "person", displayName: "林舟" }], assertions: [], confirmationState: "candidate" }] });
let workspace = makeWorkspace("publication-a"), projectId = "publication-a", refreshToken = 0;
let saves = [], reviews = [], acceptances = [], modelCalls = 0, failAfterSave = false, failReads = 0, failFormal = false, holdRead = false, releaseRead;
let addProtectedImpact = false, agentRunning = false;
let failAfterReview = false, foreignAfterReview = false, conflictingAfterReview = false, foreignRead = false;
const snapshots = [];
const api = { runtime: {
  getWorkCatalog: async () => ({ revision: 0, volumes: [], placements: [] }),
  getCreativeWorkspace: async id => {
    const copy = structuredClone(workspace); check(copy.project.id === id, "wrong project read");
    if (holdRead) { holdRead = false; await new Promise(resolve => { releaseRead = () => resolve(); }); }
    if (failReads) { failReads--; throw new Error("读取夹具中断"); }
    if (foreignRead) { foreignRead = false; copy.project.id = "foreign-project"; copy.chapters[0].candidate.title = "不属于本作品"; }
    return copy;
  },
  listPlanningImpacts: async () => ({ data: [], nextCursor: null, counts: { total: 0, open: 0, retained: 0, lockedConflict: 0 } }),
  listGenerationRuns: async () => ({ runs: [] }), listWorkTasks: async () => ({ tasks: [], nextCursor: null, activeCount: 0 }),
  ...Object.fromEntries(["Search", "Entity", "Relation", "Knowledge", "SnapshotKnowledge", "Fact"].map(kind => [`get${kind}Audit`, async () => ({ job: null })])),
  getWriterPreview: async () => ({ plannedEffects: [], relevantText: [], target: { position: {} }, coverage: { complete: true, missingScope: [], truncationReasons: [], scannedScope: {} }, budgetsUsed: {} }),
  saveDraftCheckpoint: async (id, payload, expectedRevision, expectedCandidate) => {
    check(id === workspace.project.id && expectedRevision === workspace.revision && expectedCandidate === workspace.chapters[0].candidate.id, "save baseline drift");
    saves.push(structuredClone(payload));
    const next = { ...revision(`saved-${saves.length}`, payload.title), ...payload, source: "author" };
    workspace.chapters[0].candidate = next;
    workspace.modeling = [{ ...workspace.modeling[0], sourceRevisionId: next.id, status: "model_pending", batchId: null, entities: [], assertions: [] }];
    workspace.reviews.forEach(review => { review.status = "stale"; }); workspace.factProposals = [];
    if (failAfterSave) { failAfterSave = false; failReads = 1; }
    return { revisionId: next.id };
  },
  createReview: async (id, kind, revisionId, proposals, selections, batchId) => {
    check(id === workspace.project.id && kind === "draft" && revisionId === workspace.chapters[0].candidate.id, "review used stale autosave candidate");
    const modeling = workspace.modeling[0];
    check(modeling.sourceRevisionId === revisionId, "review knowledge source drift");
    if (modeling.status === "ready") check(batchId === modeling.batchId && selections.modelingBatchId === batchId && !proposals.length, "ready six facts bypassed batch binding");
    else check(!batchId, "pending review reused old batch");
    const review = { id: `review-${reviews.length}`, artifactKind: kind, artifactRevisionId: revisionId, projectRevision: workspace.revision, status: "ready",
      checks: [{ id: "current-candidate", passed: true, blocking: true, message: "候选版本是当前待审版本" }], selections };
    const factProposals = modeling.status === "ready" ? Array.from({ length: 6 }, (_, index) => ({ id: `fact-${batchId}-${index}`, sceneGroupId: "new-first", draft_revision_id: revisionId,
      factKind: "world", subjectId: "person", predicate: `变化${index + 1}`, value: true, epistemicOwnerId: "person", epistemicState: "known", evidence: "第一场修订内容", source: "agent" })) : proposals;
    if (addProtectedImpact) {
      addProtectedImpact = false;
      review.selections = { ...review.selections, provisionalImpacts: [{ id: "protected-change", factProposalId: factProposals[1].id, severity: "locked-conflict", targetArtifactKind: "outline", targetNodeId: "protected-node", targetLabel: "保留来信悬念", predicate: "变化2", evidence: "第一场修订内容" }] };
    }
    workspace.reviews.push(review); workspace.factProposals.push(...factProposals); reviews.push({ revisionId, batchId, factIds: factProposals.map(fact => fact.id) });
    if (failAfterReview) { failAfterReview = false; failReads = 1; }
    if (foreignAfterReview) { foreignAfterReview = false; foreignRead = true; }
    if (conflictingAfterReview) { conflictingAfterReview = false; workspace.factProposals.push({ ...factProposals[0], value: false }); }
    return { ...review, factProposals };
  },
  acceptReview: async (id, reviewId, expectedRevision, selections) => {
    check(id === workspace.project.id && expectedRevision === workspace.revision && JSON.stringify(selections.sceneGroupIds) === JSON.stringify(workspace.chapters[0].candidate.sceneGroups.map(group => group.id)), "publication was not atomic whole chapter");
    const review = workspace.reviews.find(item => item.id === reviewId);
    check(review?.artifactRevisionId === workspace.chapters[0].candidate.id, "published wrong source");
    const facts = workspace.factProposals.filter(fact => fact.id.startsWith(`fact-${review.selections.modelingBatchId}-`));
    if (facts.length) check(facts.every(fact => ["confirmed", "rejected"].includes(selections.factDecisions[fact.id])) && !selections.noConfirmedChanges, "six fact decisions incomplete or body-only leaked");
    else check(selections.noConfirmedChanges, "manual consent missing");
    acceptances.push(structuredClone(selections));
    workspace.chapters[0].accepted = structuredClone(workspace.chapters[0].candidate); workspace.revision++;
    workspace.reviews.forEach(item => { item.status = item.id === reviewId ? "accepted" : "stale"; }); workspace.factProposals = [];
    return { commitId: `commit-${acceptances.length}`, resultRevision: workspace.revision };
  },
}, agent: {} };
const root = createRoot(document.getElementById("root"));
window.addEventListener("error", event => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
const render = async () => {
  root.render(<CreativeWorkspaceView taskConversationContainer={null} api={api} projectId={projectId} view="chapters" selectedChapterId="a" refreshToken={refreshToken++}
    state={{ project: { id: projectId }, canon: { characters: [], locations: [], relationships: [], events: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { scenes: [], anchors: [], version: "1" }, novelEdition: { chapters: [chapter("a")] } }}
    agentAvailable={false} agentRunning={agentRunning} onSelectChapter={() => {}} onGenerateWork={async () => { modelCalls++; }}
    onGenerateChapter={async () => { modelCalls++; }} onContinueWork={async () => {}} onRetryWork={async () => {}} onResolveFeedback={async () => {}}
    onReviewPlanning={async () => {}} onCancelPlanningReview={async () => {}} onOpenPlanningTarget={() => {}} onReturnToFeedback={() => {}} onDirtyChange={() => {}}
    onFormalChange={async () => { if (failFormal) throw new Error("发布后的作品读取失败"); }} />);
  await tick(); await tick(); await tick();
};
const confirmSix = async () => { for (let index = 1; index <= 6; index++) await change(`[aria-label="变化${index} 的确认结论"]`, index === 6 ? "rejected" : "confirmed"); };
const capture = label => snapshots.push({ label, status: document.querySelector('[aria-label="章节发布状态"]')?.textContent,
  dialog: document.querySelector('[role="dialog"]')?.textContent, actions: [...document.querySelectorAll('[role="dialog"] button')].map(item => ({ label: item.textContent, disabled: item.disabled })) });

async function run() {
  await render(); await until(() => document.querySelector('[aria-label="章节标题"]'), "editor did not mount"); check(!document.querySelector('.review-panel') && !document.querySelector('[role="dialog"]'), "check panel exposed by default");
  await change('[aria-label="章节标题"]', "作者的新标题"); await change('[aria-label="场景 2 正文"]', "作者精确的新正文");
  failAfterSave = true; const publish = button("发布章节"); publish.click(); publish.click();
  await until(() => button("重新读取已保存章节"), "committed save read failure lost recovery");
  check(saves.length === 1 && reviews.length === 0 && document.querySelector('[aria-label="场景 2 正文"]').value === "作者精确的新正文", "save failure lost editor or duplicated save");
  const savedRevisionId = workspace.chapters[0].candidate.id; workspace.chapters[0].candidate.id = "different-saved-revision";
  await click("重新读取已保存章节"); check(button("重新读取已保存章节") && saves.length === 1 && document.querySelector('[aria-label="场景 2 正文"]').value === "作者精确的新正文", "unrelated GET cleared saved receipt or local content");
  workspace.chapters[0].candidate.id = savedRevisionId;
  await click("重新读取已保存章节"); check(saves.length === 1, "read-only recovery saved again");
  await click("发布章节"); await until(() => button("继续发布前检查"), "pending author publication has no explicit choice");
  check(!document.querySelector('.no-fact-decision input').checked && button("继续发布前检查").disabled && reviews.length === 0, "body-only was silently selected");
  capture("manual consent required");
  document.querySelector('.no-fact-decision input').click(); await tick(); await click("继续发布前检查");
  await until(() => button("确认发布章节") && !button("确认发布章节").disabled, "manual checks did not bind fresh saved revision");
  check(reviews[0].revisionId === "saved-1" && !reviews[0].batchId, "autosave reused old revision/batch");
  document.querySelector('[aria-label="关闭发布前检查"]')?.click(); await tick();
  check(document.activeElement === button("发布章节"), "close did not return focus to publish");
  await click("发布章节"); await until(() => button("确认发布章节"), "checks not reopened");
  check(reviews.length === 1 && document.querySelector('.no-fact-decision input').checked, "reopening discarded checks or consent");
  failFormal = true; const finalize = button("确认发布章节"); finalize.click(); finalize.click();
  await until(() => button("重新读取发布结果"), "committed publication lacked read recovery");
  check(acceptances.length === 1 && workspace.chapters[0].accepted.title === "作者的新标题" && workspace.chapters[0].accepted.sceneGroups[1].body === "作者精确的新正文"
    && workspace.chapters[0].accepted.sceneGroups.map(group => group.sceneId).join(",") === "stable-second,stable-first", "whole chapter lost title/reorder/deletion or duplicate publish");
  failFormal = false; const publishedRevisionId = workspace.chapters[0].accepted.id; workspace.chapters[0].accepted.id = "different-published-revision";
  await click("重新读取发布结果"); check(button("重新读取发布结果") && acceptances.length === 1, "unrelated published revision cleared publication receipt");
  workspace.chapters[0].accepted.id = publishedRevisionId;
  await click("重新读取发布结果"); check(acceptances.length === 1 && !document.querySelector('[role="dialog"]'), "publication recovery mutated again");
  check(!button("继续分析本章") && !document.body.innerText.includes("故事设定分析尚未完成") && !document.querySelector(".chapter-analysis-status"), "Optional knowledge analysis remained an author obligation after body-only publication");
  capture("published whole chapter");

  workspace = makeWorkspace(projectId); workspace.modeling[0].status = "model_pending"; workspace.modeling[0].batchId = null;
  await render(); await click("发布章节"); await until(() => button("继续发布前检查"), "pending second source no consent");
  document.querySelector('.no-fact-decision input').click(); await tick();
  workspace.modeling[0].status = "ready"; workspace.modeling[0].batchId = "batch-six";
  addProtectedImpact = true; await click("继续发布前检查"); await until(() => document.querySelectorAll('.fact-review-card').length === 6, "fresh ready six-fact batch did not take priority");
  check(!document.querySelector('.no-fact-decision') && button("确认发布章节").disabled, "manual noConfirmedChanges leaked to modeled batch");
  check(document.querySelector('.fact-review-card header > div > small:nth-of-type(2)').textContent.includes("林舟"), "entity IDs were presented as primary names");
  capture("six facts require decisions");
  await change('[aria-label="变化6 的确认结论"]', "rejected");
  await change('[aria-label="变化1 的确认结论"]', "confirmed");
  await until(() => button("确认未决定的 4 项") && !button("确认未决定的 4 项").disabled, "bulk actions did not identify remaining suggestions");
  await click("确认未决定的 4 项");
  check(document.querySelector('[aria-label="变化6 的确认结论"]').value === "rejected" && document.querySelector('[aria-label="变化1 的确认结论"]').value === "confirmed", "bulk confirmation overwrote an existing single decision");
  check(!document.querySelector(".publication-bulk-decisions button") && document.querySelector('[aria-label="建议决定进展"]').textContent.includes("0 项未决定，6 项已有决定"), "completed bulk decision counts/disabled state were inaccurate");
  check(button("确认发布章节").disabled && document.querySelector('[aria-label="发布检查进展"]').textContent.includes("保护冲突"), "bulk confirmation bypassed unresolved protection");
  capture("bulk confirmation preserves existing decisions and protection");
  document.querySelector('[aria-label="关闭发布前检查"]').click(); await tick(); await click("发布章节");
  await until(() => document.querySelector('[aria-label="变化6 的确认结论"]'), "bulk decisions did not reopen");
  check(document.querySelector('[aria-label="变化6 的确认结论"]').value === "rejected" && document.querySelector('[aria-label="变化2 的确认结论"]').value === "confirmed", "closing/reopening lost bulk or single decisions");
  await change('[aria-label="变化3 的确认结论"]', "");
  agentRunning = true; await render();
  check(button("拒绝未决定的 1 项").disabled, "batch action remained enabled while model was running");
  agentRunning = false; await render(); await until(() => !button("拒绝未决定的 1 项").disabled, "batch action failed to resume after running lock");
  await click("拒绝未决定的 1 项");
  check(document.querySelector('[aria-label="变化1 的确认结论"]').value === "confirmed" && document.querySelector('[aria-label="变化2 的确认结论"]').value === "confirmed" && document.querySelector('[aria-label="变化3 的确认结论"]').value === "rejected" && document.querySelector('[aria-label="变化6 的确认结论"]').value === "rejected", "bulk rejection overwrote existing confirmations or rejections");
  check(button("确认发布章节").disabled && acceptances.length === 1 && modelCalls === 0, "batch rejection published, generated, or resolved protected confirmation");
  await change('[aria-label="保护冲突处理决定"]', "accept-and-open-impact");
  capture("bulk rejection preserves prior confirmations");
  await until(() => !button("确认发布章节").disabled, "six fact decisions failed checks");
  await click("确认发布章节"); check(acceptances.length === 2 && acceptances[1].modelingBatchId === "batch-six" && Object.values(acceptances[1].factDecisions).filter(value => value === "rejected").length === 2, "modeled publication bypassed six-fact decisions");

  workspace = makeWorkspace(projectId); workspace.tasks = [{ id: "task-active", status: "running", purpose: "writer", target: { chapterId: "a" }, outputRefs: [] }];
  await render(); check(button("发布章节").disabled && button("保存").disabled, "active chapter delivery could be bypassed");
  workspace.tasks = []; await render();
  holdRead = true; button("发布章节").click(); await until(() => releaseRead, "delayed source read absent"); const releaseOld = releaseRead;
  projectId = "publication-b"; workspace = makeWorkspace(projectId); await render();
  await click("发布章节"); await until(() => document.querySelectorAll('.fact-review-card').length === 6, "new scope was blocked by old owner");
  projectId = "publication-a"; workspace = makeWorkspace(projectId); workspace.chapters[0].candidate.title = "A 新生命周期标题"; await render();
  releaseOld(); await tick(); await tick();
  check(!document.querySelector('[role="dialog"]') && document.querySelector('[aria-label="章节标题"]').value === "A 新生命周期标题", "A-B-A late reply polluted renewed scope");

  projectId = "receipt-project"; workspace = makeWorkspace(projectId); await render();
  await api.runtime.createReview(projectId, "draft", workspace.chapters[0].candidate.id, [], { modelingBatchId: "batch-six" }, "batch-six");
  await api.runtime.createReview(projectId, "draft", workspace.chapters[0].candidate.id, [], { modelingBatchId: "batch-six" }, "batch-six");
  failAfterReview = true; await click("发布章节");
  await until(() => document.querySelector(".creative-alert.error details"), "check error details absent");
  document.querySelector<HTMLDetailsElement>(".creative-alert.error details")!.open = true;
  await until(() => document.body.innerText.includes("读取夹具中断"), "check read failure absent");
  const createdCount = reviews.length; await click("发布章节"); await until(() => document.querySelectorAll('.fact-review-card').length === 6, "read recovery duplicated or lost batch facts");
  check(reviews.length === createdCount, "successful check creation was repeated after read failure");
  check(new Set([...document.querySelectorAll('.fact-review-card select')].map(item => item.getAttribute('aria-label'))).size === 6, "duplicate batch facts produced duplicate decisions");
  document.querySelector('[aria-label="关闭发布前检查"]').click(); await tick();

  projectId = "foreign-read-project"; workspace = makeWorkspace(projectId); await render();
  foreignAfterReview = true; await click("发布章节"); await until(() => document.querySelector(".creative-alert.error details"), "error details absent"); document.querySelector<HTMLDetailsElement>(".creative-alert.error details")!.open = true; await until(() => document.body.innerText.includes("不同作品"), "foreign second read was accepted");
  check(document.querySelector('[aria-label="章节标题"]').value === "待发布标题" && !button("确认发布章节"), "foreign read polluted editor or publication target");
  projectId = "conflict-read-project"; workspace = makeWorkspace(projectId); await render();
  conflictingAfterReview = true; await click("发布章节"); await until(() => document.querySelector(".creative-alert.error details"), "error details absent"); document.querySelector<HTMLDetailsElement>(".creative-alert.error details")!.open = true; await until(() => document.body.innerText.includes("不同内容"), "conflicting duplicate facts were accepted");
  check(!button("确认发布章节"), "conflicting fact identities remained publishable");

  projectId = "final-read-project"; workspace = makeWorkspace(projectId); workspace.modeling[0].status = "model_pending"; workspace.modeling[0].batchId = null; await render();
  await click("发布章节"); document.querySelector('.no-fact-decision input').click(); await tick(); await click("继续发布前检查");
  await until(() => button("确认发布章节") && !button("确认发布章节").disabled, "manual final checks missing");
  workspace.modeling[0].status = "ready"; workspace.modeling[0].batchId = "batch-six";
  releaseRead = null; holdRead = true; const finalButton = button("确认发布章节"); finalButton.click(); finalButton.click();
  await until(() => releaseRead, "final authoritative read not started");
  check(document.querySelector('[aria-label="章节标题"]').disabled && document.querySelector('[aria-label="关闭发布前检查"]').disabled && document.querySelector('.no-fact-decision input').disabled, "final read did not lock editing and checks");
  document.querySelector('[role="dialog"]').dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); await tick();
  check(document.querySelector('[role="dialog"]'), "Escape closed a publication in flight");
  releaseRead(); await until(() => document.querySelectorAll('.fact-review-card').length === 6, "final read allowed stale manual review instead of current ready batch");
  check(acceptances.length === 2 && !document.querySelector('.no-fact-decision') && button("确认发布章节").disabled, "final read bypassed newly ready facts");
  await confirmSix(); releaseRead = null; holdRead = true; button("确认发布章节").click(); await until(() => releaseRead, "delayed final owner not started"); const releaseFinal = releaseRead;
  projectId = "final-b"; workspace = makeWorkspace(projectId); await render(); await click("发布章节");
  await until(() => document.querySelectorAll('.fact-review-card').length === 6, "new scope stayed locked by old final owner");
  projectId = "final-read-project"; workspace = makeWorkspace(projectId); await render(); releaseFinal(); await tick(); await tick();
  check(acceptances.length === 2 && !document.querySelector('[role="dialog"]') && !button("发布章节").disabled, "late final A-B-A request committed or retained a stale lock");
  projectId = "batch-scope"; workspace = makeWorkspace(projectId); await render(); await click("发布章节");
  await until(() => button("拒绝未决定的 6 项") && !button("拒绝未决定的 6 项").disabled, "new project reused earlier fact decisions");
  const queuedReject = button("拒绝未决定的 6 项");
  // Hold the callback from this rendered review to simulate delivery after its owner changes.
  const lateReject = Object.values(queuedReject).find(value => value && typeof value === "object" && typeof value.onClick === "function").onClick;
  const individual = document.querySelector('[aria-label="变化1 的确认结论"]');
  Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value").set.call(individual, "confirmed"); individual.dispatchEvent(new Event("change", { bubbles: true })); queuedReject.click(); await tick(); await tick();
  check(individual.value === "confirmed" && [...document.querySelectorAll('.fact-review-card select')].filter(item => item.value === "rejected").length === 5, "queued bulk rejection overwrote a same-turn individual confirmation");
  capture("queued rejection preserves latest individual decision");
  workspace.modeling[0].batchId = "batch-next"; await render();
  check(!document.querySelector(".publication-bulk-decisions button") && button("确认发布章节").disabled, "stale analysis batch remained actionable");
  document.querySelector('[aria-label="关闭发布前检查"]').click(); await tick(); await click("发布章节");
  await until(() => button("确认未决定的 6 项") && !button("确认未决定的 6 项").disabled, "new review inherited decisions from prior batch");
  check([...document.querySelectorAll('.fact-review-card select')].every(item => item.value === ""), "new batch reused prior source decisions");
  capture("fresh batch has isolated decisions");
  projectId = "late-bulk-scope"; workspace = makeWorkspace(projectId); await render(); await click("发布章节");
  await until(() => button("确认未决定的 6 项") && !button("确认未决定的 6 项").disabled, "late handler target did not open fresh checks");
  lateReject(); await tick(); await tick();
  check([...document.querySelectorAll('.fact-review-card select')].every(item => item.value === "") && button("确认未决定的 6 项"), "old review callback changed a new project or review");
  capture("late callback cannot decide new review");
  check(modelCalls === 0, "publisher triggered a model request");
  return { passed: true, snapshots, saves: saves.length, reviews: reviews.length, publications: acceptances.length, modelCalls };
}
run().then(value => { window.testResult = value; }).catch(error => { window.testResult = { passed: false, error: error.stack + "\nDOM:" + document.body.innerText }; });
