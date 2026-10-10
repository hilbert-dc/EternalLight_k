import { novelFixtureMedia } from "../helpers/novel-edition";
import { chapterRuntimeReads } from "../helpers/chapter-runtime";
import React from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import { PropertyRulesPanel } from "../../../src/PropertyRulesPanel";
import { ruleInputError } from "../../../src/PropertyRuleFields";
import { saveAuthor, clearAuthor } from "../helpers/author-session";
import "../../../src/styles.css";

window.addEventListener("error", (event) => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
const tick = () => new Promise((resolve) => setTimeout(resolve, 30));
const check = (value, message) => { if (!value) throw new Error(message); };
const button = (text) => [...document.querySelectorAll("button")].find((element) => element.textContent === text);
const click = async (text) => { const element = button(text); check(element && !element.disabled, `Unavailable: ${text}`); element.click(); await tick(); await tick(); };
const field = async (text, value) => {
  const label = [...document.querySelectorAll(".property-rule-fields label")].find((element) => element.textContent.startsWith(text));
  const element = label?.querySelector("input,select,textarea");
  check(element && !element.matches(":disabled"), `Unavailable field: ${text}`);
  Object.getOwnPropertyDescriptor(Object.getPrototypeOf(element), "value").set.call(element, value);
  element.dispatchEvent(new Event(element.tagName === "SELECT" ? "change" : "input", { bubbles: true })); await tick();
};
const project = { id: "rules-A", title: "规则测试作品", logline: "", updated_at: "" };
const state = { project, canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1.0", scenes: [], anchors: [] }, novelEdition: { chapters: [] } };
const blank = id => ({ project: { ...project, id }, revision: 0, headSnapshotId: "rules-head", sourceDigest: "a".repeat(64), index: { status: "ready" },
  artifacts: Object.fromEntries(["brief", "outline", "plan", "identity", "propertyRules"].map(kind => [kind, { candidate: null, accepted: null, working: null, workingSelection: null, locked: false }])),
  chapters: [], reviews: [], commits: [], workingBaselines: [], continuationIntents: [], planningReviews: [], tasks: [], impacts: [], modeling: [], factProposals: [], chapterPlanBindings: [] });
const workspaces = { "rules-A": blank("rules-A"), "rules-B": blank("rules-B") };
let serial = 0, closeRequested, confirmedCloses = 0, releaseSave, failRead = false, failReview = false;
let saveMode = "normal", heldRead = null, savedHandler, discardHandler, dirty = false;
const requests = [], receipts = new Map();
let reviews = 0, accepts = 0, undos = 0;
const api = { multimedia: novelFixtureMedia,
  app: { onCloseRequested: (handler) => { closeRequested = handler; return () => {}; }, confirmClose: async () => { confirmedCloses += 1; }, cancelClose: async () => {} },
  agent: { onEvent: () => () => {}, onUserInputRequest: () => () => {}, status: async () => ({ configured: false, state: "idle", mode: "novel-agent-engine", error: null }),
    listConnections: async () => ({ connections: [] }), recoveryReport: async () => [], listProposedActions: async () => ({ actions: [] }), setWorkspaceContext: async () => ({ selected: true }) },
  runtime: { ...chapterRuntimeReads(id => ({ ...project, id })), listWorkTasks: async () => ({ tasks: [], nextCursor: null, activeCount: 0 }),
    ...Object.fromEntries(["Search", "Entity", "Relation", "Knowledge", "SnapshotKnowledge", "Fact"].map(kind => [`get${kind}Audit`, async () => ({ job: null })])),
    status: async () => ({ state: "online", mode: "local", error: null }), listProjects: async () => ({ projects: [project] }), getState: async () => state,
    getCreativeWorkspace: async (projectId) => {
      const result = structuredClone(workspaces[projectId]);
      if (heldRead) { const hold = heldRead; heldRead = null; await new Promise((resolve) => { hold.release = resolve; }); }
      if (failRead) throw new Error("规则读取测试失败");
      return result;
    },
    savePropertyRules: async (projectId, request) => {
      requests.push(structuredClone(request));
      if (saveMode === "defer") await new Promise((resolve) => { releaseSave = resolve; });
      if (receipts.has(request.idempotencyKey)) {
        const saved = receipts.get(request.idempotencyKey);
        check(saved.projectId === projectId && saved.digest === JSON.stringify(request), "Retry changed project, key, or payload"); return saved.receipt;
      }
      const workspace = workspaces[projectId];
      check(request.expectedRevision === workspace.revision && request.expectedCandidateRevisionId === (workspace.artifacts.propertyRules.candidate?.id ?? null), "Rule save lost CAS baseline");
      const receipt = { artifactKind: "propertyRules", revisionId: `rules-${++serial}`, version: serial, payload: request.payload };
      workspace.artifacts.propertyRules.candidate = { id: receipt.revisionId, version: serial, payload: structuredClone(request.payload) };
      receipts.set(request.idempotencyKey, { projectId, digest: JSON.stringify(request), receipt });
      if (saveMode === "lost") throw new Error("写入后回执丢失");
      if (saveMode === "read-lost") failRead = true;
      return receipt;
    },
    createReview: async (projectId, kind, revisionId, facts, selections) => {
      reviews += 1;
      const workspace = workspaces[projectId];
      check(kind === "propertyRules" && revisionId === workspace.artifacts.propertyRules.candidate.id && facts.length === 0 && Object.keys(selections).length === 0, "Rule review mixed facts or changed revision");
      const review = { id: `rule-review-${reviews}`, artifactKind: kind, artifactRevisionId: revisionId, projectRevision: workspace.revision, status: "ready", checks: [], selections: {} };
      workspace.reviews.push(review);
      if (failReview) throw new Error("审阅回执丢失");
      return review;
    },
    acceptReview: async (projectId, reviewId, expectedRevision, selections) => {
      accepts += 1;
      const workspace = workspaces[projectId], review = workspace.reviews.find((entry) => entry.id === reviewId);
      check(expectedRevision === workspace.revision && review.status === "ready" && Object.keys(selections).length === 0, "Adopt lost current Author review boundary");
      review.status = "accepted"; workspace.artifacts.propertyRules.accepted = workspace.artifacts.propertyRules.candidate; workspace.revision += 1;
      workspace.commits.unshift({ id: `rule-commit-${accepts}`, operation_kind: "accept", proposalApplicationId: null, result_revision: workspace.revision, receipt: { artifactKind: "propertyRules" } });
      return { commitId: `rule-commit-${accepts}`, resultRevision: workspace.revision };
    },
    undoCommit: async (projectId, commitId, expectedRevision) => {
      const workspace = workspaces[projectId]; undos += 1;
      check(workspace.commits[0].id === commitId && workspace.revision === expectedRevision, "Undo ignored formal head or CAS");
      workspace.artifacts.propertyRules.accepted = null; workspace.revision += 1;
      workspace.commits.unshift({ id: `rule-undo-${undos}`, operation_kind: "undo", proposalApplicationId: null, result_revision: workspace.revision, receipt: {} });
      return { commitId: `rule-undo-${undos}`, resultRevision: workspace.revision };
    },
  },
};
window.novelCodex = api;
let root = createRoot(document.getElementById("root"));
const render = async (projectId = project.id, refreshToken = 0) => {
  root.render(<PropertyRulesPanel api={api} projectId={projectId} refreshToken={refreshToken} onFormalChange={async () => {}}
    onDirtyChange={(value) => { dirty = value; }} onSaveHandlerChange={(handler) => { savedHandler = handler; }} onDiscardHandlerChange={(handler) => { discardHandler = handler; }} />);
  await tick(); await tick();
};
async function run() {
  saveAuthor({ name: "规则测试作者", email: "rules@example.test", passwordSalt: "test-salt", passwordHash: "test-hash" });
  localStorage.setItem("novel-codex-active-project", project.id);
  root.render(<App />); await tick(); await tick();
  [...document.querySelectorAll(".nav-item")].find((item) => item.textContent.includes("世界与规则")).click(); await tick(); await tick();
  check(document.querySelector('[aria-label="作者属性规则"]') && document.body.textContent.includes("尚无已采用"), "App did not connect the author rule workspace");
  await click("编辑规则候选"); await click("添加结构规则"); await click("保存规则候选");
  check(requests.length === 0 && document.body.textContent.includes("属性键必填"), "Empty rule reached Runtime");
  closeRequested(); await tick();
  check(document.querySelector(".unsaved-dialog"), "Rule editing bypassed actual App close protection");
  await click("保存并继续"); check(confirmedCloses === 0 && document.querySelector(".unsaved-dialog"), "Invalid rule saved and closed App");
  await click("留在当前页面");
  document.querySelector('[aria-label="打开用户设置"]').click(); await tick();
  check(document.querySelector(".unsaved-dialog") && document.querySelector(".property-rule-editor"), "Account navigation discarded unsaved rule input");
  await click("留在当前页面");
  await field("属性键", "alive"); await field("说明", "人物存活状态，只允许同一时段一个值");
  saveMode = "lost"; await click("保存规则候选");
  check(workspaces[project.id].artifacts.propertyRules.accepted === null && requests.length === 1, "Candidate became a formal rule");
  check(document.querySelector(".property-rule-fields input")?.matches(":disabled"), "Unknown save allowed editing its frozen request");
  saveMode = "normal"; button("重试原保存请求").click(); button("重试原保存请求").click(); await tick(); await tick();
  check(requests.length === 2 && JSON.stringify(requests[0]) === JSON.stringify(requests[1]), "Same-frame retry duplicated or changed the frozen request");
  check(!document.querySelector(".property-rule-editor") && document.body.textContent.includes("待审阅候选"), "Confirmed saved rule remained an unsaved editor");
  failReview = true; await click("建立规则审阅"); failReview = false;
  check(reviews === 1 && button("采用规则") && !button("建立规则审阅"), "Review receipt loss caused a duplicate review instead of reconciliation");
  const rulePanel = document.querySelector(".property-rules-panel");
  check(document.querySelector(".content-header").getBoundingClientRect().top < rulePanel.getBoundingClientRect().top, "Rule panel rendered before the App page header");
  check(rulePanel.scrollWidth <= rulePanel.clientWidth + 1, "Rule review overflowed the central author column");
  window.uiReady = true; await tick(); await tick();
  button("采用规则").click(); button("采用规则").click(); await tick(); await tick();
  check(accepts === 1 && workspaces[project.id].artifacts.propertyRules.accepted.payload.definitions[0].predicate === "alive", "Rule adoption was duplicated or omitted");
  await click("撤销最近规则采用"); check(undos === 1 && workspaces[project.id].artifacts.propertyRules.accepted === null, "Rule adoption lacked ledger-based undo");
  await click("编辑规则候选"); await click("移除规则 1"); await click("保存规则候选"); await click("建立规则审阅"); await click("采用规则");
  check(workspaces[project.id].artifacts.propertyRules.accepted.payload.definitions.length === 0, "Empty rule candidate did not clear the adopted definition set");
  failRead = true; await click("重新读取规则");
  check(button("编辑规则候选")?.disabled && document.body.textContent.includes("保留上次结果"), "Read failure erased formal rules or allowed stale writes");
  failRead = false; await click("重新读取规则");
  root.unmount(); await tick(); clearAuthor();

  root = createRoot(document.getElementById("root"));
  await render(); await click("编辑规则候选"); await click("添加结构规则");
  await field("属性键", "owner"); await field("说明", "物品的所有者"); await field("值类型", "entity"); await field("引用种类", "organization"); await field("例外", "共同所有时需作者解释");
  saveMode = "read-lost"; await click("保存规则候选");
  const written = requests.length;
  check(button("核对保存结果") && dirty && savedHandler && discardHandler, "Known receipt did not preserve read-only reconciliation and navigation protection");
  failRead = false; saveMode = "normal"; await click("核对保存结果");
  check(requests.length === written && document.body.textContent.includes("适用性待解释"), "Known receipt replayed a write or treated exceptions as executed constraints");

  workspaces[project.id].artifacts.propertyRules.candidate.source = "agent";
  workspaces[project.id].reviews.push({ id: "stale-rule-review", artifactKind: "propertyRules", artifactRevisionId: workspaces[project.id].artifacts.propertyRules.candidate.id,
    projectRevision: workspaces[project.id].revision - 1, status: "ready", checks: [], selections: {} });
  await render(project.id, 10);
  check(document.body.textContent.includes("模型生成的规则提案：尚未生效"), "Model origin was hidden or presented as approved");
  check(button("采用规则")?.disabled && button("建立规则审阅"), "Stale review enabled formal adoption");
  await click("建立规则审阅");
  check(!button("采用规则")?.disabled && !button("建立规则审阅"), "Old ready review hid a newer current-revision review");
  workspaces[project.id].artifacts.propertyRules.locked = true; await render(project.id, 11);
  check(button("采用规则")?.disabled && button("编辑规则候选")?.disabled, "Locked rules still offered writes");
  workspaces[project.id].artifacts.propertyRules.locked = false; await render(project.id, 12);

  await click("编辑规则候选");
  workspaces[project.id].revision += 1; await render(project.id, 1);
  check(await savedHandler() === false && document.body.textContent.includes("版本已变化"), "Unified save bypassed the invalidated editor baseline");
  check(discardHandler() === true, "Explicit discard failed outside a write"); await tick(); await tick();
  const corrupt = structuredClone(workspaces[project.id]);
  workspaces[project.id].artifacts.propertyRules.candidate.payload.definitions = [null];
  await render(project.id, 2);
  check(button("编辑规则候选")?.disabled && document.body.textContent.includes("不可解析"), "Corrupt rule payload reached uncontrolled rendering");
  workspaces[project.id] = corrupt; await click("重新读取规则");

  await click("编辑规则候选"); await field("属性键", "ownerTwo");
  saveMode = "defer"; button("保存规则候选").click();
  check(discardHandler() === false && await savedHandler() === false, "Same-frame save allowed discard or another save"); await tick();
  await render("rules-B");
  releaseSave(); await tick(); await tick();
  check(!document.body.textContent.includes("ownerTwo") && !dirty && !document.querySelector(".property-rule-editor"), "Late author save polluted another project");
  saveMode = "normal";
  const hold = {}; heldRead = hold; await render(project.id, 3);
  await render("rules-B");
  workspaces[project.id].artifacts.propertyRules.candidate.payload.definitions[0].description = "新的 A 生命周期规则";
  await render(project.id, 4);
  hold.release(); await tick(); await tick();
  check(document.body.textContent.includes("新的 A 生命周期规则") && !document.body.textContent.includes("物品的所有者"), "A-B-A response replaced the new lifetime or lost the durable candidate");
  const rule = workspaces[project.id].artifacts.propertyRules.candidate.payload.definitions[0];
  check(ruleInputError([{ ...rule, description: "汉".repeat(171) }], []) && ruleInputError([rule, rule], [])
    && ruleInputError([{ ...rule, schema: { ...rule.schema, valueType: { type: "entity", kind: "unknown" } } }], []), "Typed input limits or duplicate rules were not validated");
  const eventRule = { eventType: "transfer", description: "接收物品的组织成为所有者", exceptions: [],
    condition: { type: "not", condition: { type: "state", role: "actor", subjectKind: "character", predicate: rule.predicate,
      quantifier: "all", operator: "eq", operand: { type: "participant", role: "recipient", quantifier: "any" } } } };
  const candidate = workspaces[project.id].artifacts.propertyRules.candidate;
  candidate.payload.eventRules = [eventRule];
  for (const change of [
    { ...eventRule, condition: { type: "all", conditions: [] } },
    { ...eventRule, condition: { type: "state", role: "location", subjectKind: "character", predicate: rule.predicate,
      quantifier: "all", operator: "eq", operand: { type: "participant", role: "recipient", quantifier: "any" } } },
    { ...eventRule, condition: { ...eventRule.condition, execute: "code" } },
  ]) check(ruleInputError([rule], [change]), "Invalid or executable expression passed UI validation");
  await render(project.id, 20);
  check(document.body.textContent.includes("四值条件解释") && document.body.textContent.includes("不认证世界真相") && document.body.textContent.includes("任一接收者"), "Condition expression was hidden or represented as satisfied");
  await click("编辑规则候选"); await field("说明", "仅调整属性说明，事件条件须保留"); await click("保存规则候选");
  check(JSON.stringify(requests.at(-1).payload.eventRules) === JSON.stringify([eventRule]), "Structural rule editing discarded event conditions");
  await click("编辑规则候选"); await field("属性键", "renamed");
  const beforeInvalid = requests.length; await click("保存规则候选");
  check(requests.length === beforeInvalid && document.body.textContent.includes("表达式界限无效"), "Dangling event property reference reached Runtime");
  await click("移除事件规则 1"); await click("保存规则候选");
  check(requests.at(-1).payload.eventRules.length === 0, "Explicit event condition removal did not survive saving");
  const displayed = workspaces[project.id].artifacts.propertyRules.candidate;
  displayed.payload.definitions.push({ predicate: "conditionText", subjectKind: "character", description: "文字比较",
    schema: { cardinality: "single", valueType: { type: "text" } }, exceptions: [] });
  displayed.payload.eventRules = [{ eventType: "x".repeat(80), description: "有限长文字比较", exceptions: [], condition: {
    type: "all", conditions: [{ type: "not", condition: { type: "not", condition: { type: "not", condition: {
      type: "state", role: "actor", subjectKind: "character", predicate: "conditionText", quantifier: "all",
      operator: "eq", operand: { type: "text", value: "x".repeat(512) } } } } }] } }];
  await render(project.id, 21);
  const expressionPanel = document.querySelector(".property-rules-panel");
  check(expressionPanel.scrollWidth <= expressionPanel.clientWidth + 1, "Maximum nested condition text overflowed the author column");
  root.unmount(); return { passed: true };
}
run().then((result) => { window.testResult = result; }).catch((error) => { window.testResult = { passed: false, error: error.stack + "\nDOM:" + document.body.innerText }; });
