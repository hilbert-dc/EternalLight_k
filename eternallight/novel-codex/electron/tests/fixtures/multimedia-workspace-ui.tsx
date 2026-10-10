import React from "react";
import { createRoot } from "react-dom/client";
import { MultimediaWorkspaceView } from "../../../src/multimedia/MultimediaWorkspaceView";
import { MediaServiceSettings } from "../../../src/multimedia/MediaServiceSettings";
import "../../../src/styles.css";

const tick = () => new Promise(resolve => setTimeout(resolve, 15));
const check = (value, message) => { if (!value) throw Error(message); };
const until = async (predicate, message) => { for (let i = 0; i < 130 && !predicate(); i++) await tick(); check(predicate(), message); };
const clone = value => structuredClone(value);
const canvas = document.createElement("canvas"); canvas.width = 400; canvas.height = 300; const context = canvas.getContext("2d"); context.fillStyle = "#c8d8ef"; context.fillRect(0, 0, 400, 300); context.fillStyle = "#526fa1"; context.fillRect(80, 60, 140, 190);
const asset = scopeId => ({ id: `picture-${scopeId}`, assetRevisionId: `picture-${scopeId}-v1`, projectId: "project", scopeId, name: "角色参考.png", kind: "image", mimeType: "image/png", digest: "a".repeat(64), bytes: 400, width: 400, height: 300, duration: null, sampleRate: null, channels: null, previewUrl: canvas.toDataURL(), createdAt: 1 });
const base = (id, medium) => ({ project: { id: "project", title: "独立作品", logline: "", updated_at: 1 }, edition: { id, editionId: id, scopeId: `scope-${id}`, branchId: `branch-${id}`, projectId: "project", medium, title: id === "comic" ? "独立漫画" : "独立动画", canonSnapshotId: null, sources: [] }, revisionId: `${id}-v1`, documentSource: { projectId: "project", scopeId: `scope-${id}`, branchId: `branch-${id}`, objectId: id, revisionId: `${id}-v1`, digest: "d".repeat(64), selector: { kind: "whole" } }, revision: 1, sourceDigest: "a".repeat(64), headSnapshotId: `${id}-snapshot-1`, assets: [asset(`scope-${id}`)], sources: [] });
const workspaces = {
  comic: { ...base("comic", "comic"), document: { kind: "comic", title: "独立漫画", format: "pages", pages: [{ id: "page-one", title: "雨夜", width: 1200, height: 1600, readingDirection: "leftToRight", panels: [0, 1].map(i => ({ id: `panel-${i}`, description: "同名画格", rect: { x: 30 + 600 * i, y: 30, width: 500, height: 500 }, assetRevisionId: "picture-scope-comic-v1", referenceAssetRevisionIds: [], balloons: [{ id: `balloon-${i}`, text: `原对白 ${i}`, speaker: "", kind: "speech", rect: { x: 20, y: 20, width: 200, height: 80 } }], locked: false })) }] } },
  animation: { ...base("animation", "animation"), document: { kind: "animation", title: "独立动画", width: 1280, height: 720, frameRate: { numerator: 24, denominator: 1 }, shots: [], timeline: { tracks: [] } } },
};
let editionId = "comic", refreshToken = 0, busy = false, dirty = false, lastSelection = null, saveHandler, discardHandler, holdRead = false, releaseRead;
let saves = [], imports = 0, exports = [], openedExports = 0, failSave = true, failExport = true, showSettings = false, connections = [], connectionSaves = 0;
const api = {
  readSubtitleFont: async () => null,
  readWorkspace: async (projectId, id) => { check(projectId === "project", "Project context lost"); const value = clone(workspaces[id]); if (holdRead) { holdRead = false; await new Promise(resolve => { releaseRead = resolve; }); } return value; },
  saveDocument: async (projectId, id, request) => { saves.push(clone(request)); if (failSave) { failSave = false; throw Error("保存连接中断，修改仍保留。"); } check(request.expectedRevisionId === workspaces[id].revisionId, "Save CAS not bound to exact edition revision"); const next = { ...workspaces[id], document: clone(request.document), revisionId: `${id}-v${saves.length + 1}` }; next.documentSource = { ...next.documentSource, revisionId: next.revisionId }; workspaces[id] = next; return clone(next); },
  importFiles: async (projectId, id, files) => { check(projectId === "project" && id === "comic", "Import scope lost"); check(files?.length === 1 && files[0].name === "新画面.png" && files[0].bytes instanceof Uint8Array && files[0].bytes.length === 3, "Drop did not transfer exact bytes"); imports++; const item = { ...asset("scope-comic"), id: "new-image", assetRevisionId: "new-image-v1", name: files[0].name }; workspaces.comic.assets.push(item); return { assets: [item] }; },
  exportEdition: async (projectId, id, request) => { exports.push({ id, request }); check(request.expectedRevisionId === workspaces[id].revisionId, "Export did not freeze saved revision"); if (failExport) { failExport = false; throw Error("导出未完成。"); } return { id: "export-one", editionId: id, revisionId: request.expectedRevisionId, name: "动画项目.zip", format: request.format, bytes: 1000, assetRevisionIds: [], manifestId: "manifest" }; },
  openExport: async (projectId, id) => { check(projectId === "project" && id === "export-one", "Wrong export opened"); openedExports++; return { opened: true }; },
  listConnections: async () => ({ connections, capabilities: [] }),
  saveConnection: async request => { connectionSaves++; check(request.apiKey === "test-fixture-key", "New credential missing"); const value = { ...request, apiKey: undefined, credentialConfigured: true, capabilities: [] }; connections = [value]; return value; },
};
const root = createRoot(document.getElementById("root"));
// The isolated editor may occupy a narrow pane inside the wider desktop shell.
document.body.style.minWidth = "0";
const render = async () => { root.render(<main style={{ maxWidth: 1100, margin: "auto" }}>{showSettings ? <MediaServiceSettings api={api} /> : <MultimediaWorkspaceView authorKey="fixture-author" api={api} projectId="project" editionId={editionId} busy={busy} refreshToken={refreshToken} onDirtyChange={value => { dirty = value; }} onSelectionChange={value => { lastSelection = value; }} onSaveHandlerChange={value => { saveHandler = value; }} onDiscardHandlerChange={value => { discardHandler = value; }} />}</main>); await tick(); };
const button = label => [...document.querySelectorAll<HTMLButtonElement>("button")].find(item => item.textContent === label || item.getAttribute("aria-label") === label);
const field = label => [...document.querySelectorAll<HTMLLabelElement>("label")].find(item => item.querySelector("span")?.textContent === label)?.querySelector<HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>("input,textarea,select");
const click = async element => { check(element && !element.matches(":disabled"), `Unavailable button: ${element?.textContent}`); element.scrollIntoView({ block: "nearest" }); element.focus(); element.click(); await tick(); };
const change = async (element, value) => { check(element, "Missing editor"); const proto = element instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : element instanceof HTMLSelectElement ? HTMLSelectElement.prototype : HTMLInputElement.prototype; Object.getOwnPropertyDescriptor(proto, "value").set.call(element, value); element.dispatchEvent(new Event(element instanceof HTMLSelectElement ? "change" : "input", { bubbles: true })); await tick(); };
const number = async (label, value) => { const element = field(label); element.focus(); await change(element, String(value)); element.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true })); await tick(); };
window.addEventListener("error", event => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
(async () => { try {
  await render(); await until(() => button("第 1 格") && !button("保存").matches(":disabled") === false, "Comic did not load");
  await click(button("第 1 格")); await change(field("对白 1"), "信到了。"); check(dirty, "Comic edit not dirty"); check(lastSelection?.revisionId === null, "Unsaved selection falsely bound to persisted revision");
  await click(button("延后阅读")); check([...document.querySelectorAll("[data-panel-id]")].map(item => item.getAttribute("data-panel-id")).join() === "panel-1,panel-0", "Reorder lost stable panel identities");
  await click(button("保存")); await until(() => document.querySelector('[role="alert"]'), "Save failure missing"); check(field("对白 1").value === "信到了。" && dirty, "Failed save discarded draft");
  await click(button("保存")); await until(() => !dirty, "Save success not reflected"); check(saves[0].idempotencyKey === saves[1].idempotencyKey, "Retry duplicated an uncertain save operation"); check(workspaces.comic.document.pages[0].panels[1].id === "panel-0" && workspaces.comic.document.pages[0].panels[1].balloons[0].text === "信到了。", "Save changed wrong panel");
  await click(button("选择图片区域")); await until(() => lastSelection?.selector.region, "Image region missing"); check(lastSelection.selector.panelId === "panel-0" && lastSelection.selector.region.width === 200, "Region did not use original image pixels");
  await click(button("第 1 格")); await tick(); check(!lastSelection.selector.region && lastSelection.selector.panelId === "panel-1", "Old region leaked to another panel");
  document.querySelector<HTMLDetailsElement>(".mm-reference-assets").open = true; await click(document.querySelector<HTMLInputElement>(".mm-reference-assets input"));
  await click(button("保护画格")); check(field("画面说明").matches(":disabled") && !button("取消保护").disabled, "Protected panel still editable"); await click(button("取消保护"));
  const transfer = new DataTransfer(); transfer.items.add(new File([new Uint8Array([1, 2, 3])], "新画面.png", { type: "image/png" })); document.querySelector(".multimedia-workspace").dispatchEvent(new DragEvent("drop", { bubbles: true, cancelable: true, dataTransfer: transfer })); await until(() => imports === 1 && field("画面素材")?.querySelector('option[value="new-image-v1"]'), "Dropped asset unavailable"); await change(field("画面素材"), "new-image-v1");
  await click(button("保存")); await until(() => !dirty, "Imported comic save failed");
  // A background save changes a different panel while this editor has a draft.
  await click(button("第 1 格")); await change(field("画面说明"), "本地独立修改");
  workspaces.comic.document.pages[0].panels[1].description = "后台独立修改";
  workspaces.comic.revisionId = "comic-background-1";
  workspaces.comic.documentSource.revisionId = workspaces.comic.revisionId;
  refreshToken++; await render(); await until(() => document.body.textContent.includes("已合并后台保存的独立修改") && !button("保存").disabled, "Independent background edits were not merged");
  check(field("画面说明").value === "本地独立修改" && dirty, "Merge discarded the local draft");
  await click(button("第 2 格")); check(field("画面说明").value === "后台独立修改", "Merge discarded the background panel");
  await click(button("保存")); await until(() => !dirty, "Merged save failed");
  check(saves.at(-1).expectedRevisionId === "comic-background-1", "Merged draft did not bind current CAS");
  await click(button("第 1 格")); await change(field("画面说明"), "本地冲突修改");
  const originalRevision = workspaces.comic.revisionId;
  workspaces.comic.document.pages[0].panels[0].description = "远端冲突修改";
  workspaces.comic.revisionId = "comic-background-conflict";
  refreshToken++; await render(); await until(() => document.body.textContent.includes("修改存在冲突") && !button("保存").disabled, "Overlapping change was not held");
  check(field("画面说明").value === "本地冲突修改" && dirty, "Conflict lost local text");
  check(await saveHandler() === false, "Conflicting draft bypassed CAS");
  check(saves.at(-1).expectedRevisionId === originalRevision && workspaces.comic.document.pages[0].panels[0].description === "远端冲突修改", "Conflict overwrote saved work");
  check(discardHandler(), "Explicit fixture discard failed"); await until(() => !dirty, "Conflict discard did not restore latest version");
  await change(field("画面说明"), "设定变化时保留本地");
  workspaces.comic.edition.canonSnapshotId = "canon-background";
  workspaces.comic.revisionId = "comic-canon-background";
  refreshToken++; await render(); await until(() => document.body.textContent.includes("修改存在冲突") && !button("保存").disabled, "Changed canon must not be silently merged");
  check(field("画面说明").value === "设定变化时保留本地" && dirty, "Canon change discarded the draft");
  check(discardHandler(), "Canon-change fixture discard failed"); await until(() => !dirty, "Canon-change discard failed");
  await change(field("画面说明"), "双方相同修改");
  workspaces.comic.document.pages[0].panels[0].description = "双方相同修改";
  workspaces.comic.revisionId = "comic-identical-background";
  refreshToken++; await render(); await until(() => !dirty && document.body.textContent.includes("已与最新保存版本一致"), "Identical saved content was falsely shown as unsaved");
  // A read begun before save must not roll back the save receipt when it arrives late.
  await change(field("画面说明"), "保存期间旧读取不得覆盖");
  releaseRead = null; holdRead = true; refreshToken++; await render(); await until(() => releaseRead, "Save-race read not held");
  check(await saveHandler(), "Save during a pending read failed"); const savedRevision = workspaces.comic.revisionId;
  releaseRead(); await tick(); await tick();
  check(!dirty && field("画面说明").value === "保存期间旧读取不得覆盖" && lastSelection.revisionId === savedRevision, "Late read rolled back the save receipt");
  for (const theme of ["light", "dark"]) { document.documentElement.dataset.theme = theme; window.testViewport = { width: 380, height: 900 }; await until(() => !window.testViewport && innerWidth === 380, "Viewport missing"); await tick(); check(document.documentElement.scrollWidth <= innerWidth + 1, "Comic narrow layout overflowed"); window.testScreenshot = `multimedia-comic-${theme}-380`; await until(() => !window.testScreenshot, "Comic screenshot missing"); }
  await change(field("缩放"), "75"); await change(field("画面说明"), "未保存但应恢复"); holdRead = true; refreshToken++; await render(); await until(() => releaseRead, "Delayed read not started"); editionId = "animation"; await render(); releaseRead(); await until(() => button("＋ 添加镜头") && !button("＋ 添加镜头").disabled, "Independent animation missing"); check(!document.querySelector(".mm-comic-page") && document.querySelector("h2").textContent === "独立动画", "Late comic result crossed edition");
  await click(button("＋ 添加镜头")); await change(field("镜头名称"), "萤火虫"); await change(field("画面素材"), "picture-scope-animation-v1"); await click(button("加入时间轴"));
  await click(button("复制片段")); check(document.querySelectorAll(".mm-track-clips > button").length === 2, "Shot cannot be reused in clips"); await click(button("画面片段 2")); await number("出现时间（秒）", 5); await number("片段时长（秒）", 2);
  check(lastSelection.selector.kind === "clip" && lastSelection.selector.clipId, "Clip selection lacks stable track/clip identity");
  await click(button("＋ 字幕")); await change(field("字幕文字"), "追上那束光。"); await number("出现时间（秒）", 1);
  document.querySelector<HTMLDetailsElement>(".mm-track-options").open = true; await click(button("＋ 声音轨道")); await click(button("＋ 声音轨道"));
  await click(button("保存")); await until(() => !dirty, "Animation save did not succeed"); const savedAnimation = workspaces.animation.document; check(savedAnimation.timeline.tracks.filter(item => item.kind === "audio").length === 2, "Independent sound tracks unavailable"); const clips = savedAnimation.timeline.tracks.find(item => item.kind === "video").clips;
  check(savedAnimation.shots.length === 1 && clips.length === 2 && clips[0].shotId === clips[1].shotId && clips[0].id !== clips[1].id, "Shot and clip identities collapsed"); check(clips[1].start.value === 120 && clips[1].duration.value === 48 && clips[0].start.value === 0, "Exact time edit affected another clip");
  for (const theme of ["light", "dark"]) { document.documentElement.dataset.theme = theme; check(document.documentElement.scrollWidth <= innerWidth + 1, "Animation narrow layout overflowed"); window.testScreenshot = `multimedia-animation-${theme}-380`; await until(() => !window.testScreenshot, "Animation screenshot missing"); }
  document.querySelector<HTMLDetailsElement>(".mm-export-menu").open = true; await click(button("可编辑项目")); await until(() => document.querySelector('[role="alert"]')?.textContent.includes("导出未完成"), "Export failure hidden"); check(!button("打开文件") && !dirty, "Failed export claimed success or dirtied document");
  document.querySelector<HTMLDetailsElement>(".mm-export-menu").open = true; await click(button("可编辑项目")); await until(() => button("打开文件"), "Export receipt missing"); await click(button("打开文件"));
  await click(button("画面片段 2")); await change(document.querySelector('[aria-label="播放位置"]'), "2"); workspaces.comic.document.pages[0].panels[1].description = "离开后后台更新"; workspaces.comic.revisionId = "comic-while-away"; editionId = "comic"; await render(); await until(() => button("第 1 格") && !button("第 1 格").disabled, "Comic reopen failed"); check(field("缩放").value === "75" && field("画面说明").value === "未保存但应恢复", "Comic selection or zoom bookmark lost"); await click(button("第 1 格")); check(field("画面说明").value === "未保存但应恢复" && dirty, "Per-edition local draft not restored"); check(document.body.textContent.includes("保存版本已有变化"), "Missing original base must not be guessed after remount"); check(discardHandler(), "Discard handler failed"); await until(() => !dirty, "Discard not restored to saved document");
  editionId = "animation"; await render(); await until(() => field("片段时长（秒）"), "Animation clip bookmark missing"); check(Number(document.querySelector<HTMLInputElement>('[aria-label="播放位置"]').value) === 2 && document.querySelector<HTMLDetailsElement>(".mm-timeline").open && lastSelection.selector.clipId === clips[1].id, "Animation playback, timeline or clip bookmark lost"); await number("片段时长（秒）", 1 / 24); await number("播放速度", 4); check(Number(field("播放速度").value) === 1 && Number(field("片段时长（秒）").value) > 0, "One-frame speed change created a zero-length clip"); check(discardHandler(), "Animation test edit discard failed"); showSettings = true; await render(); await until(() => field("服务名称") && !field("服务名称").disabled, "Service settings failed to load"); await change(field("服务名称"), "测试图片服务"); await change(field("API 地址"), "https://example.invalid/v1"); await change(field("模型"), "fixture-model"); await change(field("API 密钥"), "test-fixture-key"); await click(button("保存连接")); await until(() => connectionSaves === 1 && document.querySelector('input[type="password"]').value === "", "Connection credential not cleared after receipt"); check(!document.body.textContent.includes("test-fixture-key"), "Credential appeared in prose");
  window.testResult = { passed: true, modelCalls: 0, savedComic: true, savedAnimation: true, imports, openedExports };
} catch (error) { window.testResult = { passed: false, error: error.stack ?? String(error), dom: document.body.textContent.slice(-7000) }; } })();
