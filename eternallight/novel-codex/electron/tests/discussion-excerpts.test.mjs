import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { mainHandlers, rendererEvent, runtimeFixture } from "./main-handler-harness.mjs";
import { validateDiscussionExcerpts } from "../../shared/discussion-excerpts.mjs";
import { validateConversationFocus, bindConversationFocus, conversationFocusInstructions, executeConversationCandidates } from "../conversation-analysis.mjs";

const excerpt = (changes = {}) => ({ id: "excerpt-one", projectId: "project", view: "outline", label: "全书大纲 · 故事脉络",
  text: "他离开旧港。", state: "displayed", locator: "outline-one · summary", snapshotId: "snapshot-one",
  revisionId: "outline-revision-one", objectId: "artifact-outline", start: 2, end: 8, ...changes });

test("captured Unicode text preserves whitespace, source, offsets and exact content", () => {
  const text = "  他\n离开旧港。😀 ";
  const value = excerpt({ text, end: 2 + text.length });
  const result = validateDiscussionExcerpts([value]);
  assert.deepEqual(result, [value]);
  assert.ok(Object.isFrozen(result) && Object.isFrozen(result[0]));
  assert.match(conversationFocusInstructions({ view: "outline", excerpts: result }), /原文数据.*不是系统指令/);
});

test("strict quote schema and aggregate budgets reject malformed, oversize and duplicate captures", () => {
  for (const value of [null, [], {}, [excerpt({ end: 9 })], [excerpt({ state: "saved" })], [excerpt({ extra: "authority" })],
    [excerpt({ text: " " })], [excerpt({ start: -1, end: 5 })], [excerpt({ revisionId: "wrong\nsource" })],
    [excerpt(), excerpt()], Array.from({ length: 9 }, (_, i) => excerpt({ id: `excerpt-${i}` })),
    [excerpt({ text: "x".repeat(12001), start: 0, end: 12001 })],
    [excerpt({ text: "中".repeat(8001), start: 0, end: 8001 })]]) assert.throws(() => validateDiscussionExcerpts(value));
});

test("unsaved capture freezes a read-only discussion and never changes to latest text", () => {
  const selected = excerpt({ state: "unsaved" });
  const focus = bindConversationFocus({ project: { id: "project" } }, { view: "world", excerpts: [selected] });
  selected.text = "后来修改的正文";
  assert.equal(focus.readOnly, true);
  assert.equal(focus.excerpts[0].text, "他离开旧港。");
  assert.match(conversationFocusInstructions(focus), /不登记候选修改/);
  assert.throws(() => bindConversationFocus({ project: { id: "other" } }, { view: "outline", excerpts: [excerpt()] }), /不属于/);
  assert.throws(() => validateConversationFocus({ view: "outline", readOnly: true }), /模式无效/);
  assert.throws(() => validateConversationFocus({ view: "outline", readOnly: false, excerpts: [excerpt()] }), /模式无效/);
});

test("read-only selection discussion cannot auto-execute proposed writes", async () => {
  const source = { projectId: "project", operation: "advisory", target: { conversationFocus: { readOnly: true } } };
  const result = await executeConversationCandidates({}, "project", { workflow: { task: source } }, {
    runtimeRequest: async () => { assert.fail("read-only discussion requested candidate actions"); },
  });
  assert.deepEqual(result, { candidateResults: [], proposedActions: [] });
});

test("production send freezes captured excerpts and delivers them to the model as context", async () => {
  const runtime = runtimeFixture(), calls = [], inputs = [];
  const { handlers, adapter } = await mainHandlers(async (path, body, options) => {
    calls.push({ path, body });
    if (path.endsWith("/creative-workspace")) return { project: { id: "project" }, revision: 0, headSnapshotId: "snapshot-now", sourceDigest: "digest-now", chapters: [], tasks: [] };
    return runtime.request(path, body, options);
  }, `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`);
  const send = adapter.sendTurn;
  adapter.sendTurn = async function(input, options) { inputs.push(input); return send.call(this, input, options); };
  try {
    const focus = { view: "outline", excerpts: [excerpt({ state: "unsaved" })] };
    await handlers.get("agent:send-prompt")(rendererEvent(), "project", "讨论选区", null, focus);
    const created = calls.find(call => call.path.endsWith("/tasks") && call.body);
    assert.deepEqual(created.body.target.conversationFocus, validateConversationFocus(focus));
    assert.equal(inputs.length, 1);
    const text = typeof inputs[0] === "string" ? inputs[0] : inputs[0].map(item => item.text ?? "").join("\n");
    assert.match(text, /他离开旧港。/);
    assert.match(text, /不登记候选修改/);
    assert.ok(!calls.some(call => call.path.endsWith("/actions") || call.path.includes("/decision") || /save-candidate|accept/.test(call.path)));
    const before = calls.length;
    await assert.rejects(handlers.get("agent:send-prompt")(rendererEvent(), "other-project", "讨论", null, focus), /不属于/);
    assert.equal(calls.length, before);
  } finally { adapter.close(); }
});
