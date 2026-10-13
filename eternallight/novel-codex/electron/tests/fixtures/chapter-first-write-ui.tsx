import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { CreativeWorkspaceView, prepareWritingRequest } from "./creative-workspace-harness";
import { StoppedExecutionError } from "../../../src/execution-stop";

window.addEventListener("error", event => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
const tick = () => new Promise(resolve => setTimeout(resolve, 30));
const check = (value, message) => { if (!value) throw new Error(message); };
const until = async (predicate, message) => { for (let i = 0; i < 100 && !predicate(); i++) await tick(); check(predicate(), message); };
const button = text => [...document.querySelectorAll("button")].find(item => item.textContent === text || item.getAttribute("aria-label") === text);
const click = async text => { const item = button(text); check(item && !item.disabled, `Unavailable: ${text}`); item.click(); await tick(); await tick(); };
const input = async (selector, value) => {
  const item = document.querySelector(selector); check(item && !item.disabled, `Unavailable: ${selector}`);
  Object.getOwnPropertyDescriptor(item instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype, "value").set.call(item, value);
  item.dispatchEvent(new Event("input", { bubbles: true })); await tick();
};
const chapterFor = id => ({ id, number: 1, title: `${id} 第一章`, status: "draft", summary: "", updated_at: "", passages: [] });
const revision = (id, title, body) => ({ id, title, body, version: 1, sourcePlanRefs: { lineageStatus: "current", localOnly: true }, sceneGroups: [{ id: `group-${id}`, sceneId: null, body, locked: false }] });
const workspaceFor = id => ({ project: { id }, revision: 1, headSnapshotId: "head-a", sourceDigest: "a".repeat(64), index: { status: "ready" }, artifacts: Object.fromEntries(["brief", "outline", "plan"].map(kind => [kind, { candidate: null, accepted: null, working: null, locked: false }])), chapters: [{ ...chapterFor(id), candidate: null, accepted: null, working: null, workingSelection: null }], workingBaselines: [], continuationIntents: [], planningReviews: [], commits: [], tasks: [], impacts: [], modeling: [], factProposals: [], reviews: [], chapterPlanBindings: [] });
let workspace = workspaceFor("first-write"), available = false, holdRead = false, releaseRead, releaseGeneration, failAfterGeneration = false, readFailure = false, stopGeneration = false;
const generations = [], saves = [], baselines = [], intents = [], snapshots = [];
const api = { runtime: {
  getWorkCatalog: async () => ({ revision: 0, volumes: [], placements: [] }),
  getCreativeWorkspace: async () => {
    const captured = structuredClone(workspace);
    if (holdRead) await new Promise(resolve => { releaseRead = resolve; });
    if (readFailure) throw new Error("首写候选读取中断");
    return captured;
  },
  listPlanningImpacts: async () => ({ data: [], nextCursor: null, counts: { total: 0, open: 0 } }),
  listGenerationRuns: async () => ({ runs: [] }), listWorkTasks: async () => ({ tasks: [], nextCursor: null, activeCount: 0 }),
  ...Object.fromEntries(["Search", "Entity", "Relation", "Knowledge", "SnapshotKnowledge", "Fact"].map(kind => [`get${kind}Audit`, async () => ({ job: null })])),
  getWriterPreview: async (projectId, projectRevision, readingChapter, sceneOrder, povCharacterId, storyTime) => ({ purpose: "writer", taskId: null, target: { projectId, projectRevision, position: { readingChapter, sceneOrder, povCharacterId, storyTime } }, plannedEffects: [], relevantText: [], coverage: { complete: true, missingScope: [], truncationReasons: [], scannedScope: {} }, budgetsUsed: { byteSize: 0, byteLimit: 60000 } }),
  saveDraftCheckpoint: async (_, payload) => { saves.push(payload); workspace.revision++; workspace.chapters[0].candidate = { ...revision("local-saved", payload.title, payload.body), sceneGroups: payload.sceneGroups }; return { revisionId: "local-saved" }; },
  setWorkingBaseline: async (_, kind, chapterId, revisionId) => { baselines.push({ kind, chapterId, revisionId }); workspace.chapters[0].workingSelection = { revisionId, selectionRevision: 1 }; workspace.chapters[0].working = workspace.chapters[0].candidate; },
  saveContinuationIntent: async (_, kind, chapterId, revisionId, intent) => { intents.push({ kind, chapterId, revisionId, intent }); },
}, agent: {} };
const root = createRoot(document.getElementById("root"));
const render = async () => {
  const chapter = chapterFor(workspace.project.id);
  root.render(<CreativeWorkspaceView taskConversationContainer={null} api={api} projectId={workspace.project.id} view="chapters" state={{ project: workspace.project, novelEdition: { chapters: [chapter] }, canon: { characters: [] } }} selectedChapterId={chapter.id}
    onSelectChapter={() => {}} onGenerateWork={async () => {}} onContinueWork={async () => {}} onRetryWork={async () => {}} onResolveFeedback={async () => {}} onReviewPlanning={async () => {}} onCancelPlanningReview={async () => {}}
    onGenerateChapter={async (target, intent) => {
      generations.push({ projectId: workspace.project.id, chapterId: target.id, intent });
      if (stopGeneration) throw new StoppedExecutionError();
      await new Promise(resolve => { releaseGeneration = resolve; });
      workspace.revision++; workspace.chapters[0].candidate = revision(`generated-${generations.length}`, target.title, "模型实际保存的首章候选。");
      if (failAfterGeneration) readFailure = true;
    }} onOpenPlanningTarget={() => {}} onReturnToFeedback={() => {}} onFormalChange={async () => {}} onDirtyChange={() => {}} agentAvailable={available} agentRunning={false} refreshToken={0} />);
  await tick(); await tick(); await tick();
};
const nextScope = async id => { workspace = workspaceFor(id); holdRead = false; readFailure = false; failAfterGeneration = false; await render(); await until(() => button("让 AI 起草本章"), "Fresh chapter did not offer first writing"); };
let authorIntent = "";
const sendWriting = async () => { const request = await prepareWritingRequest(); void request.submit(authorIntent); await tick(); await tick(); };

(async () => { try {
  await render(); await until(() => button("让 AI 起草本章"), "Empty chapter has no start action");
  const prepared = await prepareWritingRequest();
  check(button("让 AI 起草") && document.querySelectorAll('.chapter-document [aria-label="让 AI 起草本章"]').length === 1 && !document.querySelector('.chapter-page-toolbar [aria-label="让 AI 起草本章"]'), "Empty chapter CTA was missing or duplicated in the toolbar");
  snapshots.push({ writingIntent: prepared.defaultIntent, chapterBodyEditors: document.querySelectorAll('.chapter-document textarea').length });
  authorIntent = "写一个约 400 字的港城悬疑开场。";
  await sendWriting();
  check(generations.length === 0 && saves.length === 0 && authorIntent.includes("400 字"), "Missing model consumed intent or attempted execution");
  available = true; await render();
  await input('[aria-label="章节标题"]', "还没有保存的新标题"); await sendWriting();
  await until(() => button("恢复章节标题"), "Title-only edit had no explicit recovery");
  check(document.querySelector('[aria-label="章节标题"]').value === "还没有保存的新标题" && generations.length === 0 && saves.length === 0, "First writing silently discarded a title or fabricated a checkpoint");
  snapshots.push({ titleProtection: document.querySelector('.draft-main-column [role="alert"]').textContent });
  await click("恢复章节标题");
  check(document.querySelector('[aria-label="章节标题"]').value === "first-write 第一章" && authorIntent.includes("400 字"), "Explicit title restoration consumed intent");
  holdRead = true; const request = await prepareWritingRequest(); void request.submit(authorIntent); void request.submit(authorIntent); await tick();
  await until(() => releaseRead, "First write did not check fresh workspace");
  check(document.querySelector('textarea[aria-label="场景 1 正文"]').disabled && generations.length === 0, "Pending source check did not protect the editor");
  holdRead = false; releaseRead(); await until(() => generations.length === 1, "Fresh empty chapter did not start generation");
  check(saves.length === 0 && baselines.length === 0 && intents.length === 0, "First writing created an empty checkpoint or invented a saved source");
  failAfterGeneration = true; releaseGeneration();
  await until(() => button("重新读取章节内容"), "Successful first writing plus failed read has no read-only recovery");
  snapshots.push({ result: document.querySelector('.creative-alert.success')?.textContent, recovery: button("重新读取章节内容").textContent });
  readFailure = false; await click("重新读取章节内容");
  check(generations.length === 1 && saves.length === 0 && workspace.chapters[0].accepted === null && document.querySelector('textarea[aria-label="场景 1 正文"]').value.includes("模型实际保存"), "Recovery repeated model execution or adopted the candidate");

  await nextScope("manual-then-write"); authorIntent = "保留我的开头，接着写港口的冲突。";
  await input('textarea[aria-label="场景 1 正文"]', "作者已经写下的开头。");
  check(!button("让 AI 起草本章") && !button("讨论本章"), "Manual prose retained a forced writer/discussion entry");
  await click("保存"); await until(() => workspace.chapters[0].candidate?.id === "local-saved", "Manual opening was not saved");
  check(saves.length === 1 && saves[0].body === "作者已经写下的开头。" && generations.length === 1 && baselines.length === 0 && intents.length === 0, "Manual prose save started a writer or changed writing baselines");
  snapshots.push({ manualOpening: "保存后使用普通对话", forcedWriterEntry: false, modelCalls: generations.length });

  workspace = workspaceFor("saved-empty"); workspace.chapters[0].candidate = revision("empty-saved", "saved-empty 第一章", "");
  await render(); await until(() => button("让 AI 起草本章"), "Saved empty prose lost its unique draft CTA");
  const savedEmptyRequest = await prepareWritingRequest();
  check(savedEmptyRequest.defaultIntent.includes("2500") && saves.length === 1, "Saved empty chapter changed default intent or created a checkpoint");
  check(await savedEmptyRequest.submit("长".repeat(32001)) === false && await savedEmptyRequest.submit("长".repeat(16001)) === false && generations.length === 1 && baselines.length === 0, "Oversized intent read or bound a saved-empty source");
  authorIntent = savedEmptyRequest.defaultIntent; void savedEmptyRequest.submit(authorIntent);
  await until(() => generations.length === 2, "Saved empty chapter did not start against its saved source");
  check(baselines.length === 1 && baselines[0].revisionId === "empty-saved" && intents.length === 1 && intents[0].revisionId === "empty-saved", "Saved empty chapter bypassed its exact saved-source binding as a first write");
  releaseGeneration(); await until(() => !button("让 AI 起草本章") && !document.querySelector('textarea[aria-label="场景 1 正文"]').disabled, "Saved empty result did not restore ordinary prose view");
  check(workspace.chapters[0].accepted === null && saves.length === 1, "Saved-empty writing adopted prose or made an empty checkpoint");
  snapshots.push({ savedEmpty: "保留已保存来源", defaultIntent: savedEmptyRequest.defaultIntent, sourceRevisionId: baselines[0].revisionId, noFirstWriteBypass: true });

  await nextScope("changed-source"); authorIntent = "写这一章。";
  workspace.chapters[0].candidate = revision("external-candidate", "其他修改后的标题", "其他已保存候选。"); workspace.revision++;
  await sendWriting(); await until(() => document.body.textContent.includes("已有版本变化"), "Fresh saved source was not detected");
  check(generations.length === 2, "First writing overwrote a source saved after the page loaded");

  await nextScope("late-a");
  check(await savedEmptyRequest.submit("不重放已离开的章节") === false, "Old chapter controller survived scope replacement");
  authorIntent = "只为 A 写作。"; holdRead = true; releaseRead = null; await sendWriting();
  await until(() => releaseRead, "Late first-write read was not held"); const releaseA = releaseRead;
  workspace = workspaceFor("late-b"); holdRead = false; await render(); releaseA(); await tick(); await tick();
  check(generations.length === 2 && document.querySelector('[aria-label="章节标题"]').value === "late-b 第一章", "Late first-write source check started a model request in another work");
  await nextScope("stopped-domain"); authorIntent = "起草这一章，不发布。"; stopGeneration = true;
  await sendWriting();
  await until(() => document.querySelector('.creative-alert.success')?.textContent.includes("本次执行已停止"), "Confirmed domain stop was not presented as a notice");
  check(!document.querySelector('.creative-alert.error') && !document.querySelector('.creative-alert.success').textContent.includes("正文已保存") && !document.body.textContent.includes("知识已就绪"), "Stopped generation falsely reported success or failure");
  check(document.querySelector<HTMLTextAreaElement>('textarea[aria-label="场景 1 正文"]').value === "" && workspace.chapters[0].candidate === null && saves.length === 1, "Stopped first writing fabricated a source or checkpoint");
  window.testResult = { passed: true, snapshots, stoppedBody: document.querySelector<HTMLTextAreaElement>('[aria-label="场景 1 正文"]').value, generations, saves: saves.length, baselines: baselines.length, intents: intents.length };
} catch (error) { window.testResult = { passed: false, error: error.stack }; } })();
