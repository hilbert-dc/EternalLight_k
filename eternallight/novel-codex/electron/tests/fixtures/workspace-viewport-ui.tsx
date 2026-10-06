import React from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import { saveAuthor } from "../helpers/author-session";
import { layoutFrame, visibleElement } from "../helpers/page-geometry";
import "../../../src/styles.css";

const check = (value, message) => { if (!value) throw Error(message); };
const until = async (predicate, message) => { for (let i = 0; i < 100 && !predicate(); i++) await new Promise(resolve => setTimeout(resolve, 20)); check(predicate(), message); };
const projects = ["viewport-filled", "viewport-empty"].map((id, index) => ({ id, title: index ? "空白布局作品" : "码头来信：一部有长标题、复杂规划与多个人物的小说".repeat(5).slice(0, 120), logline: "在雾中的码头寻找失落的来信。".repeat(8), updated_at: "" }));
const body = Array.from({ length: 60 }, (_, index) => `码头🙂第${index}段，林夕读着来信。`).join("\n\n");
const chapter = { id: "viewport-chapter", number: 1, title: "写给明天的长篇来信", status: "accepted", summary: "", updated_at: "", passages: [] };
const scenes = Array.from({ length: 16 }, (_, index) => ({ id: `scene-${index}`, title: `码头场景${index} · ${"来信等待被打开".repeat(4)}`, location: "雾港北岸旧码头", time: `第${index}日拂晓`, summary: "人物面对尚未说明的冲突与回信。".repeat(8), status: "ready", beats: [{ id: `beat-${index}`, position: 1, description: "林夕第一次看见那封信。" }], participants: [{ id: "person-0", name: "林夕" }], stateChanges: [], canonConstraints: [] }));
const state = id => ({ project: projects.find(item => item.id === id), canon: {
  characters: id === projects[0].id ? Array.from({ length: 14 }, (_, index) => ({ id: `person-${index}`, name: `码头人物${index}·${"完整姓名".repeat(5)}`, role: "承担复杂且尚未解决的叙事职责".repeat(5), status: "已确认", color: "#4865a5" })) : [],
  relationships: [], events: [], locations: [], notes: [], plotThreads: id === projects[0].id ? scenes.map(item => ({ id: item.id, title: item.title, state: item.summary, tone: "blue" })) : [],
  worldRules: id === projects[0].id ? scenes.map(item => ({ id: item.id, statement: item.summary, severity: "作者确认" })) : [],
}, narrativeIr: { version: "1.0", scenes: id === projects[0].id ? scenes : [], anchors: [] }, novelEdition: { chapters: id === projects[0].id ? [chapter] : [] } });
const draft = { id: "published", chapterId: chapter.id, title: chapter.title, version: 1, body, sceneGroups: [{ id: "group-one", sceneId: scenes[0].id, body, locked: false }], sourcePlanRefs: { localOnly: true, lineageStatus: "current" }, source: "author", createdAt: null };
const revision = (kind, payload) => ({ id: `${kind}-one`, version: 1, parentRevisionId: null, baseProjectRevision: 1, payload, source: "author", createdAt: null });
const outline = revision("outline", { nodes: [{ id: "book-one", parentId: null, kind: "book", position: 1, title: "雾港与回信", purpose: "讲述人物面对自己的过去", summary: "人物在码头相遇。".repeat(12), dependencyKeys: [], certainty: "settled", protection: "free" }] });
const artifact = accepted => ({ candidate: null, accepted, working: null, workingSelection: null, locked: false });
const workspace = id => ({ project: state(id).project, revision: 1, headSnapshotId: "head", sourceDigest: "a".repeat(64), index: { status: "ready" }, artifacts: {
  brief: artifact(revision("brief", { premise: "寻找失落的来信。".repeat(18), promise: "人物会亲手决定是否回信。", stakes: "来信可能改变记忆。", endingDirection: "回到旧码头。" })), outline: artifact(outline),
  plan: artifact(revision("plan", { sourceOutlineRevisionId: outline.id, chapters: Array.from({ length: 8 }, (_, index) => ({ chapterPlanId: `plan-${index}`, primaryNodeId: "book-one", chapterNumber: index + 1, title: `第${index + 1}章长标题`, goal: "人物必须面对尚未解决的情节。".repeat(10), conflict: "来信中的叙述与人物的经历不一致。", dependencyKeys: [], scenes: [] })) })), identity: artifact(null), propertyRules: artifact(null),
}, chapters: id === projects[0].id ? [{ ...chapter, candidate: draft, accepted: draft, working: null, workingSelection: null }] : [], tasks: [], reviews: [], modeling: id === projects[0].id ? [{ objectId: `draft-${chapter.id}`, sourceRevisionId: draft.id, status: "ready", batchId: "batch", entities: [], assertions: [], confirmationState: "candidate" }] : [], factProposals: [], chapterPlanBindings: [], workingBaselines: [], continuationIntents: [], planningReviews: [], commits: [], impacts: [] });
const characterKnowledge = (projectId, entityId) => ({ projectId, snapshotId: null, perspective: "author", identityRevisionId: null,
  propertyRulesRevisionId: null, propertyRulesPolicy: "approvedStructuralRules", worldTime: null, epistemicTime: null, readingChapter: null, ownerId: null, visibleThroughChapter: null,
  selectors: { entityId, predicate: null }, entries: projectId === projects[0].id ? [{ sourceObjectId: `draft-${chapter.id}`, sourceRevisionId: draft.id, modelingBatchId: "batch", assertionIndex: Number(entityId.split("-").at(-1)), sourceChapter: 1,
    canonicalSubjectId: entityId, canonicalTargetId: null, canonicalEpistemicOwnerId: null, propositionRef: null, confirmationState: "candidate", confirmedFactId: null, relation: null,
    assertion: { kind: "assertion", subjectId: entityId, predicate: "已记录职责", value: state(projectId).canon.characters.find(person => person.id === entityId)?.role,
      evidence: { quote: body.split("\n\n")[0], start: 0, end: [...body.split("\n\n")[0]].length } } }] : [], findings: [], eventRuleCount: 0, eventConditionsPolicy: "authorRulesSourceClaims",
  coverage: { complete: true, pendingSources: [], truncatedSources: false, pendingSourcesCount: 0, pendingSourceIdsTruncated: false, matchingSourcesCount: 1, scannedSourcesCount: 1, semanticCompleteness: "unknown", unknownChapterSources: 0, truncatedEntries: false, unknownTimeSkipped: 0, unavailablePropositionSources: 0, findingsTruncated: false, uncertainConflictPairs: 0, unknownStateTimes: 0, unappliedPropertyRules: 0, confirmedEntries: 0, candidateEntries: projectId === projects[0].id ? 1 : 0 } });
