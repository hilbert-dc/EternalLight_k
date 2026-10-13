import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { StoryReferenceView, type StoryReferenceKind } from "../../../src/StoryReferenceView";

const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (value, message) => { if (!value) throw Error(message); };
const until = async (predicate, message = "Reference read did not settle") => { for (let i = 0; i < 150 && !predicate(); i++) await tick(); check(predicate(), message); };
const text = () => document.body.textContent;
const button = label => [...document.querySelectorAll<HTMLButtonElement>("button")].find(button => button.textContent === label);
const scene = { id: "stable-scene", sceneId: null, body: "灯亮了，旧钟还没有停止。", locked: true };
const revision = { id: "saved-A", chapterId: "c1", title: "旧钟", version: 2, parentRevisionId: "published-A", baseProjectRevision: 7,
  sourcePlanRefs: { localOnly: true, lineageStatus: "current" }, body: scene.body, sceneGroups: [scene], source: "author", createdAt: null };
const workspace = project => ({ project: { id: project, title: `${project}作品`, logline: "" }, revision: 7, headSnapshotId: `snapshot-${project}`,
  chapters: [{ id: "c1", number: 1, title: `${project}旧钟`, candidate: { ...revision, id: `saved-${project}` }, accepted: { ...revision, id: `published-${project}`, version: 1 } },
    { id: "c2", number: 2, title: "尚未写作", candidate: null, accepted: null }] });
const entry = (index, kind, predicate, time, confirmationState = "candidate") => ({ canonicalSubjectId: "lamp", sourceObjectId: "draft-c1", sourceRevisionId: "saved-A",
  modelingBatchId: "batch-A", assertionIndex: index, confirmationState, assertion: { kind, predicate, value: kind === "event" ? { participants: [], preconditions: [], effects: [] } : true,
    storyTime: time, evidence: { quote: `A原文：${predicate}。`, start: 0, end: 4 } } });
