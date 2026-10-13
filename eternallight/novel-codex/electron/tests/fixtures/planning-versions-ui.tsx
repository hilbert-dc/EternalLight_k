import React from "react";
import { createRoot } from "react-dom/client";
import { ModuleRecordsPopover } from "../../../src/ModuleRecordsPopover";
import { ModuleRecordsButton } from "../../../src/ModuleRecordsButton";
import "../../../src/module-records.css";
import "../../../src/planning-reading.css";
import "../../../src/styles.css";
const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (value, message) => { if (!value) throw Error(message); };
const until = async (predicate, message) => { for (let i = 0; i < 150 && !predicate(); i++) await tick(); check(predicate(), message); };
const revision = (id, version, payload) => ({ id, version, payload: structuredClone(payload), parentRevisionId: null, baseProjectRevision: 1, source: "author", createdAt: null });
const payloads = {
 brief: { premise: "主角寻找母亲留下的信。", promise: "追寻记忆", stakes: "失去记忆", endingDirection: "待定" },
 outline: { nodes: [{ id: "book", kind: "book", parentId: null, position: 0, title: "完整故事", purpose: "寻找真相", summary: "起因\n转折\n结局", certainty: "settled", protection: "free", dependencyKeys: [] }] },
 plan: { chapters: [{ chapterPlanId: "plan-one", chapterNumber: 1, goal: "找到线索", publicCommitments: ["兑现来信"], allowances: ["增加环境描写"], scenes: [{ id: "scene", goal: "检查信件", conflict: "信息矛盾", beats: [{ id: "beat", description: "发现暗号", locked: false }], expectedEffects: [{ id: "effect", dimension: "knowledge", description: "得知来信来源", commitmentRefs: [] }] }] }] },
};
const make = id => ({ project: { id, title: id, logline: "" }, revision: 7, artifacts: Object.fromEntries(Object.entries(payloads).map(([kind, payload]) => [kind, {
 accepted: revision(`${id}-${kind}-accepted`, 1, payload), candidate: revision(`${id}-${kind}-candidate`, 3, payload),
 working: revision(`${id}-${kind}-historical`, 2, payload), locked: false,
 workingSelection: { objectKind: kind, objectId: kind, revisionId: `${id}-${kind}-historical`, selectionRevision: 4, selectedAt: "now" },
}])) });
let projectId = "a", kind = "brief", blocked = false, refreshes = 0, forbiddenWrites = 0, outlineNodeId: string | null = null;
const records = { a: make("a"), b: make("b") }, calls = [], snapshots = [], scopedSnapshots = [], delayed = [];
let holdRead = false, holdWrite = false, failRead = false, failWrite = false, loseReceipt = false, failRefresh = false, failAfterWrite = false;
const api = { runtime: {
 getCreativeWorkspace: async id => { const value = structuredClone(records[id]); if (holdRead) { holdRead = false; await new Promise(resolve => delayed.push(resolve)); } if (failRead) throw Error("读取连接中断"); return value; },
 setWorkingBaseline: async (id, k, objectId, revisionId, expectedSelectionRevision) => {
  calls.push({ operation: "select", id, kind: k, objectId, revisionId, expectedSelectionRevision });
  if (holdWrite) { holdWrite = false; await new Promise(resolve => delayed.push(resolve)); }
  if (failWrite) throw Error("CAS 已过期");
  const artifact = records[id].artifacts[k]; check(expectedSelectionRevision === artifact.workingSelection.selectionRevision, "Wrong selection CAS");
  artifact.working = structuredClone(artifact.candidate); artifact.workingSelection = { ...artifact.workingSelection, revisionId, selectionRevision: expectedSelectionRevision + 1 };
  if (failAfterWrite) failRead = true;
  if (loseReceipt) throw Error("保存回执断线"); return structuredClone(artifact.workingSelection);
 },
 clearWorkingBaseline: async (id, k, objectId, expectedSelectionRevision) => {
  calls.push({ operation: "clear", id, kind: k, objectId, expectedSelectionRevision });
  const artifact = records[id].artifacts[k]; check(expectedSelectionRevision === artifact.workingSelection.selectionRevision, "Wrong clear CAS");
  artifact.working = null; artifact.workingSelection = { ...artifact.workingSelection, revisionId: null, selectionRevision: expectedSelectionRevision + 1 };
  return { ...artifact.workingSelection, cleared: true };
 },
 setArtifactLock: async (id, k, locked, expectedRevision) => {
  calls.push({ operation: "lock", id, kind: k, locked, expectedRevision }); check(expectedRevision === records[id].revision, "Wrong lock CAS");
  records[id].artifacts[k].locked = locked; records[id].revision++; return { artifactKind: k, locked, resultRevision: records[id].revision };
 },
 acceptPublicationReview: async () => { forbiddenWrites++; },
} };
let recordsOpen = true;
const container = document.createElement("main"); container.className = "main-panel"; container.style.cssText = "position:absolute;left:220px;top:24px;width:740px;height:880px;overflow:auto"; document.getElementById("root")!.appendChild(container);
const fixtureRoot = createRoot(container);
const render = async () => { fixtureRoot.render(<><div className="content-header"><h1>当前模块</h1><ModuleRecordsButton label="当前模块" open={recordsOpen} onClick={() => { recordsOpen = !recordsOpen; void render(); }} /></div><textarea aria-label="后台编辑区" defaultValue="尚未保存的作者内容" /><button id="background-action">后台操作</button><input aria-label="后台对话" />{recordsOpen && <ModuleRecordsPopover source={{ projectId, view: kind === "plan" ? "plans" : kind as any, chapterId: null, outlineNodeId, volumeId: outlineNodeId ? "actual-volume" : null, label: "当前模块" }} api={api as any} dirty={false} blocked={blocked} opener={container.querySelector('[aria-label="查看修改记录"]')} container={container} onClose={() => { recordsOpen = false; void render(); }} onFormalChange={async () => { refreshes++; if (failRefresh) throw Error("页面更新断线"); }} />}</>); await tick(); await tick(); };
const button = text => [...document.querySelectorAll<HTMLButtonElement>("button")].find(node => node.textContent === text || node.getAttribute("aria-label") === text)!;
const popover = () => document.querySelector<HTMLDivElement>(".module-records-popover:popover-open")!;
const pointer = async (element: HTMLElement) => { element.scrollIntoView({ block: "nearest" }); const rect = element.getBoundingClientRect(); for (const type of ["mouseDown", "mouseUp"]) { window.testPointer = { type, x: Math.round(rect.left + rect.width / 2), y: Math.round(rect.top + rect.height / 2), button: "left", clickCount: 1 }; await until(() => !window.testPointer, "Native pointer did not settle"); } await tick(); };
const key = async keyCode => { window.testKey = { keyCode }; await until(() => !window.testKey, "Native keyboard did not settle"); await tick(); };