let modelCalls = 0, writeCalls = 0, failDiff = false, holdDiff = false, releaseDiff, checkedDiffRecovery = false;
let eventListener, releaseDiscussion, allowDiscussion = false, discussionSends = 0, stopCalls = 0, proposalActions = [];
const longInstruction = "请先讨论人物为什么不肯回信，保持只读，不要修改故事。\n".repeat(28);
const streamedReply = "先讨论回信的动机，不修改正文。".repeat(35), replyTail = "接着讨论，仍然不写入。", copiedReplies = [];
Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: async text => { copiedReplies.push(text); } } });
const sendEvent = (method, params) => eventListener?.({ method, params });
const diffRequests = [];
const diffs = [{ id: "diff-one", chapter_id: chapter.id, fromText: "来信之前。".repeat(50), toText: body, summary: "作者保存了更长的章节正文", createdAt: null, addedChars: 100, removedChars: 0 }];
window.novelCodex = { app: { onCloseRequested: () => () => {} }, runtime: { listChapterStructureChanges: async (projectId: string) => ({ projectId, items: [], nextCursor: null }),
  getWorkCatalog: async () => ({ revision: 0, volumes: [], placements: [] }),
  status: async () => ({ state: "online", mode: "local", error: null }), listProjects: async () => ({ projects }), getState: async id => structuredClone(state(id)), getCreativeWorkspace: async id => structuredClone(workspace(id)),
  listWorkTasks: async () => ({ tasks: [], nextCursor: null, activeCount: 0 }), listGenerationRuns: async () => ({ runs: [] }), listPlanningImpacts: async () => ({ data: [], nextCursor: null, counts: { total: 0, open: 0 } }),
  getWriterPreview: async () => ({ target: { position: {} }, relevantText: [], plannedEffects: [], coverage: { complete: true, missingScope: [], truncationReasons: [] } }),
  getIdentityCatalog: async id => ({ entries: id === projects[0].id ? state(id).canon.characters.map(character => ({ id: character.id, canonicalId: character.id, kind: "character", resolutionState: "candidate", claims: [{ displayName: character.name, aliases: [], sourceObjectId: `draft-${chapter.id}`, sourceRevisionId: draft.id, evidence: { quote: "人物面对尚未说明的冲突与回信。".repeat(8), start: 0, end: 128 } }] })) : [], ambiguousNames: [], identityRevisionId: null, identityDecisions: [], truncated: false, ambiguityScanComplete: true }),
  getCharacterKnowledge: async (projectId, entityId) => structuredClone(characterKnowledge(projectId, entityId)),
  getHistoricalKnowledgeState: async id => ({ projectId: id, snapshotId: "head", entries: id === projects[0].id ? scenes.map((scene, index) => ({ canonicalSubjectId: "person-0", sourceObjectId: `draft-${chapter.id}`, sourceRevisionId: draft.id, modelingBatchId: "batch", assertionIndex: index, confirmationState: "candidate", assertion: { kind: index % 2 ? "event" : "world", predicate: "故事内容", value: scene.summary, storyTime: index, evidence: { quote: "林夕读着来信。" } } })) : [], coverage: { complete: true, truncatedEntries: 0, pendingSources: [] } }),
  listCommitments: async id => ({ snapshotId: "head", nextCursor: null, data: id === projects[0].id ? scenes.map(scene => ({ id: scene.id, summary: scene.title, status: "open", confirmationState: "candidate", sourceCurrent: true })) : [] }),
  getDiffs: async (projectId, chapterId) => { diffRequests.push({ projectId, chapterId }); if (failDiff) throw Error("夹具版本读取失败"); if (holdDiff) { holdDiff = false; await new Promise(resolve => { releaseDiff = resolve; }); } return { diffs: structuredClone(diffs) }; },
  ...Object.fromEntries(["Search", "Entity", "Relation", "Knowledge", "SnapshotKnowledge", "Fact"].map(kind => [`get${kind}Audit`, async () => ({ job: null })])),
  createProject: async () => { writeCalls++; throw Error("Unexpected project mutation"); },
}, agent: { status: async () => ({ configured: true, state: "ready", mode: "novel-agent-engine", error: null }), listConnections: async () => ({ connections: [] }), recoveryReport: async () => [], listProposedActions: async () => ({ actions: proposalActions }), listThreads: async () => ({ threads: [], nextCursor: null }), setWorkspaceContext: async () => {},
  onEvent: listener => { eventListener = listener; return () => { if (eventListener === listener) eventListener = null; }; }, onUserInputRequest: () => () => {},
  sendPrompt: async (projectId, prompt, repair, focus) => {
    if (!allowDiscussion) { modelCalls++; throw Error("Unexpected model request"); }
    check(projectId === projects[0].id && prompt === longInstruction.trim() && repair === null && focus.chapterId === chapter.id, "Local discussion lost author input or current chapter focus");
    discussionSends++; const threadId = `layout-thread-${discussionSends}`, turnId = `layout-turn-${discussionSends}`;
    await layoutFrame(); sendEvent("thread/started", { thread: { id: threadId } });
    await layoutFrame(); sendEvent("turn/started", { threadId, turn: { id: turnId } });
    await layoutFrame(); sendEvent("item/agentMessage/delta", { threadId, turnId, itemId: "layout-reply", delta: streamedReply });
    return new Promise(resolve => { releaseDiscussion = () => resolve({ message: "本地受控讨论已停止", proposedActions: proposalActions }); });
  }, interrupt: async () => { stopCalls++; releaseDiscussion(); return { interrupted: true }; },
} };
saveAuthor({ name: "布局作者", email: "viewport@example.invalid", passwordSalt: "fixture", passwordHash: "fixture" }); localStorage.setItem("novel-codex-active-project", projects[0].id);
createRoot(document.getElementById("root")!).render(<App />);
const views = [["chapters", "作品目录"], ["characters", "角色"], ["brief", "作品概况"], ["outline", "全书大纲"], ["plans", "近章规划"], ["plot", "伏笔与悬念"], ["timeline", "时间线"], ["world", "世界与规则"]];
const navigate = async (view, label) => { const target = view === "chapters" ? document.querySelector<HTMLButtonElement>('.sidebar-work-directory [data-directory-chapter-id][aria-current="page"]') ?? document.querySelector<HTMLButtonElement>('.sidebar-work-directory [data-directory-chapter-id]') : [...document.querySelectorAll<HTMLButtonElement>(".nav-item")].find(item => item.textContent.includes(label)); check(target, `Missing navigation ${label}`); target!.click(); await until(() => document.querySelector<HTMLElement>(".main-panel")?.dataset.view === view, `Navigation ${label} failed`); await layoutFrame(); };
const chooseProject = async index => { document.querySelector<HTMLButtonElement>(".project-switcher")!.click(); await layoutFrame(); [...document.querySelectorAll<HTMLButtonElement>(".project-menu-item")].find(item => item.querySelector("strong")?.textContent === projects[index].title)!.click(); await until(() => document.querySelector(".project-card strong")?.textContent === projects[index].title, "Project navigation did not settle"); await layoutFrame(); };
const outerGeometry = label => { const offenders = [...document.querySelectorAll<HTMLElement>(".main-panel *")].filter(item => item.getClientRects().length && item.getBoundingClientRect().right > innerWidth + 2).slice(0, 5).map(item => ({ tag: item.tagName, class: item.className, parent: item.parentElement.className, grandparent: item.parentElement.parentElement.className, right: Math.round(item.getBoundingClientRect().right) })); const layout = [...document.querySelectorAll<HTMLElement>(".app-shell,.workspace,.main-panel")].map(item => ({ class: item.className, height: Math.round(item.getBoundingClientRect().height), scrollHeight: item.scrollHeight, overflow: getComputedStyle(item).overflow, minHeight: getComputedStyle(item).minHeight })); check(document.scrollingElement.scrollWidth <= innerWidth + 2 && document.scrollingElement.scrollHeight <= innerHeight + 2 && scrollY === 0, `${label}: outer document escaped viewport ${document.scrollingElement.scrollWidth}×${document.scrollingElement.scrollHeight} vs ${innerWidth}×${innerHeight}, y=${scrollY}; ${JSON.stringify({ offenders, layout })}`); };
window.checkViewport = async mode => {
  await until(() => document.querySelector(".nav-item") && document.querySelector(".chapter-document"), "App initialization failed");
  if (document.documentElement.dataset.theme !== mode) { document.querySelector<HTMLButtonElement>(".theme-toggle")!.click(); await until(() => document.documentElement.dataset.theme === mode, "Theme did not switch"); }
  if (!checkedDiffRecovery) {
    const readRecords = async () => { [...document.querySelectorAll<HTMLButtonElement>(".main-panel button")].find(item => item.getAttribute("aria-label") === "查看修改记录" && item.checkVisibility()).click(); await until(() => document.querySelector(".module-records-popover:popover-open"), "Chapter records did not open"); };
    const closeRecords = async () => { document.querySelector<HTMLButtonElement>('.module-records-popover [aria-label="关闭修改记录"]').click(); await until(() => !document.querySelector(".module-records-popover"), "Chapter records did not close"); };
    failDiff = true; await readRecords(); await until(() => document.querySelector('.module-records-popover [role="alert"]')?.textContent.includes("夹具版本读取失败"), "Diff failure was disguised as empty");
    failDiff = false; [...document.querySelectorAll<HTMLButtonElement>(".module-records-popover button")].find(item => item.textContent === "重新读取修改对照").click(); await until(() => document.querySelector(".module-records-popover .diff-card"), "Diff retry did not recover");
    check(JSON.stringify(diffRequests.at(-1)) === JSON.stringify({ projectId: projects[0].id, chapterId: chapter.id }), "Chapter records read another source");
    await closeRecords(); holdDiff = true; await readRecords(); await until(() => releaseDiff, "Delayed diff read did not begin");
    check(document.querySelector('.module-records-popover .view-panel [role="status"]')?.textContent.includes("正在读取修改对照"), "Diff loading was invisible");
    await chooseProject(1); releaseDiff(); await layoutFrame(); check(!document.querySelector(".module-records-popover,.diff-card") && !document.querySelector(".main-panel").textContent.includes(diffs[0].summary), "Late diff response leaked into another project");
    await chooseProject(0); await navigate("chapters", "作品目录"); checkedDiffRecovery = true;
    for (const [view, label] of [["chapters", "作品目录"], ["characters", "角色"], ["outline", "全书大纲"]]) { document.querySelector<HTMLButtonElement>('[aria-label="打开用户设置"]').click(); await until(() => document.querySelector(".user-settings"), "Settings did not open for navigation"); await navigate(view, label); check(!document.querySelector(".user-settings"), "Settings remained open after navigation"); }
    await navigate("chapters", "作品目录");
  }
  check(!document.querySelector('.sidebar [aria-label="修改记录"], [role="group"][aria-label="选择查看内容"], .main-panel[data-view="diffs"]'), "Global records navigation survived");
  const referenceGroup = [...document.querySelectorAll(".nav-group")].find(group => group.querySelector(".nav-label")?.textContent === "参考");
  const referenceLabels = [...referenceGroup.querySelectorAll(".nav-item")].map(item => item.getAttribute("aria-label"));
  check(JSON.stringify(referenceLabels) === JSON.stringify(["伏笔与悬念", "时间线", "世界与规则"]), "Reference navigation misrepresented commitments as plot planning");
  const pages = [];
  const projectButton = document.querySelector<HTMLElement>(".project-switcher")!, projectBounds = projectButton.getBoundingClientRect(); check(projectButton.querySelector<HTMLElement>("span[title]")?.title === projects[0].title && projectBounds.left >= 0 && projectBounds.right <= innerWidth, "Long project name lost its full accessible title or escaped toolbar");
  for (const [view, label] of [...views, ["account", "用户设置"]]) {
    if (view === "account") { document.querySelector<HTMLButtonElement>('[aria-label="打开用户设置"]')!.click(); await until(() => document.querySelector(".user-settings"), "Settings did not mount"); await layoutFrame(); } else await navigate(view, label);
    const main = document.querySelector<HTMLElement>(".main-panel")!; outerGeometry(label);
    if (view === "characters") { await until(() => main.querySelector(".character-biography li") && main.querySelector(".character-storyline li") && !main.querySelector<HTMLButtonElement>(".character-profile-heading button")?.disabled, "Current character profile did not load"); check(!main.querySelector('[role="alert"]'), "Character viewport used an error in place of the current profile"); await layoutFrame(); }
    const box = main.getBoundingClientRect(); check(box.width >= 350 && box.left >= 0 && box.right <= innerWidth, `${label}: central pane collapsed`);
    check(main.scrollWidth <= main.clientWidth + 2, `${label}: horizontal overflow (${main.scrollWidth}/${main.clientWidth})`);
    if (view !== "chapters") check(main.querySelectorAll("h1").length === 1, `${label}: missing or duplicate primary heading`);
    if (view === "outline" || view === "plans") { const manual = main.querySelector<HTMLDetailsElement>(".planning-manual-editor")!; check(!manual.open && !main.querySelector<HTMLElement>(view === "outline" ? ".outline-node input" : ".plan-field textarea")!.checkVisibility(), "Planning editor competed with default reading"); manual.querySelector("summary")!.click(); await layoutFrame(); }
    if (view === "plans") { main.querySelector<HTMLDetailsElement>(".plan-advanced")!.querySelector("summary")!.click(); await layoutFrame(); }
    if (view === "chapters") {
      const beforeEditor = main.querySelector(".chapter-document textarea");
      const trigger = [...main.querySelectorAll<HTMLButtonElement>("button")].find(item => item.getAttribute("aria-label") === "查看修改记录" && item.checkVisibility()); trigger.focus({ preventScroll: true }); trigger.click();
      await until(() => document.querySelector(".module-records-popover .diff-card"), "Current chapter records did not load");
      const dialog = document.querySelector<HTMLDivElement>(".module-records-popover"); const summary = dialog.querySelector(".diff-card summary");
      check(dialog.matches(":popover-open") && dialog.getAttribute("aria-label").includes("第 1 章") && main.dataset.view === "chapters" && main.querySelector(".chapter-document textarea") === beforeEditor, "Records changed chapter editor/context");
      check(summary.textContent === diffs[0].summary, "Missing timestamp was rendered as a fabricated date"); summary.click(); await layoutFrame();
      const bounds = dialog.getBoundingClientRect(); const mainBounds = main.getBoundingClientRect(); check(!dialog.hasAttribute("aria-modal") && !main.inert && bounds.width <= 441 && (innerWidth <= 640 || mainBounds.width < 320 || (bounds.left >= mainBounds.left && bounds.right <= mainBounds.right + 2)), "Records were modal or escaped the current editing column"); check(bounds.left >= 0 && bounds.right <= innerWidth + 2 && bounds.top >= 0 && bounds.bottom <= innerHeight + 2 && dialog.scrollWidth <= dialog.clientWidth + 2, "Module records popover escaped viewport");
      for (const element of [...dialog.querySelectorAll<HTMLElement>(".diff-columns del,.diff-columns ins")]) { const rgb = color => color.match(/[\d.]+/g).slice(0, 3).map(Number).map(value => value / 255).map(value => value <= .04045 ? value / 12.92 : ((value + .055) / 1.055) ** 2.4); const luminance = color => { const [r, g, b] = rgb(color); return .2126 * r + .7152 * g + .0722 * b; }; const style = getComputedStyle(element); const foreground = luminance(style.color), background = luminance(style.backgroundColor); const ratio = (Math.max(foreground, background) + .05) / (Math.min(foreground, background) + .05); check(ratio >= 4.5, `Modification comparison text has insufficient theme contrast ${mode}/${element.tagName}: ${style.color} on ${style.backgroundColor}, ratio=${ratio}`); }
      const recordsBody = dialog.querySelector<HTMLElement>(".module-records-body"); recordsBody.scrollTop = recordsBody.scrollHeight; await layoutFrame();
      const closeBox = dialog.querySelector("header button").getBoundingClientRect(); check(closeBox.top >= bounds.top && closeBox.bottom <= bounds.bottom, "Long record scrolling hid the close button");
      const lastSavedText = dialog.querySelector<HTMLElement>(".diff-columns ins").getBoundingClientRect();
      check(lastSavedText.bottom <= bounds.bottom + 2 && lastSavedText.bottom > bounds.top, "Last saved chapter comparison line was unreachable in module records");
      dialog.querySelector<HTMLButtonElement>('[aria-label="关闭修改记录"]').click(); await until(() => !document.querySelector(".module-records-popover"), "Records did not close");
      check(document.activeElement === trigger && main.querySelector(".chapter-document textarea") === beforeEditor, "Record close lost editor or exact trigger focus");
    }
    const last = [...main.querySelectorAll<HTMLElement>("button, textarea, input, p, summary, del, ins, .list-row, .big-character-card")].filter(visibleElement).at(-1); check(last, `${label}: missing page content`);
    main.scrollTop = main.scrollHeight; await layoutFrame(); const lastBox = last.getBoundingClientRect(); check(lastBox.bottom <= box.bottom + 2 && lastBox.bottom > box.top, `${label}: final page content unreachable ${JSON.stringify({ element: last.tagName, text: last.textContent?.slice(0, 100), top: lastBox.top, bottom: lastBox.bottom, panelTop: box.top, panelBottom: box.bottom, scrollTop: main.scrollTop, scrollHeight: main.scrollHeight, height: main.clientHeight })}`);
    for (const element of [...main.querySelectorAll<HTMLElement>("input:not([type=checkbox]):not([type=radio]), textarea, select")].filter(visibleElement)) { const bounds = element.getBoundingClientRect(); check(bounds.width >= (element.matches('input[type="number"]') ? 40 : 100) && bounds.left >= box.left - 2 && bounds.right <= box.right + 2, `${label}: clipped author control ${element.getAttribute("aria-label") ?? element.tagName} (${bounds.width})`); }
    outerGeometry(label); pages.push({ view, fitsHorizontally: true, finalContentReachable: true });
    if (view === "account") [...main.querySelectorAll<HTMLButtonElement>("button")].find(item => item.textContent === "返回作品")!.click();
  }
  await chooseProject(1);
  for (const [view, label] of views.filter(([view]) => ["characters", "world"].includes(view))) { await navigate(view, label); await until(() => document.querySelector(".main-panel .empty-state"), `${label}: successful empty state is blank`); outerGeometry(label); }
  await chooseProject(0); await navigate("chapters", "作品目录");
  for (const selector of [".sidebar", ".conversation-rail-scroll"]) { const pane = document.querySelector<HTMLElement>(selector)!; pane.scrollTop = pane.scrollHeight; await layoutFrame(); check(pane.scrollWidth <= pane.clientWidth + 2, `${selector}: horizontal overflow`); outerGeometry(selector); }
  const conversation = await checkConversationGeometry();
  check(modelCalls === 0 && writeCalls === 0, "Layout navigation invoked model or mutation");
  return { mode, width: innerWidth, height: innerHeight, canvas: getComputedStyle(document.body).backgroundColor, documentScroll: scrollY, pages, emptyReferencePages: 2, moduleRecords: { chapterBound: true, retry: true, foreignLateReadRejected: true, absentTimePreserved: true, editorPreserved: true, exactTriggerFocus: true }, conversation, modelCalls, writeCalls };
};

