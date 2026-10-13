import { novelFixtureMedia } from "../helpers/novel-edition";
import React from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import "../../../src/styles.css";
import { saveAuthor } from "../helpers/author-session";
import { chapterRuntimeReads } from "../helpers/chapter-runtime";
const tick = () => new Promise(resolve => setTimeout(resolve, 20));
let renderError;
window.addEventListener("error", event => { renderError = event.error ?? event.message; });
const check = (value, message) => { if (!value) throw Error(message); };
const until = async (predicate, message) => { for (let i = 0; i < 220 && !predicate(); i++) { if (renderError) throw renderError; await tick(); } check(predicate(), message); };
const buttons = label => [...document.querySelectorAll<HTMLButtonElement>("button")].filter(node => node.textContent === label || node.getAttribute("aria-label") === label);
const click = async label => { const node = buttons(label)[0]; check(node && !node.disabled, `Unavailable ${label}`); node.click(); await tick(); };
const composer = () => document.querySelector<HTMLTextAreaElement>('[aria-label="创作指令"]');
const input = async value => { Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(composer(), value); composer().dispatchEvent(new Event("input", { bubbles: true })); await tick(); };
const navigate = async (label, view) => { const node = () => view === "chapters" ? document.querySelector<HTMLButtonElement>("[data-directory-chapter-id]") : [...document.querySelectorAll<HTMLButtonElement>(".nav-item")].find(node => node.textContent.includes(label)); await until(() => node() && !node().disabled, "Navigation ready " + view); node()!.click(); await until(() => document.querySelector<HTMLElement>(".main-panel")?.dataset.view === view, `Missing ${view}`); await tick(); await tick(); };
const projects = ["project-last-lamp", "project-b"].map(id => ({ id, title: id, logline: "", updated_at: "" }));
const projectFor = id => projects.find(project => project.id === id);
const chapters = [{ id: "chapter-01", number: 1, title: "旧钟", status: "draft", summary: "", updated_at: "", passages: [] }];
const baseReads = chapterRuntimeReads(projectFor);
const sourceId = id => `${id}-draft`;
const workspace = async id => ({ ...await baseReads.getCreativeWorkspace(id), headSnapshotId: `snapshot-${id}`, chapters: chapters.map(chapter => ({ ...chapter, candidate: { id: sourceId(id), title: "旧钟", version: 2, body: "相同摘录。\n\n---\n\n相同摘录。", sourcePlanRefs: { localOnly: true, lineageStatus: "current" }, sceneGroups: ["scene-one", "scene-two"].map(scene => ({ id: scene, body: "相同摘录。", locked: false })) }, accepted: null, working: null, workingSelection: null })) });
const state = id => ({ project: projectFor(id), canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1", scenes: [], anchors: [] }, novelEdition: { chapters } });
const detail = (id, commitmentId, snapshotId) => ({ projectId: id, snapshotId, revisionId: `revision-${commitmentId}`, sourceCurrent: true, confirmationState: "candidate", confirmationCommitId: null, authorControls: { projectRevision: 1, undoAvailable: false }, extractionOrigin: { chapterId: "chapter-01", reportRevisionId: "report-original", proposalId: "proposal-one" }, payload: { id: commitmentId, summary: `准确伏笔 ${commitmentId}`, kind: "foreshadowing", status: "open", fulfillmentCriteria: "后文解释", setupEvidenceRefs: [{ objectId: "draft-chapter-01", revisionId: sourceId(id), quote: "相同摘录。", start: 0, end: 5 }], fulfillmentEvidenceRefs: [] } });
const calls = { sends: [], writes: 0 }, snapshots = [];
let failSend = true, releaseDetail, holdDetail = false, onEvent;
window.novelCodex = { multimedia: novelFixtureMedia,
 app: { onCloseRequested: () => () => {} },
 runtime: { ...baseReads, status: async () => ({ state: "online", mode: "local", error: null }), listProjects: async () => ({ projects }), getState: async id => state(id), getCreativeWorkspace: workspace,
  getWriterPreview: async () => ({ target: { position: {} }, plannedEffects: [], relevantText: [], coverage: { complete: true, missingScope: [], truncationReasons: [] } }),
  getHistoricalKnowledgeState: async (id, snapshotId) => ({ projectId: id, snapshotId, entries: [0, 1].map(index => ({ modelingBatchId: `batch-${index}`, assertionIndex: index, sourceObjectId: "draft-chapter-01", sourceRevisionId: sourceId(id), confirmationState: "candidate", assertion: { kind: "world", predicate: "钟声", value: true, evidence: { quote: index ? "第二个独立原文。" : "第一个独立原文。" } } })), coverage: { complete: true, truncatedEntries: false, pendingSources: [] } }),
  listCommitments: async id => ({ snapshotId: `snapshot-${id}`, nextCursor: null, data: ["one", "two"].map(key => ({ id: key, revisionId: `revision-${key}`, summary: `准确伏笔 ${key}`, kind: "foreshadowing", status: "open", confirmationState: "candidate", sourceCurrent: true })) }),
  getCommitment: async (id, key, snapshot = `snapshot-${id}`) => { if (holdDetail) { holdDetail = false; await new Promise(resolve => { releaseDetail = resolve; }); } return detail(id, key, snapshot); },
  getCommitmentExtraction: async () => null,
  establishCommitment: async () => { calls.writes++; throw Error("Unexpected establish"); }, createCommitmentReview: async () => { calls.writes++; throw Error("Unexpected review"); }, acceptReview: async () => { calls.writes++; throw Error("Unexpected accept"); },
 },
 agent: { status: async () => ({ configured: true, state: "ready", mode: "novel-agent-engine", provider: "fixture", model: "fixture", error: null }), listConnections: async () => ({ connections: [] }), recoveryReport: async () => [], listExecutions: async () => [], readMessage: async () => { throw Error("message receipt not found"); }, beginInterlude: async () => { throw Error("unexpected interlude"); }, returnInterlude: async () => { throw Error("unexpected interlude return"); }, listMessages: async () => [], listThreads: async () => ({ threads: [], nextCursor: null }), listProposedActions: async () => ({ actions: [] }), setWorkspaceContext: async () => {}, onEvent: listener => { onEvent = listener; return () => {}; }, onUserInputRequest: () => () => {},
  readConversation: async (projectId, threadId) => ({ thread: { id: threadId, projectId, name: "选区讨论", status: "idle", version: 0, isArchived: false }, turns: [] }),
  sendPrompt: async (...args) => { calls.sends.push(structuredClone(args));
    // This test retries a Host-accepted message, not a preflight rejection.
    const requestId = `request-${calls.sends.length}`;
    onEvent({ method: "agent/execution-request", params: { projectId: args[0], requestId } });
    onEvent({ method: "agent/message-updated", params: { projectId: args[0], messageId: requestId, status: "queued" } });
    if (failSend) { failSend = false; throw Error("controlled temporary failure"); } const id = `turn-${calls.sends.length}`; onEvent({ method: "agent/workflow", params: { workflow: { task: { id: `task-${calls.sends.length}`, projectId: args[0], threadId: "fixture-thread", operation: "advisory", target: {} } } } }); onEvent({ method: "turn/started", params: { threadId: "fixture-thread", turnId: id, turn: { id } } }); await tick(); onEvent({ method: "turn/completed", params: { threadId: "fixture-thread", turnId: id, turn: { id, status: "completed", items: [] } } }); return { message: "已读取准确来源", proposedActions: [] }; } },
};
const chooseProject = async index => { document.querySelector<HTMLButtonElement>(".project-switcher")!.click(); await tick(); document.querySelectorAll<HTMLButtonElement>(".project-menu-item")[index + 1].click(); await until(() => document.querySelector(".project-switcher")?.textContent.includes(projects[index].id), "Project switch"); await tick(); await tick(); };

const quoteCards = () => [...document.querySelectorAll(".discussion-excerpt")].filter(node => !node.closest(".conversation-turn"));
const change = async (element, value) => { const prototype = element.tagName === "TEXTAREA" ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype; Object.getOwnPropertyDescriptor(prototype, "value").set.call(element, value); element.dispatchEvent(new Event("input", { bubbles: true })); await tick(); };
const originalRead = window.novelCodex.runtime.getCreativeWorkspace;
window.novelCodex.runtime.getCreativeWorkspace = async id => {
 const value = await originalRead(id);
 value.chapters[0].candidate.sceneGroups[1].locked = true;
 value.artifacts.brief.candidate = { id: "brief-candidate", version: 3, payload: { premise: "新的核心构想", promise: "悬疑", stakes: "记忆", endingDirection: "归来" } };
 value.artifacts.brief.accepted = { id: "brief-accepted", version: 2, payload: { premise: "已确认核心构想", promise: "悬疑", stakes: "记忆", endingDirection: "归来" } };
 value.artifacts.outline.candidate = { id: "outline-v3", version: 3, payload: { nodes: [{ id: "book-one", kind: "book", position: 0, title: "钟城故事", purpose: "恢复记忆", summary: "钟声消失之后，守钟人踏上旅途。", dependencyKeys: [], certainty: "open", protection: "free" }] } };
 value.artifacts.plan.candidate = { id: "plan-v3", version: 3, payload: { chapters: [{ chapterPlanId: "plan-one", chapterNumber: 1, title: "消失的钟声", goal: "发现钟声消失的原因", conflict: "旧港拒绝说出真相", writerGuidance: "保持悬念", scenes: [] }] } };
 return value;
};
window.novelCodex.runtime.getIdentityCatalog = async id => ({ entries: [{ id: "person-one", canonicalId: "person-one", kind: "character", resolutionState: "provisional", claims: [{ displayName: "守钟人", aliases: [], sourceObjectId: "draft-chapter-01", sourceRevisionId: sourceId(id), evidence: { quote: "守钟人在旧港生活。" } }] }], ambiguousNames: [], identityRevisionId: "identity-one", identityDecisions: [], truncated: false, ambiguityScanComplete: true });
window.novelCodex.runtime.getCharacterKnowledge = async (id, entityId) => ({ projectId: id, perspective: "author", snapshotId: null, selectors: { entityId, predicate: null }, identityRevisionId: "identity-one", entries: [], coverage: { complete: true, truncatedEntries: false, pendingSources: [] } });
const historical = window.novelCodex.runtime.getHistoricalKnowledgeState;
window.novelCodex.runtime.getHistoricalKnowledgeState = async (id, snapshotId) => { const value = await historical(id, snapshotId); value.entries.push({ ...value.entries[0], modelingBatchId: "event-batch", assertion: { kind: "event", predicate: "钟声消失", storyTime: 1, evidence: { quote: "昨日钟声突然消失。" } } }); return value; };
const selectEditor = async (element, start = 0, end = element.value.length) => {
 element.scrollIntoView({ block: "center" }); element.focus(); element.setSelectionRange(start, end);
 // Native selection can scroll a long textarea. Let its scroll dismissal settle
 // before emitting the final selection event, as a completed drag would do.
 await tick(); await tick(); element.dispatchEvent(new Event("select", { bubbles: true }));
 await until(() => document.querySelector(".selection-discussion-entry"), "Missing input selection entry");
};
const selectText = async element => {
 await until(() => element?.textContent.trim(), "Missing rendered source");
 element.scrollIntoView({ block: "center" }); await tick();
 document.activeElement?.blur();
 const range = document.createRange(); range.selectNodeContents(element); const selection = document.getSelection(); selection.removeAllRanges(); selection.addRange(range);
 await until(() => document.querySelector(".selection-discussion-entry"), "Missing DOM selection entry");
};
const add = async () => { const button = document.querySelector<HTMLButtonElement>(".selection-discussion-entry button")!; const box = button.getBoundingClientRect(); check(box.left >= 0 && box.right <= innerWidth && box.top >= 0 && box.bottom <= innerHeight, "Selection entry outside viewport"); button.click(); await tick(); };
const remove = async () => { await until(() => quoteCards().at(-1)?.querySelector<HTMLButtonElement>("button")?.disabled === false, "Quote removable after submission"); const before = quoteCards().length; quoteCards().at(-1)?.querySelector<HTMLButtonElement>("button")!.click(); await until(() => quoteCards().length === before - 1, "Quote removal"); };
const root = createRoot(document.getElementById("root"));
(async () => { try {
 localStorage.clear(); saveAuthor({ name: "选区作者", email: "selection@example.invalid", passwordSalt: "fixture", passwordHash: "fixture" }); root.render(<App />);
 await until(() => composer() && document.querySelectorAll(".draft-editor").length === 2, "App startup");
 await input("保留我输入的问题");
 const editors = () => [...document.querySelectorAll<HTMLTextAreaElement>(".draft-editor")];
 await selectEditor(editors()[0], 0, 4); await add();
 check(composer().value === "保留我输入的问题" && quoteCards().length === 1, "Adding quote overwrote question");
 check(quoteCards()[0].textContent.includes("相同摘录"), "Wrong quoted text");
 const locked = editors()[1]; check(locked.readOnly && !locked.disabled, "Protected text not selectable");
 await selectEditor(locked, 0, 4); locked.dispatchEvent(new KeyboardEvent("keydown", { key: "l", ctrlKey: true, bubbles: true, cancelable: true })); await tick();
 check(quoteCards().length === 2, "Ctrl+L did not add protected source");
 check(quoteCards()[0].textContent !== quoteCards()[1].textContent, "Equal text lost scene identity");
 await remove();
 const pages = [
  ["作品概况", "brief", ".overview-direction-summary dd", "brief-accepted"],
  ["全书大纲", "outline", ".outline-reading-node dd", "outline-v3"],
  ["近章规划", "plans", ".planning-reading-card dd", "plan-v3"],
  ["角色", "characters", ".character-profile-heading h2", null],
  ["世界与规则", "world", ".story-reference-card blockquote", null],
  ["伏笔与悬念", "plot", ".story-reference-card header strong", null],
  ["时间线", "timeline", ".story-reference-card blockquote", null],
 ];
 for (const [label, view, selector, revisionId] of pages) {
  await navigate(label, view); await until(() => document.querySelector(selector), "Missing content on " + view);
  await selectText(document.querySelector(selector)); await add();
  check(quoteCards().length === pages.indexOf(pages.find(row => row[1] === view)) + 2, "Quote missing for " + view);
  if (revisionId) check(quoteCards().at(-1).querySelector("small").title === revisionId, "Wrong revision on " + view);
 }
 check(quoteCards().length === 8, "All document types not quoted");
 await chooseProject(1); check(!quoteCards().length && !document.querySelector(".selection-discussion-entry"), "Quote leaked to project B");
 await chooseProject(0); check(quoteCards().length === 8 && composer().value === "保留我输入的问题", "Project A quote draft not restored");
 await until(() => !buttons("发送创作指令")[0].disabled, "Ready send"); await click("发送创作指令");
 await until(() => document.querySelector(".composer-retry"), "Expected controlled failure");
 const original = calls.sends[0][3]; check(original.excerpts.length === 8 && original.excerpts[0].text === "相同摘录", "IPC lost original quotes");
 check(document.querySelectorAll(".conversation-turn .discussion-excerpt").length === 8, "Sent quote sources missing from conversation");
 check(original.excerpts[0].locator.includes("scene-one"), "Missing exact scene locator");
 check(original.excerpts.map(item => item.view).join(",") === "chapters,brief,outline,plans,characters,world,plot,timeline", "Not all views sent");
 await navigate("全书大纲", "outline"); await until(() => document.querySelector(".outline-reading-node dd"), "Outline again");
 await selectText(document.querySelector(".outline-reading-node dd")); await add();
 const retryDraft = composer().value;
 document.querySelector<HTMLButtonElement>(".composer-retry")!.click(); await until(() => calls.sends.length === 2 && !buttons("停止当前执行").length && !composer().disabled, "Retry finished");
 check(JSON.stringify(calls.sends[1][3]) === JSON.stringify(original), "Retry replaced captured quotes with current page");
 check(quoteCards().length === 1 && composer().value === retryDraft, "Retry consumed next quote draft");
 await remove();
 await navigate("作品概况", "brief"); await click("查看修改记录");
 await until(() => document.querySelector('.module-records-popover .planning-record-content dd'), "Saved version content");
 await selectText(document.querySelector('.module-records-popover .planning-record-content dd')); await add();
 await until(() => !document.querySelector('.module-records-popover') && document.activeElement === composer(), "Version quote did not close overlay and focus composer");
 check(quoteCards()[0].querySelector('small').title === "brief-candidate" && quoteCards()[0].textContent.includes("新的核心构想"), "Saved version quote replaced with accepted content");
 await remove(); await navigate("作品目录", "chapters"); await until(() => editors().length === 2, "Chapter reload");
 const editor = editors()[0]; await change(editor, "本地未保存的新正文。"); await selectEditor(editor, 0, editor.value.length); await add();
 check(quoteCards()[0].textContent.includes("未保存的编辑内容") && composer().value === "", "Unsaved quote supplied an implicit instruction");
 await input("先分析这段未保存正文，暂时不要修改。");
 check(!buttons("发送创作指令")[0].disabled, "Unsaved quote discussion blocked");
 const localValue = editor.value; failSend = true; await click("发送创作指令"); await until(() => calls.sends.length === 3 && document.querySelector(".composer-retry"), "Unsaved controlled failure");
 check(calls.sends[2][3].readOnly === true && calls.sends[2][3].excerpts[0].text === localValue, "Unsaved text not frozen read-only");
 document.querySelector<HTMLButtonElement>(".composer-retry")!.click(); await until(() => calls.sends.length === 4 && !buttons("停止当前执行").length && !editor.matches(":disabled"), "Unsaved retry finished");
 check(JSON.stringify(calls.sends[3][3]) === JSON.stringify(calls.sends[2][3]), "Unsaved retry lost read-only capture");
 check(editor.isConnected && editor.value === localValue && calls.writes === 0, "Discussion overwrote local editor");
 await change(editor, "超".repeat(12001)); await selectEditor(editor); await add();
 check(!quoteCards().length && editor.value.length === 12001 && document.querySelector(".dialog-error")?.textContent.includes("引用内容过长"), "Oversize quote truncated or accepted");
 await change(editor, "会变化的选区"); await selectEditor(editor); Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(editor, "点击前已改变");
 await add(); check(!quoteCards().length, "Changed selection attached old text");
 await input("对话框自身不可引用"); composer().focus(); composer().select(); composer().dispatchEvent(new Event("select", { bubbles: true })); await tick(); await tick();
 check(!document.querySelector(".selection-discussion-entry"), "Composer selection leaked as document quote");
 // Layout and visual capture use a final valid quote.
 await selectEditor(editor); await add();
 for (const theme of ["light", "dark"]) {
  document.documentElement.dataset.theme = theme;
  const card = quoteCards()[0], cardBox = card.getBoundingClientRect(), containerBox = card.parentElement.getBoundingClientRect();
  check(cardBox.width > 0 && containerBox.height >= Math.min(cardBox.height, 120) && containerBox.bottom <= composer().getBoundingClientRect().top, "Pending quote clipped by conversation history or composer");
  window.testScreenshot = "selection-discussion-" + theme; await until(() => !window.testScreenshot, "Screenshot not captured");
 }
 window.testResult = { passed: true, quotedViews: original.excerpts.map(item => item.view), retries: calls.sends.length, writes: calls.writes, unsavedPreserved: true };
} catch (error) { window.testResult = { passed: false, error: error.stack ?? String(error), dom: document.body.textContent.slice(-3500) }; } })();
