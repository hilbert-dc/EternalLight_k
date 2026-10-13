import assert from "node:assert/strict";
import test from "node:test";
import { ChapterStructureAnalysisConsumer, CHAPTER_STRUCTURE_ANALYSIS_PROMPT,
  registerChapterStructureReadHandlers, validateChapterStructureChange, validateChapterStructureContext } from "../chapter-structure-analysis.mjs";

const change = (id = "change-one", projectId = "project") => ({ id, projectId, kind: "chapterDeleted",
  chapterId: "chapter-one", chapterNumber: 2, title: "原章节名称", createdAt: 100,
  baseRevision: 2, resultRevision: 3, baseSnapshotId: "snapshot-before", resultSnapshotId: "snapshot-after",
  source: { objectId: "draft-chapter-one", savedRevisionId: "saved-source", acceptedRevisionId: null, workingRevisionId: null, knowledgeBatchId: null },
  previousChapter: { id: "chapter-first", number: 1 }, nextChapter: { id: "chapter-last", number: 4 },
  assessment: { state: "pending", skipReason: null, taskId: null, workTaskId: null } });
const bound = item => ({ ...item, assessment: { state: "bound", skipReason: null, taskId: "task-one", workTaskId: "work-one" } });
const deferred = () => { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; };
const page = (items, nextCursor = null, projectId = "project") => ({ projectId, items, nextCursor });
function harness(overrides = {}) {
  const calls = { list: [], read: [], run: [], updated: [], failed: [] };
  const consumer = new ChapterStructureAnalysisConsumer({ available: () => true,
    list: async (...args) => { calls.list.push(args); return page([change()]); },
    read: async (projectId, id) => { calls.read.push([projectId, id]); return change(id, projectId); },
    run: async (...args) => { calls.run.push(args); return { message: "检查结果已保存" }; },
    updated: (...args) => { calls.updated.push(args); }, failed: (...args) => { calls.failed.push(args); }, ...overrides });
  consumer.select({ projectId: "project", dirty: false });
  return { consumer, calls };
}

test("strict structure read IPC encodes exact identifiers/filter and rejects malformed inputs before HTTP", async () => {
  const handlers = new Map(); const requests = [];
  registerChapterStructureReadHandlers({ handle: (name, handler) => handlers.set(name, handler) }, async (...args) => { requests.push(args); return { read: true }; });
  const list = handlers.get("runtime:list-chapter-structure-changes"), read = handlers.get("runtime:read-chapter-structure-change");
  const input = { cursor: "cursor/一+", limit: 20, assessmentState: "pending" };
  assert.deepEqual(await list({}, "project/一", input), { read: true });
  await list({}, "project", { cursor: null, limit: 1, assessmentState: null });
  await read({}, "project/一", "change/一");
  assert.deepEqual(requests, [["/api/projects/project%2F%E4%B8%80/chapter-structure-changes?limit=20&cursor=cursor%2F%E4%B8%80%2B&assessmentState=pending"],
    ["/api/projects/project/chapter-structure-changes?limit=1"], ["/api/projects/project%2F%E4%B8%80/chapter-structure-changes/change%2F%E4%B8%80"]]);
  for (const invalid of [null, [], { ...input, limit: 0 }, { ...input, limit: 21 }, { ...input, limit: 1.2 },
    { ...input, cursor: undefined }, { ...input, cursor: "bad\n" }, { ...input, cursor: "一".repeat(342) },
    { ...input, assessmentState: "bound" }, { ...input, operation: "generateChapter" }, { cursor: null, limit: 10 }]) {
    assert.throws(() => list({}, "project", invalid), /读取请求无效/);
  }
  for (const id of ["", " id", "bad\n", "一".repeat(54), null, {}]) {
    assert.throws(() => read({}, "project", id), /读取请求无效/);
    assert.throws(() => list({}, id, input), /读取请求无效/);
  }
  assert.equal(requests.length, 3);
});

test("event validation binds exact project/deletion/source and rejects incomplete or forged assessment records", () => {
  const item = change(); assert.equal(validateChapterStructureChange(item, "project"), item);
  assert.deepEqual(validateChapterStructureContext({ projectId: "project", dirty: true }), { projectId: "project", dirty: true });
  assert.equal(validateChapterStructureContext(null), null);
  for (const invalid of [{ projectId: "project", dirty: "false" }, { projectId: "project", dirty: false, source: {} }]) assert.throws(() => validateChapterStructureContext(invalid));
  for (const invalid of [{ ...item, projectId: "foreign" }, { ...item, id: "other" }, { ...item, kind: "chapterInserted" },
    { ...item, resultRevision: 4 }, { ...item, chapterNumber: 0 }, { ...item, rawBody: "not authority" },
    { ...item, source: { ...item.source, objectId: "draft-foreign" } },
    { ...item, assessment: { ...item.assessment, taskId: "task-one" } }, { ...bound(item), assessment: { ...bound(item).assessment, workTaskId: null } }]) {
    assert.throws(() => validateChapterStructureChange(invalid, "project", item.id), /记录与作品或删除来源不一致/);
  }
});

