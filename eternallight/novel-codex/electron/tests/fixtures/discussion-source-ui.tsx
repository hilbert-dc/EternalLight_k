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
const navigate = async (label, view) => {
 const node = () => view === "chapters" ? document.querySelector<HTMLButtonElement>("[data-directory-chapter-id]")
  : [...document.querySelectorAll<HTMLButtonElement>(".nav-item")].find(node => node.textContent.includes(label));
 await until(() => node() && !node().disabled, `Navigation ready ${view}`); node()!.click();
 await until(() => document.querySelector<HTMLElement>(".main-panel")?.dataset.view === view, `Missing ${view}`); await tick(); await tick();
};
const openClaims = async () => {
 await navigate("世界与规则", "world");
 await until(() => buttons("在对话中讨论").length === 2, "Claims");
};
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
  sendPrompt: async (...args) => { calls.sends.push(structuredClone(args)); if (failSend) { failSend = false; throw Error("controlled temporary failure"); } const id = `turn-${calls.sends.length}`; onEvent({ method: "turn/started", params: { threadId: "fixture-thread", turnId: id, turn: { id } } }); await tick(); onEvent({ method: "turn/completed", params: { threadId: "fixture-thread", turnId: id, turn: { id, status: "completed", items: [] } } }); return { message: "已读取准确来源", proposedActions: [] }; } },
};
const chooseProject = async index => { document.querySelector<HTMLButtonElement>(".project-switcher")!.click(); await tick(); document.querySelectorAll<HTMLButtonElement>(".project-menu-item")[index + 1].click(); await until(() => document.querySelector(".project-switcher")?.textContent.includes(projects[index].id), "Project switch"); await tick(); await tick(); };
const root = createRoot(document.getElementById("root"));
(async () => { try {
 localStorage.clear(); saveAuthor({ name: "来源作者", email: "source@example.invalid", passwordSalt: "fixture", passwordHash: "fixture" }); root.render(<App />);
 await until(() => composer() && !composer().disabled, "App initialization"); await openClaims();
 await input("保留我的问题"); await click("在对话中讨论"); check(composer().value === "保留我的问题" && document.querySelector(".discussion-source")?.textContent.includes("第一个独立原文"), "Source lost original draft");
 buttons("在对话中讨论")[1].click(); await tick();
 check(buttons("发送创作指令")[0].disabled, "Pending replacement can send"); composer().dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); await tick(); check(calls.sends.length === 0, "Pending source sent old target");
 await click("保留当前来源"); buttons("在对话中讨论")[1].click(); await tick(); await click("替换讨论来源");
 check(composer().value === "保留我的问题" && document.querySelector(".discussion-source")?.textContent.includes("第二个独立原文"), "Claim labels ambiguous or draft replaced");
 await navigate("作品目录", "chapters"); await chooseProject(1); check(!document.querySelector(".discussion-source") && composer().value === "", "Source leaked to another project"); await chooseProject(0);
 check(composer().value === "保留我的问题" && document.querySelector(".discussion-source")?.textContent.includes("第二个独立原文"), "Project draft/source not restored");
 await until(() => !buttons("发送创作指令")[0]?.disabled, "Ready send"); await click("发送创作指令"); await until(() => document.querySelector(".composer-retry"), "Retry");
 const sent = calls.sends[0][3]; check(sent.view === "world" && sent.reference.kind === "claim" && sent.reference.assertionIndex === 1 && sent.reference.modelingBatchId === "batch-1", "Cross-page send lost exact original claim");
 await openClaims(); await click("在对话中讨论");
 check(composer().value === "", "Reference entry injected a discussion template");
 await input("比较第一条规则的两种解释，再讨论怎样与人物误解结合。"); const nextDraft = composer().value;
 document.querySelector<HTMLButtonElement>(".composer-retry")!.click(); await until(() => calls.sends.length === 2 && buttons("发送创作指令")[0] && !buttons("发送创作指令")[0].disabled, "Original retry");
 check(JSON.stringify(calls.sends[1][3]) === JSON.stringify(sent), "Retry rebound to new draft reference"); check(composer().value === nextDraft && document.querySelector(".discussion-source")?.textContent.includes("第一个独立原文"), "Retry erased next draft source");
 snapshots.push({ case: "exact reference across pages, projects and retry", sent: sent.reference, nextDraftRetained: true });
 window.testScreenshot = "discussion source author draft"; await until(() => !window.testScreenshot, "Source draft screenshot");
 await navigate("伏笔与悬念", "plot"); await until(() => buttons("在对话中梳理伏笔")[0] && !buttons("在对话中梳理伏笔")[0].disabled, "Generic action starter");
 await click("在对话中梳理伏笔"); check(!document.querySelector(".discussion-source") && composer().value === nextDraft, "Generic action starter retained unrelated exact source or replaced the author's draft");
 await navigate("伏笔与悬念", "plot"); await until(() => buttons("到章节查看与决定").length === 2, "Plot directory"); buttons("到章节查看与决定")[1].click();
 await until(() => document.querySelector(".commitment-detail")?.textContent.includes("准确伏笔 two"), "Exact commitment not opened");
 check(document.querySelector<HTMLElement>(".main-panel")?.dataset.view === "chapters" && !document.querySelector(".conversation-chapter-results")?.hasAttribute("hidden") && calls.writes === 0, "Catalog navigation wrote or hid selected result");
 snapshots.push({ case: "directory opens exact existing author decision", summary: document.querySelector(".commitment-detail header strong")?.textContent, writes: calls.writes });
 window.testScreenshot = "discussion source exact author decision"; await until(() => !window.testScreenshot, "Author decision screenshot");
 await navigate("伏笔与悬念", "plot"); await until(() => buttons("到章节查看与决定").length === 2, "Plot reload"); holdDetail = true; buttons("到章节查看与决定")[0].click(); await until(() => releaseDetail, "Held source read"); await chooseProject(1); releaseDetail(); await tick(); await tick();
 check(!document.querySelector(".commitment-detail"), "Late source read crossed projects");
 snapshots.push({ case: "late catalogue read isolated", writes: calls.writes });
 root.unmount(); window.testResult = { passed: true, snapshots };
} catch (error) { window.testResult = { passed: false, error: error.stack, dom: document.body.textContent.slice(-4500) }; } })();
