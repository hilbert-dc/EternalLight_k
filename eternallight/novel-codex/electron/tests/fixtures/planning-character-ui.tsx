import "../../../src/styles.css";
import "../../../src/page-layout.css";
import React, { useState } from "react";
import { createRoot } from "react-dom/client";
import { PlanningCharacterProvider, PlanningCharacterSelect } from "../../../src/PlanningCharacterContext";

window.addEventListener("error", event => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
const tick = () => new Promise(resolve => setTimeout(resolve, 25));
const check = (ok, message) => { if (!ok) throw new Error(message); };
const requests = [], changes = [], snapshots = [], saves = [];
const api = { runtime: { getIdentityCatalog: projectId => new Promise((resolve, reject) => requests.push({ projectId, resolve, reject })) } };
const catalog = prefix => ({ entries: ["a", "b", "place"].map(id => ({ id: `${prefix}-raw-${id}`, canonicalId: `${prefix}-raw-a`, kind: id === "place" ? "location" : "character", resolutionState: id === "b" ? "authorMerged" : "candidate", claims: [{ displayName: "林夕", aliases: [], sourceObjectId: "draft-first", sourceRevisionId: "revision-first", evidence: { quote: "林夕握着旧信，望向海面。", start: 0, end: 13 } }] })), ambiguousNames: [], identityRevisionId: null, identityDecisions: [], truncated: false, ambiguityScanComplete: true });
let setProject, setRefresh, setDisabled;
function Harness() {
  const [project, changeProject] = useState("A"), [refresh, changeRefresh] = useState(0), [disabled, changeDisabled] = useState(false);
  const [value, changeValue] = useState("unknown-original");
  setProject = changeProject; setRefresh = changeRefresh; setDisabled = changeDisabled;
  return <main className="main-panel"><section className="view-panel"><div className="plan-advanced">
    <input aria-label="作者备注" defaultValue="待完成的章纲" />
    <PlanningCharacterProvider api={api} projectId={project} refreshToken={refresh}>
      <PlanningCharacterSelect label="第一章视角人物" value={value} disabled={disabled} onChange={id => { changes.push(id); changeValue(id); }} />
      <PlanningCharacterSelect label="第二章视角人物" value="other-unknown" onChange={id => changes.push(id)} />
    </PlanningCharacterProvider>
    <button onClick={() => saves.push({ povCharacterId: value })}>保存规划</button>
  </div></section></main>;
}
const root = createRoot(document.getElementById("root")); root.render(<Harness />);
const select = () => document.querySelector('select[aria-label="第一章视角人物"]');
const options = () => [...select().options].map(option => ({ value: option.value, label: option.textContent }));
const click = async text => { [...document.querySelectorAll("button")].find(button => button.textContent === text).click(); await tick(); };
const choose = async value => { const element = select(); element.value = value; element.dispatchEvent(new Event("change", { bubbles: true })); await tick(); };
async function run() {
  await tick(); await tick(); check(requests.length === 1, "Two selectors performed duplicate directory reads");
  const note = document.querySelector("input"); note.value = "作者仍在编辑"; note.focus(); note.setSelectionRange(2, 5);
  check(select().value === "unknown-original" && changes.length === 0, "Loading silently changed an unknown POV");
  requests[0].resolve(catalog("A")); await tick();
  check(options().length === 4 && options().every(option => !option.value.includes("place")), "Directory used non-character or canonical choices");
  check(options()[2].label !== options()[3].label, "Same name and quote sources were indistinguishable");
  check(changes.length === 0 && select().value === "unknown-original", "Catalog refresh dirtied existing POV");
  await choose("A-raw-b"); await click("保存规划");
  check(JSON.stringify(saves) === JSON.stringify([{ povCharacterId: "A-raw-b" }]), "Merged source was saved as canonical identity");
  snapshots.push({ case: "exact raw sources", options: options(), saved: saves[0], unknownSecond: document.querySelectorAll("select")[1].value, detailsClosed: [...document.querySelectorAll("details")].every(detail => !detail.open) });
  setDisabled(true); await tick(); await choose("A-raw-a"); check(changes.length === 1 && select().disabled, "Disabled selector changed planning"); setDisabled(false); await tick();
  setRefresh(1); await tick(); requests[1].reject(new Error("只读目录暂不可用")); await tick();
  check(select().value === "A-raw-b" && options().some(option => option.value === "A-raw-a") && document.querySelector('[role="alert"]'), "Failed refresh lost existing selection or data");
  await click("重新读取人物来源"); requests[2].resolve(catalog("A")); await tick(); check(changes.length === 1 && !document.querySelector('[role="alert"]'), "Retry mutated the plan or kept a stale error");
  setRefresh(2); await tick(); const late = requests[3];
  setProject("B"); await tick(); check(!options().some(option => option.value === "A-raw-a"), "New project displayed old choices"); const projectB = requests[4];
  setProject("A"); await tick(); requests[5].resolve(catalog("A-new")); await tick();
  late.resolve(catalog("A-old")); projectB.reject(new Error("旧作品错误")); await tick();
  check(options().some(option => option.value === "A-new-raw-a") && !options().some(option => option.value.startsWith("A-old")) && !document.querySelector('[role="alert"]'), "A-B-A accepted an old response or error");
  check(select().value === "A-raw-b" && changes.length === 1 && note === document.querySelector("input") && note.value === "作者仍在编辑" && note.selectionStart === 2 && note.selectionEnd === 5, "Directory updates remounted inputs or rewrote the old value");
  snapshots.push({ case: "scope and retry", options: options(), changes: [...changes], reads: requests.map(request => request.projectId), notePreserved: true });
  for (const theme of ["light", "dark"]) for (const width of [330, 740]) {
    document.documentElement.dataset.theme = theme; document.querySelector("main").style.width = `${width}px`; await tick();
    check(select().getBoundingClientRect().right <= document.querySelector("main").getBoundingClientRect().right + 1, "POV selector overflowed a narrow pane");
    snapshots.push({ case: `${theme} ${width}`, fits: true, optionsReadable: getComputedStyle(select()).color !== getComputedStyle(select()).backgroundColor });
  }
  root.unmount(); window.testResult = { passed: true, snapshots, changes, saves };
}
run().catch(error => { window.testResult = { passed: false, error: error.stack, reads: requests.length, changes, dom: document.body.innerHTML }; });
