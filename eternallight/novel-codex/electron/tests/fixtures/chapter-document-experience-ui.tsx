import { emptyCreationIssueSummary } from "../helpers/creation-issues";
import React from "react";
import { createRoot } from "react-dom/client";
import { CreativeWorkspaceView, revealControl } from "./creative-workspace-harness";
import { ChapterDocumentView, ChapterPublicationStatus } from "../../../src/ChapterDocumentView";
import "../../../src/styles.css";

window.addEventListener("error", event => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
const tick = () => new Promise(resolve => setTimeout(resolve, 35));
const check = (condition, message) => { if (!condition) throw new Error(message); };
const until = async (predicate, message) => { const deadline = performance.now() + 2000; while (!predicate() && performance.now() < deadline) await tick(); check(predicate(), message); };
const viewport = async width => { window.testViewport = { width, height: 1000 }; await until(() => window.testViewport === null && innerWidth === width && innerHeight === 1000, `Viewport did not settle at ${width}px`); await tick(); await tick(); };
const button = text => [...document.querySelectorAll("button")].find(item => item.textContent === text || item.getAttribute("aria-label") === text);
const versions = async () => { const summary = document.querySelector(".chapter-versions summary"); check(summary, "Version details missing"); summary.focus({ preventScroll: true }); summary.click(); await tick(); await tick(); };
const click = async text => {
  const sceneAction = /^(?:上移场景|下移场景|删除场景|保护场景|取消保护场景) (\d+)$/.exec(text);
  if (sceneAction && !document.querySelector(`[role="menu"][aria-label="场景 ${sceneAction[1]} 操作"]`)) {
    const trigger = document.querySelector<HTMLButtonElement>(`button[aria-label="场景 ${sceneAction[1]} 操作"]`);
    check(trigger && !trigger.matches(":disabled"), `Unavailable scene menu: ${text}`); await revealControl(trigger); trigger.scrollIntoView({ block: "nearest" }); await tick(); trigger.focus({ preventScroll: true }); trigger.click(); await tick();
  }
  const item = text === "查看修改记录" ? [...document.querySelectorAll<HTMLButtonElement>("button")].find(item => item.getAttribute("aria-label") === text && item.checkVisibility()) : button(text); check(item && !item.matches(":disabled"), `Unavailable: ${text}`); await revealControl(item); item.click(); await tick(); await tick();
};
const input = async (label, value) => {
  const item = document.querySelector(`textarea[aria-label="${label}"]`);
  check(item && !item.matches(":disabled"), `Unavailable: ${label}`);
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(item, value);
  item.dispatchEvent(new Event("input", { bubbles: true })); await tick();
};
const longBody = Array.from({ length: 100 }, (_, index) => `第三场正文第 ${index} 行，林夕继续记录港城。`).join("\n");
const chapter = { id: "chapter-a", number: 1, title: "雾港", passages: [] };
const chapterB = { id: "chapter-b", number: 2, title: "白塔", passages: [] };
const revision = (id, version, sceneGroups) => ({ id, version, title: id === "accepted-a" ? "正式雾港 A" : "工作雾港 B", chapterId: chapter.id, body: sceneGroups.map(item => item.body).join("\n\n---\n\n"), sceneGroups, source: "agent", sourcePlanRefs: { lineageStatus: "current", localOnly: true } });
const formal = revision("accepted-a", 1, [{ id: "old-a", sceneId: "scene-a", body: "**正式第一场**", locked: false }, { id: "old-b", sceneId: "scene-b", body: "正式第二场", locked: false }, { id: "old-c", sceneId: "scene-c", body: longBody, locked: false }]);
const candidate = revision("candidate-b", 2, [{ id: "new-a", sceneId: "scene-a", body: "候选第一场", locked: false }, { id: "new-d", sceneId: "scene-d", body: "新增场景", locked: false }, { id: "new-c", sceneId: "scene-c", body: longBody, locked: false }]);
candidate.sourcePlanRefs = { lineageStatus: "current", localOnly: false, planRevisionId: "plan-current", chapterPlanId: "plan-chapter-a" };
let workspace = { project: { id: "document-test" }, revision: 1, headSnapshotId: "head-a", sourceDigest: "a".repeat(64), index: { status: "ready" },
  artifacts: Object.fromEntries(["brief", "outline", "plan"].map(kind => [kind, { candidate: null, accepted: null, working: null, workingSelection: null, locked: false }])),
  chapters: [{ ...chapter, candidate, accepted: formal, working: null, workingSelection: null }, { ...chapterB, candidate: revision("chapter-b-candidate", 1, [{ id: "chapter-b-scene", sceneId: "chapter-b-scene", body: "第二章独有正文" }]), accepted: null, working: null, workingSelection: null }],
  workingBaselines: [], continuationIntents: [], planningReviews: [], commits: [], tasks: [], impacts: [], modeling: [], factProposals: [], reviews: [], chapterPlanBindings: [] };
let workTasks = [];
const generationPhaseChecks = [];
let reportedDirty = false;
const dirtyRoundTrip = [];
let generationCalls = 0, acceptanceCalls = 0, reviewCalls = 0, saveCalls = 0, workspaceReadFailure = false, latestReviewSelections;
let metadataDiffSnapshot;
const planningVersionRequests = [];
const adoptionHints = [];
const reviewProgress = [];
const progress = () => document.querySelector('[aria-label="发布检查进展"]').textContent;
const adoptionHint = () => document.querySelector(".review-panel > p:not([role])").textContent;
const themeChecks = [];
const luminance = color => {
  const channels = color.match(/[\d.]+/g).slice(0, 3).map(Number).map(value => {
    const channel = value / 255;
    return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
  });
  return channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722;
};
const checkReadability = async (mode, selectors) => {
  document.documentElement.dataset.theme = mode;
  await tick();
  for (const selector of selectors) {
    const element = document.querySelector(selector);
    check(element, `Missing theme fixture element: ${selector}`);
    const foreground = getComputedStyle(element).color;
    let surface = element;
    while (surface && ["transparent", "rgba(0, 0, 0, 0)"].includes(getComputedStyle(surface).backgroundColor)) surface = surface.parentElement;
    check(surface, `Missing opaque surface: ${selector}`);
    const background = getComputedStyle(surface).backgroundColor;
    const values = [luminance(foreground), luminance(background)].sort((a, b) => b - a);
    const contrast = (values[0] + 0.05) / (values[1] + 0.05);
    check(contrast >= 4.5, `${mode} ${selector} contrast ${contrast.toFixed(2)}: ${foreground} on ${background}`);
    themeChecks.push({ mode, selector, foreground, background, contrast });
  }
};
const api = { runtime: {
  getWorkCatalog: async () => ({ revision: 0, volumes: [], placements: [] }),
  getCreativeWorkspace: async () => { if (workspaceReadFailure) throw new Error("章节读取中断"); return structuredClone(workspace); },
  listPlanningImpacts: async () => ({ data: [], nextCursor: null, counts: { total: 0, open: 0, retained: 0, lockedConflict: 0 } }),
  listGenerationRuns: async () => ({ runs: [] }),
  listWorkTasks: async () => ({ tasks: structuredClone(workTasks), nextCursor: null, activeCount: workTasks.length }),
  ...Object.fromEntries(["Search", "Entity", "Relation", "Knowledge", "SnapshotKnowledge", "Fact"].map(name => [`get${name}Audit`, async () => ({ job: null })])),
  saveDraftCheckpoint: async (_, payload) => {
    saveCalls++;
    workspace.revision++;
    workspace.chapters[0].candidate = { ...revision(`saved-${saveCalls}`, 2 + saveCalls, payload.sceneGroups), title: payload.title, sourcePlanRefs: workspace.chapters[0].candidate.sourcePlanRefs };
    workspace.modeling = [{ objectId: `draft-${chapter.id}`, sourceRevisionId: workspace.chapters[0].candidate.id, status: "model_pending", batchId: null, entities: [], assertions: [], confirmationState: "candidate" }];
    return { revisionId: workspace.chapters[0].candidate.id };
  },
  setWorkingBaseline: async (_project, _kind, chapterId, revisionId, selectionRevision) => {
    const item = workspace.chapters.find(item => item.id === chapterId);
    item.working = structuredClone(item.candidate);
    item.workingSelection = { revisionId, selectionRevision: selectionRevision + 1 };
  },
  saveContinuationIntent: async () => {},
  createReview: async (_, kind, revisionId, proposals, selections = {}, modelingBatchId) => {
    reviewCalls++;
    const review = { id: `review-${workspace.reviews.length}`, artifactKind: kind, artifactRevisionId: revisionId, projectRevision: workspace.revision, status: "ready", checks: [], selections, modelingBatchId };
    workspace.reviews.unshift(review);
    return { ...review, factProposals: proposals };
  },
  acceptReview: async (_, reviewId, __, selections) => {
    acceptanceCalls++;
    const review = workspace.reviews.find(item => item.id === reviewId);
    check(JSON.stringify(selections.sceneGroupIds) === JSON.stringify(workspace.chapters[0].candidate.sceneGroups.map(scene => scene.id)), "Publication must send every current scene ID to native knowledge review");
    latestReviewSelections = selections;
    review.status = "accepted"; review.selections = selections;
    workspace.chapters[0].accepted = structuredClone(workspace.chapters[0].candidate);
    workspace.revision++;
  },
}, agent: {} };
const props = { taskConversationContainer: null, api, projectId: "document-test", view: "chapters", state: { novelEdition: { chapters: [chapter, chapterB] }, canon: { characters: [], locations: [], factions: [] } }, selectedChapterId: chapter.id,
  agentAvailable: true, agentRunning: false, refreshToken: 0, onSelectChapter() {}, async onGenerateWork() {}, async onContinueWork() {}, async onRetryWork() {}, async onResolveFeedback() {}, async onReviewPlanning() {}, async onCancelPlanningReview() {},
  async onGenerateChapter() { generationCalls++; workspace.revision++; workspace.chapters[0].candidate = revision("generated-c", 8, workspace.chapters[0].candidate.sceneGroups.map((scene, index) => ({ ...scene, body: index === 0 ? "AI 新生成第一场" : scene.body }))); workspace.modeling = [{ objectId: `draft-${chapter.id}`, sourceRevisionId: "generated-c", status: "ready", batchId: "knowledge-generated", entities: [], assertions: [], confirmationState: "candidate" }]; workspaceReadFailure = true; },
  onOpenVersions() { planningVersionRequests.push("versions"); }, onOpenPlanningTarget() {}, onReturnToFeedback() {}, async onFormalChange() {}, onDirtyChange(value) { reportedDirty = value; } };
const host = document.getElementById("root"); host.className = "workspace"; host.style.cssText = "display:block;height:100vh";
const main = document.createElement("main"); main.className = "main-panel chapter-main-panel"; main.style.cssText = "height:100vh;width:980px;max-width:100vw"; host.append(main);
const moduleActionsContainer = document.createElement("div"); moduleActionsContainer.className = "module-records-toolbar-slot"; host.prepend(moduleActionsContainer);
let root = createRoot(main);
const render = async (extra = {}) => { root.render(<CreativeWorkspaceView {...props} {...extra} moduleActionsContainer={moduleActionsContainer} recordsOpen={false} />); await tick(); await tick(); await tick(); };
const offset = element => element.getBoundingClientRect().top - main.getBoundingClientRect().top;
const nearOffset = (element, expected) => Math.abs(offset(element) - expected) <= 2;
const readAt = async (element, position) => { check(element.isConnected, "Reading target is detached"); main.scrollTop += offset(element) - position; main.dispatchEvent(new Event("scroll")); await tick(); return offset(element); };
const layoutChecks = [];
const readingChecks = [];
const settingsChecks = [];
const checkChapterSettings = async (theme, width) => {
  main.style.width = `${width}px`; document.documentElement.dataset.theme = theme;
  await viewport(Math.max(width, 330));
  main.scrollTop = 0; await tick();
  const settings = document.querySelector<HTMLDetailsElement>('.chapter-settings');
  const summary = settings.querySelector<HTMLElement>(':scope > summary');
  summary.focus({ preventScroll: true }); summary.click(); await tick(); await tick();
  const bounds = settings.querySelector('.chapter-settings-content').getBoundingClientRect(), panel = main.getBoundingClientRect();
  check(settings.open && bounds.left >= panel.left - 1 && bounds.right <= panel.right + 1 && bounds.top >= document.querySelector('.chapter-page-toolbar').getBoundingClientRect().bottom - 1 && main.scrollWidth <= main.clientWidth + 1, `${theme}/${width}: chapter settings escaped its toolbar or produced horizontal overflow`);
  window.testScreenshot = `chapter-settings-${theme}-${width}`; await new Promise(resolve => setTimeout(resolve, 130));
  settings.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); await tick();
  check(!settings.open && document.activeElement === summary, `${theme}/${width}: closing settings with Esc lost trigger focus`);
  summary.click(); await tick();
  const title = document.querySelector<HTMLInputElement>('[aria-label="章节标题"]');
  title.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true })); title.focus({ preventScroll: true }); await tick(); await tick();
  check(!settings.open && document.activeElement === title, `${theme}/${width}: outside click did not close settings or stole editor focus`);
  settingsChecks.push({ theme, width, contained: true, escapeFocus: true, outsideEditorFocus: true });
};
const checkLayout = async (theme, width) => {
  document.documentElement.dataset.theme = theme; main.style.width = `${width}px`; await tick();
  const editors = [...document.querySelectorAll(".chapter-document textarea")];
  check(CSS.supports("field-sizing", "content"), "Electron does not support content-sized textareas");
  check(editors.every(item => item.scrollHeight <= item.clientHeight + 2 && getComputedStyle(item).maxHeight === "none" && getComputedStyle(item).overflowY === "hidden"), `${theme}/${width}: inner textarea scroll remains`);
  check(main.scrollHeight > main.clientHeight && main.scrollWidth <= main.clientWidth + 2, "Main chapter must scroll vertically without horizontal overflow");
  main.scrollTop = main.scrollHeight; await tick();
  const last = editors.at(-1).getBoundingClientRect(), panel = main.getBoundingClientRect();
  check(last.bottom <= panel.bottom + 2, "Last prose line is unreachable through main scroll");
  check(getComputedStyle(main).backgroundColor === getComputedStyle(document.querySelector(".chapter-document")).backgroundColor, "Main and document surfaces differ");
  const toolbarButton = document.querySelector(".chapter-page-actions button");
  check(getComputedStyle(toolbarButton).backgroundColor === "rgba(0, 0, 0, 0)", "Chapter toolbar retained a card button background");
  const editor = editors[0]; editor.focus({ preventScroll: true }); editor.setSelectionRange(1, 3);
  const focusedStyle = getComputedStyle(editor);
  check(focusedStyle.boxShadow === "none" && focusedStyle.outlineStyle === "none", "Prose focus painted a block decoration");
  check(focusedStyle.caretColor === focusedStyle.color && editor.selectionStart === 1 && editor.selectionEnd === 3, "Prose focus lost its visible caret or selection");
  layoutChecks.push({ theme, width, autoGrowing: true, mainOnlyScroll: true, endReachable: true, continuousSurface: true,
    focusedProse: { shadow: focusedStyle.boxShadow, outline: focusedStyle.outlineStyle, caretMatchesText: true, selectionPreserved: true } });
};

