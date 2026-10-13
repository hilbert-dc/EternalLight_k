import React from "react";
import { createRoot } from "react-dom/client";
import { CreativeWorkspaceView, revealControl } from "./creative-workspace-harness";
import { OutlineTreeEditor } from "../../../src/OutlineTreeEditor";
import { chapterRuntimeReads } from "../helpers/chapter-runtime";
import "../../../src/page-layout.css";
import "../../../src/styles.css";

const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (value, message) => { if (!value) throw Error(message); };
const until = async (predicate, message) => { for (let i = 0; i < 140 && !predicate(); i++) await tick(); check(predicate(), message); };
const clone = value => structuredClone(value);
const revision = (id, payload, version = 1) => ({ id, payload, version, source: "author", parentRevisionId: null, baseProjectRevision: 1, createdAt: null });
const artifact = () => ({ candidate: null, accepted: null, working: null, workingSelection: null, locked: false });
const workspace = { project: { id: "planning-ui", title: "规划体验", logline: "一封迟到的信" }, revision: 1, headSnapshotId: "head-1", sourceDigest: "a".repeat(64), planWindow: 3, index: { status: "ready" },
  artifacts: Object.fromEntries(["brief", "outline", "plan", "identity", "propertyRules"].map(kind => [kind, artifact()])), chapters: [], workingBaselines: [], continuationIntents: [], planningReviews: [], commits: [], tasks: [], impacts: [], modeling: [], factProposals: [], reviews: [], chapterPlanBindings: [] };