test("Native determines inspection necessity independently of whether a saved revision exists", async () => {
  const pendingWithoutProse = { ...change(), source: { ...change().source, savedRevisionId: null } };
  const skippedEmptyProse = { ...change("empty-saved"), assessment: { state: "skipped", skipReason: "noSavedEvidence", taskId: null, workTaskId: null } };
  const { consumer, calls } = harness({ list: async () => page([skippedEmptyProse, pendingWithoutProse]), read: async () => pendingWithoutProse });
  await consumer.tick(); assert.equal(calls.run.length, 1);
  assert.deepEqual(calls.run[0][3], { type: "project", structureChangeId: pendingWithoutProse.id }); consumer.stop();
  const skipped = harness({ list: async () => page([skippedEmptyProse]) });
  await skipped.consumer.tick(); assert.equal(skipped.calls.read.length, 0); assert.equal(skipped.calls.run.length, 0); skipped.consumer.stop();
});

test("dirty/unavailable contexts do no reads or work and a reloaded pending event runs one exact read-only operation", async () => {
  let qualified = false; const { consumer, calls } = harness({ available: () => qualified });
  await consumer.tick(); qualified = true; consumer.select({ projectId: "project", dirty: true }); await consumer.tick();
  assert.deepEqual(calls, { list: [], read: [], run: [], updated: [], failed: [] });
  consumer.select({ projectId: "project", dirty: false }); await consumer.tick(); await consumer.tick();
  assert.deepEqual(calls.list[0], ["project", { cursor: null, limit: 20, assessmentState: "pending" }]);
  assert.equal(calls.run.length, 1);
  const [context, prompt, operation, target, isCurrent] = calls.run[0];
  assert.deepEqual({ context, prompt, operation, target, current: isCurrent() }, { context: { projectId: "project", dirty: false },
    prompt: CHAPTER_STRUCTURE_ANALYSIS_PROMPT, operation: { operation: "assessChapterStructure" }, target: { type: "project", structureChangeId: "change-one" }, current: true });
  assert.ok(Buffer.byteLength(prompt, "utf8") <= 1000); assert.ok(!prompt.includes("原章节名称"));
  assert.equal(calls.failed.length, 0); consumer.stop();
});

test("singleflight stays busy through run, blocks overlapping consumers and discards cross-project/ABA late reads", async () => {
  const holding = deferred(); let reads = 0;
  const { consumer, calls } = harness({ read: async (projectId, id) => { reads++; return reads === 1 ? holding.promise : change(id, projectId); } });
  const first = consumer.tick(); await new Promise(resolve => setImmediate(resolve));
  assert.equal(consumer.busy, true); await consumer.tick();
  consumer.select({ projectId: "other", dirty: false }); consumer.select({ projectId: "project", dirty: false });
  holding.resolve(change()); await first;
  assert.equal(calls.run.length, 0); assert.equal(consumer.busy, false);
  const model = deferred(); consumer.run = async (...args) => { calls.run.push(args); return model.promise; };
  const next = consumer.tick(); await new Promise(resolve => setImmediate(resolve)); await consumer.tick();
  assert.equal(consumer.busy, true); assert.equal(calls.run.length, 1);
  consumer.select({ projectId: "other", dirty: false }); model.resolve({ message: "saved" }); await next;
  assert.equal(calls.updated.length, 0); assert.equal(calls.run[0][4](), false); consumer.stop();
});

