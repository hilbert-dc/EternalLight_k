import React from "react";
import { createRoot } from "react-dom/client";
import { CharacterCatalog } from "../../../src/CharacterCatalog";
import "../../../src/page-layout.css";
import "../../../src/styles.css";

const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (value, message) => { if (!value) throw Error(message); };
const until = async (predicate, label = "Character read did not settle") => { for (let i = 0; i < 180 && !predicate(); i++) await tick(); check(predicate(), label); };
const entry = (id, name, kind = "character") => ({ id, canonicalId: id, resolutionState: "candidate", kind,
  claims: [{ displayName: name, aliases: kind === "character" ? ["小岑"] : [], sourceObjectId: "draft-chapter-one", sourceRevisionId: "saved-source", modelingBatchId: "batch-one",
    evidence: { quote: id === "other-lamp" ? "另一位岑灯在岸边等待。" : "岑灯来到灯塔门前。", start: 0, end: id === "other-lamp" ? 11 : 9 } }] });
const initial = { entries: [entry("lamp", "岑灯"), entry("other-lamp", "岑灯"), entry("parent", "岑母"), entry("tower", "灯塔", "location")],
  ambiguousNames: [{ name: "岑灯", candidateIds: ["lamp", "other-lamp"], canonicalIds: ["lamp", "other-lamp"], authorDecidedDistinct: false }],
  identityRevisionId: null, identityDecisions: [], truncated: false, ambiguityScanComplete: true };
const chapters = [{ id: "chapter-one", number: 1, title: "灯塔", status: "draft", summary: "", updated_at: "", passages: [] },
  { id: "chapter-two", number: 2, title: "归航", status: "draft", summary: "", updated_at: "", passages: [] }];
const artifact = () => ({ candidate: null, accepted: null, locked: false, working: null, workingSource: null });
let batchVersion = 1, identityAdopted = false, longWork = false;
const workspace = projectId => ({ project: { id: projectId, title: "人物体验", logline: "一座灯塔" }, revision: 1, headSnapshotId: `snapshot-${projectId}`, sourceDigest: `digest-${projectId}-batch-${batchVersion}`,
  artifacts: { brief: artifact(), outline: artifact(), identity: identityAdopted ? { ...artifact(), candidate: { id: "identity-adopted", payload: { decisions: [] } }, accepted: { id: "identity-adopted", payload: { decisions: [] } } } : artifact(), propertyRules: artifact(), plan: { ...artifact(), candidate: { id: "plan-current", payload: { sourceOutlineRevisionId: "outline-current", chapters: [
    { chapterPlanId: "plan-lamp", chapterNumber: 3, title: "未来归航", goal: "人物视角规划：寻找归航的船", conflict: "船尚未靠岸", povCharacterId: "lamp", scenes: [] },
    { chapterPlanId: "plan-other", chapterNumber: 4, goal: "同名另一人物的未来", povCharacterId: "other-lamp", scenes: [] },
    { chapterPlanId: "plan-name", chapterNumber: 5, goal: "岑灯这个名字出现在目标中", povCharacterId: null, scenes: [] },
  ] } } } }, chapters: chapters.map(chapter => ({ ...chapter, candidate: { id: "saved-source", payload: { body: "岑灯来到灯塔门前。" } }, accepted: null })),
  planLineageStatus: "current", workingBaselines: [], modeling: [1, 2].map(chapter => ({ objectId: `draft-chapter-${chapter === 1 ? "one" : "two"}`, sourceRevisionId: "saved-source", status: "ready", batchId: `batch-${chapter}-version-${batchVersion}`, confirmationState: "candidate", entities: [], assertions: [] })) });
const coverage = { complete: true, pendingSources: [], truncatedSources: false, pendingSourcesCount: 0, pendingSourceIdsTruncated: false, matchingSourcesCount: 2,
  scannedSourcesCount: 2, semanticCompleteness: "unknown", unknownChapterSources: 0, truncatedEntries: false, unknownTimeSkipped: 0, unavailablePropositionSources: 0,
  findingsTruncated: false, uncertainConflictPairs: 0, unknownStateTimes: 0, unappliedPropertyRules: 0, confirmedEntries: 1, candidateEntries: 4 };
