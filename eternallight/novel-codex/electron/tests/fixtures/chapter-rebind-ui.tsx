import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { ChapterPlanRebindPanel } from "../../../src/ChapterPlanRebindPanel";

const tick = () => new Promise(resolve => setTimeout(resolve, 60));
const check = (value, message) => { if (!value) throw new Error(message); };
const button = label => [...document.querySelectorAll("button")].find(value => value.textContent === label);
const click = async label => { const value = button(label); check(value && !value.disabled, `Unavailable ${label}`); value.click(); await tick(); };
const acknowledge = async () => { document.querySelector('input[type="checkbox"]').click(); await tick(); };
let planId = "new-plan"; let draftId = "draft-current"; let reads = 0; let fail = true; let deferPreview = false; let deferSubmit = false; let release; let submissions = []; let disabled = false; let hasUpdate = true; let refreshes = 0;
const workspace = projectId => ({ project: { id: projectId }, revision: 2,
  chapters: [{ id: "chapter-1", number: 1, candidate: { id: draftId, body: "他藏好密函，等待白塔亮灯。" }, accepted: null }],
  chapterPlanBindings: [{ chapter_id: "chapter-1", chapter_plan_id: "plan-1", plan_revision_id: "old-plan", writerProjection: { publicTitle: "焚信", writerGuidance: "主角焚烧密函，拒绝留下证词。" } }, { chapter_id: "chapter-2", chapter_plan_id: "occupied", plan_revision_id: "old-plan" }],
  artifacts: { plan: { candidate: { id: planId, payload: { chapters: [
    { chapterPlanId: "plan-1", chapterNumber: 1, publicTitle: "藏信", writerGuidance: "主角保留密函，在亮灯时记录证词。" },
    { chapterPlanId: "wrong-number", chapterNumber: 2 }, { chapterPlanId: "duplicate", chapterNumber: 1 },
    { chapterPlanId: "duplicate", chapterNumber: 1 }, { chapterPlanId: "occupied", chapterNumber: 1 }
  ] } }, accepted: null } }
});
const receipt = request => ({ previousPlanRevisionId: request.previousPlanRevisionId, previousChapterPlanId: request.previousChapterPlanId, sourceRevisionId: request.sourceRevisionId, binding: { planRevisionId: request.planRevisionId, chapterPlanId: request.chapterPlanId, chapterId: request.chapterId, chapterNumber: 1, resultRevision: 3 } });
const api = { runtime: { getCreativeWorkspace: async projectId => { reads++; return deferPreview ? new Promise(resolve => { release = resolve; }) : workspace(projectId); } } };
const submit = async request => { submissions.push(request); if (fail) throw new Error("当前正文或规划已变化"); if (deferSubmit) return new Promise(resolve => { release = resolve; }); hasUpdate = false; render("project"); return receipt(request); };
const root = createRoot(document.getElementById("root"));
const render = (projectId, chapterId = "chapter-1") => root.render(<main style={{ width: "min(680px, 90vw)", margin: "30px auto" }}><ChapterPlanRebindPanel api={api} projectId={projectId} chapterId={chapterId} disabled={disabled} hasUpdate={hasUpdate} submit={submit} refresh={async () => { refreshes++; }} /></main>);
(async () => {
  render("project"); await tick(); await click("预览章纲调整");
  check(document.querySelectorAll("option").length === 1, "Invalid plan entries offered");
  check(button("确认替换章纲关联").disabled, "No explicit acknowledgment required");
  disabled = true; render("project"); await tick(); check(button("确认替换章纲关联").disabled, "Dirty workspace could confirm");
  disabled = false; render("project"); await tick(); await acknowledge();
  const snapshot = { title: document.querySelector("strong").textContent, descriptions: [...document.querySelectorAll("section p")].map(value => value.textContent), options: [...document.querySelectorAll("option")].map(value => value.textContent), sources: document.querySelector("dl").textContent, acknowledgment: document.querySelector(".rebind-acknowledgment").textContent };
  document.querySelector("main").style.width = "320px"; await tick();
  const cards = [...document.querySelectorAll('.chapter-plan-comparison > div')].map(value => value.getBoundingClientRect());
  check(Math.abs(cards[0].left - cards[1].left) < 1 && document.querySelector('section').scrollWidth <= document.querySelector('section').clientWidth + 1, "Narrow confirmation layout overflowed");
  document.querySelector("main").style.width = "min(680px, 90vw)"; await tick();
  window.uiReady = true; await new Promise(resolve => setTimeout(resolve, 400));
  planId = "changed-plan"; draftId = "changed-draft"; await click("确认替换章纲关联");
  check(reads === 1 && submissions[0].planRevisionId === "new-plan" && submissions[0].sourceRevisionId === "draft-current" && submissions[0].expectedRevision === 2, "Authority refreshed on submission");
  check(document.body.textContent.includes("已变化") && !button("确认替换章纲关联"), "Rejected preview retained authority");
  fail = false; await click("预览章纲调整"); await acknowledge(); await click("确认替换章纲关联");
  check(submissions[1].planRevisionId === planId && submissions[1].sourceRevisionId === draftId && submissions[1].idempotencyKey !== submissions[0].idempotencyKey, "New preview not explicitly rebound");
  check(document.body.textContent.includes("正文保留") && document.body.textContent.includes("基于当前章重写"), "Receipt or next step missing after workspace refresh");
  const receiptSnapshot = document.querySelector('[role="status"]').textContent;
  check(refreshes === 0, "Confirmation auto-started another action"); await click("重新读取作品");
  check(refreshes === 1 && submissions.length === 2, "Read-only refresh repeated the mutation");
  hasUpdate = true; deferPreview = true; render("project"); await tick(); await click("预览章纲调整");
  render("other-project"); await tick(); release(workspace("project")); await tick();
  check(!button("确认替换章纲关联") && !document.querySelector('[role="status"]'), "Late preview crossed projects");
  deferPreview = false; render("project"); await tick(); await click("预览章纲调整"); await acknowledge();
  deferSubmit = true; await click("确认替换章纲关联"); check(button("确认替换章纲关联").disabled, "Pending confirmation not disabled");
  render("project", "chapter-2"); await tick(); release(receipt(submissions.at(-1))); await tick();
  check(!document.querySelector('[role="status"]'), "Late receipt crossed chapters");
  root.unmount(); window.testResult = { passed: true, snapshot, receiptSnapshot };
})().catch(error => { window.testResult = { passed: false, error: error.stack }; });