const settings = () => document.querySelector(".planning-writing-settings")! as HTMLDetailsElement;
const click = async text => { const target = button(text); if (target.closest(".planning-writing-settings") && !settings().open) { settings().querySelector("summary")!.click(); await tick(); } target.click(); await tick(); await tick(); };
const text = () => document.querySelector(".planning-versions-panel")!.textContent!;
const ready = () => !button("将已保存稿用于后续创作").disabled;
const reload = async () => { await click("重新读取并核对"); };
(async () => { try {
 await render(); await until(ready, "Initial source not ready");
 check(!settings().open && !document.querySelector(".planning-versions-panel")!.innerText.includes("a-brief-historical") && document.querySelector(".planning-versions-current")!.textContent!.includes("尚未采用"), "Default settings exposed identities or treated another adopted source as current adoption");
 settings().querySelector("summary")!.click(); await tick(); check(calls.length === 0, "Opening settings wrote"); const mountedAction = button("将已保存稿用于后续创作"); settings().querySelector("summary")!.click(); await tick(); settings().querySelector("summary")!.click(); await tick(); check(button("将已保存稿用于后续创作") === mountedAction && calls.length === 0, "Disclosure unmounted controls or wrote");
 check(text().includes("a-brief-historical"), "Historical working source was replaced");
 check([...document.querySelectorAll("summary")].some(node => node.textContent === "查看后续创作参考内容"), "Historical working payload inaccessible");
 holdWrite = true; button("将已保存稿用于后续创作").click(); button("将已保存稿用于后续创作").click(); await tick(); check(calls.length === 1, "Double click repeated write"); delayed.shift()(); await until(() => text().includes("已保存并核对"), "Selection not verified");
 await click("取消指定依据"); await click("锁定已采用内容"); settings().open = false; check(document.querySelector(".planning-versions-panel")!.innerText.includes("请展开写作设置") && calls.length === 3, "Lock hidden after collapsing"); await click("解除锁定");
 check(JSON.stringify(calls.slice(0, 4)) === JSON.stringify([
  { operation: "select", id: "a", kind: "brief", objectId: "brief", revisionId: "a-brief-candidate", expectedSelectionRevision: 4 },
  { operation: "clear", id: "a", kind: "brief", objectId: "brief", expectedSelectionRevision: 5 },
  { operation: "lock", id: "a", kind: "brief", locked: true, expectedRevision: 7 },
  { operation: "lock", id: "a", kind: "brief", locked: false, expectedRevision: 8 },
 ]), "CAS requests differ");
 failWrite = true; await click("将已保存稿用于后续创作"); const failedCount = calls.length; settings().open = false; check(!document.querySelector('[role="alert"]')!.closest("details") && document.querySelector(".planning-versions-panel")!.innerText.includes("CAS 已过期"), "Collapsed settings hid failed write"); await click("将已保存稿用于后续创作"); check(calls.length === failedCount && text().includes("CAS 已过期"), "Failed write retried without read");
 failRead = true; await reload(); check(text().includes("a-brief-candidate") && !ready(), "Read failure erased sources or enabled write"); failRead = false; failWrite = false; await reload(); check(ready(), "Explicit read failed to restore controls");
 loseReceipt = true; await click("将已保存稿用于后续创作"); const uncertainCount = calls.length; check(!ready(), "Lost receipt falsely succeeded"); loseReceipt = false; await reload(); check(calls.length === uncertainCount && button("将已保存稿用于后续创作").disabled && !button("取消指定依据").disabled, "Reconcile replayed mutation or missed saved source");
 await click("取消指定依据"); failAfterWrite = true; await click("将已保存稿用于后续创作"); const submittedCount = calls.length; check(text().includes("操作已提交，但结果尚未确认") && !ready(), "Readback failure claimed success"); failAfterWrite = false; failRead = false; await reload(); check(calls.length === submittedCount && !button("取消指定依据").disabled, "Readback recovery repeated save");
 await click("取消指定依据"); failRefresh = true; await click("将已保存稿用于后续创作"); check(text().includes("操作已保存并核对，但作品视图更新失败"), "Verified mutation mislabeled write failure"); failRefresh = false;
 kind = "outline"; holdRead = true; await render(); const staleRead = delayed.shift(); projectId = "b"; await render(); check(text().includes("b-outline-candidate"), "B did not read while A pending"); projectId = "a"; await render(); staleRead(); await tick(); check(text().includes("a-outline-candidate"), "ABA stale source installed");
 holdWrite = true; await click("将已保存稿用于后续创作"); const oldWrite = delayed.shift(); projectId = "b"; await render(); check(ready(), "Old flight blocked new scope"); oldWrite(); await tick(); await tick(); check(text().includes("b-outline-historical"), "Late mutation crossed scope");
 blocked = true; await render(); const beforeBlocked = calls.length; await click("将已保存稿用于后续创作"); check(calls.length === beforeBlocked, "Blocked mutation dispatched"); blocked = false; await render();
 for (const k of ["brief", "outline", "plan"]) {
  kind = k; await render(); const comparison = document.querySelector(".planning-versions-comparison")! as HTMLDetailsElement; check(comparison, "Distinct sources missing comparison"); comparison.open = true;
  comparison.querySelectorAll("details").forEach(node => { node.open = true; }); await tick();
  const sections = [...comparison.querySelectorAll(":scope > section")]; check(sections.length === 2, "Comparison missing saved source");
  const expected = k === "brief" ? ["主角寻找母亲留下的信。", "追寻记忆"] : k === "outline" ? ["寻找真相", "起因", "转折", "结局"] : ["找到线索", "兑现来信", "增加环境描写", "发现暗号", "信息矛盾", "得知来信来源"];
  check(sections.every(section => expected.every(value => section.textContent!.includes(value))), "Saved preview omitted actual story content");
 }
 records.b.artifacts.plan.accepted = records.b.artifacts.plan.candidate; await reload(); check(!document.querySelector(".planning-versions-comparison") && document.querySelector(".planning-versions-current")!.textContent!.endsWith("· 已采用"), "Same source compared or current adoption wrong"); records.b.artifacts.plan.accepted = null; await reload(); check(!document.querySelector(".planning-versions-comparison") && button("锁定已采用内容").disabled && document.querySelector<HTMLElement>('.planning-record-content[aria-label="当前已保存内容"]')?.checkVisibility() && document.querySelector(".planning-record-content")?.textContent.includes("找到线索") && text().includes("取消指定依据后"), "Empty adopted source fabricated comparison or hid content");
 const node = (identity, purpose, extra = {}) => ({ ...payloads.outline.nodes[0], ...identity, kind: "volume", title: "同名卷", purpose, ...extra });
 const first = node({ id: "volume-first" }, "第一卷阶段"), second = node({ id: undefined, clientKey: "volume-second" }, "第二卷阶段");
 records.b.artifacts.outline.candidate.payload.nodes = [payloads.outline.nodes[0], first, second, node({ id: "unit-first" }, "第一卷独有单元", { kind: "unit", parentId: "volume-first" })];
 records.b.artifacts.outline.accepted.payload.nodes = [second]; records.b.artifacts.outline.working.payload.nodes = [first, { ...first, purpose: "重复身份不得猜测" }];
 kind = "outline"; outlineNodeId = "volume-first"; blocked = true; await render();
 const expandPreviews = async () => { document.querySelectorAll(".planning-versions-panel details").forEach(node => { (node as HTMLDetailsElement).open = true; }); await tick(); };
 await expandPreviews(); let currentPreview = document.querySelector(".planning-versions-comparison > section")!;
 check(currentPreview.textContent!.includes("第一卷独有单元") && !currentPreview.textContent!.includes("第二卷阶段") && !currentPreview.textContent!.includes("寻找真相"), "Scoped preview leaked another same-name volume or book");
 check(document.querySelectorAll('.planning-versions-panel [role="status"]').length >= 3 && text().includes("不包含可唯一定位的本卷"), "Missing or duplicate historical scope was guessed");
 check(text().includes("作用于整份大纲"), "Scoped settings presented independent volume authority"); const beforeScoped = calls.length; await click("将已保存稿用于后续创作"); check(calls.length === beforeScoped, "Dirty/blocked scoped record wrote");
 scopedSnapshots.push({ outlineNodeId, text: document.querySelector(".planning-versions-panel")!.innerText, writes: calls.length - beforeScoped });
 holdRead = true; outlineNodeId = "volume-second"; await render(); const lateScopeRead = delayed.shift(); outlineNodeId = "volume-first"; records.b.artifacts.outline.candidate.payload.nodes[1].purpose = "新的第一卷阶段"; await render(); lateScopeRead(); await tick(); await expandPreviews(); check(text().includes("新的第一卷阶段") && !text().includes("第二卷阶段"), "Late same-project volume scope overwrote current source");
 outlineNodeId = "volume-second"; await render(); await expandPreviews(); currentPreview = document.querySelector(".planning-versions-comparison > section")!; check(currentPreview.textContent!.includes("第二卷阶段") && !currentPreview.textContent!.includes("新的第一卷阶段"), "Client-key scope fallback selected same-name node");
 scopedSnapshots.push({ outlineNodeId, text: document.querySelector(".planning-versions-panel")!.innerText });
 records.b.artifacts.outline.candidate.payload.nodes = records.b.artifacts.outline.candidate.payload.nodes.map(node => (node.id ?? node.clientKey) === "volume-second" ? { ...node, kind: "unit", purpose: "改成单元后不再视作本卷" } : node);
 blocked = false; await reload(); await expandPreviews(); currentPreview = document.querySelector(".planning-versions-comparison > section")!; check(!ready() && currentPreview.textContent!.includes("不包含可唯一定位的本卷") && !currentPreview.textContent!.includes("改成单元后"), "Same ID with non-volume kind was treated as a volume");
 scopedSnapshots.push({ outlineNodeId, kindChanged: true, text: document.querySelector(".planning-versions-panel")!.innerText, settingsDisabled: !ready() });
 outlineNodeId = "missing-volume"; blocked = false; await render(); await expandPreviews(); check(!ready() && text().includes("写作设置暂不可修改") && !text().includes("第二卷阶段"), "Unavailable current scope fell back or enabled settings");
 scopedSnapshots.push({ outlineNodeId, text: document.querySelector(".planning-versions-panel")!.innerText, settingsDisabled: !ready() });
 outlineNodeId = "volume-first"; await render();
 for (const theme of ["light", "dark"]) for (const width of [330, 740]) {
  document.documentElement.dataset.theme = theme; container.style.width = `${width}px`; await tick(); await tick(); const main = popover(); const body = main.querySelector<HTMLDivElement>(".module-records-body")!;
  document.querySelectorAll(".planning-versions-panel > details").forEach(node => { (node as HTMLDetailsElement).open = false; }); body.scrollTop = 0; await tick();
  const panelBounds = main.getBoundingClientRect(), containerBounds = container.getBoundingClientRect(); check(panelBounds.width <= 441 && panelBounds.left >= containerBounds.left && panelBounds.right <= containerBounds.right + 2 && Math.abs(panelBounds.width - Math.min(440, width - 24)) <= 2, "Resized popover escaped the main editing column or retained old width");
  check(main.scrollWidth <= main.clientWidth + 2, "Narrow panel horizontal overflow");
  check(main.querySelector("header")!.getBoundingClientRect().top < document.querySelector(".planning-versions-panel")!.getBoundingClientRect().top, "Module header placed below records panel");
  check(document.querySelector<HTMLElement>(".planning-record-content")!.checkVisibility() && !document.querySelector(".planning-record-content")!.closest("details") && !settings().open && !document.querySelector<HTMLDetailsElement>(".planning-versions-comparison")!.open, "Saved content was not default-visible or advanced comparison/settings escaped disclosures");
  const savedPreview = document.querySelector<HTMLElement>(".planning-record-content")!; check(savedPreview.innerText.includes("新的第一卷阶段") && [...savedPreview.querySelectorAll<HTMLElement>("dd")].some(item => item.textContent === "新的第一卷阶段" && item.checkVisibility()), "Default saved node purpose was folded or hidden");
  body.scrollTop = 0; await tick(); window.testScreenshot = `planning-records-default-${theme}-${width}`; await until(() => !window.testScreenshot, "Default screenshot did not settle");
  const defaultText = document.querySelector(".planning-versions-panel")!.innerText; check(!defaultText.includes("b-outline-") && !defaultText.includes("将已保存稿用于后续创作"), "Default view exposed advanced sources/actions");
  snapshots.push({ theme, width, defaultText, settingsOpen: settings().open, comparisonLabel: document.querySelector(".planning-versions-comparison > summary")!.textContent, controls: [...main.querySelectorAll("button")].map(node => ({ text: node.textContent, disabled: node.disabled })) });
  const comparison = document.querySelector(".planning-versions-comparison")! as HTMLDetailsElement; comparison.open = true; comparison.querySelectorAll("details").forEach(node => { node.open = true; }); await tick();
  const list = comparison.querySelector(".planning-reading-list")!, card = comparison.querySelector(".planning-reading-card")!, description = card.querySelector("dd")!;
  const layout = { listStyle: getComputedStyle(list).listStyleType, descriptionMargin: parseFloat(getComputedStyle(description).marginLeft), cardDisplay: getComputedStyle(card).display };
  check(layout.listStyle === "none" && layout.descriptionMargin <= 1 && layout.cardDisplay === "grid", `Popover outline inherited native list/description layout: ${JSON.stringify(layout)}`);
  body.scrollTop = comparison.offsetTop - body.offsetTop; await tick(); window.testScreenshot = `planning-versions-${theme}-${width}`; await until(() => !window.testScreenshot, "Screenshot did not settle");
  settings().querySelector("summary")!.click(); await tick(); check(main.scrollWidth <= main.clientWidth + 2, "Expanded settings horizontal overflow"); window.testScreenshot = `planning-versions-settings-${theme}-${width}`; await until(() => !window.testScreenshot, "Settings screenshot did not settle");
 }
 const backgroundEditor = container.querySelector<HTMLTextAreaElement>('[aria-label="后台编辑区"]')!; const draft = backgroundEditor.value; backgroundEditor.setSelectionRange(2, 5);
 const opener = button("查看修改记录"); const beforeInteraction = calls.length;
 const body = popover().querySelector<HTMLElement>(".module-records-body")!; body.scrollTop = body.scrollHeight; await tick(); const bounds = popover().getBoundingClientRect(), closeBox = button("关闭修改记录").getBoundingClientRect();
 check(bounds.width <= 441 && closeBox.top >= bounds.top && closeBox.bottom <= bounds.bottom && !popover().hasAttribute("aria-modal") && !container.inert, "Compact long records hid close or made the editor modal");
 await pointer(container.querySelector<HTMLButtonElement>("#background-action")!); await until(() => !popover(), "Background click did not light-dismiss"); check(document.activeElement?.id === "background-action" && backgroundEditor.value === draft && calls.length === beforeInteraction, "Light-dismiss stole target focus, draft or wrote");
 await pointer(opener); await until(() => popover(), "Records did not reopen"); await key("Escape"); await until(() => !popover(), "Panel Escape did not close"); check(document.activeElement === opener, "Panel Escape lost opener");
 await pointer(opener); await until(() => popover(), "Records did not reopen for background Escape"); backgroundEditor.focus({ preventScroll: true }); backgroundEditor.setSelectionRange(2, 5); await key("Escape"); await until(() => !popover(), "Background Escape did not close native popover"); check(document.activeElement === backgroundEditor && backgroundEditor.selectionStart === 2 && backgroundEditor.selectionEnd === 5, "Background Escape stole editor focus/caret");
 await pointer(opener); await until(() => popover(), "Records did not reopen for explicit close"); await pointer(button("关闭修改记录")); await until(() => !popover(), "Close did not settle"); check(document.activeElement === opener && calls.length === beforeInteraction && backgroundEditor.value === draft, "Close mutated authority or lost exact focus");
 window.testResult = { passed: true, interaction: { nonmodal: true, compact: true, backgroundFocus: true, nativeEscape: true, sameEditorAndCaret: true, noExtraWrites: true }, snapshots, scopedSnapshots, calls, refreshes, forbiddenWrites };
} catch (error) { window.testResult = { passed: false, error: error.stack ?? String(error), calls }; } })();
