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
const until = async (predicate, message) => { for (let i = 0; i < 200 && !predicate(); i++) { if (renderError) throw renderError; await tick(); } check(predicate(), message); };
const deferred = () => { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; };
const button = text => [...document.querySelectorAll<HTMLButtonElement>("button")].find(node => node.textContent === text || node.getAttribute("aria-label") === text);
const composer = () => document.querySelector<HTMLTextAreaElement>('[aria-label="创作指令"]');
const send = () => document.querySelector<HTMLButtonElement>('[aria-label="发送创作指令"]');
const input = async (node, value) => { check(node && !node.matches(":disabled"), "Input unavailable"); Object.getOwnPropertyDescriptor(node instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype, "value").set.call(node, value); node.dispatchEvent(new Event("input", { bubbles: true })); await tick(); };
const enter = () => composer().dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
const clone = value => structuredClone(value);
const projects = ["project-last-lamp", "project-b"].map(id => ({ id, title: id, logline: "作者目标", updated_at: "" }));
const projectFor = id => projects.find(project => project.id === id);
const revision = (id, payload, version = 1) => ({ id, payload, version, source: "author", parentRevisionId: null, baseProjectRevision: 1, createdAt: null });
const outline = { nodes: [{ id: "book", parentId: null, kind: "book", position: 0, title: "全书", purpose: "围绕来信寻找选择", certainty: "open", protection: "free", writerGuidance: "保留未知", dependencyKeys: [] }] };
const plan = { sourceOutlineRevisionId: "outline-source", chapters: [{ chapterPlanId: "plan-one", chapterNumber: 1, primaryNodeId: "book", title: "第一封信", goal: "决定如何回复", conflict: "来信与记忆矛盾", writerGuidance: "保留犹豫", publicTitle: "门缝里的信", publicCommitments: [], allowances: [], dependencyKeys: [], povCharacterId: null, storyTime: null, scenes: [] }], window: 1 };
const workspaces = new Map();
const calls = { saves: [], sends: [], reads: [], accept: 0, working: 0 }, snapshots = [];
let stateReads = 0, onEvent, execution, turn = 0, failSave = false, nextRead = null, heldRead, awaitSavedRead = false;
const baseReads = chapterRuntimeReads(projectFor);
const forbidden = kind => { calls[kind]++; throw Error(`Unexpected ${kind}`); };
window.novelCodex = { multimedia: novelFixtureMedia,
  app: { onCloseRequested: () => () => {} },
  runtime: { ...baseReads, status: async () => ({ state: "online", mode: "local", error: null }), listProjects: async () => ({ projects }),
    getState: async id => { stateReads++; return { project: projectFor(id), canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1", scenes: [], anchors: [] }, novelEdition: { chapters: [] } }; },
    getCreativeWorkspace: async id => {
      if (!workspaces.has(id)) { const data = await baseReads.getCreativeWorkspace(id); data.artifacts.brief.candidate = revision("brief-source", { premise: "未知的来信", promise: "悬念", stakes: "失去信任", endingDirection: "面对过去" }); data.artifacts.outline.candidate = revision("outline-source", clone(outline)); data.artifacts.outline.working = clone(data.artifacts.outline.candidate); data.artifacts.plan.candidate = revision("plan-source", clone(plan)); workspaces.set(id, data); }
      const data = clone(workspaces.get(id)); calls.reads.push({ id, revision: data.revision, candidates: Object.fromEntries(Object.entries(data.artifacts).map(([kind, artifact]) => [kind, artifact.candidate?.id ?? null])) });
      if (awaitSavedRead && nextRead) { const mode = nextRead; nextRead = null; awaitSavedRead = false; if (mode === "fail") throw Error("已保存规划读回暂失败"); heldRead = deferred(); return heldRead.promise.then(() => data); }
      return data;
    },
    saveArtifact: async (id, kind, payload, expectedRevision, expectedCandidate, linkedImpactIds = []) => {
      const workspace = workspaces.get(id); check(expectedRevision === workspace.revision && expectedCandidate === workspace.artifacts[kind].candidate.id, "Save lost exact planning CAS");
      calls.saves.push({ id, kind, payload: clone(payload) }); if (failSave) { failSave = false; throw Error("规划保存被拒绝"); }
      workspace.headSnapshotId = `head-${calls.saves.length}`; const saved = revision(`${kind}-saved-${calls.saves.length}`, clone(payload), calls.saves.length + 1); workspace.artifacts[kind].candidate = saved; awaitSavedRead = true;
      return { artifactKind: kind, revisionId: saved.id, version: saved.version, payload: clone(payload), linkedImpactIds };
    },
    acceptReview: async () => forbidden("accept"), setWorkingBaseline: async () => forbidden("working"),
    getHistoricalKnowledgeState: async (id, snapshotId) => ({ projectId: id, snapshotId, entries: [{ modelingBatchId: "batch-source", assertionIndex: 0, sourceObjectId: "draft-source", sourceRevisionId: "draft-revision", confirmationState: "candidate", assertion: { kind: "world", predicate: "信件", value: "尚未打开", evidence: { quote: "他没有拆开那封信。" } } }], coverage: { complete: true, truncatedEntries: false, pendingSources: [] } }),
  },
  agent: { status: async () => ({ configured: true, state: "ready", mode: "novel-agent-engine", provider: "fixture", model: "fixture", error: null }), listConnections: async () => ({ connections: [] }), recoveryReport: async () => [], listExecutions: async () => [], readMessage: async () => { throw Error("message receipt not found"); }, beginInterlude: async () => { throw Error("unexpected interlude"); }, returnInterlude: async () => { throw Error("unexpected interlude return"); }, listMessages: async () => [], listThreads: async () => ({ threads: [], nextCursor: null }), listProposedActions: async () => ({ actions: [] }), setWorkspaceContext: async () => {}, onEvent: listener => { onEvent = listener; return () => {}; }, onUserInputRequest: () => () => {},
    sendPrompt: async (id, prompt, repair, focus) => { const workspace = workspaces.get(id), lastSave = calls.saves.at(-1); check(calls.reads.at(-1)?.id === id && calls.reads.at(-1)?.revision === workspace.revision && (!lastSave || calls.reads.at(-1)?.candidates[lastSave.kind] === workspace.artifacts[lastSave.kind].candidate.id), "Model started without the fresh saved source"); calls.sends.push({ id, prompt, repair, focus: clone(focus), revision: workspace.revision }); execution = deferred(); turn++; onEvent({ method: "thread/started", params: { thread: { id: "planning-thread" } } }); onEvent({ method: "turn/started", params: { threadId: "planning-thread", turn: { id: `turn-${turn}` } } }); return execution.promise; },
  },
};
const root = createRoot(document.getElementById("root"));
const navigate = async (label, view) => {
  [...document.querySelectorAll<HTMLButtonElement>(".nav-item")].find(node => node.textContent.includes(label)).click();
  await until(() => document.querySelector<HTMLElement>(".main-panel")?.dataset.view === view || document.querySelector(".unsaved-dialog"), `Navigation request ${view}`);
  if (document.querySelector(".unsaved-dialog")) button("放弃修改并离开").click();
  await until(() => document.querySelector<HTMLElement>(".main-panel")?.dataset.view === view, `Navigation ${view}`); await tick(); await tick();
};
const field = view => document.querySelector<HTMLTextAreaElement>(view === "brief" ? ".artifact-editor-grid textarea" : view === "outline" ? ".outline-node textarea" : ".plan-field textarea");
const open = async (view = "brief") => { await navigate({ brief: "作品概况", outline: "全书大纲", plans: "近章规划" }[view], view); await until(() => field(view) && !field(view).matches(":disabled"), `Planning ${view} did not load`); const details = document.querySelector<HTMLDetailsElement>(view === "brief" ? ".overview-direction-editor" : ".planning-manual-editor"); if (!details.open) { details.querySelector("summary").click(); await tick(); } check(field(view).checkVisibility(), "Planning editor was not revealed"); };
const finish = async () => { onEvent({ method: "turn/completed", params: { threadId: "planning-thread", turn: { id: `turn-${turn}`, status: "completed", items: [] } } }); execution.resolve({ message: "本轮建议已说明，未采用规划", proposedActions: [] }); await until(() => !document.querySelector('[aria-label="停止当前执行"]'), "Execution did not settle"); await tick(); };
const capture = label => snapshots.push({ case: label, saves: calls.saves.length, sends: calls.sends.length, input: composer().value, formalWrites: calls.accept + calls.working });
const switchProject = async index => { document.querySelector<HTMLButtonElement>(".project-switcher").click(); await tick(); [...document.querySelectorAll<HTMLButtonElement>(".project-menu-item")].filter(node => node.querySelector("strong"))[index].click(); await until(() => document.querySelector(".project-switcher")?.textContent.includes(projects[index].title) || document.querySelector(".unsaved-dialog"), "Project switch not requested"); if (document.querySelector(".unsaved-dialog")) button("放弃修改并离开").click(); await until(() => document.querySelector(".project-switcher")?.textContent.includes(projects[index].title), "Project switch did not settle"); await tick(); };
(async () => { try {
  saveAuthor({ name: "规划提交作者", email: "planning-submit@example.invalid", passwordSalt: "fixture", passwordHash: "fixture" }); root.render(<App />);
  await until(() => stateReads >= 2 && composer(), "App initialization");
  for (const [view, kind] of [["brief", "brief"], ["outline", "outline"], ["plans", "plan"]]) {
    phase = `dirty ${view} direct submit`; await open(view); const value = `作者修改的${view}目标`;
    await input(field(view), value); const prompt = `请根据刚修改的${view}目标生成这一份规划，先保存候选。`; await input(composer(), prompt);
    const before = calls.sends.length, saves = calls.saves.length; nextRead = "hold"; heldRead = null; enter(); enter();
    await until(() => heldRead, "Submit did not save and read back"); check(calls.saves.length === saves + 1 && calls.saves.at(-1).kind === kind && calls.sends.length === before && composer().value === prompt, "Submit duplicated saves or sent before readback");
    heldRead.resolve(); await until(() => calls.sends.length === before + 1, "Saved planning did not send");
    check(calls.sends.at(-1).prompt === prompt && calls.sends.at(-1).focus.view === view && calls.sends.at(-1).repair === null, "Planning altered author request or page focus");
    await input(composer(), "下一条未发送构想"); await finish(); check(composer().value === "下一条未发送构想", "Old execution erased next draft"); capture(phase);
  }
  phase = "save rejection preserves input"; await open(); await input(field("brief"), "保存失败时仍保留的修改"); await input(composer(), "保存后再起草简报");
  const beforeFailure = calls.sends.length; failSave = true; enter(); await until(() => document.body.textContent.includes("规划保存被拒绝") && !send().disabled, "Rejected save did not settle");
  check(calls.sends.length === beforeFailure && composer().value === "保存后再起草简报" && field("brief").value === "保存失败时仍保留的修改", "Rejected save lost edits or called model"); capture(phase);
  enter(); await until(() => calls.sends.length === beforeFailure + 1, "Save retry did not send"); await finish();
  phase = "readback failure retries only the receipt"; await input(field("brief"), "已经保存但首次读取失败"); await input(composer(), "基于这一版继续调整"); nextRead = "fail";
  const readFailureModelCount = calls.sends.length; enter(); await until(() => document.body.textContent.includes("规划已保存，但读取结果尚未确认") && !send().disabled, "Readback failure did not settle"); const savedCount = calls.saves.length;
  check(calls.sends.length === readFailureModelCount && composer().value === "基于这一版继续调整", "Readback failure sent or consumed author input"); enter(); await until(() => calls.sends.length === readFailureModelCount + 1, "Receipt recovery did not resume submission");
  check(calls.saves.length === savedCount, "Receipt recovery duplicated a successful save"); await finish(); capture(phase);
  phase = "local validation returns false"; await open("outline"); const title = document.querySelector<HTMLInputElement>('.outline-node header input'); const previousTitle = title.value; await input(title, ""); await input(composer(), "根据当前大纲检查结构");
  const validationSends = calls.sends.length, validationSaves = calls.saves.length; enter(); await tick(); await tick();
  check(calls.sends.length === validationSends && calls.saves.length === validationSaves && composer().value === "根据当前大纲检查结构" && document.querySelector('[aria-invalid="true"]'), "Invalid planning crossed local validation"); await input(title, previousTitle); capture(phase);
  await open();
  for (const replacement of ["等待时输入的新要求", ""]) {
    phase = replacement ? "changed input during preparation" : "cleared input during preparation";
    await input(field("brief"), phase); await input(composer(), "不得迟到发送的原要求"); nextRead = "hold"; heldRead = null; const oldSends = calls.sends.length; enter(); await until(() => heldRead, "No pending readback");
    await input(composer(), replacement); heldRead.resolve(); await tick(); await tick(); await tick();
    check(calls.sends.length === oldSends && composer().value === replacement, "Late prepare sent an obsolete request or restored cleared input"); capture(phase);
  }
  phase = "view incarnation A-B-A"; await input(field("brief"), phase); await input(composer(), "页面离开后不得发送"); nextRead = "hold"; heldRead = null; const beforeView = calls.sends.length; enter(); await until(() => heldRead, "View race lacked pending read"); const viewRead = heldRead;
  [...document.querySelectorAll<HTMLButtonElement>(".nav-item")].find(node => node.textContent.includes("角色")).click();
  await until(() => document.querySelector(".unsaved-dialog"), "Pending navigation did not ask about edits"); check(button("放弃修改并离开").disabled && calls.sends.length === beforeView, "Pending save allowed discard or sent before readback"); viewRead.resolve();
  await until(() => !button("放弃修改并离开").disabled, "Save did not release leave controls"); button("放弃修改并离开").click(); await until(() => document.querySelector<HTMLElement>(".main-panel")?.dataset.view === "characters", "Deferred navigation failed"); await open(); await tick(); await tick(); check(calls.sends.length === beforeView && composer().value === "页面离开后不得发送", "A-B-A submitted an old editor incarnation"); capture(phase);
  phase = "project incarnation A-B-A"; await input(field("brief"), phase); await input(composer(), "作品离开后不得发送"); nextRead = "hold"; heldRead = null; const beforeProject = calls.sends.length; enter(); await until(() => heldRead, "Project race lacked pending read"); const projectRead = heldRead;
  document.querySelector<HTMLButtonElement>(".project-switcher").click(); await tick(); [...document.querySelectorAll<HTMLButtonElement>(".project-menu-item")].filter(node => node.querySelector("strong"))[1].click(); await until(() => document.querySelector(".unsaved-dialog"), "Pending project switch did not ask about edits");
  check(button("放弃修改并离开").disabled && calls.sends.length === beforeProject, "Pending project save bypassed leave protection"); projectRead.resolve(); await until(() => !button("放弃修改并离开").disabled, "Project save did not release leave controls"); button("放弃修改并离开").click(); await until(() => document.querySelector(".project-switcher")?.textContent.includes(projects[1].title), "Deferred project switch failed"); await tick(); check(composer().value === "", "Author draft leaked to another project"); await switchProject(0); await open(); await tick(); await tick(); check(calls.sends.length === beforeProject && composer().value === "作品离开后不得发送", "Project A-B-A allowed old submission"); capture(phase);
  phase = "exact discussion source survives planning preparation"; await input(composer(), ""); await navigate("世界与规则", "world"); await until(() => button("在对话中讨论"), "Claim source not available"); button("在对话中讨论").click(); await tick();
  const exactPrompt = composer().value; await open(); await input(field("brief"), "带精确来源讨论时保存规划编辑"); const beforeSource = calls.sends.length, sourceSaves = calls.saves.length; enter(); await tick(); await tick();
  check(calls.sends.length === beforeSource && calls.saves.length === sourceSaves && composer().value === exactPrompt && field("brief").value === "带精确来源讨论时保存规划编辑" && document.querySelector(".discussion-source"), "Typed source bypassed unsaved protection or lost its binding");
  button("保存").click(); await until(() => !send().disabled, "Manual planning save did not restore exact discussion"); enter(); await until(() => calls.sends.length === beforeSource + 1, "Manually saved source-bound discussion did not send");
  const sent = calls.sends.at(-1); check(sent.prompt === exactPrompt && sent.focus.view === "world" && sent.focus.reference.kind === "claim" && sent.focus.reference.modelingBatchId === "batch-source" && sent.focus.reference.assertionIndex === 0, "Planning preparation rebound an exact discussion source"); await finish(); capture(phase);
  check(calls.accept === 0 && calls.working === 0, "Automatic preparation changed formal authority"); root.unmount(); window.testResult = { passed: true, snapshots, sends: calls.sends, saves: calls.saves.length, formalWrites: calls.accept + calls.working };
} catch (error) { window.testResult = { passed: false, error: phase + ": " + (error.stack ?? String(error)), calls, dom: document.body.innerText.slice(-7000) }; } })();