test("availability and dirty are rechecked after source reads; prebind guard stays false across a scope change", async () => {
  const read = deferred(); let qualified = true;
  const { consumer, calls } = harness({ available: () => qualified, read: async () => read.promise });
  const pending = consumer.tick(); await new Promise(resolve => setImmediate(resolve)); qualified = false;
  read.resolve(change()); await pending; assert.equal(calls.run.length, 0);
  qualified = true; consumer.read = async () => change();
  const beforeBind = deferred(); let guard;
  consumer.run = async (_context, _prompt, _operation, _target, isCurrent) => { guard = isCurrent; await beforeBind.promise; if (!isCurrent()) throw new Error("prebind source changed"); assert.fail("must not bind"); };
  const next = consumer.tick(); await new Promise(resolve => setImmediate(resolve));
  consumer.select({ projectId: "project", dirty: true }); assert.equal(guard(), false);
  beforeBind.resolve(); await next;
  assert.equal(calls.updated.length, 0); assert.equal(calls.failed.length, 1);
  assert.deepEqual(calls.failed[0][0], { projectId: "project", dirty: false });
  assert.equal(calls.failed[0][2].message, "prebind source changed"); consumer.stop();
});

test("unknown execution reads its binding once and preserves the original error without an automatic new root", async () => {
  const original = Object.assign(new Error("original failure"), { code: "TASK_CANCELLED" }); let reads = 0;
  const { consumer, calls } = harness({ read: async () => ++reads === 1 ? change() : bound(change()),
    run: async (...args) => { calls.run.push(args); throw original; },
    updated: (...args) => { calls.updated.push(args); throw new Error("presentation failed"); } });
  await consumer.tick(); await consumer.tick();
  assert.equal(calls.run.length, 1); assert.equal(reads, 2); assert.equal(calls.failed.length, 1);
  assert.equal(calls.failed[0][2], original); assert.deepEqual(calls.updated[0].slice(1), [bound(change()), null]);
  await assert.rejects(consumer.retry("project", "change-one"), /原任务恢复/); assert.equal(calls.run.length, 1); consumer.stop();
});

test("explicit retry must read exact still-pending state; failed reads retain the unknown hold and no timer retries", async () => {
  let unreadable = false; let runs = 0;
  const { consumer, calls } = harness({ read: async (projectId, id) => { if (unreadable) throw new Error("read unavailable"); return change(id, projectId); },
    run: async (...args) => { calls.run.push(args); if (++runs === 1) throw new Error("prebind failed"); return { message: "saved" }; } });
  await consumer.tick(); await consumer.tick(); assert.equal(runs, 1);
  unreadable = true; await assert.rejects(consumer.retry("project", "change-one"), /read unavailable/);
  unreadable = false; consumer.select({ projectId: "project", dirty: false }); await consumer.tick(); assert.equal(runs, 1);
  await consumer.retry("project", "change-one"); assert.equal(runs, 2);
  assert.deepEqual(calls.run.at(-1)[3], { type: "project", structureChangeId: "change-one" });
  for (const [projectId, id] of [["foreign", "change-one"], ["project", " id"], ["project", "一".repeat(54)]]) await assert.rejects(consumer.retry(projectId, id));
  assert.equal(calls.failed.length, 1); consumer.stop();
});

test("bounded pagination retains its cursor and skips bound/no-evidence records without spawning work", async () => {
  const requests = []; const skipped = { ...change("skipped"), source: { ...change().source, savedRevisionId: null }, assessment: { state: "skipped", skipReason: "noSavedEvidence", taskId: null, workTaskId: null } };
  const { consumer, calls } = harness({ list: async (_projectId, input) => {
    requests.push(input); const index = input.cursor === null ? 0 : Number(input.cursor.slice(1));
    return index === 5 ? page([change("pending-later")]) : page([bound(change(`bound-${index}`)), { ...skipped, id: `skipped-${index}` }], `c${index + 1}`);
  } });
  await consumer.tick(); assert.equal(requests.length, 4); assert.equal(calls.run.length, 0);
  await consumer.tick(); assert.equal(requests.length, 6); assert.equal(requests[4].cursor, "c4");
  assert.deepEqual(calls.run[0][3], { type: "project", structureChangeId: "pending-later" }); consumer.stop();
});

test("invalid page or exact event from a read never reaches the runner", async () => {
  for (const response of [page([change()], null, "foreign"), page([change(), change()]), page([], "same"),
    { ...page([]), rawBody: "unexpected" }]) {
    const { consumer, calls } = harness({ list: async () => response });
    if (response.nextCursor === "same") consumer.list = async (_projectId, input) => input.cursor === null ? page([], "same") : response;
    await consumer.tick(); assert.equal(calls.run.length, 0); assert.equal(calls.failed.length, 1); consumer.stop();
  }
  const { consumer, calls } = harness({ read: async () => ({ ...change(), projectId: "foreign" }) });
  await consumer.tick(); assert.equal(calls.run.length, 0); assert.equal(calls.failed.length, 1); consumer.stop();
});
