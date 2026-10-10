import { novelFixtureMedia } from "../helpers/novel-edition";
import { chapterRuntimeReads } from "../helpers/chapter-runtime";
import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import { saveAuthor } from "../helpers/author-session";
import { ComposerModelPicker } from "../../../src/ComposerModelPicker";
import { layoutFrame, resizeTestViewport } from "../helpers/page-geometry";
window.addEventListener("error", event => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
const tick = () => new Promise(resolve => setTimeout(resolve, 25));
const check = (value, message) => { if (!value) throw Error(message); };
const until = async predicate => { for (let i = 0; i < 200 && !predicate(); i++) await tick(); check(predicate(), `Connection UI did not settle: ${document.querySelector('[aria-label="模型连接状态"]')?.textContent}`); };
const project = { id: "project-last-lamp", title: "连接作品 A", logline: "", updated_at: "" }, other = { ...project, id: "project-b", title: "连接作品 B" };
const state = project => ({ project, canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1.0", scenes: [], anchors: [] }, novelEdition: { chapters: [] } });
const connection = { id: "fixture", provider: "fixture-provider", model: "fixture-model", protocol: "responses", selectedProtocol: "responses", declaredCapabilities: [], verifiedCapabilities: [], failedCapabilities: [], collaborationStatus: "ready", operations: [], certificationIds: [], accountStatus: "configured", isDefault: true };
const currentConnection = { ...connection, id: "current", provider: "old-provider", model: "old-model", status: "verified" };
const remainingConnections = Array.from({ length: 10 }, (_, i) => ({ ...connection, id: `other-${i}`, model: `长名称备用模型-${i}`, status: i === 0 ? "failed" : "unverified" }));
let configured = false, verified = false, releaseVerify, releaseList, releaseApply, holdList = false;
const calls = { verify: 0, apply: 0, model: 0 }, snapshots = [];
saveAuthor({ name: "连接作者", email: "fixture@example.invalid", passwordSalt: "fixture", passwordHash: "fixture" });
const api = { multimedia: novelFixtureMedia,
  app: { onCloseRequested: () => () => {} },
  runtime: { ...chapterRuntimeReads(id => id === other.id ? other : project), listWorkTasks: async () => ({ tasks: [], nextCursor: null, activeCount: 0 }), status: async () => ({ state: "online", mode: "local", error: null }), listProjects: async () => ({ projects: [project, other] }), getState: async id => state(id === other.id ? other : project) },
  agent: { status: async () => ({ configured, state: "ready", mode: "novel-agent-engine", error: null, provider: "old-provider", model: "old-model" }),
    listConnections: async () => { if (holdList) await new Promise(resolve => { releaseList = resolve; }); return { connections: [currentConnection, { ...connection, status: verified ? "verified" : "unverified" }, ...remainingConnections] }; },
    verifyConnection: async (provider, model) => { check(provider === connection.provider && model === connection.model, "Keyboard chose a different connection"); calls.verify++; await new Promise((resolve, reject) => { releaseVerify = { resolve, reject }; }); verified = true; return { records: [], reportId: "fixture", selectedProtocol: "responses" }; },
    configure: async (provider, model) => { check(provider === connection.provider && model === connection.model, "Apply switched connection identity"); calls.apply++; await new Promise((resolve, reject) => { releaseApply = { resolve, reject }; }); configured = true; return { configured: true, state: "ready", mode: "novel-agent-engine", error: null, provider: connection.provider, model: connection.model }; },
    sendPrompt: async () => { calls.model++; throw Error("Unexpected model call"); },
    listExecutions: async () => [], readMessage: async () => { throw Error("message receipt not found"); }, beginInterlude: async () => { throw Error("unexpected interlude"); }, returnInterlude: async () => { throw Error("unexpected interlude return"); }, listMessages: async () => [], listThreads: async () => ({ threads: [], nextCursor: null }), recoveryReport: async () => [], listProposedActions: async () => ({ actions: [] }), setWorkspaceContext: async () => {}, onEvent: () => () => {}, onUserInputRequest: () => () => {},
  },
};
window.novelCodex = api; const root = createRoot(document.getElementById("root")); root.render(<App />);
const notice = () => document.querySelector('[aria-label="模型连接状态"]');
const choose = async () => { document.querySelector('.composer-model-picker .composer-option').click(); await tick(); const item = document.querySelector('[data-connection-id="fixture"]'); item.click(); item.click(); };
const key = async (keyCode, modifiers = []) => { window.testKey = { keyCode, modifiers }; await until(() => !window.testKey); await layoutFrame(); };
const menu = () => document.querySelector<HTMLElement>('.composer-model-menu');
const trigger = () => document.querySelector<HTMLButtonElement>('.composer-model-picker .composer-option');
const focusedConnection = () => (document.activeElement as HTMLElement)?.dataset.connectionId;
const input = async value => { const node = document.querySelector('[aria-label="创作指令"]'); Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(node, value); node.dispatchEvent(new Event("input", { bubbles: true })); await tick(); return node; };
const capture = name => snapshots.push({ name, text: notice()?.textContent, draft: document.querySelector('[aria-label="创作指令"]').value });
(async () => { try {
  await until(() => document.querySelector('.project-switcher span[title]')?.textContent === project.title);
  const originalViewport = { width: innerWidth, height: innerHeight };
  const draft = await input("验证后继续讨论，先保留构思"); draft.setSelectionRange(3, 8); trigger().focus(); await key("Down");
  check(menu()?.textContent.includes("复杂任务自动协作"), "Verified collaboration capability is not visible in the model menu");
  check(focusedConnection() === "current" && menu().querySelector('[aria-checked="true"]')?.getAttribute("data-connection-id") === "current", "Opening menu did not focus or identify the current connection");
  await key("Down"); check(focusedConnection() === connection.id, "ArrowDown did not move to next connection"); await key("Up"); check(focusedConnection() === "current", "ArrowUp did not move back");
  await key("End"); check(focusedConnection() === "other-9", "End did not reach final connection"); await key("Home"); check(focusedConnection() === "current", "Home did not reach first connection");
  await key("Tab", ["shift"]); check(menu() && document.activeElement === trigger(), "Shift+Tab did not naturally return to the model trigger"); await key("Down"); check(focusedConnection() === "current", "ArrowDown could not reenter an already open menu");
  check(calls.verify === 0 && calls.apply === 0 && calls.model === 0, "Menu navigation started validation or execution"); await key("Escape"); check(!menu() && document.activeElement === trigger(), "Escape did not return focus to model trigger");
  trigger().focus(); await key("Up"); check(focusedConnection() === "other-9", "ArrowUp opening did not focus the last connection");
  draft.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, button: 0 })); draft.focus(); await layoutFrame(); check(!menu() && document.activeElement === draft && draft.selectionStart === 3 && draft.selectionEnd === 8, "Outside pointer stole composer focus or selection");
  const initialHint = document.querySelector<HTMLElement>('.composer-keyboard-hint'), sendDisabled = document.querySelector<HTMLButtonElement>('[aria-label="发送创作指令"]').disabled;
  check(initialHint.checkVisibility() && initialHint.textContent === (sendDisabled ? "Shift+Enter 换行" : "Enter 发送 · Shift+Enter 换行"), `Shortcut guidance disagreed with sending gate: ${JSON.stringify({ visible: initialHint.checkVisibility(), display: getComputedStyle(initialHint).display, text: initialHint.textContent, sendDisabled, configured, focused: draft.matches(":focus"), windowFocused: document.hasFocus() })}`);
  await resizeTestViewport(900, 600); trigger().focus(); await key("Down"); await key("End");
  const list = menu(), bounds = list.getBoundingClientRect(), rail = document.querySelector('.conversation-rail').getBoundingClientRect(), last = document.activeElement.getBoundingClientRect();
  check(rail.width >= 299 && rail.width <= 301 && list.scrollHeight > list.clientHeight && list.scrollTop > 0 && getComputedStyle(list).overflowY === "auto", "Narrow model menu did not independently scroll");
  check(bounds.left >= rail.left - 1 && bounds.right <= rail.right + 1 && bounds.top >= 0 && bounds.bottom <= innerHeight && last.top >= bounds.top && last.bottom <= bounds.bottom + 1, "Last model option was clipped or menu escaped narrow rail");
  snapshots.push({ name: "keyboard-and-narrow-menu", text: "当前项语义 · 上下/Home/End · Escape · 外点 · 300px滚动", draft: draft.value });
  await key("Home"); await key("Down"); check(focusedConnection() === connection.id, "Narrow navigation did not target the intended connection"); await key("Return"); await until(() => releaseVerify);
  check(calls.verify === 1 && document.querySelector('.composer-model-picker .composer-option').disabled, "duplicate selection started two verifications");
  check(!menu(), "Disabled verification left a stale menu open");
  await until(() => Number(notice()?.textContent.match(/[0-9]+/)?.[0]) >= 1); capture("verifying");
  draft.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); await tick(); check(calls.model === 0 && draft.value.includes("保留构思"), "verification sent or erased draft");
  holdList = true; releaseVerify.resolve(); await until(() => releaseList); check(notice().textContent.includes("读取验证结果"), "verification-list phase was hidden"); capture("reading");
  releaseList(); holdList = false; await until(() => releaseApply); check(document.querySelector('.composer-model-picker .composer-option').disabled && calls.apply === 1, "verify-to-apply guard released early"); capture("applying");
  releaseApply.reject(Error("受控配置失败")); await until(() => notice()?.textContent.includes("受控配置失败")); capture("failed");
  notice().querySelector('button').click(); notice().querySelector('button').click(); await until(() => calls.verify === 2); check(calls.apply === 1, "retry duplicated or skipped validation");
  releaseVerify.resolve(); await until(() => calls.apply === 2); releaseApply.resolve(); await until(() => notice()?.textContent.includes("已就绪")); capture("ready");
  check(document.querySelector('.composer-model-picker .composer-option').textContent.includes(connection.model) && draft.value.includes("保留构思"), "successful apply lost draft or model");
  draft.focus(); await layoutFrame(); check(document.querySelector('.composer-keyboard-hint').textContent === "Enter 发送 · Shift+Enter 换行", "Ready composer did not show idle shortcut guidance");
  trigger().focus(); await key("Down"); check(focusedConnection() === connection.id && menu().querySelector('[aria-checked="true"]')?.getAttribute("data-connection-id") === connection.id, "Configured current item was not announced accurately");
  await key("Tab"); check(!menu() && document.activeElement === document.querySelector('[aria-label="发送创作指令"]'), "Tab was trapped or returned to the model trigger");
  snapshots.push({ name: "ready-keyboard-exit", text: "Tab自然离开 · 当前已配置项 · 输入焦点快捷键", draft: draft.value });
  await resizeTestViewport(originalViewport.width, originalViewport.height);
  verified = false; await until(() => !document.querySelector('.composer-model-picker .composer-option').disabled);
  // Reload current unverified connection state, then change project during its wait.
  root.unmount(); configured = false; const nextRoot = createRoot(document.getElementById('root')); nextRoot.render(<App />);
  await until(() => document.querySelector('.project-switcher span[title]')); await choose(); await until(() => calls.verify === 3);
  document.querySelector('.project-switcher').click(); await tick(); [...document.querySelectorAll('.project-menu-item')].find(node => node.textContent.includes(other.title)).click(); await until(() => document.querySelector('.project-switcher span[title]')?.textContent === other.title);
  releaseVerify.resolve(); await until(() => !document.querySelector('.composer-model-picker .composer-option').disabled);
  check(calls.apply === 2 && !notice(), "late project A verification applied configuration or leaked notice into B");
  nextRoot.unmount();
  const edgeRoot = createRoot(document.getElementById('root')), selections = [];
  const edge = async (connections, disabled = false) => { edgeRoot.render(<div className="conversation-rail" style={{ width: 300, height: 450, position: "relative" }}><div style={{ position: "absolute", bottom: 30, left: 12 }}><ComposerModelPicker connections={connections} provider={currentConnection.provider} model={currentConnection.model} label="边界模型" disabled={disabled} onSelect={value => selections.push(value.id)} /></div><button id="outside">外部按钮</button></div>); await layoutFrame(); };
  await edge([currentConnection, { ...connection, status: "unverified" }]); trigger().focus(); await key("Down"); await key("End");
  await edge([{ ...currentConnection, collaborationStatus: "failed" }, { ...connection, collaborationStatus: "unverified", status: "verified" }]);
  check(menu()?.textContent.includes("协作验证失败 · 单智能体执行") && menu()?.textContent.includes("协作待验证"), "Model readiness concealed unavailable collaboration");
  await edge([{ ...currentConnection }]); check(menu() && focusedConnection() === currentConnection.id, "Removing focused connection stranded keyboard focus");
  await edge([]); check(menu()?.textContent.includes("没有已配置的模型连接") && menu().querySelectorAll('[role="menuitemradio"]').length === 0 && document.activeElement === menu(), "Empty list retained a false current item or inaccessible focus");
  await key("Home"); await key("End"); await key("Escape"); check(!menu() && document.activeElement === trigger() && selections.length === 0, "Empty list navigation selected a missing connection");
  await edge([currentConnection]); trigger().click(); await layoutFrame(); await edge([currentConnection], true); check(!menu() && trigger().disabled, "Dynamic disable did not close the open menu");
  await edge([{ ...currentConnection, model: "gpt-5.3-codex" }]); trigger().click(); await layoutFrame();
  check(menu().querySelector('[data-connection-id="current"]').textContent.includes("gpt-5.3-codex"), "Application branding rewrote a real model identifier"); await key("Escape");
  await edge([currentConnection], false); trigger().click(); await layoutFrame(); await key("Return"); check(selections.length === 1 && selections[0] === currentConnection.id && !menu(), "Native Enter did not select exactly one current connection");
  snapshots.push({ name: "live-list-boundaries", text: "删除焦点项重定位 · 空列表无选择 · 禁用关菜单 · Enter精确一次", draft: "" });
  edgeRoot.unmount(); window.testResult = { passed: true, snapshots, calls };
} catch (error) { window.testResult = { passed: false, error: error.stack }; } })();