let view = "brief", refreshToken = 0, dirty = false, running = false, writes = 0, modelCalls = 0, reviewCalls = 0, acceptCalls = 0;
const prepared = [], snapshots = [], savedPayloads = [], versionRequests = [];
let taskSelection = null, directoryRefreshFailures = 0, failOutlineSave = false, failOutlineReadback = false, workspaceReadFailures = 0;
const api = { runtime: { ...chapterRuntimeReads(id => ({ id })),
  getCreativeWorkspace: async () => { if (workspaceReadFailures) { workspaceReadFailures--; throw Error("读取网络中断"); } return clone(workspace); },
  listPlanningImpacts: async () => ({ data: clone(workspace.impacts), nextCursor: null, counts: { total: workspace.impacts.length, open: workspace.impacts.length, retained: 0, lockedConflict: 0 } }),
  getIdentityCatalog: async () => ({ entries: [{ id: "raw-pov", kind: "character", canonicalId: "canonical-other", resolutionState: "authorMerged", claims: [{ displayName: "老周", evidence: { quote: "老周拿着迟到的信。" } }] }], ambiguousNames: [], identityRevisionId: "identity-source", identityDecisions: [], truncated: false, ambiguityScanComplete: true }),
  saveArtifact: async (id, kind, payload, expectedRevision, expectedCandidate) => {
    check(id === workspace.project.id && expectedRevision === workspace.revision && expectedCandidate === (workspace.artifacts[kind].candidate?.id ?? null), "Planning save lost current CAS");
    if (kind === "outline" && failOutlineSave) { failOutlineSave = false; throw Error("保存网络中断"); }
    if (kind === "outline" && failOutlineReadback) { failOutlineReadback = false; workspaceReadFailures++; }
    writes++; savedPayloads.push(clone(payload)); workspace.artifacts[kind].candidate = revision(`${kind}-saved-${writes}`, clone(payload), writes); return { revisionId: workspace.artifacts[kind].candidate.id };
  },
  createReview: async (id, kind, source, proposals) => {
    check(id === workspace.project.id && source === workspace.artifacts[kind].candidate.id && proposals.length === 0, "Planning review lost exact source or added facts");
    reviewCalls++; workspace.reviews.push({ id: `review-${reviewCalls}`, artifactKind: kind, artifactRevisionId: source, projectRevision: workspace.revision, status: "ready", checks: [{ id: "content", passed: true, blocking: true, message: "规划内容检查通过" }], selections: {} }); return {};
  },
  acceptReview: async (id, reviewId, expectedRevision, selections) => {
    const review = workspace.reviews.find(item => item.id === reviewId);
    check(id === workspace.project.id && review.projectRevision === workspace.revision && expectedRevision === workspace.revision && Object.keys(selections).length === 0, "Planning adoption bypassed source/CAS");
    acceptCalls++; workspace.artifacts[review.artifactKind].accepted = clone(workspace.artifacts[review.artifactKind].candidate); review.status = "accepted"; workspace.revision++; return {};
  },
}, agent: {} };
const moduleActionsContainer = document.createElement("div"); moduleActionsContainer.className = "module-records-toolbar-slot";
const root = createRoot(document.getElementById("root")!);
const button = label => [...document.querySelectorAll<HTMLButtonElement>("button")].find(item => item.textContent === label);
const click = async label => { const item = label === "查看修改记录" ? [...document.querySelectorAll<HTMLButtonElement>("button")].find(item => item.getAttribute("aria-label") === label && item.checkVisibility()) : button(label); check(item && !item.matches(":disabled"), `Unavailable ${label}`); await revealControl(item); item.focus({ preventScroll: true }); item.click(); await tick(); };
const field = async (item, value) => { check(item && !item.matches(":disabled"), "Unavailable planning field"); await revealControl(item); const prototype = item.tagName === "TEXTAREA" ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype; Object.getOwnPropertyDescriptor(prototype, "value").set.call(item, value); item.dispatchEvent(new Event("input", { bubbles: true })); await tick(); };
const render = async () => {
  root.render(<div className="workspace"><button id="outside-focus">对话草稿</button><main className="main-panel" data-view={view} style={{ width: 740, height: 640, overflow: "auto" }}><div className="content-header" ref={element => { element?.appendChild(moduleActionsContainer); }}><div className="breadcrumbs"><h1>{view === "brief" ? "作品概况" : view === "outline" ? "全书大纲" : "近章规划"}</h1></div></div>
    <CreativeWorkspaceView api={api} projectId={workspace.project.id} view={view} state={{ project: workspace.project, canon: { characters: [], locations: [], relationships: [], events: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1", scenes: [], anchors: [] }, novelEdition: { chapters: [] } }}
      moduleActionsContainer={moduleActionsContainer} recordsOpen={false} selectedChapterId="" taskResultSelection={taskSelection} refreshToken={refreshToken} taskConversationContainer={null} agentAvailable={false} agentRunning={running} onDirtyChange={value => { dirty = value; }}
      onOpenVersions={() => versionRequests.push(view)} onDiscuss={prompt => prepared.push(prompt)} onFormalChange={async () => { if (view === "outline" && writes === 2 && directoryRefreshFailures === 0) { directoryRefreshFailures++; throw Error("目录网络中断"); } }} onSelectChapter={() => {}} onGenerateWork={async () => { modelCalls++; }} onGenerateChapter={async () => { modelCalls++; }}
      onContinueWork={async () => {}} onRetryWork={async () => {}} onResolveFeedback={async () => {}} onReviewPlanning={async () => { modelCalls++; }} onCancelPlanningReview={async () => {}} onOpenPlanningTarget={kind => { view = kind === "outline" ? "outline" : "plans"; void render(); }} onReturnToFeedback={() => {}} />
  </main></div>); await tick(); await tick(); await until(() => document.querySelector(".planning-workspace-header"), "Planning header missing");
};
const capture = label => snapshots.push({ label, state: document.querySelector(".planning-workspace-state")?.textContent, working: document.querySelector(".planning-reading-basis")?.textContent ?? null,
  actions: [...document.querySelectorAll<HTMLButtonElement>(".planning-workspace-actions button")].map(item => ({ label: item.textContent, disabled: item.matches(":disabled") })), reviewVisible: document.querySelector<HTMLElement>(".review-panel")?.checkVisibility() ?? false,
  ...(view !== "brief" ? { reading: document.querySelector(".planning-reading")?.textContent, ...(view === "outline" ? { outlineBranches: [...document.querySelectorAll<HTMLDetailsElement>(".outline-reading-node")].map(node => ({ identity: node.parentElement.dataset.outlineReadingNode, open: node.open, bodyVisible: node.querySelector("article").checkVisibility() })) } : {}), manualOpen: document.querySelector<HTMLDetailsElement>(".planning-manual-editor")?.open, readingActions: [...document.querySelectorAll<HTMLButtonElement>(".planning-reading > .planning-reading-actions button")].map(item => ({ label: item.textContent, disabled: item.disabled })) } : {}),
  ...(view === "brief" ? { overview: { statistics: [...document.querySelectorAll(".work-overview-stats > div")].map(item => ({ label: item.querySelector("dt").textContent, value: item.querySelector("dd").textContent })), direction: document.querySelector(".overview-direction-summary")?.textContent, editorOpen: document.querySelector<HTMLDetailsElement>(".overview-direction-editor")?.open, lastEdited: document.querySelector(".work-overview-edited")?.textContent ?? null } } : {}) });
const screenshot = async label => { window.testScreenshot = label; await until(() => !window.testScreenshot, "Screenshot hook did not finish"); };
const contrast = (foreground, background) => {
  const luminance = color => { const rgb = color.match(/[\d.]+/g)?.slice(0, 3).map(Number); check(rgb?.length === 3, `Unsupported computed color ${color}`); return rgb.map(value => { const channel = value / 255; return channel <= .04045 ? channel / 12.92 : ((channel + .055) / 1.055) ** 2.4; }).reduce((sum, value, index) => sum + value * [.2126, .7152, .0722][index], 0); };
  const a = luminance(foreground), b = luminance(background); return (Math.max(a, b) + .05) / (Math.min(a, b) + .05);
};
window.addEventListener("error", event => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
(async () => { try {
  for (const page of ["brief", "outline", "plans"]) {
    view = page; await render(); check(document.querySelector(".planning-workspace-state")!.textContent === "尚未开始", "Empty planning looked saved");
    if (page === "brief") {
      const details = document.querySelector<HTMLDetailsElement>(".overview-direction-editor")!;
      check(!details.open && !document.querySelector<HTMLElement>(".artifact-editor-grid textarea").checkVisibility(), "Overview default exposed direction form");
      check(JSON.stringify([...document.querySelectorAll(".work-overview-stats dd")].map(item => item.textContent)) === JSON.stringify(["0", "0 字符", "0"]) && !document.querySelector(".work-overview-edited"), "Empty overview fabricated progress or edit time");
      capture("overview-default"); details.querySelector("summary").click(); await tick();
      check(document.querySelector<HTMLElement>(".artifact-editor-grid textarea").checkVisibility() && writes === 0 && reviewCalls === 0 && acceptCalls === 0 && modelCalls === 0, "Opening direction executed a write or model");
    }
    if (page !== "brief") {
      const manual = document.querySelector<HTMLDetailsElement>(".planning-manual-editor")!;
      check(!manual.open && document.querySelector<HTMLElement>(".planning-reading").checkVisibility() && !document.querySelector<HTMLElement>(".planning-workspace-header").checkVisibility(), "Empty planning exposed manual editor by default");
    }
    check(button("只读检查").disabled && button("检查并采用").disabled, "Empty source offered checking/adoption"); await click("开始规划"); capture(`empty-${page}`);
    check(writes === 0 && modelCalls === 0 && reviewCalls === 0, "Preparing conversation executed planning");
  }
  view = "brief"; await render(); const briefEditor = document.querySelector<HTMLTextAreaElement>(".artifact-editor-grid textarea")!;
  await field(briefEditor, "一封信让人物决定是否面对过去。".repeat(40));
  check(dirty && button("讨论调整").disabled && button("只读检查").disabled && button("检查并采用").disabled, "Dirty planning bypassed saved-source checks");
  briefEditor.focus({ preventScroll: true }); briefEditor.setSelectionRange(2, 5);
  const directionEditor = document.querySelector<HTMLDetailsElement>(".overview-direction-editor")!; directionEditor.querySelector("summary").click(); await tick();
  check(!directionEditor.open && directionEditor.querySelector("summary").textContent.includes("有未保存修改") && dirty && writes === 0 && reviewCalls === 0 && modelCalls === 0, "Closing direction lost dirty state or saved implicitly"); capture("brief-unsaved-collapsed");
  directionEditor.querySelector("summary").click(); await tick(); check(briefEditor.isConnected && document.querySelector(".artifact-editor-grid textarea") === briefEditor && briefEditor.value === "一封信让人物决定是否面对过去。".repeat(40) && briefEditor.selectionStart === 2 && briefEditor.selectionEnd === 5 && dirty, "Disclosure replaced direction value/selection"); capture("brief-unsaved");
  await click("保存"); await until(() => !dirty, "Brief save did not settle"); capture("brief-saved"); document.querySelector("main")!.scrollTop = 0; await screenshot("planning-brief");
  await click("讨论调整"); await click("只读检查"); check(prepared.at(-1).includes("不要修改或采用") && modelCalls === 0 && acceptCalls === 0, "Read-only preparation changed authority");
  await click("检查并采用"); await until(() => button("确认采用这份规划"), "Adoption check missing"); check(acceptCalls === 0 && reviewCalls === 1, "Preparing check adopted or duplicated");
  await click("查看采用检查"); check(reviewCalls === 1, "Opening existing check recreated review"); await click("确认采用这份规划"); await until(() => acceptCalls === 1 && document.querySelector(".planning-workspace-state")!.textContent.includes("已采用") && document.querySelector(".creative-workspace-view")!.getAttribute("aria-busy") === "false", "Explicit adoption failed"); check(button("已采用")?.disabled, "Adopted planning invited duplicate adoption"); capture("brief-adopted");
  workspace.artifacts.brief.candidate = revision("brief-new", { ...workspace.artifacts.brief.accepted.payload, premise: "新的待采用方向" }, 2);
  refreshToken++; await render(); check(document.querySelector(".overview-direction-summary").textContent.includes(workspace.artifacts.brief.accepted.payload.premise) && !document.querySelector(".overview-direction-summary dd").textContent.includes("新的待采用方向") && document.querySelector(".overview-direction-summary").textContent.includes("另有新的方向建议"), "New suggestion replaced the confirmed direction"); capture("confirmed-direction-with-suggestion");
  workspace.artifacts.brief.working = clone(workspace.artifacts.brief.candidate); workspace.artifacts.brief.workingSelection = { revisionId: "brief-new", selectionRevision: 1 };
  refreshToken++; await render(); check(document.querySelector(".overview-direction-summary")?.textContent.includes("当前后续创作依据 · 尚未正式采用"), "Working override was disguised as adopted");
  check(!document.querySelector(".planning-version-tools") && !document.querySelector(".planning-compare") && !document.querySelector(".planning-workspace-state").textContent.match(/v\d+/), "Default editing retained version controls or version number"); capture("working-override");
  workspace.artifacts.brief.locked = true; refreshToken++; await render(); check(button("保存").disabled && button("检查并采用").disabled && !button("只读检查").disabled && !button("讨论调整").disabled, "Locked planning blocked read-only discussion or allowed writes");
  await click("只读检查");
  const lockedWrites = writes; await click("查看修改记录"); check(versionRequests.at(-1) === "brief" && writes === lockedWrites && reviewCalls === 1 && modelCalls === 0, "Locked planning lost its read-only version navigation or performed a write");
  capture("locked-read-only"); workspace.artifacts.brief.locked = false;
  workspace.reviews.push({ id: "old-review", artifactKind: "brief", artifactRevisionId: "brief-new", projectRevision: workspace.revision - 1, status: "ready", checks: [], selections: {} });
  refreshToken++; await render(); check(!button("确认采用这份规划") && button("检查并采用"), "Stale project review became current");
  await click("检查并采用"); await until(() => reviewCalls === 2, "Stale check was reused"); check(workspace.reviews.at(-1).projectRevision === workspace.revision && acceptCalls === 1, "Review replacement adopted or used stale CAS"); capture("fresh-check");
  view = "outline"; await render(); view = "brief"; await render();
  const existingCheck = document.querySelector<HTMLElement>(".review-panel")!, direction = document.querySelector<HTMLDetailsElement>(".overview-direction-editor")!;
  check(!direction.open && !existingCheck.checkVisibility() && existingCheck.closest(".overview-direction-editor") === direction && !document.querySelector(".planning-compare"), "Existing brief check escaped its editor or old inline comparison remained"); capture("overview-existing-check-collapsed");
  const beforeDisclosure = { writes, reviewCalls, acceptCalls, modelCalls };
  direction.querySelector("summary").click(); await tick();
  check(existingCheck.checkVisibility() && !button("确认采用这份规划").disabled, "Expanded brief check was not actionable");
  direction.querySelector("summary").click(); await tick();
  check(!existingCheck.checkVisibility(), "Closed direction leaked its adoption check");
  direction.querySelector("summary").click(); await tick();
  check(document.querySelector(".review-panel") === existingCheck && existingCheck.checkVisibility() && JSON.stringify({ writes, reviewCalls, acceptCalls, modelCalls }) === JSON.stringify(beforeDisclosure), "Direction disclosure replaced its adoption check or executed a write"); capture("overview-existing-check-retained");
  const storyArc = "起因：收到迟到的来信，林澄发现邮戳日期不可能。\n转折：她核对港口记录，得知母亲曾隐藏一次失败的救援。\n高潮：她交出最后一段母亲记忆，阻止港城被新的预言吞没。\n结局：林澄凭修钟笔记重新理解母亲，并留下修复城市。";
  const book = { id: "book", kind: "book", position: 0, title: "全书", purpose: "一封信改变选择。".repeat(40), summary: "", dependencyKeys: [], certainty: "tentative", protection: "free", writerGuidance: "保留人物选择" };
  workspace.artifacts.outline.candidate = revision("outline-source", { nodes: [book, { ...book, id: undefined, clientKey: "volume-other", parentId: "book", kind: "volume", position: 1, title: "第一卷", purpose: "同名卷的另一条安排" }, { ...book, id: "volume", parentId: "book", kind: "volume", title: "第一卷", purpose: "发现来信与记录之间的矛盾，决定追查母亲隐瞒的救援。" }, { ...book, id: "unit-one", parentId: "volume", kind: "unit", title: "核对来信", purpose: "发现邮戳" }, { ...book, id: "unit-two", parentId: "volume", kind: "unit", position: 1, title: "决定回信", purpose: "作出选择" }, { ...book, id: "volume-last", parentId: "book", kind: "volume", position: 2, title: "第三卷", purpose: "在记忆与港城之间作出选择，走向修复的结局。" }] });
  workspace.artifacts.outline.accepted = clone(workspace.artifacts.outline.candidate);
  workspace.artifacts.plan.candidate = revision("plan-source", { sourceOutlineRevisionId: "outline-source", chapters: [{ chapterPlanId: "plan-one", chapterNumber: 1, primaryNodeId: "volume", title: "第一封信", goal: "决定如何回复。".repeat(35), conflict: "来信与记忆矛盾。", writerGuidance: "保留犹豫。".repeat(30), publicTitle: "门缝里的信", publicCommitments: ["信件没有立即打开"], allowances: [], dependencyKeys: [], povCharacterId: "unavailable-original", scenes: [{ id: "scene-plan", title: "收信", goal: "发现来信", locked: true, beats: [{ id: "beat", description: "发现来信。", locked: true }] }] }] });
  for (const page of ["outline", "plans"]) {
    view = page; refreshToken++; await render(); check(!button("在对话中讨论这份规划") && document.querySelectorAll(".planning-reading > .planning-reading-actions").length === 1, "Duplicate discussion entry retained");
    if (page === "plans") {
      const beforeBasis = { writes, reviewCalls, acceptCalls, modelCalls };
      workspace.artifacts.plan.working = revision("plan-working-other", clone(workspace.artifacts.plan.candidate.payload)); refreshToken++; await render();
      const basis = document.querySelector(".planning-reading-basis");
      check(basis?.textContent.includes("后续写作仍参考另一份已保存规划") && !basis.textContent.match(/v\d+/), "Different working source lost its concise provenance warning");
      await click("查看依据"); check(versionRequests.at(-1) === "plans" && JSON.stringify({ writes, reviewCalls, acceptCalls, modelCalls }) === JSON.stringify(beforeBasis), "Reading basis entry executed a write/model instead of version navigation");
      capture("plans-different-working-source");
      workspace.artifacts.plan.working = clone(workspace.artifacts.plan.candidate); refreshToken++; await render();
      check(!document.querySelector(".planning-reading-basis"), "Same saved working source retained a redundant basis notice");
      workspace.artifacts.plan.working = null;
    }
    const manual = document.querySelector<HTMLDetailsElement>(".planning-manual-editor")!;
    check(!document.querySelector(".planning-compare,.planning-version-tools") && !document.querySelector(".planning-reading-status").textContent.match(/v\d+/), "Reading retained inline versions or version numbers");
    check(!manual.open && document.querySelector<HTMLElement>(".planning-reading").checkVisibility(), "Saved planning did not default to reading"); capture(`saved-reading-${page}`);
    if (page === "outline") {
      const readingBranch = id => document.querySelector<HTMLDetailsElement>(`[data-outline-reading-node="${id}"] > .outline-reading-node`)!;
      const openOutline = async id => { const overview = document.querySelector(`[data-outline-reading-node="${id}"].outline-volume-overview`); check(overview, `Missing exact volume overview ${id}`); overview.querySelector<HTMLButtonElement>("button").click(); await tick(); };
      const untouched = { writes, reviewCalls, acceptCalls, modelCalls };
      const bookFields = document.querySelector('[data-outline-reading-node="book"] > details > article > dl');
      check(JSON.stringify([...bookFields.querySelectorAll("dt")].map(item => item.textContent)) === JSON.stringify(["核心主线", "故事脉络"]) && bookFields.textContent.includes(book.purpose) && document.querySelector('[data-outline-reading-node="book"]').textContent.includes("脉络") && !bookFields.textContent.includes("同名卷的另一条安排"), "Empty story arc was fabricated from volume content or lost its full main thread");
      check(readingBranch("book").open && document.querySelectorAll(".outline-volume-overview").length === 3 && !document.querySelector('[data-outline-reading-node="unit-one"]'), "Full outline did not show main thread and concise volume overviews");
      await openOutline("volume"); check(!document.querySelector('[data-outline-reading-node="book"]') && !document.querySelector('[data-outline-reading-node="volume-other"]') && readingBranch("volume").open && !readingBranch("unit-one").open, "Selected outline volume included another subtree");
      check(JSON.stringify([...document.querySelectorAll('[data-outline-reading-node="volume"] > details > article > dl dt')].map(item => item.textContent)) === JSON.stringify(["本卷推动什么"]), "Volume reused full-story labels instead of its stage purpose");
      const unitDiscuss = document.querySelector<HTMLButtonElement>('button[aria-label="讨论核对来信"]'); await revealControl(unitDiscuss); unitDiscuss.click(); check(prepared.at(-1).includes("unit-one") && prepared.at(-1).includes("全书 / 第一卷 / 核对来信"), "Unit discussion lost actual parent identity");
      readingBranch("unit-one").querySelector("summary").click(); await tick(); refreshToken++; await render(); check(!readingBranch("unit-one").open, "Background refresh reopened author's closed unit");
      document.querySelector<HTMLButtonElement>('button[aria-label="讨论第一卷"]').click(); check(prepared.at(-1).includes("outline-source") && prepared.at(-1).includes("节点 ID volume。") && prepared.at(-1).includes("全书 / 第一卷"), "Selected volume discussion lost exact source");
      workspace.artifacts.outline.locked = true; refreshToken++; await render(); check(!document.querySelector<HTMLButtonElement>('button[aria-label="讨论第一卷"]').disabled && button("保存").disabled, "Outline lock denied read-only discussion or allowed writes"); workspace.artifacts.outline.locked = false; refreshToken++; await render();
      await click("返回全书大纲"); await openOutline("volume-other"); document.querySelector<HTMLButtonElement>('button[aria-label="讨论第一卷"]').click(); check(prepared.at(-1).includes("outline-source") && prepared.at(-1).includes("节点 clientKey volume-other。"), "Client-key volume lost exact identity");
      check(JSON.stringify(untouched) === JSON.stringify({ writes, reviewCalls, acceptCalls, modelCalls }) && !manual.open, "Volume reading/discussion executed a write or opened manual editing");
      await click("返回全书大纲");
    }

    let editor = document.querySelector<HTMLTextAreaElement>(page === "outline" ? ".outline-node textarea" : ".plan-field textarea")!;
    await field(editor, `${editor.value}作者补充。`); editor.focus(); editor.setSelectionRange(2, 5);
    const beforeVersions = { writes, reviewCalls, acceptCalls, modelCalls }; await click("查看修改记录");
    check(versionRequests.at(-1) === page && dirty && editor.isConnected && editor.selectionStart === 2 && editor.selectionEnd === 5 && JSON.stringify({ writes, reviewCalls, acceptCalls, modelCalls }) === JSON.stringify(beforeVersions), "Preparing dirty version navigation lost the draft/selection or performed authority work");
    if (page === "outline") {
      check(document.querySelector<HTMLButtonElement>('button[aria-label="讨论全书"]').disabled && [...document.querySelectorAll<HTMLButtonElement>(".outline-volume-overview button")].every(item => item.disabled) && document.querySelector(".planning-reading").textContent.includes("先保存"), "Dirty outline discussion silently rebound saved provenance");
    }
    manual.querySelector("summary").click(); await tick();
    check(!manual.open && dirty && manual.querySelector("summary").textContent.includes("有未保存修改") && !document.querySelector(".planning-reading-list").textContent.includes("作者补充") && document.querySelector(".planning-reading-hint"), "Closed manual editor lost dirty feedback or exposed unsaved summary");
    manual.querySelector("summary").click(); await tick();
    check(document.querySelector(page === "outline" ? ".outline-node textarea" : ".plan-field textarea") === editor && editor.selectionStart === 2 && editor.selectionEnd === 5, "Manual disclosure replaced editor or selection");
    if (page === "outline") {
      const node = id => document.querySelector<HTMLElement>(`[data-outline-node-id="${id}"]`)!;
      const structure = () => document.querySelector<HTMLDetailsElement>(".outline-structure-tools")!;
      const structureKeys = () => [...structure().querySelectorAll<HTMLElement>("[data-outline-structure-node]")].map(item => item.dataset.outlineStructureNode);
      const selectNode = async id => {
        structure().open = true; await tick();
        const row = structure().querySelector<HTMLElement>(`[data-outline-structure-node="${id}"]`);
        const action = row?.matches("button") ? row : row?.querySelector<HTMLButtonElement>("button");
        check(action && action.getAttribute("aria-label")?.startsWith("编辑"), `Missing exact structure selection ${id}`);
        await revealControl(action); action.click(); await tick();
        check(document.querySelectorAll(".outline-node").length === 1 && node(id), `Selecting ${id} exposed recursive node forms`);
      };
      const extra = () => document.querySelector<HTMLDetailsElement>(".outline-node-extra")!;
      const ownButton = label => [...node(document.querySelector<HTMLElement>(".outline-node").dataset.outlineNodeId).querySelectorAll<HTMLButtonElement>(":scope > article button")].find(item => item.getAttribute("aria-label") === label)!;
      check(document.querySelectorAll(".outline-node").length === 1 && node("book")?.classList.contains("depth-0") && !structure().open, "Default outline editor did not isolate the full-story node");
      check(!document.querySelector<HTMLElement>(".planning-reading").checkVisibility(), "Manual editing competed with the reading surface");
      extra().open = true;
      check(ownButton("上移全书").disabled && ownButton("下移全书").disabled, "Book offered invalid sibling moves");
      const protectedNode = node("book").querySelector<HTMLSelectElement>("select")!;
      for (const protection of ["free", "preserve-intent", "preserve-arrangement"]) { protectedNode.value = protection; protectedNode.dispatchEvent(new Event("change", { bubbles: true })); await tick(); check(protectedNode.value === protection && dirty, "Author protection selection lost its current enum"); }
      const story = [...node("book").querySelectorAll<HTMLLabelElement>(":scope > article > label")].find(label => label.querySelector("span")?.textContent === "故事脉络")?.querySelector<HTMLTextAreaElement>("textarea"); check(story && story.checkVisibility(), "Primary story arrangement editor missing"); await field(story, storyArc); story.focus(); story.setSelectionRange(4, 7); extra().open = false; await tick(); check(story.checkVisibility() && story.selectionStart === 4 && story.selectionEnd === 7, "More disclosure hid primary story arrangement or reset selection");
      const note = [...extra().querySelectorAll<HTMLLabelElement>("label")].find(label => label.querySelector("span")?.textContent === "写作备注")?.querySelector<HTMLTextAreaElement>("textarea"); check(note && !note.checkVisibility(), "Writing note was not inside low-frequency controls"); await field(note, "保留人物选择，结尾只留下回信这一动作。");
      snapshots.push({ label: "outline-manual-controls", selected: "book", fields: [...node("book").querySelectorAll(":scope > article > label > span")].map(label => label.textContent), summary: extra().querySelector("summary").textContent, protection: node("book").querySelector("label:has(select) > span").textContent, choices: [...protectedNode.options].map(option => option.textContent), forms: document.querySelectorAll(".outline-node").length });
      await selectNode("volume"); check(node("volume").classList.contains("depth-1"), "Volume form lost actual hierarchy depth"); extra().open = true;
      check(ownButton("上移第一卷").disabled && ownButton("删除第一卷").disabled, "Parent volume offered invalid move or deletion");
      node("volume").scrollIntoView({ block: "center" }); await screenshot("planning-outline-manual");
      await selectNode("volume-last"); extra().open = true; check(ownButton("下移第三卷").disabled, "Last volume offered invalid move");
      await selectNode("unit-one"); extra().open = true; check(node("unit-one").classList.contains("depth-2"), "Unit form lost actual hierarchy depth"); ownButton("下移核对来信").click(); await tick();
      check(ownButton("下移核对来信").disabled && structureKeys().indexOf("unit-two") < structureKeys().indexOf("unit-one"), "Move changed parent or failed to update sibling boundaries");
      await selectNode("unit-two"); extra().open = true; check(ownButton("上移决定回信").disabled, "Moved unit lost first-sibling boundary");
      await selectNode("volume-last"); extra().open = true; ownButton("删除第三卷").click(); await tick();
      check(!structureKeys().includes("volume-last") && structureKeys().indexOf("volume") < structureKeys().indexOf("volume-other"), "Deleting last sibling reordered surviving nodes from raw payload order");
      await selectNode("volume-other"); extra().open = true; ownButton("删除第一卷").click(); await tick(); check(!structureKeys().includes("volume-other") && structureKeys().includes("volume"), "Deleting exact client-key leaf removed the same-name parent");
      await selectNode("book"); extra().open = false; structure().open = false;
      editor = node("book").querySelector<HTMLTextAreaElement>(":scope > article > label textarea")!;
      check(editor.value === book.purpose + "作者补充。" && node("book").querySelector<HTMLTextAreaElement>(":scope > article > label:nth-of-type(2) textarea").value === storyArc, "Switching node forms erased the full-story draft");
      editor.focus({ preventScroll: true }); editor.setSelectionRange(2, 5);
    }
    const details = document.querySelector<HTMLDetailsElement>(page === "outline" ? ".outline-node-extra" : ".plan-advanced")!; details.querySelector("summary")!.click(); await tick(); details.querySelector("summary")!.click(); await tick();
    check(editor.isConnected && document.querySelector(page === "outline" ? ".outline-node textarea" : ".plan-field textarea") === editor && editor.selectionStart === 2 && editor.selectionEnd === 5, "Advanced disclosure replaced editor/selection");
    if (page === "outline") {
      const draft = editor.value, beforeWrites = writes;
      failOutlineSave = true; await click("保存");
      await until(() => document.querySelector('[role="alert"]')?.textContent.includes("保存网络中断") && !button("保存").disabled, "Explicit save failure did not settle");
      check(manual.open && dirty && editor.isConnected && document.querySelector(".outline-node textarea") === editor && editor.value === draft && editor.selectionStart === 2 && editor.selectionEnd === 5 && writes === beforeWrites, "Failed save exited or replaced the node draft/selection"); capture("outline-save-failed-retained");
      failOutlineReadback = true; await click("保存");
      await until(() => document.querySelector('[role="alert"]')?.textContent.includes("再次保存只会核对") && !button("保存").disabled, "Saved receipt read failure did not remain recoverable");
      check(manual.open && dirty && editor.isConnected && document.querySelector(".outline-node textarea") === editor && editor.value === draft && editor.selectionStart === 2 && editor.selectionEnd === 5 && writes === beforeWrites + 1 && directoryRefreshFailures === 0, "Readback failure exited or rewrote the saved draft"); capture("outline-saved-readback-pending");
    }
    await click("保存"); await until(() => !dirty, "Planning edit did not save");
    if (page === "outline") {
      check(directoryRefreshFailures === 1 && document.querySelector('[role="alert"]')?.textContent.includes("大纲已保存，但目录刷新失败：目录网络中断") && !dirty && writes === 2, "Directory refresh failure lost the saved receipt or falsely left outline unsaved");
      refreshToken++; await render(); check(writes === 2 && !dirty, "Read-only refresh replayed the saved outline"); capture("outline-saved-directory-refresh-failed");
      const nodes = savedPayloads.at(-1).nodes;
      check(nodes.find(item => item.id === "book").summary === storyArc && workspace.artifacts.outline.candidate.payload.nodes.find(item => item.id === "book").summary === storyArc && document.querySelector<HTMLTextAreaElement>('[data-outline-node-id="book"] > article > label:nth-of-type(2) textarea').value === storyArc, "Saved story arrangement did not survive authoritative readback");
      const beforeReturn = writes; await click("返回阅读");
      check(!manual.open && writes === beforeReturn && document.querySelector<HTMLElement>(".planning-reading").checkVisibility() && editor.isConnected, "Returning to reading saved implicitly or unmounted the editor");
      const displayedArc = document.querySelector<HTMLElement>('[data-outline-reading-node="book"] > details > article > dl > div:nth-of-type(2) dd');
      check(displayedArc.textContent === storyArc && displayedArc.getBoundingClientRect().height >= parseFloat(getComputedStyle(displayedArc).lineHeight) * 4 - 2, "Story arc lost complete paragraphs in its saved reading surface");
      check(JSON.stringify(nodes.map(item => ({ id: item.id, parentId: item.parentId ?? null, position: item.position, certainty: item.certainty, protection: item.protection, writerGuidance: item.writerGuidance }))) === JSON.stringify([
        { id: "book", parentId: null, position: 0, certainty: "tentative", protection: "preserve-arrangement", writerGuidance: "保留人物选择，结尾只留下回信这一动作。" },
        { id: "volume", parentId: "book", position: 0, certainty: "tentative", protection: "free", writerGuidance: "保留人物选择" },
        { id: "unit-one", parentId: "volume", position: 1, certainty: "tentative", protection: "free", writerGuidance: "保留人物选择" },
        { id: "unit-two", parentId: "volume", position: 0, certainty: "tentative", protection: "free", writerGuidance: "保留人物选择" }
      ]), "Saving note/structure lost hidden certainty, stable IDs, parent, order or author protection");
    }
    if (page === "plans") {
      check(savedPayloads.at(-1).chapters[0].povCharacterId === "unavailable-original", "Saving unrelated fields erased unknown POV");
      const pov = document.querySelector<HTMLSelectElement>('[aria-label="第 1 章视角人物"]')!; await revealControl(pov);
      check(pov.value === "unavailable-original" && [...pov.options].some(option => option.value === "raw-pov"), "Raw or unknown POV source missing");
      pov.value = "raw-pov"; pov.dispatchEvent(new Event("change", { bubbles: true })); await tick(); await click("保存"); await until(() => !dirty, "POV save did not settle");
      check(savedPayloads.at(-1).chapters[0].povCharacterId === "raw-pov", "Author POV selection was replaced by canonical identity");
      document.querySelector<HTMLDetailsElement>(".plan-advanced")!.open = false;
    }
    manual.open = false;
    document.documentElement.dataset.theme = "light"; document.querySelector("main")!.scrollTop = 0; await tick(); await screenshot(`planning-${page}`);
    if (page === "plans") { manual.open = true; details.querySelector("summary")!.click(); await tick(); details.scrollIntoView({ block: "center" }); await screenshot("planning-plans-advanced"); details.querySelector("summary")!.click(); }
    for (const theme of ["light", "dark"]) for (const width of [330, 740]) {
      document.documentElement.dataset.theme = theme; const main = document.querySelector<HTMLElement>("main")!; main.style.width = `${width}px`; await tick();
      const box = main.getBoundingClientRect(); check(Math.abs(box.width - width) <= 1 && main.scrollWidth <= main.clientWidth + 2, `Planning overflow ${page}-${theme}-${width}`);
      manual.open = false; await tick();
      check(document.querySelector<HTMLElement>(".planning-reading").checkVisibility() && !editor.checkVisibility(), "Default reading geometry exposed manual fields");
      manual.open = true; await tick();
      if (page === "outline") check(!document.querySelector<HTMLElement>(".planning-reading").checkVisibility(), "Expanded outline editor did not hide reading");
      check([...document.querySelectorAll<HTMLElement>(".planning-workspace-actions button")].every(item => { const bounds = item.getBoundingClientRect(); return bounds.width > 0 && bounds.left >= box.left - 2 && bounds.right <= box.right + 2; }), "Planning main action clipped");
      if (page === "plans") {
        const advanced = main.querySelector<HTMLDetailsElement>(".plan-advanced")!; advanced.open = true; await tick();
        for (const item of [...advanced.querySelectorAll<HTMLElement>("input,select")]) { const bounds = item.getBoundingClientRect(); check(bounds.width > 0 && bounds.left >= box.left - 2 && bounds.right <= box.right + 2 && (!item.matches("select") || bounds.height >= 40), "Advanced planning field clipped or too short"); }
        advanced.open = false;
      }
      if (page === "outline") {
        const saveStyle = getComputedStyle(button("保存")), ratio = contrast(saveStyle.color, saveStyle.backgroundColor);
        check(ratio >= 4.5, `Save text contrast below 4.5: ${theme} ${ratio}`);
        snapshots.push({ label: `outline-save-contrast-${theme}-${width}`, foreground: saveStyle.color, background: saveStyle.backgroundColor, ratio: Number(ratio.toFixed(2)) });
        main.scrollTop = 0; await tick(); await screenshot(`planning-outline-editor-${theme}-${width}`);
      }
      main.scrollTop = main.scrollHeight; await tick(); const last = [...main.querySelectorAll<HTMLElement>("button,summary,textarea")].filter(item => item.checkVisibility()).at(-1)!;
      check(last.getBoundingClientRect().bottom <= box.bottom + 2, "Long planning end unreachable"); capture(`${page}-${theme}-${width}`);
    }
  }
  view = "outline"; refreshToken++; await render();
  const manual = document.querySelector<HTMLDetailsElement>(".planning-manual-editor")!;
  const outside = document.querySelector<HTMLButtonElement>("#outside-focus")!; outside.focus({ preventScroll: true });
  const main = document.querySelector<HTMLElement>("main")!, beforeScroll = main.scrollTop;
  workspace.reviews.push({ id: "background-outline", artifactKind: "outline", artifactRevisionId: workspace.artifacts.outline.candidate.id, projectRevision: workspace.revision, status: "ready", checks: [{ id: "content", passed: true, blocking: true, message: "检查通过" }], selections: {} });
  refreshToken++; await render();
  check(!manual.open && document.activeElement === outside && main.scrollTop === beforeScroll && !document.querySelector<HTMLElement>(".review-panel").checkVisibility(), "Background review stole focus or opened manual editor"); capture("background-review-collapsed");
  const result = { kind: "outline", projectId: workspace.project.id, workTaskId: "work", stepId: "step", attemptId: "attempt", referenceId: workspace.artifacts.outline.candidate.id };
  taskSelection = { requestId: "exact-result", view: "outline", result }; await render();
  await until(() => manual.open && document.activeElement === document.querySelector(".review-panel"), "Exact result did not reveal and focus its adoption check");
  check(reviewCalls === 2 && acceptCalls === 1, "Opening exact ready result created or adopted a new review"); capture("exact-result-check");
  manual.open = false; outside.focus({ preventScroll: true }); taskSelection = { requestId: "stale-result", view: "outline", result: { ...result, referenceId: "stale-source" } }; await render();
  await until(() => [...document.querySelectorAll('[role="alert"]')].some(item => item.textContent.includes("变化")), "Stale result lacked source error");
  check(!manual.open && reviewCalls === 2 && acceptCalls === 1, "Stale result substituted latest check"); taskSelection = null;
  workspace.impacts.push({ id: "impact-volume", status: "affected", state: "open", severity: "warning", reason: "卷内安排待核对", evidence: "原文证据", targetArtifactKind: "outline", targetRevisionId: workspace.artifacts.outline.candidate.id, targetNodeId: "volume", targetLabel: "第一卷", dependencyKey: "volume" });
  refreshToken++; await render(); await click("打开并修改");
  await until(() => manual.open && document.activeElement === document.querySelector('[aria-label="第一卷标题"]'), "Feedback did not reveal exact volume");
  const purpose = document.querySelector<HTMLTextAreaElement>('.outline-node.depth-1 > article > label textarea')!;
  purpose.focus({ preventScroll: true }); await field(purpose, purpose.value + "连续"); purpose.setSelectionRange(3, 3); await field(purpose, purpose.value + "输入");
  check(document.activeElement === purpose && purpose.isConnected && dirty, "Feedback target stole focus on each purpose keystroke");
  manual.open = false; outside.focus({ preventScroll: true }); refreshToken++; await render();
  check(!manual.open && document.activeElement === outside && purpose.value.endsWith("连续输入"), "Background refresh reopened feedback editor or erased local input"); capture("feedback-edit-retained");
  running = true; await render(); check([...document.querySelectorAll<HTMLButtonElement>(".planning-workspace-actions button,.planning-reading button")].every(item => item.disabled), "Running task left planning write/discussion actions enabled"); check([...document.querySelectorAll<HTMLButtonElement>('.content-header [aria-label="查看修改记录"]')].some(item => !item.disabled), "Running task blocked read-only module records"); capture("running");
  check(writes === 4 && acceptCalls === 1 && modelCalls === 0, "Planning actions unexpectedly wrote or generated");
  const scopedSource = [book, { ...book, id: "scope-volume", kind: "volume", parentId: "book", title: "本卷" }, { ...book, id: "scope-unit", kind: "unit", parentId: "scope-volume", title: "本卷单元" }, { ...book, id: "outside-volume", kind: "volume", parentId: "book", position: 1, title: "其他卷" }];
  let scopedNodes = clone(scopedSource), scopeFocus = null, scopeError = null, scopeEdits = 0;
  const renderScoped = async () => { root.render(<main className="main-panel" style={{ width: 740, height: 640, overflow: "auto" }}><OutlineTreeEditor nodes={scopedNodes} rootNodeId="scope-volume" focusNodeId={scopeFocus} error={scopeError} onChange={next => { scopedNodes = next; scopeEdits++; void renderScoped(); }} /></main>); await tick(); await tick(); };
  await renderScoped();
  check(document.querySelectorAll(".outline-node").length === 1 && document.querySelector<HTMLElement>(".outline-node").dataset.outlineNodeId === "scope-volume" && document.querySelector(".outline-node.depth-1"), "Volume editor did not isolate its exact root at actual depth");
  const longPurpose = "本卷推进人物选择，保留局部因果而不替其他卷编写。".repeat(160);
  await field(document.querySelector(".outline-node > article > label:nth-of-type(2) textarea"), longPurpose);
  check(scopedNodes.find(node => node.id === "scope-volume").summary === longPurpose && JSON.stringify(scopedNodes.filter(node => node.id !== "scope-volume")) === JSON.stringify(scopedSource.filter(node => node.id !== "scope-volume")), "Scoped long input changed outside nodes or hidden fields");
  const longField = document.querySelector<HTMLTextAreaElement>(".outline-node > article > label:nth-of-type(2) textarea")!;
  check(longField.getBoundingClientRect().height > 240 && longField.scrollHeight <= longField.clientHeight + 2, "Long outline text retained an inner scroll or clipped its last lines");
  const scopedMain = document.querySelector<HTMLElement>("main")!;
  for (const theme of ["light", "dark"]) for (const width of [330, 740]) {
    document.documentElement.dataset.theme = theme; scopedMain.style.width = `${width}px`; await tick();
    longField.focus({ preventScroll: true }); longField.setSelectionRange(longPurpose.length, longPurpose.length);
    scopedMain.scrollTop += longField.getBoundingClientRect().bottom - scopedMain.getBoundingClientRect().bottom + 16; await tick();
    check(longField.scrollHeight <= longField.clientHeight + 2 && longField.getBoundingClientRect().bottom <= scopedMain.getBoundingClientRect().bottom + 2 && longField.value === longPurpose, "Long story arrangement last line remained clipped or changed");
    await screenshot(`planning-outline-summary-end-${theme}-${width}`);
  }
  scopedMain.scrollTop = scopedMain.scrollHeight; await tick();
  check(scopedMain.scrollWidth <= scopedMain.clientWidth + 2 && scopedMain.querySelector(".outline-structure-tools > summary").getBoundingClientRect().bottom <= scopedMain.getBoundingClientRect().bottom + 2, "Long outline form end was unreachable or overflowed horizontally");
  scopeFocus = "scope-unit"; await renderScoped();
  await until(() => document.querySelector<HTMLElement>(".outline-node")?.dataset.outlineNodeId === "scope-unit", "Focus did not select the exact descendant");
  check(document.querySelector(".outline-node.depth-2") && document.activeElement?.getAttribute("aria-label") === "本卷单元标题", "Focused descendant lost actual depth or keyboard focus");
  scopeFocus = null; scopeError = { nodeId: "outside-volume", message: "其他卷的故事目标需要修正" }; await renderScoped();
  check(document.querySelector<HTMLElement>(".outline-node").dataset.outlineNodeId !== "outside-volume" && button("定位并修正") && scopeEdits === 1, "Outside error silently switched volume scope or changed data");
  snapshots.push({ label: "scoped-outside-error", selected: document.querySelector<HTMLElement>(".outline-node").dataset.outlineNodeId, forms: document.querySelectorAll(".outline-node").length, warning: document.querySelector('[role="alert"]')?.textContent, locate: button("定位并修正").textContent, edits: scopeEdits });
  await click("定位并修正");
  await until(() => document.querySelector<HTMLElement>(".outline-node")?.dataset.outlineNodeId === "outside-volume", "Explicit error location did not select the exact outside node");
  check(document.activeElement?.getAttribute("aria-label") === "其他卷标题" && document.querySelector('[role="alert"]')?.textContent.includes(scopeError.message) && scopeEdits === 1, "Explicit correction did not expose/focus its error or wrote data");
  scopeError = { nodeId: "scope-unit", message: "单元安排需要修正" }; await renderScoped();
  await until(() => document.querySelector<HTMLElement>(".outline-node")?.dataset.outlineNodeId === "scope-unit", "In-scope error did not select its exact node");
  snapshots.push({ label: "scoped-error-focused", selected: document.querySelector<HTMLElement>(".outline-node").dataset.outlineNodeId, fields: [...document.querySelectorAll(".outline-node > article > label > span")].map(item => item.textContent), warning: document.querySelector('[role="alert"]')?.textContent, longInputPreserved: scopedNodes.find(node => node.id === "scope-volume").summary === longPurpose, edits: scopeEdits });
  const scopedStructure = document.querySelector<HTMLDetailsElement>(".outline-structure-tools")!; scopedStructure.open = true; await tick();
  const volumeRow = scopedStructure.querySelector<HTMLElement>('[data-outline-structure-node="scope-volume"]')!;
  (volumeRow.matches("button") ? volumeRow : volumeRow.querySelector<HTMLButtonElement>("button")).click(); await tick();
  document.querySelector<HTMLDetailsElement>(".outline-node-extra").open = true; await click("＋ 添加单元");
  const addedUnit = scopedNodes.find(node => !scopedSource.some(original => original.id === node.id && original.clientKey === node.clientKey));
  check(addedUnit?.kind === "unit" && addedUnit.parentId === "scope-volume" && scopedNodes.length === scopedSource.length + 1 && document.querySelectorAll(".outline-node").length === 1 && document.querySelector<HTMLElement>(".outline-node").dataset.outlineNodeId === addedUnit.clientKey && scopeEdits === 2, "Adding a unit escaped its exact parent or reopened recursive forms");
  check(scopedNodes.find(node => node.id === "scope-volume").summary === longPurpose && JSON.stringify(scopedNodes.find(node => node.id === "outside-volume")) === JSON.stringify(scopedSource.find(node => node.id === "outside-volume")), "Adding a scoped unit erased the draft or another volume");
  snapshots.push({ label: "scoped-added-unit", selectedPurpose: document.querySelector<HTMLTextAreaElement>(".outline-node > article > label textarea").value, title: addedUnit.title, parentId: addedUnit.parentId, forms: document.querySelectorAll(".outline-node").length, edits: scopeEdits });
  for (const nodes of [[{ ...book, id: "cycle-self", parentId: "cycle-self", kind: "volume" }], [{ ...book, id: "cycle-a", parentId: "cycle-b", kind: "volume" }, { ...book, id: "cycle-b", parentId: "cycle-a", kind: "volume" }]]) {
    const frozen = JSON.stringify(nodes); let edits = 0;
    root.render(<OutlineTreeEditor nodes={nodes} rootNodeId={nodes[0].id} onChange={() => { edits++; }} />); await tick();
    check(document.querySelectorAll(".outline-node").length === 1 && document.querySelector<HTMLElement>(".outline-node").dataset.outlineNodeId === nodes[0].id && document.querySelector('[role="alert"]')?.textContent.includes("循环") && JSON.stringify(nodes) === frozen && edits === 0, "Cyclic selected subtree did not render bounded warning or mutated source");
    snapshots.push({ label: `cycle-${nodes.length}`, nodes: [...document.querySelectorAll<HTMLElement>("[data-outline-node-id]")].map(node => node.dataset.outlineNodeId), warning: document.querySelector('[role="alert"]').textContent, edits });
  }
  window.testResult = { passed: true, snapshots, writes, reviewCalls, acceptCalls, modelCalls, prepared: prepared.length, versionRequests };
} catch (error) { window.testResult = { passed: false, error: error.stack ?? String(error), view, writes, reviewCalls, acceptCalls }; } })();