async function checkConversationGeometry() {
  const rail = document.querySelector<HTMLElement>(".conversation-rail"), scroller = rail.querySelector<HTMLElement>(".conversation-rail-scroll"),
    inspector = rail.querySelector<HTMLElement>("#conversation-inspector"), toggle = rail.querySelector<HTMLButtonElement>('[aria-controls="conversation-inspector"]'),
    input = rail.querySelector<HTMLTextAreaElement>('textarea[aria-label="创作指令"]');
  const fill = value => { Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(input, value); input.dispatchEvent(new Event("input", { bubbles: true })); };
  const beforeSends = discussionSends, beforeStops = stopCalls;
  check(rail.querySelectorAll(".conversation-shell-header").length === 1 && toggle.getAttribute("aria-expanded") === "false", "Conversation shell lacked a unique closed details entry");
  check(inspector.hidden && inspector.getBoundingClientRect().height === 0 && !inspector.checkVisibility()
    && !rail.querySelector('[aria-label="调整功能区高度"]'), "Hidden inspector still reserved space or exposed its sash");
  const closedHeight = scroller.getBoundingClientRect().height;
  toggle.focus(); toggle.click(); await layoutFrame();
  check(rail.querySelector("#conversation-inspector") === inspector && !inspector.hidden && inspector.checkVisibility()
    && rail.querySelector('[aria-label="调整功能区高度"]') && toggle.getAttribute("aria-expanded") === "true", "Inspector toggle replaced DOM or hid controls");
  check(closedHeight > scroller.getBoundingClientRect().height + 100, "Closing task details did not return space to the conversation");
  toggle.click(); await layoutFrame(); check(document.activeElement === toggle && inspector.hidden, "Closing details lost its keyboard launcher");
  fill("两行以内的未发送构思"); await layoutFrame(); const shortHeight = input.getBoundingClientRect().height;
  check(shortHeight >= 48 && shortHeight <= 49, `Short composer was not two rows: ${shortHeight}`);
  input.focus(); input.setSelectionRange(2, 6); input.dispatchEvent(new CompositionEvent("compositionstart", { bubbles: true }));
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", isComposing: true, bubbles: true, cancelable: true }));
  input.dispatchEvent(new CompositionEvent("compositionend", { bubbles: true })); await layoutFrame();
  check(discussionSends === beforeSends && input.value === "两行以内的未发送构思" && input.selectionStart === 2 && input.selectionEnd === 6, "IME Enter sent or changed the draft/selection");
  fill(longInstruction); input.setSelectionRange(9, 18); await layoutFrame(); const longHeight = input.getBoundingClientRect().height;
  check(longHeight >= 143 && longHeight <= 144 && input.scrollHeight > input.clientHeight && getComputedStyle(input).overflowY === "auto", `Long composer escaped its six-row limit: ${longHeight}`);
  check(input === rail.querySelector('textarea[aria-label="创作指令"]') && input.selectionStart === 9 && input.selectionEnd === 18, "Autoheight replaced the textarea or moved its caret");
  input.scrollTop = input.scrollHeight; const draftScroll = input.scrollTop;
  toggle.click(); await layoutFrame(); toggle.click(); await layoutFrame();
  check(input.value === longInstruction && input.selectionStart === 9 && input.selectionEnd === 18 && input.scrollTop === draftScroll, "Task details toggle lost the author draft/caret/scroll");
  check(scroller.getBoundingClientRect().height >= 200, "Conversation had fewer than 200 readable pixels with details closed");
  allowDiscussion = true; proposalActions = [];
  rail.querySelector<HTMLButtonElement>('[aria-label="发送创作指令"]').click();
  await until(() => releaseDiscussion && discussionSends === beforeSends + 1 && rail.querySelector('[aria-label="停止当前执行"]') && rail.querySelector(".conversation-answer"), "Actual App local stream did not start");
  const request = [...rail.querySelectorAll<HTMLDetailsElement>(".user-message .conversation-request")].at(-1);
  await until(() => request?.dataset.overflow === "true", "Long user instruction was not measured for folding");
  check(!request.open && parseFloat(getComputedStyle(request.querySelector(".conversation-request-preview")).webkitLineClamp) === 4, "Long instruction did not begin with a four-line preview");
  request.querySelector<HTMLElement>("summary").click(); await until(() => request.open, "Full instruction did not expand");
  check(request.querySelector("p").textContent === longInstruction.trim() && request.querySelector("p").innerText.trimEnd() === longInstruction.trim() && request.querySelector("p").checkVisibility(), "Expanded author instruction lost its original text");
  const detailKey = request.dataset.conversationDetail;
  const threadId = `layout-thread-${discussionSends}`, turnId = `layout-turn-${discussionSends}`;
  sendEvent("item/agentMessage/delta", { threadId, turnId, itemId: "layout-reply", delta: replyTail }); await layoutFrame();
  const currentActions = [...rail.querySelectorAll<HTMLElement>(".conversation-turn")].at(-1).querySelectorAll<HTMLButtonElement>('.conversation-reply-actions [aria-label="复制回复"]');
  check(currentActions.length === 1 && currentActions[0].checkVisibility(), "Current App reply footer was missing, duplicated or hidden in metadata"); currentActions[0].click(); await layoutFrame();
  check(copiedReplies.at(-1) === streamedReply + replyTail, "Current App copy used rendered/trimmed content instead of the original reply");
  check([...rail.querySelectorAll<HTMLDetailsElement>(".user-message .conversation-request")].at(-1) === request && request.open && request.dataset.conversationDetail === detailKey, "Streaming replaced the expanded author disclosure");
  const stop = rail.querySelector<HTMLButtonElement>('[aria-label="停止当前执行"]'), stopBounds = stop.getBoundingClientRect(), inputBounds = input.getBoundingClientRect();
  check(stopBounds.top >= 0 && stopBounds.bottom <= innerHeight && inputBounds.bottom <= innerHeight && stop.checkVisibility() && !stop.disabled, "Long input/stream made Stop unreachable");
  check(rail.scrollWidth <= rail.clientWidth + 2 && scroller.scrollWidth <= scroller.clientWidth + 2, "Actual conversation overflowed its rail");
  proposalActions = [{ id: `action-${"a".repeat(32)}`, sourceTaskId: `task-${"b".repeat(32)}`, operation: "generateOutline", target: { type: "project" }, instruction: longInstruction, status: "proposed", inputRevision: 1, intentRevision: 0 }];
  stop.click(); await until(() => stopCalls === beforeStops + 1 && !rail.querySelector('[aria-label="停止当前执行"]') && rail.querySelector(".conversation-proposal .conversation-request"), "Local Stop did not settle or reveal the controlled proposal");
  const proposal = rail.querySelector<HTMLDetailsElement>(".conversation-proposal .conversation-request");
  await until(() => proposal.dataset.overflow === "true", "Long proposal did not fold"); proposal.querySelector<HTMLElement>("summary").click(); await layoutFrame();
  check(proposal.open && proposal.querySelector("p").textContent === longInstruction && proposal.querySelector("p").innerText.trimEnd() === longInstruction.trimEnd() && proposal.querySelector("p").checkVisibility(), "Proposal disclosure lost its complete text");
  proposal.querySelector<HTMLElement>("summary").click(); request.querySelector<HTMLElement>("summary").click();
  releaseDiscussion = null; proposalActions = []; fill(longInstruction); await layoutFrame(); rail.querySelector<HTMLButtonElement>('[aria-label="发送创作指令"]').click();
  await until(() => discussionSends === beforeSends + 2 && releaseDiscussion && rail.querySelector('[aria-label="停止当前执行"]'), "Second local turn did not promote the previous reply to history");
  const historyActions = [...rail.querySelectorAll<HTMLElement>(".conversation-turn")].slice(0, -1).flatMap(turn => [...turn.querySelectorAll<HTMLButtonElement>('.conversation-reply-actions [aria-label="复制回复"]')]);
  check(historyActions.length >= 1 && historyActions.at(-1).checkVisibility(), "EarlierTurn had no reachable reply footer"); historyActions.at(-1).click(); await layoutFrame();
  check(copiedReplies.at(-1) === streamedReply + replyTail, "EarlierTurn copy lost the original Markdown");
  rail.querySelector<HTMLButtonElement>('[aria-label="停止当前执行"]').click(); await until(() => stopCalls === beforeStops + 2 && !rail.querySelector('[aria-label="停止当前执行"]'), "Second local turn did not stop");
  allowDiscussion = false; releaseDiscussion = null; fill(""); await layoutFrame(); outerGeometry("conversation controls");
  return { inspectorMountedHidden: true, detailsRestoredSpace: true, shortInputHeight: Math.round(shortHeight), longInputHeight: Math.round(longHeight), inputCaretAndScrollPreserved: true, imeDidNotSend: true, userAndProposalFullText: true, stableDisclosure: true, stopReachable: true, currentAndHistoryOriginalCopy: true, localDiscussionSends: discussionSends - beforeSends, localStopCalls: stopCalls - beforeStops, providerRequests: 0 };
}