const claim = (index, kind, fields, chapter = 1) => ({ sourceObjectId: `draft-chapter-${chapter === 1 ? "one" : "two"}`, sourceRevisionId: "saved-source", modelingBatchId: `batch-${chapter}`, assertionIndex: index,
  sourceChapter: chapter, canonicalSubjectId: fields.subjectId ?? "lamp", canonicalTargetId: fields.targetId ?? null, canonicalEpistemicOwnerId: fields.epistemicOwnerId ?? null,
  propositionRef: null, confirmationState: index === 0 ? "confirmed" : "candidate", confirmedFactId: index === 0 ? "fact-one" : null, relation: null,
  assertion: { kind, subjectId: "lamp", predicate: "目标", value: "找到归航的船", evidence: { quote: "岑灯来到灯塔门前。", start: 0, end: 9 }, storyTime: null, ...fields } });
const lampEntries = [claim(0, "assertion", {}), claim(1, "event", { subjectId: "arrival-event", predicate: "抵达灯塔", storyTime: 80,
  value: { participants: [{ role: "actor", entityId: "lamp" }], preconditions: [], effects: [], untilExclusive: null } }),
  claim(2, "relation", { subjectId: "parent", targetId: "lamp", predicate: "parentOf", relationType: "parentOf", propositionId: "parent-proposition", value: true }),
  claim(3, "epistemic", { subjectId: "secret", epistemicOwnerId: "lamp", predicate: "knows", propositionId: "secret", epistemicState: "known", epistemicTime: 9, value: "航船尚未归来" }, 2),
  claim(4, "event", { subjectId: "wait-event", predicate: "等待归航", value: { participants: [{ role: "witness", entityId: "lamp" }], preconditions: [], effects: [], untilExclusive: null } }, 2)];
let catalog = initial, catalogFail = false, knowledgeFail = false, holdCatalog = false, holdKnowledge: string | null = null, releaseCatalog, releaseKnowledge;
let mismatch = "", partial = false, watermarkChange = false, workspaceSequence = 0, advanceOnKnowledge = false;
const reads = [], opened = [], prepared = [], snapshots = [];
const counters = { writes: 0, models: 0 };
const knowledge = (projectId, entityId) => ({ projectId, snapshotId: null, perspective: "author", identityRevisionId: catalog.identityRevisionId,
  selectors: { entityId, predicate: null }, entries: projectId === "A" ? entityId === "lamp" ? [...lampEntries, ...(batchVersion > 1 ? [claim(5, "assertion", { value: "未发布正文的新分析批次已读取" })] : [])] : [claim(8, "assertion", { subjectId: entityId, value: "另一位人物独有目标" })] : [claim(9, "assertion", { subjectId: entityId, value: `${projectId}作品人物独有资料`, evidence: { quote: "乙作品人物独有原文。", start: 0, end: 10 } })], findings: [], coverage: partial ? { ...coverage, complete: false, truncatedEntries: true, pendingSources: ["draft-chapter-two"] } : coverage });
const api = { runtime: new Proxy({
  getCreativeWorkspace: async projectId => { reads.push(["workspace", projectId]); const value = workspace(projectId);
    if (longWork) { value.chapters.push(...Array.from({ length: 99 }, (_, index) => ({ ...value.chapters[1], id: `later-${index}`, number: index + 3 })));
      value.modeling = value.chapters.filter(chapter => chapter.id !== "chapter-one").map(chapter => ({ ...value.modeling[1], objectId: `draft-${chapter.id}` })); }
    if (watermarkChange && ++workspaceSequence > 1) value.sourceDigest += "-changed-batch"; return structuredClone(value); },
  getIdentityCatalog: async projectId => { reads.push(["catalog", projectId]); const value = { ...structuredClone(catalog), snapshotId: null };
    if (holdCatalog) return new Promise(resolve => { releaseCatalog = resolve; }); if (catalogFail) throw Error("controlled catalog read failure"); return value; },
  getCharacterKnowledge: async (projectId, entityId) => { reads.push(["knowledge", projectId, entityId]); const value = structuredClone(knowledge(projectId, entityId)); value.entries = value.entries.map(row => ({ ...row, modelingBatchId: `batch-${row.sourceChapter}-version-${batchVersion}` }));
    if (advanceOnKnowledge) { advanceOnKnowledge = false; batchVersion++; }
    if (holdKnowledge === entityId) return new Promise(resolve => { releaseKnowledge = resolve; }); if (knowledgeFail) throw Error("controlled knowledge read failure");
    if (mismatch === "snapshot") value.snapshotId = "wrong-snapshot"; if (mismatch === "identity") value.identityRevisionId = "wrong-identity"; if (mismatch === "selector") value.selectors.entityId = "other-lamp";
    if (mismatch === "batch") value.entries[0].modelingBatchId = "wrong-present-batch"; return value; },
}, { get(target, key) { if (key in target) return target[key]; return async () => { counters.writes++; throw Error(`Unexpected runtime call ${String(key)}`); }; } }) };
const root = createRoot(document.getElementById("root"));
const render = (projectId = "A", refreshToken = 0) => root.render(<div className="workspace"><main className="main-panel" data-view="characters" style={{ width: 700, height: 650 }}><header className="content-header"><h1>角色</h1></header><CharacterCatalog api={api as any} projectId={projectId} refreshToken={refreshToken}
  chapters={chapters as any} onOpenChapter={chapter => opened.push(chapter.id)} onDiscuss={prompt => prepared.push(prompt)} /></main></div>);
