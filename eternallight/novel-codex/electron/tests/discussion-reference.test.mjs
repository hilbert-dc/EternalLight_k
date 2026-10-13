import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { validateDiscussionReference, verifyDiscussionReference } from "../discussion-reference.mjs";
import { validateConversationFocus, conversationFocusInstructions } from "../conversation-analysis.mjs";
import { assertRevisionRepairTaskTarget } from "../revision-repair-handlers.mjs";
import { mainHandlers, rendererEvent, runtimeFixture } from "./main-handler-harness.mjs";
import { novelTurnText } from "../codex-runtime.mjs";

const base = { projectId: "project", snapshotId: "snapshot" };
const scene = { kind: "scene", ...base, objectId: "draft-chapter", revisionId: "revision", sceneGroupId: "场景一" };
const claim = { kind: "claim", ...base, objectId: scene.objectId, revisionId: scene.revisionId, modelingBatchId: "batch", assertionIndex: 0 };
const commitment = { kind: "commitment", ...base, commitmentId: "commitment", revisionId: "commitment-revision" };
const workspace = () => ({ project: { id: "project" }, revision: 4, headSnapshotId: "snapshot", sourceDigest: "digest",
  chapters: [{ id: "chapter", number: 1, candidate: { id: "revision", sceneGroups: [{ id: "场景一", body: "原文" }] }, accepted: null }] });
const knowledge = () => ({ ...base, entries: [{ modelingBatchId: "batch", assertionIndex: 0, sourceObjectId: scene.objectId, sourceRevisionId: scene.revisionId }] });
const detail = () => ({ ...base, revisionId: commitment.revisionId, payload: { id: "commitment" }, sourceCurrent: true });
function verifier(reference, changes = {}) {
  const captured = workspace(); const calls = [];
  return { captured, calls, run: () => verifyDiscussionReference({ projectId: "project", workspace: captured, reference,
    runtimeRequest: async path => { calls.push(path);
      if (path.endsWith("creative-workspace")) return changes.workspace ?? captured;
      if (path.includes("knowledge/state")) return changes.knowledge ?? knowledge();
      if (path.includes("commitments/")) return changes.detail ?? detail();
      assert.fail(path);
    } }) };
}

test("references freeze exact union identity without renderer text or permission", () => {
  for (const reference of [scene, claim, commitment]) {
    const supplied = { ...reference }; const focus = validateConversationFocus({ view: "world", reference: supplied });
    supplied.revisionId = "changed";
    assert.deepEqual(focus.reference, reference);
    assert.ok(Object.isFrozen(focus) && Object.isFrozen(focus.reference));
    assert.match(conversationFocusInstructions(focus), /不是写入授权/);
  }
  for (const invalid of [null, [], {}, { ...scene, body: "renderer text" }, { ...scene, operation: "accept" },
    { ...scene, projectId: " other" }, { ...scene, sceneGroupId: "字".repeat(54) }, { ...scene, snapshotId: "bad\n" },
    { ...claim, assertionIndex: -1 }, { ...claim, assertionIndex: 0.5 }, { ...claim, assertionIndex: Number.MAX_SAFE_INTEGER + 1 },
    { ...claim, [Symbol("hidden")]: true }, Object.create(scene)]) assert.throws(() => validateDiscussionReference(invalid), /讨论来源引用/);
  assert.deepEqual(validateConversationFocus({ view: "world" }), { view: "world" });
});

test("scene permits unique saved/published revision and group, not an ordinal", async () => {
  await verifier(scene).run();
  const published = workspace(); published.chapters[0].accepted = published.chapters[0].candidate; published.chapters[0].candidate = null;
  await verifyDiscussionReference({ projectId: "project", workspace: published, reference: scene, runtimeRequest: async () => published });
  for (const change of [value => value.chapters.push(value.chapters[0]), value => value.chapters[0].candidate.id = "new",
    value => value.chapters[0].candidate.sceneGroups.push({ id: "场景一" })]) {
    const value = workspace(); change(value);
    await assert.rejects(verifyDiscussionReference({ projectId: "project", workspace: value, reference: scene, runtimeRequest: () => assert.fail("invalid source must stop") }), /讨论来源已变化/);
  }
});

