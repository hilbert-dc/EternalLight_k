import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { runInNewContext } from "node:vm";
import { registerRevisionDecisionHandlers, registerRevisionOutcomeHandlers } from "../revision-outcome-handlers.mjs";
import { mainHandlers, rendererEvent } from "./main-handler-harness.mjs";

const writer = `task-${"a".repeat(32)}`;
const request = () => ({ decisionId: "decision-1", expectedAssessmentRevisionId: "revision-outcome-1",
  expectedDecisionId: null, choice: "acceptRevision", note: "保留当前修改" });

function bridge(transport) {
  const handlers = new Map();
  const ipc = { handle: (channel, handler) => handlers.set(channel, handler) };
  registerRevisionOutcomeHandlers(ipc, transport);
  registerRevisionDecisionHandlers(ipc, transport);
  return { read: handlers.get("runtime:get-revision-outcome"), decide: handlers.get("runtime:decide-revision-outcome") };
}

test("Author revision decision POST forwards the exact CAS request and returns the receipt without starting work", async () => {
  const calls = [];
  const receipt = { id: "decision-1", writerTaskId: writer, assessmentRevisionId: "revision-outcome-1",
    draftRevisionId: "draft-1", knowledgeBatchId: "batch-1", choice: "acceptRevision", note: "保留当前修改", createdAt: 42 };
  const { decide } = bridge(async (...args) => { calls.push(args); return receipt; });
  const body = request();
  assert.deepEqual(await decide({}, "作品/a?b#c", writer, body), receipt);
  assert.deepEqual(body, request());
  assert.deepEqual(calls, [[`/api/projects/${encodeURIComponent("作品/a?b#c")}/revision-outcomes/${writer}/decisions`,
    { method: "POST", body: JSON.stringify(body) }]]);
});

test("the three choices, CAS decision IDs, empty notes and Unicode character limits are forwarded unchanged", async () => {
  const calls = [];
  const { decide } = bridge(async (...args) => { calls.push(args); return {}; });
  const bodies = [
    { ...request(), decisionId: "a".repeat(128), expectedDecisionId: "previous-1", note: "😀".repeat(1000) },
    { ...request(), choice: "repairDraft", note: "" },
    { ...request(), choice: "defer", note: "  暂缓  " },
  ];
  for (const body of bodies) await decide({}, "project", writer, body);
  assert.deepEqual(calls, bodies.map(body => [`/api/projects/project/revision-outcomes/${writer}/decisions`,
    { method: "POST", body: JSON.stringify(body) }]));
});

test("invalid route identities are rejected before Author transport", () => {
  const calls = [];
  const { decide } = bridge((...args) => calls.push(args));
  for (const project of [null, {}, "", " ", "x".repeat(161), "a\n", "a\r", "a\0", "a\t", "a\x7f", ".", ".."]) {
    assert.throws(() => decide({}, project, writer, request()), /标识无效/);
  }
  for (const id of [null, {}, "", "../task", writer + "?x=1", writer + "/", "task-" + "a".repeat(31), "action-" + "a".repeat(32)]) {
    assert.throws(() => decide({}, "project", id, request()), /标识无效/);
  }
  assert.deepEqual(calls, []);
});

test("decision IPC requires all five fields and rejects unknown fields, invalid IDs and oversized notes", () => {
  const calls = [];
  const { decide } = bridge((...args) => calls.push(args));
  const invalid = [null, undefined, [], "decision", 1, {}, { ...request(), extra: true }];
  for (const field of Object.keys(request())) {
    const missing = request();
    delete missing[field];
    invalid.push(missing, { ...request(), [field]: undefined });
  }
  for (const decisionId of [null, 1, "", "x".repeat(129), "决定", "with space", "../id", "under_score", "x\0", "x\n", "x\x7f"]) {
    invalid.push({ ...request(), decisionId });
    if (decisionId !== null) invalid.push({ ...request(), expectedDecisionId: decisionId });
  }
  for (const expectedAssessmentRevisionId of [null, 1, "", " x", "x ", "x".repeat(161), "界".repeat(54), "x\0", "x\n", "x\x85"]) {
    invalid.push({ ...request(), expectedAssessmentRevisionId });
  }
  for (const choice of [null, 1, "", "continue", "accept", "acceptRevision "]) invalid.push({ ...request(), choice });
  for (const note of [null, 1, {}, [], "界".repeat(1001), "😀".repeat(1001)]) invalid.push({ ...request(), note });
  for (const body of invalid) assert.throws(() => decide({}, "project", writer, body), /参数无效/);
  assert.deepEqual(calls, []);
});

