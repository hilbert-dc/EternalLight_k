import { novelFixtureMedia } from "../helpers/novel-edition";
import React from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import "../../../src/styles.css";
import { saveAuthor } from "../helpers/author-session";
import { chapterRuntimeReads } from "../helpers/chapter-runtime";

const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (ok, message) => { if (!ok) throw Error(message); };
let renderError, phase = "initialization";
window.addEventListener("error", event => { renderError = event.error ?? event.message; });
const until = async (predicate, message) => { for (let i = 0; i < 220 && !predicate(); i++) { if (renderError) throw renderError; await tick(); } check(predicate(), message); };
const project = { id: "project-last-lamp", title: "模型资格与作者草稿", logline: "", updated_at: "" };
const connection = { id: "fixture", provider: "fixture", model: "fixture", protocol: "responses", selectedProtocol: "responses", status: "verified", declaredCapabilities: [], verifiedCapabilities: [], failedCapabilities: [], collaborationStatus: "unverified", operations: [], certificationIds: [], accountStatus: "configured", isDefault: true };
const unavailable = "Error invoking remote method 'agent:send-prompt': Error: MODEL_CONNECTION_UNAVAILABLE: 当前模型证据已过期\n原始诊断：qualification expired";
let configured = false, stateReads = 0, listener, pending, saved = null;
const calls = { sends: [], saves: 0, verify: 0, apply: 0, formal: 0 }, snapshots = [];
const base = chapterRuntimeReads(() => project);
const status = () => ({ configured, state: configured ? "ready" : "error", mode: "novel-agent-engine", provider: "fixture", model: "fixture", error: configured ? null : unavailable });
window.novelCodex = { multimedia: novelFixtureMedia,
  app: { onCloseRequested: () => () => {} },
  runtime: { ...base, status: async () => ({ state: "online", mode: "local", error: null }), listProjects: async () => ({ projects: [project] }),
    getState: async () => { stateReads++; return { project, canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1", scenes: [], anchors: [] }, novelEdition: { chapters: [] } }; },
    getCreativeWorkspace: async () => { const workspace = await base.getCreativeWorkspace(project.id); workspace.artifacts.brief.candidate = saved; return workspace; },
    saveArtifact: async (id, kind, payload, revision, candidate) => { check(id === project.id && kind === "brief" && revision === 1 && candidate === (saved?.id ?? null), "Planning save lost exact source"); calls.saves++; saved = { id: "saved-brief", payload: structuredClone(payload), version: 1, source: "author", parentRevisionId: null, baseProjectRevision: 1, createdAt: null }; return { artifactKind: kind, revisionId: saved.id, version: 1, payload: saved.payload, linkedImpactIds: [] }; },
    getHistoricalKnowledgeState: async (id, snapshotId) => ({ projectId: id, snapshotId, entries: [{ modelingBatchId: "batch-exact", assertionIndex: 0, sourceObjectId: "draft-source", sourceRevisionId: "source-exact", confirmationState: "candidate", assertion: { kind: "world", predicate: "信件", value: "尚未打开", evidence: { quote: "他没有拆开那封信。" } } }], coverage: { complete: true, truncatedEntries: false, pendingSources: [] } }),
    acceptReview: async () => { calls.formal++; throw Error("Unexpected adoption"); }, setWorkingBaseline: async () => { calls.formal++; throw Error("Unexpected working selection"); },
  },
  agent: { status: async () => status(), listConnections: async () => ({ connections: [connection] }), recoveryReport: async () => [], listExecutions: async () => [], readMessage: async () => { throw Error("message receipt not found"); }, beginInterlude: async () => { throw Error("unexpected interlude"); }, returnInterlude: async () => { throw Error("unexpected interlude return"); }, listMessages: async () => [], listThreads: async () => ({ threads: [], nextCursor: null }), listProposedActions: async () => ({ actions: [] }), setWorkspaceContext: async () => {}, onEvent: fn => { listener = fn; return () => {}; }, onUserInputRequest: () => () => {},
    verifyConnection: async () => { calls.verify++; throw Error("Unexpected automatic paid verification"); },
    configure: async () => { calls.apply++; configured = true; return status(); },
    sendPrompt: async (...args) => { calls.sends.push(structuredClone(args)); return new Promise((resolve, reject) => { pending = { resolve, reject }; }); },
  },
};
const composer = () => document.querySelector<HTMLTextAreaElement>('[aria-label="创作指令"]');
const send = () => document.querySelector<HTMLButtonElement>('[aria-label="发送创作指令"]');
const input = async (node, value) => { Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(node, value); node.dispatchEvent(new Event("input", { bubbles: true })); await tick(); };
const enter = () => composer().dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
const apply = async () => { const before = calls.apply; document.querySelector<HTMLButtonElement>(".composer-model-picker .composer-option").click(); await tick(); document.querySelector<HTMLButtonElement>('.composer-model-menu [role="menuitemradio"]').click(); await until(() => calls.apply === before + 1 && !send().disabled, "Explicit model application did not enable send"); };
const navigate = async (label, view) => { [...document.querySelectorAll<HTMLButtonElement>(".nav-item")].find(node => node.textContent.includes(label)).click(); await until(() => document.querySelector<HTMLElement>(".main-panel")?.dataset.view === view, `Navigate ${view}`); await tick(); await tick(); };
const begin = async () => { const count = calls.sends.length; pending = null; enter(); enter(); await until(() => pending, "Send did not reach controlled Host"); check(calls.sends.length === count + 1, "Duplicate Enter created two requests"); };
const reject = async (message = unavailable) => { configured = message === unavailable ? false : configured; pending.reject(Error(message)); await until(() => document.querySelector(".transcript-error") && !document.querySelector('[aria-label="停止当前执行"]'), "Rejected Host request did not settle"); await tick(); };
const capture = name => snapshots.push({ name, status: document.querySelector(".agent-status-bar")?.textContent, draft: composer().value, source: document.querySelector(".discussion-source")?.textContent ?? null, sends: calls.sends.length, saves: calls.saves });
const root = createRoot(document.getElementById("root"));
(async () => { try {
  localStorage.clear(); saveAuthor({ name: "资格作者", email: "availability@example.invalid", passwordSalt: "fixture", passwordHash: "fixture" }); root.render(<App />);
  await until(() => stateReads >= 2 && composer() && !composer().disabled, "App did not initialize");
  phase = "initial unavailable"; await input(composer(), "模型恢复后继续，先保留输入"); enter(); await tick();
  check(send().disabled && calls.sends.length === 0 && calls.verify === 0 && composer().value === "模型恢复后继续，先保留输入" && document.querySelector(".agent-status-bar")?.textContent.includes("模型需要重新验证"), "Invalid initial qualification sent or hid editable draft"); capture(phase);
  await apply(); check(calls.sends.length === 0, "Applying model automatically sent existing input");
  phase = "saved planning then preflight rejected"; await navigate("作品概况", "brief"); await until(() => document.querySelector(".artifact-editor-grid textarea"), "Brief editor missing"); document.querySelector<HTMLDetailsElement>(".overview-direction-editor").querySelector("summary").click(); await tick(); check(document.querySelector<HTMLElement>(".artifact-editor-grid textarea").checkVisibility(), "Direction editor was not revealed");
  await input(document.querySelector(".artifact-editor-grid textarea"), "刚修改并保存的作者目标"); const original = "  依据刚改的简报生成大纲，先保存候选。  "; await input(composer(), original); await begin();
  check(calls.saves === 1 && saved.payload.premise === "刚修改并保存的作者目标", "Model preflight ran before planning save"); await reject();
  check(composer().value === original && send().disabled && document.querySelector(".agent-status-bar")?.textContent.includes("qualification expired"), "Preflight rejection lost original draft or raw diagnostic"); capture(phase);
  await apply(); await begin(); check(calls.saves === 1 && calls.sends.at(-1)[1] === original.trim(), "Reapplication repeated saved planning or changed original request");
  phase = "ordinary task failure keeps actual readiness"; listener({ method: "turn/started", params: { threadId: "real-thread", turn: { id: "real-turn" } } }); await reject("provider_output_item_limit: 模型返回超限");
  check(document.querySelector(".agent-status-bar")?.classList.contains("agent-ready") && !document.querySelector(".agent-status-bar")?.textContent.includes("模型需要重新验证") && composer().value === "", "Ordinary started-task failure falsified model availability or restored an executed instruction"); capture(phase);
  phase = "exact source restored on local rejection"; await navigate("世界与规则", "world"); await until(() => [...document.querySelectorAll("button")].some(node => node.textContent === "在对话中讨论"), "Exact claim unavailable"); [...document.querySelectorAll<HTMLButtonElement>("button")].find(node => node.textContent === "在对话中讨论").click(); await tick();
  const sourcePrompt = composer().value; await begin(); const exactFocus = calls.sends.at(-1)[3]; await reject();
  check(composer().value === sourcePrompt && document.querySelector(".discussion-source")?.textContent.includes("他没有拆开那封信") && exactFocus.reference.modelingBatchId === "batch-exact", "Local rejection lost exact author-selected source"); capture(phase);
  for (const nextInput of ["等待拒绝时新写的要求", ""]) {
    phase = nextInput ? "changed input survives rejection" : "cleared input stays cleared";
    await input(composer(), ""); const sourceButton = [...document.querySelectorAll<HTMLButtonElement>("button")].find(node => node.textContent === "在对话中讨论"); sourceButton.click(); await tick(); await apply(); await begin(); if (!nextInput) await input(composer(), "删除前实际新输入的内容"); await input(composer(), nextInput); await reject();
    check(composer().value === nextInput && !document.querySelector(".discussion-source"), "Late rejection revived replaced/cleared input or source"); capture(phase);
  }
  check(calls.verify === 0 && calls.formal === 0 && calls.saves === 1 && calls.sends.length === 5, "Availability handling caused verification, formal changes, or duplicate writes");
  root.unmount(); window.testResult = { passed: true, snapshots, calls: { sends: calls.sends.length, saves: calls.saves, verify: calls.verify, apply: calls.apply, formal: calls.formal } };
} catch (error) { root.unmount(); window.testResult = { passed: false, error: `${phase}: ${error.stack ?? error}`, calls }; } })();
