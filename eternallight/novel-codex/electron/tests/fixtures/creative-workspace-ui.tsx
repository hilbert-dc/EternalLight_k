import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { CreativeWorkspaceView } from "../../../src/CreativeWorkspaceView";
window.addEventListener("error", (event) => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
const tick = () => new Promise((resolve) => setTimeout(resolve, 30));
const check = (condition, message) => { if (!condition) throw new Error(message); };
const waitForFocus = async (label) => {
  const deadline = performance.now() + 2000;
  while (document.activeElement?.getAttribute("aria-label") !== label && performance.now() < deadline) await tick();
  check(document.activeElement?.getAttribute("aria-label") === label, `Focus did not reach ${label}`);
};
const button = (text) => [...document.querySelectorAll("button")].find((item) => item.textContent === text);
const click = async (text) => { const element = button(text); check(element && !element.matches(":disabled"), `Unavailable: ${text}`); element.click(); await tick(); };
const input = async (label, value) => {
  const element = document.querySelector(`textarea[aria-label="${label}"]`);
  check(element && !element.matches(":disabled"), `Unavailable: ${label}`);
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(element, value);
  element.dispatchEvent(new Event("input", { bubbles: true })); await tick();
};
const chapter = { id: "chapter-1", number: 1, title: "第一章", passages: [] };
const scenes = [{ id: "a", sceneId: "original-a", body: "原场景 A", locked: false, baseAcceptedRevisionId: "accepted-1", candidateDraftRevisionId: "draft-1" }, { id: "b", sceneId: "original-b", body: "原场景 B", locked: false }];
const revision = { id: "draft-1", version: 1, body: "原场景 A\n\n---\n\n原场景 B", sceneGroups: scenes };
let workspace = { project: { id: "ui-test" }, revision: 1, index: { status: "ready" }, artifacts: Object.fromEntries(["brief", "outline", "plan"].map((kind) => [kind, { candidate: null, accepted: null, locked: false, workingSelection: null }])), chapters: [{ ...chapter, candidate: revision, accepted: revision, workingSelection: null }], workingBaselines: [], continuationIntents: [], commits: [], tasks: [], impacts: [], factProposals: [], reviews: [], chapterPlanBindings: [] };
let releaseSave, saveResult, selectedForAccept;
let failSave = false;
const api = { runtime: {
  getCreativeWorkspace: async () => structuredClone(workspace),
  listPlanningImpacts: async () => ({ data: [], nextCursor: null, counts: { total: 0, open: 0, retained: 0, lockedConflict: 0 } }),
  getWriterPreview: async () => ({ planningContext: { chapter: { publicTitle: "第一章", writerGuidance: "", publicCommitments: [] }, outlinePath: [], lineageStatus: "current" }, contextBudget: {}, omissions: {} }),
  saveDraftCheckpoint: async (_, payload) => { saveResult = payload; if (failSave) throw new Error("磁盘写入失败"); await new Promise((resolve) => { releaseSave = resolve; }); workspace = { ...workspace, revision: 2, chapters: [{ ...workspace.chapters[0], candidate: { ...revision, ...payload, id: "draft-2" } }] }; return { revisionId: "draft-2" }; },
  createReview: async () => { workspace.reviews = [{ id: "review-1", artifact_kind: "draft", artifact_revision_id: "draft-2", status: "ready", checks: [], selections: {} }]; },
  acceptReview: async (_, __, ___, selections) => { selectedForAccept = selections; },
} };
const props = { api, projectId: "ui-test", view: "chapters", state: { novelEdition: { chapters: [chapter] } }, selectedChapterId: chapter.id, agentAvailable: true, agentRunning: false, refreshToken: 0, onSelectChapter() {}, onGeneratePlanning() {}, async onGenerateChapter() {}, onOpenPlanningTarget() {}, onReturnToFeedback() {}, async onFormalChange() {}, onDirtyChange() {} };
let root = createRoot(document.getElementById("root"));
const render = async (extra = {}) => { root.render(<CreativeWorkspaceView {...props} {...extra} />); await tick(); await tick(); await tick(); };
async function run() {
  await render();
  window.uiReady = true; await new Promise((resolve) => setTimeout(resolve, 200));
  check(!document.querySelector(".unguided-writer-confirm") && !button("保留并按此意图继续").disabled, "Unguided creation still asks for duplicate confirmation");
  await input("第 1 章创作意图", "保留我的创作意图");
  root.unmount(); root = createRoot(document.getElementById("root")); await render();
  check(document.querySelector('[aria-label="第 1 章创作意图"]').value === "保留我的创作意图", "Intent lost after unmount");
  await input("场景 1 正文", "修改后的 A"); await click("＋ 添加场景"); await input("场景 3 正文", "新增 C"); await click("＋ 添加场景"); await waitForFocus("场景 4 正文"); await click("删除场景 4"); await waitForFocus("场景 3 正文"); await click("上移场景 3"); await click("上移场景 2");
  await click("保存候选");
  check([...document.querySelectorAll('[data-scene-id] textarea, [data-scene-id] button')].every((element) => element.matches(":disabled")), "Save did not lock editing");
  check(saveResult.sceneGroups[1].id === "a" && saveResult.sceneGroups[1].sceneId === "original-a" && saveResult.sceneGroups[1].body === "修改后的 A", "Edit/insertion changed existing identity");
  check(Object.keys(saveResult.sceneGroups[1]).sort().join(",") === "body,id,locked,sceneId", "Save leaked read-only Runtime metadata");
  check(saveResult.sceneGroups[2].id === "b" && !["a", "b"].includes(saveResult.sceneGroups[0].id), "Scene move/new identity failed");
  releaseSave(); await tick(); await tick(); await click("进入审阅");
  const selections = [...document.querySelectorAll('.scene-selection input')]; selections[0].click(); selections[2].click(); document.querySelector('.no-fact-decision input').click(); await tick();
  await click("采用并推进正式版本");
  check(JSON.stringify(selectedForAccept.sceneGroupIds) === '["a"]', "Partial adoption changed identity");
  await input("第 1 章创作意图", "长".repeat(32001));
  check(document.querySelector('[aria-label="第 1 章创作意图"]').value.length === 32001 && document.body.textContent.includes("创作意图过长"), "Long intent not preserved with feedback");
  await input("第 1 章创作意图", "长".repeat(16001));
  check(document.querySelector('[aria-label="第 1 章创作意图"]').value.length === 16001 && document.body.textContent.includes("48000 UTF-8 字节") && button("保留并按此意图继续").disabled, "Under-character-limit byte overflow was allowed");
  await render({ view: "brief", planningIntentSeed: "这是作者对故事的构想。".repeat(400) });
  check(!document.body.textContent.includes("创作意图过长") && !document.body.textContent.includes("超过本轮文本预算"), "Valid long Chinese planning intent was rejected");
  await render({ view: "brief", planningIntentSeed: "讨论结果" }); await input("规划生成意图", "作者自己的要求"); await render({ view: "outline", planningIntentSeed: "讨论结果" }); await render({ view: "brief", planningIntentSeed: "讨论结果" });
  check(document.querySelector('[aria-label="规划生成意图"]').value === "作者自己的要求", "Intent lost on view switch");
  await render({ view: "brief", planningIntentSeed: "新的讨论结果" });
  check(document.querySelector('[aria-label="规划生成意图"]').value === "新的讨论结果", "New seed ignored");
  await render();
  await input("场景 2 正文", "失败时保留的正文");
  failSave = true; await click("保存候选");
  check(document.querySelector('[aria-label="场景 2 正文"]').value === "失败时保留的正文", "Failed save discarded local prose");
  failSave = false; await click("保存候选");
  check(Object.keys(saveResult.sceneGroups[1]).sort().join(",") === "body,id,locked,sceneId", "Second save leaked metadata");
  await render({ view: "brief" });
  releaseSave(); await tick(); await tick();
  check(document.querySelector('[aria-label="规划生成意图"]') && !document.querySelector('[data-scene-id]'), "Old save completion polluted another editor");
  return { passed: true };
}
run().then((result) => { window.testResult = result; }).catch((error) => { window.testResult = { passed: false, error: (window.testResult?.error ?? error.stack) + "\nDOM:" + document.body.innerText }; });