async function run() {
  await render();
  await checkChapterSettings("light", 980); await checkChapterSettings("dark", 330);
  main.style.width = "980px"; document.documentElement.dataset.theme = "light"; await viewport(1400);
  for (const theme of ["light", "dark"]) for (const width of [980, 540]) await checkLayout(theme, width);
  main.style.width = "980px"; document.documentElement.dataset.theme = "light"; main.scrollTop = 0; await tick();
  const expectDirty = (label, expected) => { check(reportedDirty === expected, `${label}: incorrect unsaved state`); dirtyRoundTrip.push({ label, dirty: reportedDirty }); };
  expectDirty("loaded baseline", false);
  await input("场景 1 正文", "改写第一场"); expectDirty("same length changed prose", true);
  await input("场景 1 正文", candidate.sceneGroups[0].body); expectDirty("prose restored", false);
  const baselineTitle = document.querySelector('[aria-label="章节标题"]');
  const setTitle = async value => { Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(baselineTitle, value); baselineTitle.dispatchEvent(new Event("input", { bubbles: true })); await tick(); };
  await setTitle("改写标题"); expectDirty("title changed", true);
  await input("场景 1 正文", "改写第一场"); await input("场景 1 正文", candidate.sceneGroups[0].body); expectDirty("prose restored with changed title", true);
  await setTitle(candidate.title); expectDirty("title restored", false);
  check(!document.querySelector(".chapter-manual-tools,.writer-runtime-preview,.fact-proposal-form") && !button("讨论这个场景"), "Chapter retained a manual toolbox or duplicated scene discussion");
  await click("场景 1 操作");
  const sceneMenu = document.querySelector('[role="menu"][aria-label="场景 1 操作"]');
  check(sceneMenu?.parentElement === document.body && sceneMenu.contains(document.activeElement) && button("上移场景 1").disabled, "Scene controls were not reachable through the focused contextual menu");
  window.testScreenshot = "chapter-scene-menu-light"; await new Promise(resolve => setTimeout(resolve, 130));
  sceneMenu.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); await tick();
  check(!document.querySelector('[role="menu"]') && document.activeElement?.getAttribute("aria-label") === "场景 1 操作", "Esc did not close the scene menu and return focus");
  main.style.width = "540px"; document.documentElement.dataset.theme = "dark"; await viewport(540); await click("场景 1 操作");
  const narrowMenu = document.querySelector('[role="menu"][aria-label="场景 1 操作"]');
  check(narrowMenu.getBoundingClientRect().right <= innerWidth && narrowMenu.scrollWidth <= narrowMenu.clientWidth + 1, "Scene menu overflowed the narrow viewport");
  window.testScreenshot = "chapter-scene-menu-dark-narrow"; await new Promise(resolve => setTimeout(resolve, 130));
  narrowMenu.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); await tick();
  main.style.width = "980px"; document.documentElement.dataset.theme = "light"; await viewport(1400);
  await click("保护场景 1"); expectDirty("protection changed", true);
  await input("场景 2 正文", "改写第二场"); await input("场景 2 正文", candidate.sceneGroups[1].body); expectDirty("prose restored with protection", true);
  await click("取消保护场景 1"); expectDirty("protection restored", false);
  await click("下移场景 1"); expectDirty("scene order changed", true);
  await click("上移场景 2"); expectDirty("scene order restored", false);
  check(saveCalls === 0 && generationCalls === 0 && acceptanceCalls === 0, "Restoring baseline performed a write or model call");
  workspace.modeling = [{ objectId: `draft-${chapter.id}`, sourceRevisionId: candidate.id, status: "model_pending", batchId: null, entities: [], assertions: [], confirmationState: "candidate" }];
  await render({ agentRunning: false, refreshToken: 99 });
  check(!button("继续分析本章") && !document.querySelector(".chapter-analysis-status") && !document.body.textContent.includes("故事设定分析尚未完成"), "An unmodeled saved revision was presented as an author obligation");
  workTasks = [{ status: "running", deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], unsettledAttemptIds: [] }, steps: [{ operation: "modelKnowledge", status: "running", target: { chapterId: chapter.id } }] }];
  await render({ agentRunning: true, refreshToken: 100 });
  await until(() => document.querySelector('.draft-main-column > small[role="status"]')?.textContent.includes("正文已保存，正在分析…"), "Current chapter analysis was labelled as generation");
  const analysisStatus = document.querySelector(".chapter-analysis-status")?.textContent;
  check(analysisStatus === "正在分析本章的故事设定…" && !document.querySelector(".chapter-analysis-status.creative-alert") && !button("继续分析本章") && document.querySelector('[aria-label="场景 1 正文"]').disabled, "Analysis did not expose saved prose with its execution lock");
  generationPhaseChecks.push({ phase: "same chapter analysis", action: "正文已保存，正在分析…", status: analysisStatus });
  workTasks[0].steps[0].target.chapterId = chapterB.id;
  await render({ agentRunning: true, refreshToken: 101 });
  await until(() => !document.querySelector('.draft-main-column > small[role="status"]')?.textContent.includes("正文已保存，正在分析…"), "Another chapter analysis leaked into current chapter status");
  generationPhaseChecks.push({ phase: "other chapter", action: document.querySelector('.draft-main-column > small[role="status"]').textContent });
  workTasks = []; workspace.modeling = [];
  await render({ agentRunning: false, refreshToken: 102 });

  check(document.querySelector('[aria-label="章节发布状态"]').textContent === "有未发布修改", "Chapter state was not simplified to publication");
  const editorFieldset = document.querySelector(".chapter-document fieldset");
  const originalEditor = document.querySelector('[data-scene-id="new-c"] textarea');
  await versions();
  check(document.querySelector(".chapter-published-reading h3").textContent === "正式雾港 A" && document.querySelector('[aria-label="章节标题"]').value === "工作雾港 B", "Published reading and current title were conflated");
  check(document.querySelector(".chapter-document fieldset") === editorFieldset && document.querySelector('[data-scene-id="new-c"] textarea') === originalEditor, "Opening versions remounted the editor");
  check(!document.querySelector(".chapter-published-reading strong") && getComputedStyle(document.querySelector(".story-reading-text")).fontFamily.includes("serif"), "Published prose was rendered as chat markdown");
  const readingSelectors = [".story-reading-text p", ".chapter-published-reading h3", ".chapter-versions summary", ".chapter-versions-return"];
  await checkReadability("dark", readingSelectors); await checkReadability("light", readingSelectors);
  window.readingUiReady = true;
  check(document.querySelector('[data-change-kind="标题"]').textContent.includes("正式雾港 A") && document.querySelector('[data-change-kind="标题"]').textContent.includes("工作雾港 B"), "Title diff was absent");
  const sourceDiff = document.querySelector('[data-change-kind="章纲来源"]').textContent;
  check(sourceDiff.includes("章纲版本已变化") && sourceDiff.includes("关联章纲") && !sourceDiff.includes("plan-current") && !sourceDiff.includes("plan-chapter-a"), "Source change was hidden or exposed internal IDs");
  await checkReadability("dark", [".chapter-diff-columns h5", ".chapter-diff-columns p", ".chapter-scene-diff > p"]); document.documentElement.dataset.theme = "light";
  metadataDiffSnapshot = { title: document.querySelector('[data-change-kind="标题"]').textContent, sourcePlan: sourceDiff };
  const initialDiff = [...document.querySelectorAll("[data-diff-scene-id]")].map(item => ({ id: item.dataset.diffSceneId, kind: item.dataset.changeKind }));
  check(JSON.stringify(initialDiff) === JSON.stringify([{ id: "scene-a", kind: "修改" }, { id: "scene-d", kind: "新增" }, { id: "scene-b", kind: "删除" }]), "Scene identity diff treated regenerated IDs as replacements");
  await click("返回正文");
  const titleInput = document.querySelector('[aria-label="章节标题"]');
  await checkReadability("dark", [".chapter-title input", ".draft-editor", ".chapter-scene-menu-trigger:not(:disabled)", ".draft-add-scene"]); document.documentElement.dataset.theme = "light";
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(titleInput, "作者调整的工作标题"); titleInput.dispatchEvent(new Event("input", { bubbles: true })); await tick();
  await input("场景 1 正文", "作者未保存改动");
  const third = document.querySelector('[data-scene-id="new-c"] textarea');
  third.focus({ preventScroll: true }); third.setSelectionRange(43, 51, "backward"); const readingOffset = await readAt(third, -170);
  await versions();
  check(document.querySelector(".chapter-published-reading h3").textContent === "正式雾港 A" && document.querySelector('[data-diff-scene-id="scene-a"]').textContent.includes("作者未保存改动"), "Unsaved editing changed published text or vanished from diff");
  check(document.querySelector(".chapter-document fieldset") === editorFieldset && document.querySelector('[data-scene-id="new-c"] textarea') === third && titleInput.value === "作者调整的工作标题", "Versions lost local editing DOM/title");
  await click("返回正文");
  check(document.activeElement === third && third.selectionStart === 43 && third.selectionEnd === 51 && third.selectionDirection === "backward" && nearOffset(third, readingOffset), "Closing versions lost caret/main reading position");
  const naturalSummary = document.querySelector(".chapter-versions summary"); naturalSummary.focus(); await tick(); const afterNaturalFocus = offset(third);
  naturalSummary.click(); await tick(); await click("返回正文");
  check(nearOffset(third, afterNaturalFocus), "Version return discarded the latest actual outer reading position");
  await click("上移场景 3"); await versions();
  check(document.querySelector('[data-diff-scene-id="scene-c"]').dataset.changeKind === "重排", "Reordered scene was missing from exact diff");
  await click("返回正文");
  check(document.activeElement === third && third.getAttribute("aria-label") === "场景 2 正文", "Bookmark followed index rather than stable scene identity");
  await versions(); await click("保存");
  check(document.querySelector(".chapter-document fieldset") === editorFieldset && document.querySelector('[data-scene-id="new-c"] textarea') === third && document.querySelector(".chapter-versions").open, "Save refresh remounted editor or closed versions");
  check(workspace.chapters[0].candidate.title === "作者调整的工作标题" && workspace.chapters[0].accepted.title === "正式雾港 A" && workspace.chapters[0].candidate.id === "saved-1", "Saving changed published revision/title");
  await click("返回正文");
  const external = document.querySelector('[aria-label="章节标题"]'); external.focus();
  const beforeRefreshScroll = main.scrollTop;
  await render({ refreshToken: 1 });
  check(document.activeElement === external && main.scrollTop === beforeRefreshScroll, "Background refresh stole focus or main reading position");
  check(!button("让 AI 起草本章") && !button("讨论本章") && !button("新建章节"), "Saved document retained persistent writer or duplicate creation entries");
  // Simulate a completed ordinary conversation's durable output; this reader never calls a writer.
  workspace.revision++; workspace.chapters[0].candidate = revision("generated-c", 8, workspace.chapters[0].candidate.sceneGroups.map((scene, index) => ({ ...scene, body: index === 0 ? "AI 新生成第一场" : scene.body })));
  workspace.modeling = [{ objectId: `draft-${chapter.id}`, sourceRevisionId: "generated-c", status: "ready", batchId: "knowledge-generated", entities: [], assertions: [], confirmationState: "candidate" }];
  await render({ refreshToken: 2 });
  check(generationCalls === 0 && saveCalls === 1 && reviewCalls === 0 && acceptanceCalls === 0 && workspace.chapters[0].accepted.id === "accepted-a" && document.querySelector('[aria-label="场景 1 正文"]').value.includes("AI 新生成第一场"), "Reading a saved conversation output called a writer, repeated writes or adopted it");
  check(button("发布章节") && !button("发布章节").disabled, "Generated candidate has no actionable review entry");
  await click("发布章节");
  reviewProgress.push(progress());
  check(progress().includes("确认本章无需写入新的故事事实") && button("确认发布章节").disabled, "No-facts confirmation gate had no actionable explanation");
  adoptionHints.push(adoptionHint());
  check(!document.querySelector(".scene-selection input") && adoptionHint().includes("标题") && adoptionHint().includes("删除"), "Whole publication was hidden behind partial scene choices");
  const noFacts = document.querySelector(".no-fact-decision input"); noFacts.click(); await tick();
  reviewProgress.push(progress());
  check(progress() === "已完成检查，可以发布这一章。" && !button("确认发布章节").disabled, "Completed checks were not ready to publish");
  await click("确认发布章节");
  check(acceptanceCalls === 1 && workspace.chapters[0].accepted.id === "generated-c" && JSON.stringify(latestReviewSelections.sceneGroupIds) === JSON.stringify(workspace.chapters[0].candidate.sceneGroups.map(scene => scene.id)) && latestReviewSelections.modelingBatchId === "knowledge-generated", "Whole publication omitted modeling source or sent incomplete scene selections");
  check(JSON.stringify(workspace.chapters[0].accepted.sceneGroups.map(scene => scene.sceneId)) === JSON.stringify(["scene-a", "scene-c", "scene-d"]), "Publication retained deleted scene or lost new order");

  workspace.modeling = [{ objectId: `draft-${chapter.id}`, sourceRevisionId: "generated-c", status: "ready", batchId: "knowledge-current", entities: [], assertions: [], confirmationState: "candidate" }];
  await render({ refreshToken: "knowledge-current" }); await click("更新故事设定");
  document.querySelector(".no-fact-decision input").click(); await tick(); await click("确认发布章节");
  check(latestReviewSelections.modelingBatchId === "knowledge-current" && !button("更新故事设定"), "Resolved knowledge review remained pending");
  workspace.modeling[0].batchId = "knowledge-new";
  await render({ refreshToken: 3 });
  check(button("更新故事设定"), "New modeling batch was incorrectly treated as resolved");
  workspace.modeling[0].sourceRevisionId = "prior-source";
  await render({ refreshToken: 31 });
  check(!button("更新故事设定") && [...document.querySelectorAll(".creative-alert.warning")].some(item => item.textContent.includes("来源与当前正文不同")), "Stale source was exposed as current publication knowledge");
  workspace.modeling = [];

  workspace.revision++; await render({ refreshToken: 4 });
  await input("场景 1 正文", "又一次本地修改");
  await click("保存");
  const beforeManualPublish = generationCalls;
  await click("发布章节");
  check(document.querySelector(".no-fact-decision input") && workspace.modeling[0].status === "model_pending", "Pending author edition had no explicit manual facts decision");
  document.querySelector(".no-fact-decision input").click(); await tick(); await click("继续发布前检查"); await click("确认发布章节");
  check(generationCalls === beforeManualPublish && workspace.chapters[0].accepted.id === "saved-2" && JSON.stringify(latestReviewSelections.sceneGroupIds) === JSON.stringify(workspace.chapters[0].candidate.sceneGroups.map(scene => scene.id)), "Manual publication called model or used partial semantics");
  const lastSceneEditor = document.querySelector('[data-scene-id="new-c"] textarea');
  lastSceneEditor.focus({ preventScroll: true }); lastSceneEditor.setSelectionRange(43, 51, "backward"); await readAt(lastSceneEditor, readingOffset);
  readingChecks.push({ phase: "before chapter switch", offset: offset(lastSceneEditor) });
  workspace.revision++; await render({ refreshToken: 5 });
  await render({ selectedChapterId: chapterB.id, refreshToken: 6 });
  check(document.querySelector('[aria-label="场景 1 正文"]').value === "第二章独有正文" && !document.querySelector(".writer-runtime-preview"), "Chapter selection leaked the old body or restored removed preview tools");
  await render({ selectedChapterId: chapter.id, refreshToken: 7 });
  await until(() => document.activeElement?.closest("[data-scene-id]")?.dataset.sceneId === "new-c", "Chapter return did not restore stable scene bookmark");
  readingChecks.push({ phase: "chapter return", offset: offset(document.activeElement) });
  main.style.width = "540px"; await tick(); await readAt(document.querySelector('[data-scene-id="new-c"] textarea'), readingOffset);
  readingChecks.push({ phase: "before remount", offset: offset(document.querySelector('[data-scene-id="new-c"] textarea')) });
  root.unmount(); main.scrollTop = 0; root = createRoot(main); await render({ refreshToken: 7 });
  readingChecks.push({ phase: "remount", offset: offset(document.querySelector('[data-scene-id="new-c"] textarea')), selection: [document.activeElement.selectionStart, document.activeElement.selectionEnd] });
  await until(() => document.activeElement?.closest("[data-scene-id]")?.dataset.sceneId === "new-c" && document.activeElement.selectionStart === 43 && document.activeElement.selectionEnd === 51 && nearOffset(document.activeElement, readingOffset), "Workspace remount lost active scene selection and relative reading position after reflow");

  workspace.artifacts.brief.accepted = { id: "brief-adopted", version: 3, payload: { premise: "正式简报" } };
  workspace.artifacts.brief.candidate = null;
  await render({ view: "brief", refreshToken: 8 });
  const acceptedState = document.querySelector(".planning-workspace-state").textContent;
  check(acceptedState === "已采用" && !document.querySelector(".planning-version-tools,.planning-compare") && button("查看修改记录"), "Accepted-only planning retained inline version UI or lost its version entry");
  await click("查看修改记录");
  workspace.artifacts.brief.candidate = { id: "brief-candidate", version: 4, payload: { premise: "新候选简报" } };
  workspace.artifacts.brief.working = { id: "brief-working", version: 2, payload: { premise: "工作来源简报" } };
  await render({ view: "brief", refreshToken: 9 });
  check(document.querySelector(".planning-workspace-state").textContent === "已保存，待采用" && document.querySelector(".overview-direction-summary").textContent.includes("当前后续创作依据 · 尚未正式采用") && document.querySelector(".overview-direction-summary").textContent.includes("工作来源简报") && !document.querySelector(".planning-version-tools,.planning-compare"), "Simple planning state conflated saved candidate and selected working source");
  const beforeVersionOpen = { saveCalls, generationCalls, acceptanceCalls };
  await click("查看修改记录"); check(planningVersionRequests.length === 2 && JSON.stringify({ saveCalls, generationCalls, acceptanceCalls }) === JSON.stringify(beforeVersionOpen), "Version preparation wrote, adopted or called the model");
  const status = { accepted: acceptedState, saved: document.querySelector(".planning-workspace-state").textContent, working: document.querySelector(".overview-source").textContent, versionRequests: planningVersionRequests.length, inlineControls: 0 };
  const cacheScene = { id: "cache-scene", sceneId: "cache-stable", body: longBody, locked: false };
  const renderCache = async index => { root.render(<ChapterDocumentView key={`cache-${index}`} scope={`cache-${index}`} scenes={[cacheScene]} accepted={formal} title="缓存章节" sourcePlanRefs={null} lockedIds={[]} dirty={false} disabled={false} onTitleChange={() => {}} onChange={() => {}} onActiveScene={() => {}} />); await tick(); };
  for (let index = 0; index < 130; index++) {
    await renderCache(index);
    const item = document.querySelector(".chapter-document textarea"); item.focus({ preventScroll: true }); item.setSelectionRange(10, 15); await readAt(item, -100);
  }
  await renderCache(0);
  check(!document.querySelector(".chapter-versions").open && document.activeElement !== document.querySelector(".chapter-document textarea"), "Evicted bookmark or versions leaked into old scope");
  await renderCache(129); check(!document.querySelector(".chapter-versions").open, "Details open state leaked across scope");
  check(document.activeElement === document.querySelector(".chapter-document textarea") && document.activeElement.selectionStart === 10 && nearOffset(document.activeElement, -100), "Recent bookmark was not preserved by bounded caching");
  const renderReading = async (scope, locked = false) => { root.render(<ChapterDocumentView key={scope} scope={scope} scenes={[{ ...cacheScene, locked }]} accepted={null} title="连续阅读" sourcePlanRefs={null} lockedIds={locked ? [cacheScene.id] : []} dirty={false} disabled={false} onTitleChange={() => {}} onChange={() => {}} onActiveScene={() => {}} />); await tick(); await tick(); };
  for (const locked of [false, true]) {
    const scope = `pure-reading:${locked ? "locked" : "unlocked"}`; await renderReading(scope, locked);
    const item = document.querySelector("textarea"); document.querySelector('[aria-label="章节标题"]').focus({ preventScroll: true });
    const readingPosition = await readAt(item, -700); check(document.activeElement !== item, "Pure reading unexpectedly focused prose");
    await renderReading("other-project:reading"); main.style.width = "760px"; await renderReading(scope, locked);
    check(nearOffset(document.querySelector("textarea"), readingPosition), "Unfocused or locked reading did not restore relative viewport after chapter/project change");
    layoutChecks.push({ reading: locked ? "locked" : "unfocused", restoredAfterReflow: true });
  }
  root.unmount(); root = createRoot(main);
  workspace.tasks = [{ id: "writer-current", taskKind: "draft", purpose: "writer", status: "completed", stage: "ready_for_review", deliveryStatus: "ready_for_review", target: { chapterId: chapter.id, chapterNumber: 1 }, outputRefs: [{ kind: "draft", objectId: `draft-${chapter.id}`, revisionId: "draft-write-1" }] }];
  await render({ refreshToken: 60 });
  check(!document.querySelector(".generation-task-status") && !document.body.textContent.includes("查看生成记录"), "Chapter settings duplicated execution records from the conversation");
  workspace.tasks = []; root.unmount(); root = createRoot(main);
  workspace.factProposals = [{ id: "progress-fact", draft_revision_id: workspace.chapters[0].candidate.id, sceneGroupId: "new-a", factKind: "world", subjectId: "linxi", predicate: "letterArrived", value: true, epistemicOwnerId: null, epistemicState: "unknown", evidence: workspace.chapters[0].candidate.sceneGroups[0].body, source: "agent", reviewDecision: "candidate", readingChapter: 1, sceneOrder: 1 }];
  workspace.reviews.unshift({ id: "review-progress", artifactKind: "draft", artifactRevisionId: workspace.chapters[0].candidate.id, projectRevision: workspace.revision, status: "ready", checks: [{ id: "semantic-review", message: "语义检查待完成", blocking: true, passed: false }], selections: { provisionalImpacts: [{ id: "progress-impact", factProposalId: "progress-fact", severity: "locked-conflict", targetArtifactKind: "outline", targetNodeId: "protected-node", targetLabel: "受保护的大纲", predicate: "letterArrived", evidence: "来信" }] } });
  workspace.modeling = [{ objectId: `draft-${chapter.id}`, sourceRevisionId: workspace.chapters[0].candidate.id, status: "ready", batchId: "knowledge-progress", entities: [], assertions: [], confirmationState: "candidate" }];
  workspace.reviews[0].selections.modelingBatchId = "knowledge-progress";
  await render({ refreshToken: 70 }); await click("更新故事设定");
  reviewProgress.push(progress());
  check(progress().includes("还有 1 项事实未决定") && progress().includes("发布前检查尚未通过"), "Unresolved fact count or blocking-check explanation was missing");
  const factDecision = document.querySelector('[aria-label="letterArrived 的确认结论"]');
  Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value").set.call(factDecision, "confirmed"); factDecision.dispatchEvent(new Event("change", { bubbles: true })); await tick();
  reviewProgress.push(progress());
  check(progress().includes("完成保护冲突处理决定") && !progress().includes("项事实未决定"), "Confirmed fact did not expose its protected-direction gate");
  const protection = document.querySelector('[aria-label="保护冲突处理决定"]');
  Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value").set.call(protection, "accept-and-open-impact"); protection.dispatchEvent(new Event("change", { bubbles: true })); await tick();
  check(!progress().includes("保护冲突") && progress().includes("检查尚未通过"), "Resolved protection did not leave only the actual blocking check");
  workspace.reviews[0].checks[0].passed = true; await render({ refreshToken: 71 });
  reviewProgress.push(progress());
  check(progress() === "已完成检查，可以发布这一章。", "Completed fact, protection and check did not become ready");
  const returnDecision = document.querySelector('[aria-label="保护冲突处理决定"]');
  Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value").set.call(returnDecision, "return-draft"); returnDecision.dispatchEvent(new Event("change", { bubbles: true })); await tick();
  reviewProgress.push(progress());
  check(progress() === "已完成检查，可按决定返回编辑。" && !button("返回编辑").disabled, "Return-draft decision was misleadingly described as adoption");
  Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value").set.call(returnDecision, "accept-and-open-impact"); returnDecision.dispatchEvent(new Event("change", { bubbles: true })); await tick();
  await input("场景 1 正文", "尚未保存的正文修改");
  reviewProgress.push(progress());
  check(progress().includes("先保存章节") && button("确认发布章节").disabled, "Local dirty draft lacked a visible adoption blocker");
  root.unmount(); root = createRoot(main);
  const publicationStates = [];
  for (const [published, saved, dirty] of [[null, candidate, true], [formal, candidate, false], [formal, formal, true], [formal, formal, false]]) {
    root.render(<ChapterPublicationStatus published={published} saved={saved} dirty={dirty} />); await tick(); publicationStates.push(document.querySelector('[aria-label="章节发布状态"]').textContent);
  }
  check(JSON.stringify(publicationStates) === JSON.stringify(["未发布", "有未发布修改", "有未发布修改", "已发布"]), "Publication state mapping is incorrect");
  root.render(<ChapterDocumentView scope="locked-scope" scenes={[{ ...formal.sceneGroups[0], locked: true }]} accepted={formal} title={formal.title} sourcePlanRefs={formal.sourcePlanRefs} lockedIds={[formal.sceneGroups[0].id]} dirty={false} disabled={false} onTitleChange={() => {}} onChange={() => {}} onActiveScene={() => {}} />); await tick(); await versions();
  check(document.querySelector("textarea").readOnly && document.querySelector('[data-change-kind="保护调整"]').textContent.includes("未锁定 → 已锁定"), "Locked editor or exact protection difference was lost");
  await click("场景 1 操作");
  check([...document.querySelectorAll('[role="menu"] button')].every(item => item.disabled), "Protected scene exposed move or delete operations");
  root.render(<ChapterDocumentView scope="locked-scope" scenes={[{ ...formal.sceneGroups[0], locked: true }]} accepted={formal} title={formal.title} sourcePlanRefs={formal.sourcePlanRefs} lockedIds={[formal.sceneGroups[0].id]} dirty={false} disabled={true} onTitleChange={() => {}} onChange={() => {}} onActiveScene={() => {}} />); await tick();
  check(document.querySelector("fieldset").disabled && document.querySelector(".chapter-versions").open, "Task lock closed the read-only version comparison");
  await renderCache("scope-isolation"); await versions();
  const priorNode = document.querySelector("textarea"); await renderCache("other-project:same-chapter");
  check(document.querySelector("textarea") !== priorNode && !document.querySelector(".chapter-versions").open && document.activeElement !== document.querySelector("textarea"), "New project reused editor/versions/bookmark state");
  root.unmount();
  window.testResult = { passed: true, dirtyRoundTrip, generationPhaseChecks, publicationStates, initialDiff, metadataDiffSnapshot, adoptionHints, reviewProgress, themeChecks, layoutChecks, settingsChecks, candidateJourney: { saveCalls, generationCalls, acceptanceCalls, formalRevision: workspace.chapters[0].accepted.id }, planningAuthority: status, sceneBookmark: { scene: "scene-c", positionAfterReorder: 2, start: 43, end: 51, viewportOffset: Math.round(readingOffset) } };
}
run().catch(error => { window.testResult = { passed: false, error: `${error.stack}\n${JSON.stringify(readingChecks)}\nmain: ${main.scrollTop}/${main.scrollHeight}/${main.clientHeight}; offsets: ${[...document.querySelectorAll("textarea")].map(item => `${item.getAttribute("aria-label")}:${offset(item)}/${item.clientHeight}/${item.scrollHeight}`).join(";")}` }; });
