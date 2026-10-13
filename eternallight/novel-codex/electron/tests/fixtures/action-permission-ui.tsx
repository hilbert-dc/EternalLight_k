import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { ActionPermissionDetails } from "../../../src/ActionPermissionDetails";

const root = createRoot(document.getElementById("root"));
const tick = () => new Promise((resolve) => setTimeout(resolve, 30));
const check = (ok, message) => { if (!ok) throw new Error(message); };
const wait = async (predicate) => { for (let n = 0; n < 100 && !predicate(); n++) await tick(); check(predicate(), "UI did not settle"); };
const pending = [];
const api = { agent: { readActionPermission: (...args) => new Promise((resolve, reject) => pending.push({ args, resolve, reject })) } };
const permission = (projectId) => ({ kind: "exactAction", actionId: "action", workTaskId: "work", grantedAt: 1790000000, actionStatus: "completed",
  evidence: { projectId, sourceTaskId: "source", intentRevision: 0, inputRevision: 2, operation: "generateChapter",
    target: { chapterId: "chapter-2", scope: { type: "selectedScenes", groupIds: ["letter"] } },
    instruction: "只修改密函场景，保留其他正文", authorIntent: `${projectId}：将烧信改为藏信。` } });
async function render(projectId, version = 1) {
  root.render(<main style={{ maxWidth: 780, margin: "40px auto" }}><h1>创作任务 · 第二章局部改稿</h1>
    <ActionPermissionDetails api={api} projectId={projectId} workTaskId="work" sourceTaskId="source" actionId="action" version={version} />
  </main>); await tick();
}
void (async () => {
  await render("旧作品");
  check(pending.length === 0, "collapsed view must not request permission");
  document.querySelector("summary").click(); await wait(() => pending.length === 1);
  await render("港城"); await wait(() => pending.length === 2);
  pending[0].resolve({ permission: permission("旧作品") }); await tick();
  check(!document.body.textContent.includes("旧作品："), "late permission leaked across projects");
  pending[1].reject(new Error("读取断开")); await wait(() => document.querySelector('[role="alert"]'));
  document.querySelector("button").click(); await wait(() => pending.length === 3);
  pending[2].resolve({ permission: permission("别的作品") }); await wait(() => document.body.textContent.includes("许可归属不匹配"));
  check(!document.body.textContent.includes("别的作品："), "foreign permission shown");
  document.querySelector("button").click(); await wait(() => pending.length === 4);
  pending[3].resolve({ permission: null }); await wait(() => document.body.textContent.includes("没有作者许可"));
  await render("港城", 2); await wait(() => pending.length === 5);
  pending[4].resolve({ permission: permission("港城") }); await wait(() => document.body.textContent.includes("港城：将烧信改为藏信。"));
  check(document.body.textContent.includes("不会恢复或重新批准任务"), "historical evidence warning missing");
  check(document.querySelectorAll("button").length === 0, "read-only view exposed mutation controls");
  await render("港城", 3); await wait(() => pending.length === 6);
  pending[5].resolve({ permission: { ...permission("港城"), kind: "finiteScope", scopePermitId: "scope-test" } });
  await wait(() => document.body.textContent.includes("已确认的有限操作范围"));
  check(document.querySelectorAll("button").length === 0, "scope history exposed mutation controls");
  await render("港城", 4); await wait(() => pending.length === 7);
  pending[6].resolve({ permission: { ...permission("港城"), kind: "finiteScope", scopePermitId: null } });
  await wait(() => document.body.textContent.includes("许可归属不匹配"));
  await render("港城", 5); await wait(() => pending.length === 8);
  pending[7].resolve({ permission: { ...permission("港城"), kind: "finiteScope", scopePermitId: "scope-test" } });
  await wait(() => document.body.textContent.includes("已确认的有限操作范围"));
  window.uiReady = true; await new Promise((resolve) => setTimeout(resolve, 200));
  window.testResult = { passed: true };
})().catch((error) => { window.testResult = { passed: false, error: error.stack }; });