test("exact replay is transported independently and conflicts propagate without source refresh or retry", async () => {
  const calls = [];
  const receipt = { id: "decision-1" };
  const conflict = new Error("assessment or decision is stale");
  const { decide } = bridge(async (...args) => {
    calls.push(args);
    if (calls.length === 3) throw conflict;
    return receipt;
  });
  const body = request();
  assert.deepEqual(await decide({}, "project", writer, body), receipt);
  assert.deepEqual(await decide({}, "project", writer, body), receipt);
  const changed = { ...body, note: "不同的决定" };
  await assert.rejects(decide({}, "project", writer, changed), error => error === conflict);
  assert.deepEqual(calls, [body, body, changed].map(value => [`/api/projects/project/revision-outcomes/${writer}/decisions`,
    { method: "POST", body: JSON.stringify(value) }]));
});

test("report GET preserves newest-first decision history and remains a separate read", async () => {
  const report = { revisionId: "revision-outcome-1", authority: "candidateAnalysis", state: "current", assessment: {},
    decisions: Array.from({ length: 20 }, (_, i) => ({ id: `decision-${20 - i}` })) };
  const calls = [];
  const { read } = bridge(async (...args) => { calls.push(args); return report; });
  assert.deepEqual(await read({}, "project", writer), report);
  assert.deepEqual(calls, [[`/api/projects/project/revision-outcomes/${writer}`]]);
});

test("production preload exposes the Author decision bridge and returns its receipt", async () => {
  let api;
  const calls = [];
  const receipt = { id: "decision-1" };
  runInNewContext(await readFile(new URL("../preload.cjs", import.meta.url), "utf8"), {
    process: { platform: "darwin" },
    require: () => ({ contextBridge: { exposeInMainWorld: (_name, value) => { api = value; } },
      ipcRenderer: { invoke: async (...args) => { calls.push(args); return receipt; } } }),
  });
  const body = request();
  assert.deepEqual(await api.runtime.decideRevisionOutcome("project", writer, body), receipt);
  assert.deepEqual(calls, [["runtime:decide-revision-outcome", "project", writer, body]]);
});

test("production main registers revision reads and decisions through Author transport without starting generation", async () => {
  const calls = [];
  const receipt = { id: "decision-1", writerTaskId: writer, assessmentRevisionId: "revision-outcome-1",
    draftRevisionId: "draft-1", knowledgeBatchId: "batch-1", choice: "acceptRevision", note: "保留当前修改", createdAt: 42 };
  const report = { revisionId: "revision-outcome-1", authority: "candidateAnalysis", state: "current", assessment: {}, decisions: [receipt] };
  const path = `/api/projects/project/revision-outcomes/${writer}`;
  const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;
  const { handlers, adapter } = await mainHandlers(async (requestedPath, body, options) => {
    assert.equal(options.headers["Content-Type"], "application/json");
    assert.match(options.headers.Authorization, /^Bearer \S+$/);
    calls.push({ path: requestedPath, body, method: options.method ?? "GET" });
    if (requestedPath === path && !body) return report;
    if (requestedPath === `${path}/decisions` && options.method === "POST") return receipt;
    assert.fail(`Unexpected runtime request: ${requestedPath}`);
  }, command);
  try {
    const body = request();
    assert.deepEqual(await handlers.get("runtime:decide-revision-outcome")(rendererEvent(), "project", writer, body), receipt);
    assert.deepEqual(await handlers.get("runtime:get-revision-outcome")(rendererEvent(), "project", writer), report);
    assert.deepEqual(calls, [
      { path: `${path}/decisions`, body, method: "POST" },
      { path, body: null, method: "GET" },
    ]);
  } finally { adapter.close(); }
});