test("claim requires unique batch/index plus exact object/source at selected snapshot", async () => {
  const valid = verifier(claim); await valid.run();
  assert.equal(valid.calls[0], "/api/projects/project/knowledge/state?perspective=author&snapshotId=snapshot");
  for (const change of [value => value.entries.push(value.entries[0]), value => value.entries[0].sourceRevisionId = "other",
    value => value.entries[0].sourceObjectId = "other", value => value.entries[0].assertionIndex = 1,
    value => value.projectId = "other", value => value.snapshotId = "other"]) {
    const value = knowledge(); change(value); await assert.rejects(verifier(claim, { knowledge: value }).run(), /讨论来源已变化/);
  }
});

test("commitment validates exact detail and current evidence without accepting it", async () => {
  const valid = verifier(commitment); await valid.run();
  assert.deepEqual(valid.calls, ["/api/projects/project/commitments/commitment?snapshotId=snapshot", "/api/projects/project/creative-workspace"]);
  for (const change of [value => value.projectId = "other", value => value.snapshotId = "other", value => value.payload.id = "other",
    value => value.revisionId = "other", value => value.sourceCurrent = false]) {
    const value = detail(); change(value); await assert.rejects(verifier(commitment, { detail: value }).run(), /讨论来源已变化/);
  }
});

test("scope/source races reject without rebasing and read failures propagate", async () => {
  for (const key of ["revision", "headSnapshotId", "sourceDigest"]) {
    const fresh = workspace(); fresh[key] = key === "revision" ? 5 : "changed";
    await assert.rejects(verifier(claim, { workspace: fresh }).run(), /讨论来源已变化/);
  }
  for (const reference of [{ ...claim, projectId: "other" }, { ...claim, snapshotId: "other" }]) {
    const f = verifier(reference); await assert.rejects(f.run(), /讨论来源已变化/); assert.deepEqual(f.calls, []);
  }
  await assert.rejects(verifyDiscussionReference({ projectId: "project", workspace: workspace(), reference: claim,
    runtimeRequest: async () => { throw new Error("read unavailable"); } }), /read unavailable/);
});

test("repair Runtime JSON roundtrip preserves full reference, changed identity is refused", () => {
  const source = { writerTaskId: `task-${"a".repeat(32)}`, assessmentRevisionId: "assessment", decisionId: "decision" };
  const target = { type: "project", revisionRepairSource: source, conversationFocus: { view: "world", reference: claim } };
  const task = { projectId: "project", operation: "advisory", target: structuredClone(target) };
  assert.doesNotThrow(() => assertRevisionRepairTaskTarget(task, "project", target));
  task.target.conversationFocus.reference.assertionIndex = 1;
  assert.throws(() => assertRevisionRepairTaskTarget(task, "project", target), /精确的修复/);
});

test("production advisory rejects stale reference before model or task creation", async () => {
  const runtime = runtimeFixture(); const calls = [];
  const { handlers, adapter } = await mainHandlers(async (path, body) => {
    calls.push({ path, body });
    if (path.endsWith("creative-workspace")) return workspace();
    return runtime.request(path, body);
  }, `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`);
  let turns = 0; adapter.sendTurn = async () => { turns++; assert.fail("invalid source must not call model"); };
  try {
    for (const reference of [{ ...scene, snapshotId: "old" }, { ...scene, revisionId: "old" }, { ...scene, projectId: "other" }]) {
      await assert.rejects(handlers.get("agent:send-prompt")(rendererEvent(), "project", "讨论这个场景", null, { view: "chapters", chapterId: "chapter", chapterNumber: 1, reference }), /讨论来源已变化/);
    }
    assert.equal(turns, 0); assert.ok(!calls.some(call => call.path.endsWith("/tasks")));
  } finally { adapter.close(); }
});