const initial = [entry(0, "event", "点灯", 10), entry(1, "event", "进门", 2, "confirmed"), entry(2, "event", "藏信", null), entry(3, "state", "灯已亮", null), entry(4, "epistemic", "相信信件已到", null, "confirmed")];
let entries = initial, fail = false, wrongSnapshot = false, hold = false, release, emptyCommitments = false;
const reads = [], opened = [], discussed = [], sources = [], decisions = [], snapshots = [];
let draft = "我还没发出的构想", executions = 0;
const item = id => ({ id, summary: `伏笔${id}`, kind: "foreshadowing", status: "open", revisionId: `commitment-revision-${id}`, confirmationState: "candidate", sourceCurrent: true });
const api = { runtime: {
  getCreativeWorkspace: async project => { reads.push(["workspace", project]); return workspace(project); },
  getHistoricalKnowledgeState: async (project, snapshot) => {
    reads.push(["knowledge", project, snapshot]);
    if (hold) return new Promise(resolve => { release = resolve; });
    if (fail) throw Error("controlled knowledge failure");
    return { projectId: project, snapshotId: wrongSnapshot ? "wrong" : snapshot,
      entries: structuredClone(entries).map(row => ({ ...row, sourceRevisionId: `saved-${project}`, assertion: { ...row.assertion, evidence: { quote: `${project}原文：${row.assertion.predicate}。` } } })),
      coverage: { complete: false, truncatedEntries: false, pendingSources: ["draft-pending"] } };
  },
  listCommitments: async (project, cursor) => { reads.push(["commitments", project, cursor]); return { snapshotId: `snapshot-${project}`, nextCursor: emptyCommitments || cursor ? null : "page2", data: emptyCommitments ? [] : [{ ...item(cursor ? "第二条" : "第一条"), status: cursor ? "fulfilled" : "open" }] }; },
  getCommitment: async (project, id, snapshot) => { reads.push(["detail", project, id, snapshot]); return { projectId: project, snapshotId: snapshot, objectId: `commitment-${id}`, revisionId: "detail-r1",
    payload: { id, summary: "旧钟反复敲响", fulfillmentCriteria: "后文解释钟声", setupEvidenceRefs: [{ objectId: "draft-c1", revisionId: `saved-${project}`, quote: "旧钟还没有停止。", start: 4, end: 12 }], fulfillmentEvidenceRefs: [] }, sourceCurrent: true, confirmationState: "candidate" }; },
}, agent: { sendPrompt: () => { executions++; throw Error("Reference page executed a model"); } } };
const root = createRoot(document.getElementById("root"));
let view: StoryReferenceKind = "timeline", project = "A", token = 0;
const render = (nextView = view, nextProject = project) => {
  view = nextView; project = nextProject;
  root.render(<React.StrictMode><main className="main-panel"><header className="content-header"><h1>{view}</h1></header><StoryReferenceView api={api as any} projectId={project} view={view} refreshToken={token}
    onOpenCommitment={request => decisions.push(request)} onOpenChapter={id => opened.push(id)} onDiscuss={(prompt, source) => { sources.push(source); discussed.push(prompt); if (!draft.trim()) draft = prompt; }} /></main></React.StrictMode>);
};
(async () => { try {
  render(); await until(() => text().includes("时间尚未指定"));
  const heads = [...document.querySelectorAll(".story-reference-card header strong")].map(node => node.textContent);
  check(JSON.stringify(heads) === JSON.stringify(["进门", "点灯", "藏信"]), "Timeline guessed story order or included timeless non-event claims");
  check(text().includes("作者已确认") && text().includes("未确认候选") && text().includes("范围不完整"), "Timeline overstated confirmation or coverage");
  check(reads.some(row => row[0] === "knowledge" && row[2] === "snapshot-A"), "Timeline did not read the captured workspace snapshot");
  snapshots.push({ view, events: heads, knownTimes: [2, 10], unknown: "藏信", complete: false });
  const readsBeforeFilter = reads.length;
  button("时间已知").click(); await tick();
  check(document.querySelectorAll(".story-reference-card").length === 2 && !text().includes("藏信"), "Known-time filter guessed unknown chronology");
  button("未指定").click(); await tick();
  check(document.querySelectorAll(".story-reference-card").length === 1 && text().includes("藏信") && text().includes("显示 1 / 3 条"), "Unknown-time filter lost loaded coverage");
  button("在对话中梳理时间").click(); await tick();
  check(reads.length === readsBeforeFilter && executions === 0 && draft === "我还没发出的构想" && sources.at(-1) === undefined, "Reference controls read/write/executed or overwrote author draft");
  snapshots.push({ controls: document.querySelector(".story-reference-controls").textContent, filter: "unknown", preparationOnly: true });
  render("world"); await until(() => text().includes("相信信件已到"));
  check(document.querySelectorAll(".story-reference-card").length === 2 && text().includes("人物认知") && !text().includes("点灯"), "World confused event or belief with confirmed world truth");
  fail = true; token++; render(); await until(() => document.querySelector('[role="alert"]'));
  check(text().includes("上次读取结果") && text().includes("灯已亮"), "Read failure discarded good scoped data or hid its age");
  fail = false; button("重新读取资料").click(); await until(() => !document.querySelector('[role="alert"]'));
  hold = true; token++; render(); await until(() => release); const old = release;
  hold = false; render("world", "B"); await until(() => text().includes("B原文"));
  old({ projectId: "A", snapshotId: "snapshot-A", entries: initial, coverage: { complete: true, truncatedEntries: false, pendingSources: [] } }); await tick();
  check(!text().includes("A原文") && text().includes("B原文"), "Late project data crossed the current owner");
  fail = true; token++; render(); await until(() => document.querySelector('[role="alert"]'));
  fail = false; hold = true; release = null; render("world", "C"); await until(() => release);
  check(!document.querySelector('[role="alert"]') && !text().includes("B原文") && text().includes("正在读取作品资料"), "Old project errors or rows leaked while the new project was loading");
  hold = false; release({ projectId: "C", snapshotId: "snapshot-C", entries: [], coverage: { complete: true, truncatedEntries: false, pendingSources: [] } });
  await until(() => text().includes("暂无故事设定主张"));
  wrongSnapshot = true; token++; render("timeline", "D"); await until(() => document.querySelector('[role="alert"]'));
  check(document.querySelectorAll(".story-reference-card").length === 0 && text().includes("快照不一致"), "Wrong snapshot installed claims");
  wrongSnapshot = false; entries = []; token++; render(); await until(() => text().includes("暂无事件"));
  snapshots.push({ view: "world/timeline", staleReadPreserved: true, projectIsolated: true, snapshotRejected: true, empty: "已读取范围暂无事件或明确故事时刻" });
  check(button("全部").getAttribute("aria-pressed") === "true", "Reference filter crossed a view/project owner");
  button("时间已知").click(); await tick();
  check(text().includes("没有匹配条目") && !text().includes("暂无事件"), "Filtered empty was falsely reported as an empty store");
  button("清除筛选").click(); await tick();
  check(text().includes("暂无事件"), "Clearing filter did not restore the true loaded empty state");
  render("plot", "A"); await until(() => reads.findLast(row => row[0] === "workspace")?.[1] === "A" && document.querySelector(".story-reference")?.getAttribute("aria-busy") === "false" && text().includes("伏笔第一条"), "A plot owner did not reread");
  check(text().includes("不代表全部情节线") && !text().includes("作者已确认"), "Commitment directory invented plot coverage or confirmation");

  button("已兑现").click(); await tick();
  check(document.querySelectorAll(".story-reference-list .story-reference-card").length === 0 && text().includes("没有匹配条目") && !text().includes("尚无已建立"), "Fulfilled filter fabricated empty directory");
  snapshots.push({ view: "plot-filter", controls: document.querySelector(".story-reference-controls").textContent, furtherPageAccessible: Boolean(button("加载更多")) });
  button("清除筛选").click(); await tick();  const moreBefore = reads.filter(row => row[0] === "commitments").length;
  const more = button("加载更多"); more.click(); more.click(); await until(() => text().includes("伏笔第二条"));
  check(reads.filter(row => row[0] === "commitments").length === moreBefore + 1 && document.querySelectorAll(".story-reference-list .story-reference-card").length === 2, "Pagination duplicated requests or lost entries");

  const loadedReads = reads.length;
  button("未兑现").click(); await tick();
  check(text().includes("伏笔第一条") && !text().includes("伏笔第二条"), "Open filter included fulfilled commitment");
  button("已兑现").click(); await tick();
  check(!text().includes("伏笔第一条") && text().includes("伏笔第二条"), "Fulfilled filter lost the second loaded page");
  check(reads.length === loadedReads, "Filtering fetched beyond loaded scope");
  render("plot", "B"); await until(() => reads.findLast(row => row[0] === "workspace")?.[1] === "B" && document.querySelector(".story-reference")?.getAttribute("aria-busy") === "false" && button("全部")?.getAttribute("aria-pressed") === "true" && text().includes("伏笔第一条"), "B plot owner did not reset/read");
  check(!text().includes("伏笔第二条"), "Project filter/read state crossed scope");
  render("plot", "A"); await until(() => reads.findLast(row => row[0] === "workspace")?.[1] === "A" && document.querySelector(".story-reference")?.getAttribute("aria-busy") === "false" && text().includes("伏笔第一条"), "A plot owner did not reread");
  button("加载更多").click(); await until(() => text().includes("伏笔第二条"), "Reopened A pagination did not settle");
  button("查看原文与记录").click(); await until(() => text().includes("后文解释钟声"));
  check(reads.some(row => row[0] === "detail" && row[3] === "snapshot-A") && text().includes("旧钟还没有停止。"), "Commitment detail lost fixed-snapshot evidence");
  button("查看所在章节").click(); button("收起记录").click(); await tick();
  check(!text().includes("后文解释钟声"), "Read-only detail failed to close");
  for (const theme of ["light", "dark"]) for (const width of [330, 740]) {
    document.documentElement.dataset.theme = theme; document.getElementById("root").style.width = `${width}px`; await tick();
    const pane = document.querySelector<HTMLElement>(".story-reference");
    check(pane.scrollWidth <= pane.clientWidth + 2 && [...pane.querySelectorAll<HTMLElement>(".story-reference-card")].every(card => card.scrollWidth <= card.clientWidth + 2), "Reference cards overflowed narrow/theme layout");
  }
  snapshots.push({ view, candidates: 2, detailSource: "旧钟还没有停止。", pageReads: 1, executions });
  emptyCommitments = true; token++; render(); await until(() => text().includes("尚无已建立"));
  const beforePreparation = reads.length;
  button("在对话中梳理伏笔").click(); await tick();
  check(executions === 0 && reads.length === beforePreparation && draft === "我还没发出的构想" && discussed.at(-1).includes("不自动确认"), "Empty-directory action skipped explicit send or author confirmation");
  snapshots.push({ view: "plot-empty", action: button("在对话中梳理伏笔").textContent, empty: document.querySelector(".empty-state").textContent });
  root.unmount(); window.testResult = { passed: true, snapshots, opened, executions };
} catch (error) { window.testResult = { passed: false, error: error.stack, dom: document.body.innerText }; } })();
