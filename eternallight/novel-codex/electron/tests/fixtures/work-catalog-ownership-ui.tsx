import React from "react";
import { createRoot } from "react-dom/client";
import { useWorkCatalog } from "../../../src/useWorkCatalog";
const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (value, message) => { if (!value) throw Error(message); };
const until = async (predicate, message) => { for (let i = 0; i < 150 && !predicate(); i++) await tick(); check(predicate(), message); };
const root = createRoot(document.getElementById("root")!);
let project = "a", enabled = true, refreshToken = 0, control;
const catalog = id => ({ revision: 1, volumes: [{ id: `volume-${id}`, title: id, position: 0, source: null, outlineStatus: "independent" }], placements: [] });
const records = { a: catalog("a"), b: catalog("b") }, reads = [], writes = [], delayedReads = [], delayedWrites = [];
let holdRead = false, holdWrite = false, lostReceipt = false, readFailure = false;
const api = { runtime: {
 getWorkCatalog: async id => { reads.push(id); const value = structuredClone(records[id]); if (holdRead) { holdRead = false; await new Promise(resolve => delayedReads.push(resolve)); } if (readFailure) throw Error("读取失败"); return value; },
 updateWorkCatalog: async (id, request) => { writes.push({ id, request: structuredClone(request) }); if (holdWrite) { holdWrite = false; await new Promise(resolve => delayedWrites.push(resolve)); } records[id] = { ...records[id], revision: records[id].revision + 1 }; if (lostReceipt) throw Error("回执断线"); return { catalog: structuredClone(records[id]) }; }
} };
function Probe() { control = useWorkCatalog(api, project, refreshToken, enabled); return <p>{control.catalog?.volumes[0]?.title ?? "loading"}:{control.error ?? ""}:{String(control.busy)}</p>; }
const render = async () => { root.render(<Probe />); await tick(); await tick(); };
const operation = { type: "createVolume", title: "明确创建" };
(async () => { try {
 await render(); await until(() => control.catalog, "Initial catalog missing");
 holdRead = true; refreshToken++; await render(); const staleRead = delayedReads.shift();
 project = "b"; await render(); check(control.catalog.volumes[0].id === "volume-b", "B failed to read while A pending");
 project = "a"; records.a = { ...records.a, revision: 5 }; await render(); staleRead(); await tick(); check(control.catalog.revision === 5, "A-B-A installed stale catalog");
 holdWrite = true; const oldWrite = control.update(operation); await tick(); check(control.busy, "Mutation not visibly busy");
 project = "b"; await render(); check(control.catalog?.volumes[0].id === "volume-b" && !control.busy, "Old mutation blocked new project");
 holdWrite = true; const newWrite = control.update(operation); await tick(); delayedWrites.shift()(); await oldWrite; await tick(); check(control.busy && control.catalog.volumes[0].id === "volume-b", "Old finally cleared new busy or installed foreign receipt"); delayedWrites.shift()(); await newWrite; await tick(); check(!control.busy && control.catalog.revision === records.b.revision, "New scope result did not settle");
 lostReceipt = true; try { await control.update(operation); } catch {} await tick(); const count = writes.length; check(control.error && !control.busy, "Unknown receipt claimed success");
 refreshToken++; await render(); check(control.error && writes.length === count, "Background refresh auto-cleared uncertain write or replayed creation");
 try { await control.update(operation); } catch {} check(writes.length === count, "Unresolved write was repeated before authoritative read");
 readFailure = true; await control.refresh(); await tick(); check(control.error && writes.length === count, "Failed reconcile erased error or replayed write");
 readFailure = false; lostReceipt = false; await control.refresh(); await tick(); check(!control.error && control.catalog.revision === records.b.revision && writes.length === count, "Explicit read did not reconcile saved receipt without writing");
 holdRead = true; refreshToken++; await render(); const late = delayedReads.shift(); enabled = false; await render(); late(); await tick(); check(control.catalog === null, "Disabled view installed late catalog"); enabled = true; await render(); check(control.catalog.volumes[0].id === "volume-b", "Reopening chapter did not fresh-read catalog");
 const staleUpdate = control.update; project = "a"; await render(); try { await staleUpdate(operation); } catch {} check(writes.length === count, "Old callback wrote to new scope");
 window.testResult = { passed: true, snapshot: { reads: reads.length, writes: writes.length, requests: writes.map(item => ({ project: item.id, expectedRevision: item.request.expectedRevision, operation: item.request.operation })), current: control.catalog, error: control.error, busy: control.busy } };
} catch (error) { window.testResult = { passed: false, error: error.stack ?? String(error), reads, writes: writes.length }; } })();