test("production freezes exact source before a model turn and rereads the same workspace for Work completion", async () => {
  const runtime = runtimeFixture(); const calls = [], inputs = [];
  const { handlers, adapter } = await mainHandlers(async (path, body, options, url) => {
    calls.push({ path, body, method: options?.method ?? "GET", search: url.search });
    if (path.endsWith("creative-workspace")) return workspace();
    if (path.includes("knowledge/state")) return knowledge();
    if (path.includes("/commitments/")) return detail();
    return runtime.request(path, body, options, url);
  }, `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`, {
    onPrepareDomain(domain) {
      const active = domain.codexAdapter, send = active.sendTurn;
      active.sendTurn = async function(input, options) {
        inputs.push({ input, additionalContext: options.additionalContext });
        return send.call(this, input, options);
      };
    },
  });
  try {
    for (const reference of [scene, claim, commitment]) {
      const focus = { view: reference.kind === "scene" ? "chapters" : reference.kind === "claim" ? "world" : "plot",
        ...(reference.kind === "scene" ? { chapterId: "chapter", chapterNumber: 1 } : {}), reference };
      const before = calls.length;
      await handlers.get("agent:send-prompt")(rendererEvent(), "project", "讨论这个来源，不要修改", null, focus);
      const current = calls.slice(before);
      const created = current.find(call => call.path.endsWith("/tasks") && call.body);
      assert.deepEqual(created.body.target.conversationFocus, focus);
      assert.equal(created.body.target.type, "project");
      assert.ok(created.body.target.messageDispatch.messageId);
      const workspaces = current.filter(call => call.path.endsWith("creative-workspace"));
      assert.equal(workspaces.length, 3);
      assert.ok(workspaces.every(call => call.path === "/api/projects/project/creative-workspace" && call.method === "GET" && !call.body));
      const creationIndex = current.indexOf(created), taskId = runtime.tasks.get(created.body.threadId).id;
      assert.ok(workspaces.slice(0, 2).every(call => current.indexOf(call) < creationIndex));
      const completionIndex = current.findIndex(call => call.path === `/api/projects/project/tasks/${taskId}/complete` && call.body);
      const decisionIndex = current.findIndex(call => call.path === `/api/projects/project/work-tasks/work-${taskId}/decision` && call.body);
      // Initial source verification freezes two reads; the third is the same
      // project's current revision/digest for the Native Work-close decision.
      assert.ok(completionIndex > creationIndex && current.indexOf(workspaces[2]) > completionIndex && decisionIndex > current.indexOf(workspaces[2]));
      assert.equal(current[decisionIndex].body.expectedRevision, 4);
      assert.equal(current[decisionIndex].body.expectedSourceDigest, "digest");
      const sourceReads = current.filter(call => call.path.includes("/knowledge/state") || call.path.includes("/commitments/"));
      assert.deepEqual(sourceReads.map(call => [call.path, call.search, call.method, call.body]), reference.kind === "scene" ? []
        : [[reference.kind === "claim" ? "/api/projects/project/knowledge/state" : "/api/projects/project/commitments/commitment",
          reference.kind === "claim" ? "?perspective=author&snapshotId=snapshot" : "?snapshotId=snapshot", "GET", null]]);
      assert.ok(sourceReads.every(call => current.indexOf(call) < creationIndex));
    }
    assert.equal(inputs.length, 3);
    assert.ok(inputs.every(turn => novelTurnText(turn).includes("不能用同名")));
    assert.ok(!calls.some(call => /accept|establish|save-candidate/.test(call.path)));
  } finally { adapter.close(); }
});

test("production rejects a lost reference in the creation receipt before sending a model turn", async () => {
  const runtime = runtimeFixture();
  const { handlers, adapter } = await mainHandlers(async (path, body, options) => {
    if (path.endsWith("creative-workspace")) return workspace();
    const result = await runtime.request(path, body, options);
    if (path.endsWith("/tasks") && body) result.target = { ...result.target, conversationFocus: { view: "chapters", chapterId: "chapter", chapterNumber: 1 } };
    return result;
  }, `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`);
  let turns = 0; adapter.sendTurn = async () => { turns++; assert.fail("lost reference must not call model"); };
  try {
    await assert.rejects(handlers.get("agent:send-prompt")(rendererEvent(), "project", "讨论这个场景", null,
      { view: "chapters", chapterId: "chapter", chapterNumber: 1, reference: scene }), /Runtime 未保留精确的讨论来源引用/);
    assert.equal(turns, 0);
  } finally { adapter.close(); }
});