const text = () => document.querySelector(".character-catalog")?.textContent ?? "";
const buttons = () => [...document.querySelectorAll<HTMLButtonElement>(".character-select")];
const button = label => [...document.querySelectorAll<HTMLButtonElement>("button")].find(item => item.textContent === label);
const ready = () => button("讨论人物故事线") && !button("讨论人物故事线").disabled;
const capture = state => snapshots.push({ state, identities: buttons().map(item => ({ label: item.textContent, selected: item.getAttribute("aria-pressed") })),
  biography: document.querySelector(".character-biography")?.textContent ?? null, story: [...document.querySelectorAll(".character-story-chapter")].map(item => item.textContent),
  future: document.querySelector(".character-plans")?.textContent ?? null, warning: document.querySelector(".character-catalog-warning")?.textContent ?? null });
(async () => { try {
  render(); await until(ready);
  check(buttons().length === 3 && buttons().slice(0, 2).every(item => item.textContent.includes("同名待核对")), "Same name entities merged without an author decision");
  check(document.querySelectorAll(".content-header h1").length === 1 && document.querySelector(".character-catalog-card h2").textContent === "岑灯", "Page and selected profile headings lost their hierarchy");
  check(text().includes("抵达灯塔") && text().includes("岑母") && text().includes("航船尚未归来") && text().includes("等待归航"), "Entity-scoped participant, target relation or epistemic-owner row was dropped");
  check(document.querySelector(".character-biography").textContent.includes("作者已确认") && text().includes("分析建议") && !text().includes("性格勇敢"), "Candidates were confirmed or personality was invented");
  check([...document.querySelectorAll(".character-story-chapter h4")].map(item => item.textContent).join("|") === "第 1 章 · 灯塔|第 2 章 · 归航", "Source chapter order became inferred story-time chronology");
  check(document.querySelector(".character-plans").textContent.includes("寻找归航") && !document.querySelector(".character-plans").textContent.includes("另一人物") && !document.querySelector(".character-storyline").textContent.includes("未来归航"), "Name-matched plan or future event appeared as an occurred story");
  const source = document.querySelector<HTMLDetailsElement>(".character-sources"); source.open = true; await tick(); source.querySelector<HTMLButtonElement>("button").click();
  check(source.textContent.includes("已保存编辑稿") && !source.textContent.includes("saved-source"), "Source revision authority was lost or raw ID replaced readable provenance");
  button("讨论人物故事线").click(); check(prepared[0].includes("人物身份：lamp") && counters.writes === 0, "Discussion did not freeze the selected identity or executed a write"); capture("同名角色与已记录经历");
  for (const theme of ["light", "dark"]) for (const width of [300, 700]) { document.documentElement.dataset.theme = theme; document.querySelector<HTMLElement>("main").style.width = `${width}px`; await tick();
    for (const item of [...document.querySelectorAll<HTMLElement>(".character-catalog,.character-list,.character-catalog-card,.character-profile-section,button")]) check(item.scrollWidth <= item.clientWidth + 2, `Character UI overflowed ${theme}/${width}: ${item.className}`);
  }
  holdKnowledge = "other-lamp"; releaseKnowledge = null; buttons()[1].click(); await until(() => releaseKnowledge); const oldEntity = releaseKnowledge;
  check(!text().includes("抵达灯塔") && button("讨论人物故事线").disabled, "A previous selected role remained actionable while another role loaded");
  holdKnowledge = null; buttons()[0].click(); await until(ready); oldEntity(knowledge("A", "other-lamp")); await tick(); check(!text().includes("另一位人物独有目标"), "Late entity response crossed selection incarnation");
  knowledgeFail = true; catalog = { ...initial }; render("A", 1); await until(() => text().includes("controlled knowledge read failure"));
  check(text().includes("抵达灯塔") && button("讨论人物故事线").disabled, "Failed same-snapshot read discarded prior entries or left stale discussion enabled");
  knowledgeFail = false; button("重新读取人物经历").click(); await until(ready); capture("经历读取失败后只读重试");
  catalog = { ...initial, identityRevisionId: "identity-merged", entries: initial.entries.map(item => item.id === "other-lamp" ? { ...item, canonicalId: "lamp", resolutionState: "authorMerged" } : item), ambiguousNames: [] };
  render("A", 2); await until(() => ready() && buttons().length === 2 && text().includes("身份已合并"));
  check(text().includes("名称与原文来源 · 2 处") && document.querySelector(".character-plans").textContent.includes("同名另一人物的未来"), "Author same-identity closure lost exact member sources or raw POV links"); capture("作者合并与精确视角规划");
  catalog = { ...initial, identityRevisionId: "identity-distinct", ambiguousNames: initial.ambiguousNames.map(group => ({ ...group, authorDecidedDistinct: true })) }; partial = true;
  render("A", 3); await until(() => ready() && buttons().length === 3 && text().includes("已明确区分")); check(text().includes("仅为已读取范围"), "Partial knowledge claimed a complete biography"); capture("明确区分与部分覆盖");
  catalogFail = true; render("A", 4); await until(() => text().includes("controlled catalog read failure")); check(buttons().length === 3 && text().includes("保留上次读取结果") && button("讨论人物故事线").disabled, "Catalog failure lost last good records or stale source remained actionable");
  catalogFail = false; button("重新读取角色").click(); await until(ready);
  for (const mode of ["snapshot", "identity", "selector"]) { mismatch = mode; render("A", 5 + ["snapshot", "identity", "selector"].indexOf(mode)); await until(() => text().includes("作品已变化")); check(button("讨论人物故事线").disabled, `${mode} mismatch allowed discussion`); }
  mismatch = ""; watermarkChange = true; workspaceSequence = 0; render("A", 8); await until(() => text().includes("作品已变化")); check(button("讨论人物故事线").disabled, "Changed workspace watermark installed an actionable mixed read");
  watermarkChange = false; render("A", 9); await until(ready);
  holdCatalog = true; releaseCatalog = null; render("A", 10); await until(() => releaseCatalog); const oldProject = releaseCatalog;
  holdCatalog = false; const bEntry = entry("B-character", "乙作品人物"); bEntry.claims[0].evidence = { quote: "乙作品人物独有原文。", start: 0, end: 10 };
  catalog = { ...initial, entries: [bEntry], ambiguousNames: [], truncated: true, ambiguityScanComplete: false };
  render("B"); await until(() => ready() && text().includes("B作品人物独有资料")); oldProject({ ...initial, snapshotId: null }); await tick();
  check(!text().includes("岑灯") && text().includes("仅展示部分角色来源"), "Late project catalog contaminated the active project or hid incomplete identity coverage");
  catalog = initial; render("A", 11); await until(() => ready() && text().includes("抵达灯塔")); check(!text().includes("B作品人物独有资料"), "ABA project return reused another project's knowledge");
  capture("跨人物和作品隔离及水位校验");
  const priorSnapshot = workspace("A").headSnapshotId; batchVersion++; identityAdopted = true; render("A", 12);
  await until(() => ready() && text().includes("未发布正文的新分析批次已读取"));
  check(workspace("A").headSnapshotId === priorSnapshot && workspace("A").chapters.every(chapter => chapter.accepted === null)
    && !text().includes("有身份建议尚未采用"), "Unpublished analysis required publication, stale batch stayed visible, or accepted identity still appeared pending");
  capture("同一正式快照下未发布正文分析更新");
  mismatch = "batch"; render("A", 13); await until(() => text().includes("作品已变化")); check(button("讨论人物故事线").disabled, "A present current-modeling row accepted the wrong batch");
  mismatch = ""; longWork = true; render("A", 14); await until(() => ready() && text().includes("抵达灯塔"));
  const longWorkspace = await api.runtime.getCreativeWorkspace("A");
  check(longWorkspace.chapters.length === 101 && longWorkspace.modeling.length === 100 && !longWorkspace.modeling.some(item => item.objectId === "draft-chapter-one")
    && text().includes("航船尚未归来"), "The bounded workspace modeling preview erased a character's authoritative current knowledge");
  capture("101章有界分析预览之外的人物仍可读取");
  for (const [theme, width] of [["light", 700], ["dark", 300]] as const) {
    window.testViewport = { width: width + 24, height: 900 }; await until(() => !window.testViewport, "Character viewport did not settle");
    document.documentElement.dataset.theme = theme; document.querySelector<HTMLElement>("main").style.width = `${width}px`; document.querySelector<HTMLElement>("main").scrollTop = 0; await tick();
    const picker = document.querySelector<HTMLSelectElement>('.character-picker select');
    if (width === 300) {
      check(picker.checkVisibility() && !document.querySelector<HTMLElement>(".character-list").checkVisibility()
        && picker.options[0].text !== picker.options[1].text, `Narrow character picker is invisible, duplicates the long list, or fails to distinguish same-name sources: ${JSON.stringify({ picker: picker.checkVisibility(), list: document.querySelector<HTMLElement>(".character-list").checkVisibility(), pickerDisplay: getComputedStyle(picker.closest("label")).display, listDisplay: getComputedStyle(document.querySelector(".character-list")).display, mainWidth: document.querySelector("main").clientWidth, container: getComputedStyle(document.querySelector("main")).containerName, options: [...picker.options].map(item => item.text) })}`);
      picker.value = "other-lamp"; picker.dispatchEvent(new Event("change", { bubbles: true }));
      await until(() => ready() && text().includes("另一位人物独有目标"));
      check(reads.filter(row => row[0] === "knowledge").at(-1)[2] === "other-lamp" && !text().includes("抵达灯塔"), "Narrow selector changed only the name instead of exact entity-scoped knowledge");
      picker.value = "lamp"; picker.dispatchEvent(new Event("change", { bubbles: true })); await until(() => ready() && text().includes("抵达灯塔"));
      snapshots.push({ state: "窄屏原生选择角色", options: [...picker.options].map(option => option.text), selected: picker.value, duplicateListVisible: false });
    } else check(!picker.checkVisibility() && document.querySelector<HTMLElement>(".character-list").checkVisibility(), "Wide layout lost the visible character directory");
    window.testScreenshot = `character-story-${theme}-${width}`; await until(() => !window.testScreenshot, "Character screenshot did not finish");
  }
  const sourceBeforeRecovery = workspace("A").sourceDigest; advanceOnKnowledge = true; render("A", 15);
  await until(() => Boolean(button("重新读取人物经历")) && text().includes("作品已变化"));
  check(workspace("A").sourceDigest !== sourceBeforeRecovery && button("讨论人物故事线").disabled, "Knowledge read did not reject a permanently advanced source watermark");
  const retryReadsStart = reads.length; const recoverySource = workspace("A").sourceDigest;
  button("重新读取人物经历").click(); await until(ready);
  check(workspace("A").sourceDigest === recoverySource && !document.querySelector('[role="alert"]')
    && reads.slice(retryReadsStart).filter(row => row[0] === "catalog").length === 1
    && reads.slice(retryReadsStart).filter(row => row[0] === "workspace").length === 3,
    "Retry did not refresh the catalog/workspace against the new source or silently rolled back the watermark");
  capture("分析水位永久变化后完整只读恢复");
  check(reads.filter(row => row[0] === "knowledge").every(row => row.length === 3 && typeof row[2] === "string") && counters.writes === 0 && counters.models === 0, "Role page performed a global knowledge scan, write, or model call");
  root.unmount(); window.testResult = { passed: true, snapshots, reads: reads.length, opened, prepared: prepared.length, ...counters };
} catch (error) { window.testResult = { passed: false, error: error.stack, dom: document.body.innerText, reads: reads.slice(-12) }; } })();
