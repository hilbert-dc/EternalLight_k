import assert from "node:assert/strict";
import test from "node:test";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { WorkControlCommands } from "../work-control-commands.mjs";

async function fixture(t) {
  const directory = await mkdtemp(join(tmpdir(), "durable-controls-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const path = join(directory, "pending.json");
  const state = { work: { id: "work-first", projectId: "project", threadId: "thread-first", controlRevision: 0, version: 5, status: "running" },
    commands: new Map(), writes: 0, offline: false, loseResponse: false, unknownBeforeWrite: false };
  async function request(url, options) {
    if (state.offline) throw Error("offline");
    if (url.includes("/control-commands/")) return { receipt: state.commands.get(url.split("/").at(-1))?.receipt ?? null };
    if (!options) return structuredClone(state.work);
    const envelope = JSON.parse(options.body);
    const persisted = JSON.parse(await readFile(path, "utf8"));
    assert.equal(persisted.commands[0].commandId, envelope.commandId, "must persist before HTTP write");
    if (state.unknownBeforeWrite) throw Error("request not observed");
    let outcome = state.commands.get(envelope.commandId);
    const replayed = !!outcome;
    if (!outcome) {
      state.writes++;
      state.work = { ...state.work, status: envelope.command === "resume" ? "running" : envelope.command === "cancel" ? "cancelled" : "paused",
        controlRevision: state.work.controlRevision + 1, version: state.work.version + 3 };
      outcome = { work: structuredClone(state.work), receipt: { ...envelope, projectId: "project", workTaskId: "work-first", status: "applied",
        appliedControlRevision: state.work.controlRevision, workVersion: state.work.version, reason: null, createdAt: 1 } };
      state.commands.set(envelope.commandId, outcome);
    }
    if (state.loseResponse) { state.offline = true; throw Error("response lost"); }
    return { ...structuredClone(outcome), work: structuredClone(state.work), replayed };
  }
  const client = () => new WorkControlCommands({ request, path: () => path });
  return { state, path, client };
}

test("lost committed receipt survives restart; reconciliation performs zero control writes", async t => {
  const { state, path, client } = await fixture(t);
  state.loseResponse = true;
  await assert.rejects(client().send(state.work, "pause"), error => error.code === "WORK_CONTROL_DELIVERY_UNKNOWN");
  const pending = JSON.parse(await readFile(path, "utf8")).commands[0];
  assert.equal(state.writes, 1);
  const restarted = client();
  assert.equal((await restarted.project(state.work)).controlDelivery.commandId, pending.commandId);
  state.offline = false;
  await restarted.reconcile("project");
  assert.equal(state.writes, 1);
  assert.equal((await restarted.project(state.work)).controlDelivery, undefined);
  assert.deepEqual(JSON.parse(await readFile(path, "utf8")), { commands: [] });
});

test("retry of unobserved command keeps its durable identity and concurrent clicks coalesce", async t => {
  const { state, path, client } = await fixture(t);
  const commands = client();
  state.unknownBeforeWrite = true;
  await assert.rejects(commands.send(state.work, "pause"), /尚未确认/);
  const id = JSON.parse(await readFile(path, "utf8")).commands[0].commandId;
  state.unknownBeforeWrite = false;
  const results = await Promise.all([commands.send(state.work, "pause"), commands.send(state.work, "pause")]);
  assert.equal(state.writes, 1);
  assert.ok(state.commands.has(id));
  assert.equal(results[0].status, "paused");
  assert.deepEqual(results[0], results[1]);
});

test("old pause receipt cannot control later execution and reconciled resume never dispatches again", async t => {
  const { state, client } = await fixture(t);
  const original = structuredClone(state.work), commands = client();
  state.loseResponse = true;
  await assert.rejects(commands.send(original, "pause"), /尚未确认/);
  state.offline = false; state.loseResponse = false;
  state.work = { ...state.work, status: "running", controlRevision: 2 };
  await assert.rejects(commands.send(original, "pause"), error => error.code === "WORK_CONTROL_SUPERSEDED");
  assert.equal(state.writes, 1);
  state.work.status = "paused";
  const paused = structuredClone(state.work);
  state.loseResponse = true;
  await assert.rejects(commands.send(paused, "resume"), /尚未确认/);
  state.offline = false; state.loseResponse = false;
  await assert.rejects(commands.send(paused, "resume"), error => error.code === "WORK_CONTROL_RECONCILED");
  assert.equal(state.writes, 2);
});

test("a definitive rejected command clears uncertainty even when thread binding changed", async t => {
  const { path } = await fixture(t);
  const before = { id: "work-first", projectId: "project", threadId: "old-thread", controlRevision: 0, version: 1 };
  const commands = new WorkControlCommands({ path: () => path, request: async (_url, options) => {
    const envelope = JSON.parse(options.body);
    return { replayed: false, work: { ...before, threadId: "current-thread" }, receipt: { ...envelope,
      projectId: before.projectId, workTaskId: before.id, status: "rejected", reason: "TaskBindingInvalid",
      appliedControlRevision: null, workVersion: 1, createdAt: 1 } };
  } });
  await assert.rejects(commands.send(before, "pause"), error => error.code === "WORK_CONTROL_REJECTED");
  assert.equal((await commands.project(before)).controlDelivery, undefined);
  assert.deepEqual(JSON.parse(await readFile(path, "utf8")), { commands: [] });
});

test("foreign receipt stays uncertain and blocks a different command without another write", async t => {
  const { path } = await fixture(t);
  const before = { id: "work-first", projectId: "project", threadId: "thread", controlRevision: 0, version: 1 };
  let writes = 0, receipt;
  const commands = new WorkControlCommands({ path: () => path, request: async (_url, options) => {
    if (!options) return { receipt };
    writes++;
    const envelope = JSON.parse(options.body);
    receipt = { ...envelope, projectId: "foreign-project", workTaskId: before.id, status: "applied",
      appliedControlRevision: 1, workVersion: 2, createdAt: 1, reason: null };
    return { replayed: false, work: { ...before, controlRevision: 1 }, receipt };
  } });
  await assert.rejects(commands.send(before, "pause"), error => error.code === "WORK_CONTROL_RECEIPT_INVALID");
  await assert.rejects(commands.send(before, "cancel"), error => error.code === "WORK_CONTROL_DELIVERY_UNKNOWN");
  assert.equal(writes, 1);
  assert.equal((await commands.project(before)).controlDelivery.command, "pause");
  assert.equal(JSON.parse(await readFile(path, "utf8")).commands.length, 1);
});

test("a budget rejection is reported as exhausted allowance and clears only command uncertainty", async t => {
  const { path } = await fixture(t);
  const before = { id: "work-first", projectId: "project", threadId: "thread", controlRevision: 0, version: 1, status: "blocked" };
  let writes = 0;
  const commands = new WorkControlCommands({ path: () => path, request: async (_url, options) => {
    writes++;
    const envelope = JSON.parse(options.body);
    return { replayed: false, work: before, receipt: { ...envelope,
      projectId: before.projectId, workTaskId: before.id, status: "rejected", reason: "WorkBudgetExhausted",
      appliedControlRevision: null, workVersion: 1, createdAt: 1 } };
  } });
  await assert.rejects(commands.send(before, "resume"), error => error.code === "WORK_CONTROL_REJECTED"
    && /额度已用完/.test(error.message) && !/输入已变化/.test(error.message));
  assert.equal(writes, 1);
  assert.equal(before.status, "blocked");
  assert.deepEqual(JSON.parse(await readFile(path, "utf8")), { commands: [] });
});

test("resume without an explicit replay marker cannot authorize dispatch", async t => {
  const { path } = await fixture(t);
  const before = { id: "work-first", projectId: "project", threadId: "thread", controlRevision: 0, version: 1, status: "paused" };
  const commands = new WorkControlCommands({ path: () => path, request: async (_url, options) => {
    const envelope = JSON.parse(options.body);
    return { work: { ...before, status: "running", controlRevision: 1 }, receipt: { ...envelope,
      projectId: before.projectId, workTaskId: before.id, status: "applied", reason: null,
      appliedControlRevision: 1, workVersion: 2, createdAt: 1 } };
  } });
  await assert.rejects(commands.send(before, "resume"), error => error.code === "WORK_CONTROL_RECEIPT_INVALID");
  assert.equal((await commands.project(before)).controlDelivery.command, "resume");
});
